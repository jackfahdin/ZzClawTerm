use gpui::{IntoElement, SharedString};

pub(in crate::features) fn disabled_inspector_panel(
    palette: crate::theme::ThemePalette,
    detail: impl Into<SharedString>,
) -> impl IntoElement {
    zzclawterm_ui::empty_panel(detail, palette)
}
