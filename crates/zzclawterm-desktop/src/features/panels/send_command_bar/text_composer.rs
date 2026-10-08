use super::state::SendCommandBarViewState;
use crate::features::{ZzClawTermApp, text_inputs::TextInputSetup};
use gpui::{Context, IntoElement, div, prelude::*, px};

impl ZzClawTermApp {
    pub(super) fn send_command_text_composer(
        &mut self,
        state: &SendCommandBarViewState,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let app = cx.entity().downgrade();
        let input = self
            .text_input_box(
                "send-command.draft",
                &state.send.draft,
                TextInputSetup::multi_line(state.input_hint.clone()),
                cx,
            )
            .fill_height()
            .bare()
            .on_secondary_enter(move |_, cx| {
                let _ = app.update(cx, |app, cx| app.send_bottom_command(false, cx));
            });
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .px_2()
            .py(px(2.))
            .flex()
            .flex_col()
            .font_family(crate::features::shell::gpui_code_font_family())
            .child(input)
            .into_any_element()
    }
}
