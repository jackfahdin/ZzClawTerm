use gpui::Context;

use crate::features::NyaTermApp;
use crate::models::AiInputField;

impl NyaTermApp {
    /// Apply an edit from one of the AI settings inputs.
    pub(in crate::features) fn apply_ai_input(
        &mut self,
        field: AiInputField,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.apply_settings_input(field, text);
        // The user-agent is a live setting rather than a draft, so it is saved
        // as it is typed the way it always was.
        if !matches!(
            field,
            AiInputField::Model | AiInputField::BaseUrl | AiInputField::ApiKey
        ) {
            self.persist_ai_settings_now(cx);
        } else {
            cx.notify();
        }
    }
}
