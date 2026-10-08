use crate::features::pages::settings::panel::SettingsPanel;
use gpui::{Context, IntoElement};

impl SettingsPanel {
    pub(in crate::features) fn ai_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ai_general_content(cx)
    }
    pub(in crate::features) fn ai_agents_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ai_agents_content(cx)
    }
}
