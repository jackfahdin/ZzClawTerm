use gpui::{Context, div, prelude::*, px};

use crate::features::NyaTermApp;
use crate::models::BottomPanelMode;

impl NyaTermApp {
    pub(in crate::features) fn bottom_panel_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match self.shell.bottom_panel_mode() {
            BottomPanelMode::QuickCommands => {
                let palette = self.theme_palette();
                div()
                    .h(px(self.shell.quick_commands_height().clamp(36., 520.)))
                    .flex_none()
                    .bg(self.shell_surface_color(palette.surface))
                    .child(self.quick_commands_panel(cx))
                    .into_any_element()
            }
            BottomPanelMode::CommandSend => self.bottom_command_send_bar(cx).into_any_element(),
            BottomPanelMode::Hidden => div().into_any_element(),
        }
    }
}
