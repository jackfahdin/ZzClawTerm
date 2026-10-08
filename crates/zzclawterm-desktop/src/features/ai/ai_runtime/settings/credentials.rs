use gpui::Context;

use crate::features::ZzClawTermApp;

impl ZzClawTermApp {
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
            self.defer_settings_persistence(cx);
            self.request_settings_panel_refresh(cx);
        }
    }
}
