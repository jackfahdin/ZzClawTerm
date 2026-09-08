use rust_i18n::t;

use gpui::{Context, KeyDownEvent, Window};

use crate::features::ai::AiSettingsMutation;
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};

impl NyaTermApp {
    pub(in crate::features) fn toggle_ai_model_enabled(
        &mut self,
        model_id: String,
        cx: &mut Context<Self>,
    ) {
        let before = self.ai_header_presentation();
        self.ai.toggle_settings_model_enabled(&model_id);
        self.persist_ai_settings_now(cx);
        self.notify_root_if_ai_header_changed(before, cx);
    }

    pub(in crate::features) fn apply_ai_settings_model_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.set_settings_model_query(text);
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn set_ai_default_model(
        &mut self,
        model_id: String,
        cx: &mut Context<Self>,
    ) {
        let before = self.ai_header_presentation();
        self.ai.set_settings_default_model(&model_id);
        self.persist_ai_settings_now(cx);
        self.notify_root_if_ai_header_changed(before, cx);
    }

    pub(in crate::features) fn remove_ai_manual_model(
        &mut self,
        model_id: String,
        cx: &mut Context<Self>,
    ) {
        match self.ai.remove_settings_manual_model(&model_id) {
            AiSettingsMutation::Ignored => {}
            AiSettingsMutation::Notify => self.request_settings_panel_refresh(cx),
            AiSettingsMutation::Persist => self.persist_ai_settings_now(cx),
        }
    }

    pub(in crate::features) fn add_ai_manual_model(
        &mut self,
        credential_id: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        match self.ai.add_settings_manual_model(&credential_id, &name) {
            AiSettingsMutation::Ignored => {}
            AiSettingsMutation::Notify => self.request_settings_panel_refresh(cx),
            AiSettingsMutation::Persist => self.persist_ai_settings_now(cx),
        }
    }

    pub(in crate::features) fn toggle_ai_model_group(
        &mut self,
        group_key: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.toggle_settings_model_group(group_key.clone());
        // Expanding a group reveals its manual-model field, so build it here rather
        // than in the row's render.
        let draft = self.ai.settings_manual_model_draft(&group_key);
        self.ensure_text_input(
            format!("ai.settings.manual-model.{group_key}"),
            &draft,
            crate::features::text_inputs::TextInputSetup::placeholder(
                rust_i18n::t!("ai.manualModelPlaceholder").to_string(),
            ),
            cx,
        );
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn handle_ai_manual_model_key_down(
        &mut self,
        group_key: &str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.ai.begin_settings_manual_model_edit(group_key);
        match event.keystroke.key.as_str() {
            "escape" => {
                let focus = self.ai.cancel_settings_manual_model_edit();
                window.focus(&focus, cx);
                self.request_settings_panel_refresh(cx);
            }
            "enter" => {
                if let Some((credential_id, name)) =
                    self.ai.settings_manual_model_submission(group_key)
                {
                    self.add_ai_manual_model(credential_id, name, cx);
                    self.clear_ai_manual_model_draft(group_key, cx);
                }
            }
            _ => return false,
        }
        true
    }

    pub(in crate::features) fn apply_ai_manual_model_input(
        &mut self,
        group_key: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if !self.ai.apply_settings_manual_model_input(group_key, text) {
            return;
        }
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn focus_ai_manual_model_input(
        &mut self,
        group_key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self.ai.settings_manual_model_draft(&group_key);
        let input = self.text_input(
            format!("ai.settings.manual-model.{group_key}"),
            &draft,
            TextInputSetup::placeholder(t!("ai.manualModelPlaceholder")),
            cx,
        );
        self.ai.focus_settings_manual_model_edit(group_key);
        window.focus(&input.read(cx).focus_handle(), cx);
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn clear_ai_manual_model_draft(
        &mut self,
        group_key: &str,
        cx: &mut Context<Self>,
    ) {
        self.ai.clear_settings_manual_model_draft(group_key);
        self.reset_text_input(&format!("ai.settings.manual-model.{group_key}"), "", cx);
        self.request_settings_panel_refresh(cx);
    }
}
