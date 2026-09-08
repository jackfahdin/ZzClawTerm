use gpui::{Context, IntoElement, div, prelude::*, px, rgb};
use std::time::Instant;

use crate::features::NyaTermApp;
use crate::features::perf::record_gpui_perf_sample;
use crate::models::{NavItem, PanelSide};

impl NyaTermApp {
    pub(in crate::features) fn right_panel(
        &mut self,
        draw_shared_edge: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let started_at = Instant::now();
        let mut width = self.shell.right_panel_width().clamp(200., 720.);
        if !cfg!(target_os = "macos") && self.shell.viewport_size().0 < 768. {
            width = width.min((self.shell.viewport_size().0 - 80.).max(120.));
        }
        let palette = self.theme_palette();
        let output = div()
            .w(px(width))
            .flex_none()
            .flex()
            .flex_col()
            .when(draw_shared_edge, |this| this.border_l_1())
            .border_color(rgb(palette.border))
            .bg(self.shell_surface_color(palette.surface))
            .child(self.side_panel_stack(PanelSide::Right, window, cx));
        record_gpui_perf_sample(
            "right_panel",
            started_at.elapsed(),
            self.gpui_perf_context(0, None),
        );
        output
    }

    pub(in crate::features) fn right_panel_body(
        &mut self,
        panel: NavItem,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.panel_body(panel, window, cx)
    }
}
