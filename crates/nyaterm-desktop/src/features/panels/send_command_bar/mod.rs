use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::features::NyaTermApp;

mod controls;
mod editor;
mod header;
mod state;

impl NyaTermApp {
    pub(in crate::features) fn bottom_command_send_bar(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let state = self.send_command_bar_view_state();
        let palette = state.palette;
        let header = self.send_command_bar_header(&state, cx);
        let controls = self.send_command_bar_controls(&state, cx);
        let editor = self.send_command_bar_editor(&state, cx);

        // Tauri SendCommandPanel: title row + labeled control groups + editor with floating action.
        div()
            .h(px(self.shell.command_send_height().clamp(60., 520.)))
            .flex_none()
            .flex()
            .flex_col()
            .bg(self.shell_surface_color(palette.surface))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px_2()
                    .py(px(6.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(header)
                    .child(controls)
                    .child(editor),
            )
    }
}
