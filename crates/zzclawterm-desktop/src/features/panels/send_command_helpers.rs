use gpui::{IntoElement, SharedString, div, prelude::*, px, rgb};

pub(super) fn send_command_control_group(
    palette: crate::theme::ThemePalette,
    label: impl Into<SharedString>,
    content: impl IntoElement,
) -> impl IntoElement {
    let label: SharedString = label.into();
    // Tauri labeled control: h-8 bordered group with muted label prefix.
    div()
        .relative()
        .h(px(32.))
        .min_w(px(136.))
        .flex()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.bg))
        .child(
            div()
                .flex_none()
                .px_2()
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(label),
        )
        .child(
            div()
                .h_full()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .border_l_1()
                .border_color(rgb(palette.border))
                .child(content),
        )
}
