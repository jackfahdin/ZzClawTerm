use gpui::{Context, Window};

use crate::features::NyaTermApp;
use crate::features::ai::AiSettingsMutation;

impl NyaTermApp {
    pub(in crate::features) fn toggle_ai_credential_enabled(
        &mut self,
        credential_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.toggle_settings_credential_enabled(&credential_id) == AiSettingsMutation::Persist
        {
            self.persist_ai_settings_now(cx);
        }
    }

    /// Apply an edit from one of a credential's inputs.
    ///
    /// `rest` is what follows `ai.credential.` in the field id: the credential
    /// id, then the field.
    pub(in crate::features) fn apply_ai_credential_input(
        &mut self,
        rest: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.apply_settings_credential_input(rest, text) {
            self.request_settings_panel_refresh(cx);
        }
    }

    pub(in crate::features) fn persist_ai_credential_edits(
        &mut self,
        credential_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.ai.commit_settings_credential_edits(credential_id);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn add_ai_credential(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = format!(
            "credential-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let focus = self.ai.add_settings_credential(id.clone());
        // The new row draws three inputs, so the add is what builds them.
        self.ensure_ai_credential_inputs(&id, cx);
        window.focus(&focus, cx);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn remove_ai_credential(
        &mut self,
        credential_id: String,
        cx: &mut Context<Self>,
    ) {
        match self.ai.remove_settings_credential(&credential_id) {
            AiSettingsMutation::Ignored => {}
            AiSettingsMutation::Notify => self.request_settings_panel_refresh(cx),
            AiSettingsMutation::Persist => self.persist_ai_settings_now(cx),
        }
    }
}
