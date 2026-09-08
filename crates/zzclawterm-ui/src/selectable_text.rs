use gpui::{IntoElement, RenderOnce, SharedString};
use gpui_component::text::TextView;

/// Selectable plain text backed by gpui-component's window selection support.
#[derive(IntoElement)]
pub struct ZzClawSelectableText {
    id: SharedString,
    text: SharedString,
}

impl ZzClawSelectableText {
    pub fn new(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

impl RenderOnce for ZzClawSelectableText {
    fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        let escaped = self
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        TextView::html(self.id, format!("<pre>{escaped}</pre>")).selectable(true)
    }
}
