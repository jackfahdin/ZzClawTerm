use gpui::{
    App, ClickEvent, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px, rgb, svg,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};

use crate::theme::ThemePalette;

type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A fixed trailing tab action. The parent must declare the supplied hover group.
#[derive(IntoElement)]
pub struct ZzClawTabCloseButton {
    id: SharedString,
    tab_group: SharedString,
    palette: ThemePalette,
    tooltip: SharedString,
    active: bool,
    dirty: bool,
    on_click: Option<CloseHandler>,
}

impl ZzClawTabCloseButton {
    pub fn new(
        id: impl Into<SharedString>,
        tab_group: impl Into<SharedString>,
        palette: ThemePalette,
        tooltip: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            tab_group: tab_group.into(),
            palette,
            tooltip: tooltip.into(),
            active: false,
            dirty: false,
            on_click: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ZzClawTabCloseButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Share the kit button's keyed focus handle so keyboard focus reveals
        // the action even when the pointer is outside the tab.
        let focused = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .is_focused(window);
        let close_group = SharedString::from(format!("{}-hover", self.id));
        let debug_id = self.id.clone();
        let palette = self.palette;
        Button::new(self.id)
            .debug_selector(move || debug_id.to_string())
            .ghost()
            .size(px(20.))
            .p_0()
            .border_0()
            .rounded_sm()
            .flex_none()
            .group(close_group.clone())
            .accessibility_label(self.tooltip.clone())
            .tooltip(self.tooltip)
            .when(!self.active && !self.dirty && !focused, |this| {
                this.opacity(0.)
                    .group_hover(self.tab_group.clone(), |style| style.opacity(1.))
            })
            .child(
                // Both glyphs share one footprint; neither affects tab width.
                div()
                    .size(px(13.))
                    .relative()
                    .child(
                        div()
                            .size_full()
                            .when(self.dirty && !focused, |this| {
                                this.opacity(0.)
                                    .group_hover(self.tab_group.clone(), |style| style.opacity(1.))
                            })
                            .child(
                                svg()
                                    .size_full()
                                    .path("icons/window/close.svg")
                                    .text_color(rgb(palette.text_muted))
                                    .group_hover(close_group, |style| {
                                        style.text_color(rgb(palette.danger))
                                    }),
                            ),
                    )
                    .when(self.dirty && !focused, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(3.5))
                                .left(px(3.5))
                                .size(px(6.))
                                .rounded_full()
                                .bg(rgb(palette.link))
                                .group_hover(self.tab_group, |style| style.opacity(0.)),
                        )
                    }),
            )
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                if let Some(on_click) = &self.on_click {
                    on_click(event, window, cx);
                }
            })
    }
}
