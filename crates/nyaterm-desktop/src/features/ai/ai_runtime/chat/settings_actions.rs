use rust_i18n::t;

use gpui::{Context, KeyDownEvent, Window};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::{AiActionEditorField, AiActionListKind};
use nyaterm_core::AiSettings;
use nyaterm_store::{StoreDomain, store_request};

impl NyaTermApp {
    pub(in crate::features) fn pending_ai_settings(&self) -> AiSettings {
        self.ai.pending_settings()
    }

    /// Persist current `ai_settings` without rewriting active profile drafts (Tauri live update).
    pub(in crate::features) fn persist_ai_settings_now(&mut self, cx: &mut Context<Self>) {
        if self.defer_settings_persistence(cx) {
            self.ai.set_panel_status("AI settings staged".to_string());
            self.request_settings_panel_refresh(cx);
            self.defer_ai_panel_snapshot_flush(cx);
            return;
        }
        let snapshot = self.ai.settings_config_cloned();
        if let Some((generation, snapshot)) = self.ai.queue_settings_persistence(snapshot) {
            self.submit_ai_settings_save(generation, snapshot, cx);
        }
        self.request_settings_panel_refresh(cx);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    fn submit_ai_settings_save(
        &mut self,
        generation: u64,
        snapshot: AiSettings,
        cx: &mut Context<Self>,
    ) {
        let request = store_request(StoreDomain::Ai, move |store| {
            store.save_ai_settings(snapshot)
        });
        let task = match self.store_ui.try_submit(generation, request) {
            Ok(task) => task,
            Err(error) => {
                self.ai.finish_settings_persistence(generation, false);
                self.ai
                    .set_panel_status(format!("AI settings save was not queued: {error}"));
                self.settings
                    .update_store_status(self.ai.panel_status().to_string(), false);
                self.request_settings_panel_refresh(cx);
                self.defer_ai_panel_snapshot_flush(cx);
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            let event = task.await;
            let _ = this.update(cx, |this, cx| {
                let completion = this
                    .ai
                    .finish_settings_persistence(event.generation, event.outcome.is_ok());
                if completion.apply_result
                    && let Ok(saved) = event.outcome.as_ref()
                {
                    this.ai.accept_saved_settings(saved.clone());
                    this.refresh_ai_usage_counts(cx);
                }
                if completion.report_result {
                    match event.outcome {
                        Ok(_) => this.ai.set_panel_status("AI settings saved".to_string()),
                        Err(error) => this
                            .ai
                            .set_panel_status(format!("AI settings save failed: {error}")),
                    }
                    this.settings.update_store_status(
                        this.ai.panel_status().to_string(),
                        completion.apply_result,
                    );
                }
                if let Some((generation, snapshot)) = completion.next {
                    this.submit_ai_settings_save(generation, snapshot, cx);
                }
                this.request_settings_panel_refresh(cx);
                this.defer_ai_panel_snapshot_flush(cx);
            });
        })
        .detach();
    }

    pub(in crate::features) fn focus_ai_action_field(
        &mut self,
        kind: AiActionListKind,
        action_id: String,
        field: AiActionEditorField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.ai.settings_action_value(kind, &action_id, field);
        let setup = match field {
            AiActionEditorField::Name => TextInputSetup::placeholder(t!("ai.actionName")),
            AiActionEditorField::Prompt => TextInputSetup::multi_line(t!("ai.actionPrompt")),
        };
        let input_id = Self::ai_action_text_input_id(kind, &action_id, field);
        let input = self.text_input(input_id, &value, setup, cx);
        self.ai.focus_settings_action(kind, action_id, field);
        window.focus(&input.read(cx).focus_handle(), cx);
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn toggle_ai_action_enabled(
        &mut self,
        kind: AiActionListKind,
        action_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.toggle_settings_action_enabled(kind, &action_id) {
            self.persist_ai_settings_now(cx);
        }
    }

    pub(in crate::features) fn add_ai_action(
        &mut self,
        kind: AiActionListKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = format!(
            "ai-action-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        self.ai.add_settings_action(kind, id.clone());
        // The new row draws a name and a prompt, so the add is what builds both.
        self.ensure_ai_action_inputs(kind, &id, cx);
        let name_id = Self::ai_action_text_input_id(kind, &id, AiActionEditorField::Name);
        self.focus_text_input_if_present(&name_id, window, cx);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn remove_ai_action(
        &mut self,
        kind: AiActionListKind,
        action_id: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.remove_settings_action(kind, &action_id);
        self.forget_text_inputs(&format!(
            "ai.settings.action.{}.{action_id}.",
            kind.input_key()
        ));
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn handle_ai_action_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((kind, action_id, field)) = self.ai.settings_action_edit() else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                let focus = self.ai.cancel_settings_action_edit();
                window.focus(&focus, cx);
                self.request_settings_panel_refresh(cx);
            }
            "tab" => {
                self.focus_ai_action_field(kind, action_id, field.next(), window, cx);
            }
            "enter" if field == AiActionEditorField::Name => {
                self.focus_ai_action_field(
                    kind,
                    action_id,
                    AiActionEditorField::Prompt,
                    window,
                    cx,
                );
            }
            _ => return false,
        }
        true
    }

    pub(in crate::features) fn apply_ai_action_input(
        &mut self,
        field_id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some((kind, action_id, field)) = parse_ai_action_text_input_id(field_id) else {
            return;
        };
        if self
            .ai
            .apply_settings_action_input(kind, action_id, field, text)
        {
            self.persist_ai_settings_now(cx);
        }
    }

    pub(in crate::features) fn ai_action_text_input_id(
        kind: AiActionListKind,
        action_id: &str,
        field: AiActionEditorField,
    ) -> String {
        format!(
            "ai.settings.action.{}.{action_id}.{}",
            kind.input_key(),
            field.input_key()
        )
    }
}

fn parse_ai_action_text_input_id(
    field_id: &str,
) -> Option<(AiActionListKind, &str, AiActionEditorField)> {
    let (kind, rest) = field_id.split_once('.')?;
    let (action_id, field) = rest.rsplit_once('.')?;
    if action_id.is_empty() {
        return None;
    }
    Some((
        AiActionListKind::from_input_key(kind)?,
        action_id,
        AiActionEditorField::from_input_key(field)?,
    ))
}

#[cfg(test)]
mod tests {
    use crate::models::{AiActionEditorField, AiActionListKind};

    use super::parse_ai_action_text_input_id;

    #[test]
    fn parses_ai_action_text_input_id() {
        assert_eq!(
            parse_ai_action_text_input_id("terminal.some-action.name"),
            Some((
                AiActionListKind::Terminal,
                "some-action",
                AiActionEditorField::Name,
            ))
        );
    }

    #[test]
    fn parses_ai_action_id_containing_dots() {
        assert_eq!(
            parse_ai_action_text_input_id("file.some.nested.action.prompt"),
            Some((
                AiActionListKind::File,
                "some.nested.action",
                AiActionEditorField::Prompt,
            ))
        );
    }

    #[test]
    fn rejects_invalid_ai_action_text_input_ids() {
        for field_id in [
            "terminal.action",
            "terminal..name",
            "unknown.action.name",
            "file.action.unknown",
        ] {
            assert_eq!(parse_ai_action_text_input_id(field_id), None, "{field_id}");
        }
    }
}
