use gpui::{IntoElement, SharedString, div, prelude::*, px};
use zzclawterm_ui::ZzClawTooltip;

pub(super) fn send_command_control_group(
    _palette: crate::theme::ThemePalette,
    label: impl Into<SharedString>,
    content: impl IntoElement,
) -> impl IntoElement {
    let label = label.into();
    div()
        .id(label.clone())
        .h(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .tooltip(move |window, cx| ZzClawTooltip::new(label.clone()).build(window, cx))
        .child(content)
}
