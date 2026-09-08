use gpui::{
    App, ClickEvent, Context, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::*, px, rgb, svg,
};

use crate::features::pages::transfers::panel::TransferPanel;
use crate::theme::ThemePalette;
use nyaterm_ui::NyaTooltip;

pub(super) fn compact_transfer_footer_button(
    palette: ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: impl Into<String>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    // Tauri footer icons: h-6 w-6 (24px)
    let mut button = div()
        .id(SharedString::from(id.into()))
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(rgb(palette.text_muted))
        .hover(|this| {
            this.bg(rgb(palette.surface_elevated))
                .text_color(rgb(palette.text))
        })
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(icon_path)
                .text_color(rgb(palette.text_muted)),
        );
    if enabled {
        button = button.cursor_pointer().on_click(on_click);
    } else {
        button = button.opacity(0.4);
    }
    button
}

pub(super) fn compact_transfer_footer_button_active(
    palette: ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: impl Into<String>,
    active: bool,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    let color = if active {
        rgb(palette.link)
    } else {
        rgb(palette.text_muted)
    };
    let mut button = div()
        .id(SharedString::from(id.into()))
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(if active {
            rgb(palette.hover)
        } else {
            rgb(palette.surface)
        })
        .text_color(color)
        .hover(|this| {
            this.bg(rgb(palette.surface_elevated))
                .text_color(if active {
                    rgb(0x79b8ff)
                } else {
                    rgb(palette.text)
                })
        })
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(icon_path)
                .text_color(color),
        );
    if enabled {
        button = button.cursor_pointer().on_click(on_click);
    } else {
        button = button.opacity(0.4);
    }
    button
}

pub(super) fn compact_transfer_upload_menu_button(
    palette: ThemePalette,
    tooltip: impl Into<String>,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    // Tauri: single Upload icon opens DropdownMenu (Upload Files / Upload Folder).
    div()
        .id(SharedString::from("transfer-browser-upload"))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(rgb(palette.text_muted))
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(palette.surface_elevated))
                .text_color(rgb(palette.text))
        })
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path("icons/fe/upload.svg")
                .text_color(rgb(palette.text_muted)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|panel, event: &MouseDownEvent, _, cx| {
                panel.with_app(cx, |this, cx| {
                    this.open_transfer_browser_upload_menu(event, cx);
                })
            }),
        )
}

pub(super) fn compact_transfer_toolbar_button(
    palette: ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: impl Into<String>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    // Tauri FileExplorerToolbar: h-7 ghost icon buttons, muted until hover.
    div()
        .id(SharedString::from(id.into()))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(rgb(palette.text_muted))
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(palette.surface_elevated))
                .text_color(rgb(palette.text))
        })
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon_path)
                .text_color(rgb(palette.text_muted)),
        )
        .on_click(on_click)
}

pub(super) fn compact_transfer_toolbar_button_enabled(
    palette: ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: impl Into<String>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    div()
        .id(SharedString::from(id.into()))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(if enabled {
            rgb(palette.text_muted)
        } else {
            rgb(palette.text_dimmed)
        })
        .opacity(if enabled { 1.0 } else { 0.45 })
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon_path)
                .text_color(if enabled {
                    rgb(palette.text_muted)
                } else {
                    rgb(palette.text_dimmed)
                }),
        )
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(|this| {
                    this.bg(rgb(palette.surface_elevated))
                        .text_color(rgb(palette.text))
                })
                .on_click(on_click)
        })
}

pub(super) fn compact_transfer_toolbar_button_active(
    palette: ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: impl Into<String>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    let color = if active {
        rgb(palette.link)
    } else {
        rgb(palette.text_muted)
    };
    div()
        .id(SharedString::from(id.into()))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(if active {
            rgb(palette.hover)
        } else {
            rgb(palette.surface)
        })
        .text_color(color)
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(palette.surface_elevated))
                .text_color(if active {
                    rgb(0x79b8ff)
                } else {
                    rgb(palette.text)
                })
        })
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon_path)
                .text_color(color),
        )
        .on_click(on_click)
}

pub(super) fn transfer_toolbar_divider(palette: ThemePalette) -> impl IntoElement {
    div()
        .h(px(16.))
        .w(px(1.))
        .mx_1()
        .rounded_sm()
        .bg(rgb(palette.border))
}
