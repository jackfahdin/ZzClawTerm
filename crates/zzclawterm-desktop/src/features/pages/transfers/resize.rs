//! The divider between the browser and the queue.
//!
//! Lives with the panel rather than with the other resize handles because it is part
//! of the panel body, and the panel renders from a snapshot with no app access. The
//! drag itself is still the app's: the handlers hop back through `with_app`.

use gpui::{Context, IntoElement, MouseButton, MouseDownEvent, SharedString, deferred, prelude::*};

use crate::features::view_widgets::horizontal_resize_handle_visual;

use super::panel::{TransferChrome, TransferPanel};

pub(in crate::features::pages::transfers) fn transfer_height_resize_handle(
    chrome: TransferChrome,
    resizing: bool,
    highlighted: bool,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    let id = SharedString::from("transfer-height-resize");
    let hover_id = id.clone();
    let drag_id = id.clone();
    deferred(
        horizontal_resize_handle_visual(chrome.palette, resizing, highlighted)
            .id(id.clone())
            .cursor_row_resize()
            .on_hover(cx.listener(move |panel, hovered: &bool, _, cx| {
                let hovered = *hovered;
                let hover_id = hover_id.clone();
                panel.with_app(cx, |this, cx| {
                    this.update_resize_handle_hover(hover_id, hovered, cx);
                });
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |panel, event: &MouseDownEvent, _, cx| {
                    let event = event.clone();
                    let drag_id = drag_id.clone();
                    panel.with_app(cx, |this, cx| {
                        this.activate_resize_handle_immediately(drag_id, cx);
                        this.start_transfer_height_resize(&event, cx);
                    });
                }),
            ),
    )
}
