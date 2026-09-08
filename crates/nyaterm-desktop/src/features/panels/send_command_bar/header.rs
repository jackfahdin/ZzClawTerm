use rust_i18n::t;

use super::state::SendCommandBarViewState;
use gpui::{Context, FontWeight, div, prelude::*, px, rgb};

use crate::features::NyaTermApp;
impl NyaTermApp {
    pub(super) fn send_command_bar_header(
        &mut self,
        state: &SendCommandBarViewState,
        _cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = state.palette;
        let target_kind = state.target_kind.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight(500.))
                    .text_color(rgb(palette.text))
                    .child(t!("serialSend.title")),
            )
            .child(
                div()
                    .ml_auto()
                    .text_size(px(10.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(target_kind),
            )
            .into_any_element()
    }
}
