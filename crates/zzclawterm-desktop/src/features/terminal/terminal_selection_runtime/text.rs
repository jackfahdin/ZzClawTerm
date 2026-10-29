use gpui::{ClipboardItem, Context};
use zzclawterm_ui::notification::ZzClawNotificationKind;

use crate::features::ZzClawTermApp;
use crate::features::terminal::state::TerminalSelectionTextStatus;
use crate::models::{TerminalSelection, TerminalSelectionTextError};

#[cfg(test)]
mod tests;

impl ZzClawTermApp {
    pub(in crate::features) fn terminal_has_selection_for_session(&self, session_id: &str) -> bool {
        self.terminal
            .selection
            .session_id
            .as_deref()
            .or(self.session.active_id())
            == Some(session_id)
            && self
                .terminal
                .selection
                .selection
                .is_some_and(|selection| !selection.is_empty())
    }

    pub(in crate::features) fn ensure_terminal_selection_text(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self
            .terminal
            .selection
            .selection
            .filter(|selection| !selection.is_empty())
        else {
            return;
        };
        let Some(session_id) = self
            .terminal
            .selection
            .session_id
            .clone()
            .or_else(|| self.session.active_id_owned())
        else {
            return;
        };
        let key = (session_id.clone(), selection);
        if self.terminal.selection.text.key.as_ref() == Some(&key) {
            return;
        }
        self.terminal.selection.text.invalidate();
        // Small selections keep the snapshot fast path. Large ranges are read only
        // on the frame worker; never rebuild the entire UI-side history here.
        let immediate = self.selected_terminal_text();
        self.terminal.selection.text.key = Some(key.clone());
        let history = self.terminal.selection.source_history.or_else(|| {
            self.terminal
                .view
                .views
                .get(&session_id)
                .map(|view| view.history_state())
        });
        if let Some(source) = self.terminal.selection.source_history
            && self
                .terminal
                .view
                .views
                .get(&session_id)
                .is_some_and(|view| !view.history_state().preserves_coordinates_from(source))
        {
            self.terminal.selection.text.status =
                TerminalSelectionTextStatus::Failed(TerminalSelectionTextError::StaleHistory);
            return;
        }
        if let Some(text) = immediate {
            self.terminal.selection.text.status = TerminalSelectionTextStatus::Ready(Some(text));
            return;
        }
        let Some(history) = history else {
            self.terminal.selection.text.status =
                TerminalSelectionTextStatus::Failed(TerminalSelectionTextError::MissingSession);
            return;
        };
        self.terminal.selection.text.status = TerminalSelectionTextStatus::Pending;
        let generation = self.terminal.selection.text.generation;
        let response = if selection.all_buffer {
            futures::future::Either::Left(
                self.terminal
                    .view
                    .frame_pipeline
                    .request_all_text(session_id),
            )
        } else {
            futures::future::Either::Right(
                self.terminal
                    .view
                    .frame_pipeline
                    .request_selection_text(session_id, selection, history),
            )
        };
        self.terminal.selection.text.task = Some(cx.spawn(async move |this, cx| {
            let result = match response {
                futures::future::Either::Left(response) => response.await.map(Ok),
                futures::future::Either::Right(response) => response.await,
            }
            .unwrap_or(Err(TerminalSelectionTextError::WorkerUnavailable));
            let _ = this.update(cx, |this, cx| {
                if this.terminal.selection.text.generation != generation
                    || this.terminal.selection.text.key.as_ref() != Some(&key)
                    || !this.terminal_has_selection_for_session(&key.0)
                    || this.terminal.selection.selection != Some(key.1)
                {
                    return;
                }
                this.terminal.selection.text.task = None;
                this.terminal.selection.text.status = match result {
                    Ok(text) => TerminalSelectionTextStatus::Ready(text),
                    Err(error) => TerminalSelectionTextStatus::Failed(error),
                };
                if let Some(feedback) = this.terminal.selection.text.pending_copy.take() {
                    this.copy_terminal_selection_with_feedback(feedback, cx);
                    this.terminal.selection.text.deferred_copy_completed = true;
                }
                this.schedule_terminal_selected_occurrence_search(cx);
                cx.notify();
            });
        }));
    }

    pub(in crate::features) fn copy_terminal_selection(&mut self, cx: &mut Context<Self>) -> bool {
        self.copy_terminal_selection_with_feedback(false, cx)
    }

    pub(in crate::features) fn copy_terminal_selection_request(
        &mut self,
        key: &(String, TerminalSelection),
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal.selection.text.deferred_copy_completed
            && self.terminal.selection.text.generation == generation
            && self.terminal.selection.text.key.as_ref() == Some(key)
            && self.terminal.selection.selection == Some(key.1)
            && self.terminal_has_selection_for_session(&key.0)
        {
            self.copy_terminal_selection_with_feedback(true, cx);
            if !matches!(
                self.terminal.selection.text.status,
                TerminalSelectionTextStatus::Pending
            ) {
                self.terminal.selection.text.deferred_copy_completed = true;
            }
        }
    }

    fn copy_terminal_selection_with_feedback(
        &mut self,
        feedback: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .terminal
            .selection
            .selection
            .is_none_or(|selection| selection.is_empty())
        {
            return false;
        }
        self.ensure_terminal_selection_text(cx);
        match &self.terminal.selection.text.status {
            TerminalSelectionTextStatus::Ready(Some(text)) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                self.shell
                    .set_status("copied terminal selection".to_string());
                self.terminal.menus.actions_open = false;
                if feedback
                    && self
                        .settings
                        .summary()
                        .interaction_copy_success_notification
                {
                    self.notify_operation(
                        "copy-terminal-selection",
                        ZzClawNotificationKind::Success,
                        rust_i18n::t!("common.copied").to_string(),
                        cx,
                    );
                }
                cx.notify();
            }
            TerminalSelectionTextStatus::Pending => {
                let pending = &mut self.terminal.selection.text.pending_copy;
                *pending = Some(pending.unwrap_or(false) || feedback);
            }
            TerminalSelectionTextStatus::Failed(_)
            | TerminalSelectionTextStatus::Ready(None)
            | TerminalSelectionTextStatus::Unrequested => {
                if feedback {
                    self.notify_operation(
                        "copy-terminal-selection",
                        ZzClawNotificationKind::Error,
                        rust_i18n::t!("common.copyFailed").to_string(),
                        cx,
                    );
                }
            }
        }
        // An existing selection consumes Copy, including pending/failed extraction.
        // Falling back here would silently copy unrelated visible text.
        true
    }

    pub(in crate::features) fn copy_terminal_selection_or_visible(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.copy_terminal_selection_with_feedback(true, cx) {
            self.copy_terminal_visible_text(cx);
        }
    }
}
