use gpui::{IntoElement, RenderOnce};

use crate::text::{ZzClawCodeHighlights, ZzClawTextState, ZzClawTextStyle};

/// Selectable code whose syntax has already been parsed by a background job.
#[derive(IntoElement)]
pub struct ZzClawSelectableCode {
    state: ZzClawTextState,
    style: ZzClawTextStyle,
    highlights: ZzClawCodeHighlights,
}

impl ZzClawSelectableCode {
    pub fn from_state(
        state: &ZzClawTextState,
        style: ZzClawTextStyle,
        highlights: ZzClawCodeHighlights,
    ) -> Self {
        Self {
            state: state.clone(),
            style,
            highlights,
        }
    }
}

impl RenderOnce for ZzClawSelectableCode {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let style = self.style.raw_code(window);
        self.state.set_code_highlights(self.highlights);
        let view = style.apply(self.state.view());
        self.state.present(view, style, window, cx)
    }
}
