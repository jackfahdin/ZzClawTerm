use gpui::{IntoElement, div, prelude::*, px, rgb};

use crate::theme::ThemePalette;

pub(in crate::features) fn stats_progress_bar(
    palette: ThemePalette,
    ratio: f64,
) -> impl IntoElement {
    let ratio = ratio.clamp(0., 1.) as f32;
    div()
        .h(px(5.))
        .w_full()
        .overflow_hidden()
        .rounded_sm()
        .bg(rgb(palette.border))
        .child(
            div()
                .h(px(5.))
                .w(gpui::relative(ratio))
                .rounded_sm()
                .bg(if ratio >= 0.9_f32 {
                    rgb(0xfb7185)
                } else if ratio >= 0.7_f32 {
                    rgb(palette.warning)
                } else {
                    rgb(0x38bdf8)
                }),
        )
}
