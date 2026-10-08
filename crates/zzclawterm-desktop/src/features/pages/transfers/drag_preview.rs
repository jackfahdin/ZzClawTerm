use gpui::{Context, IntoElement, Pixels, Point, Render, Window, div, prelude::*, px, rgb};

use crate::theme::ThemePalette;

pub(super) struct TransferDragPreview {
    pub(super) label: String,
    pub(super) position: Point<Pixels>,
    pub(super) palette: ThemePalette,
}

impl Render for TransferDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x + px(12.))
            .pt(self.position.y + px(12.))
            .child(
                div()
                    .debug_selector(|| "transfer-file-drag-preview".into())
                    .w(px(240.))
                    .h(px(36.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(self.palette.primary))
                    .bg(rgb(self.palette.surface_elevated))
                    .shadow_lg()
                    .text_size(px(12.))
                    .text_color(rgb(self.palette.text))
                    .child(crate::features::view_widgets::mono_icon(
                        "icons/fe/download.svg",
                        rgb(self.palette.primary).into(),
                        14.,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(self.label.clone()),
                    ),
            )
    }
}

pub(super) fn with_transfer_drag<E: gpui::StatefulInteractiveElement>(
    element: E,
    drag: crate::features::transfers::drag_export::DraggedSelection,
    app: gpui::WeakEntity<crate::features::ZzClawTermApp>,
    name: String,
    palette: ThemePalette,
) -> E {
    let resolve_app = app.clone();
    element
        .on_drag(drag, move |drag, position, _, cx| {
            if let Some(app) = app.upgrade() {
                app.update(cx, |app, cx| app.capture_transfer_drag(drag, cx));
            }
            let count = drag.file_count();
            let label = if count > 1 {
                format!("{name} (+{})", count - 1)
            } else {
                name.clone()
            };
            cx.new(|_| TransferDragPreview {
                label,
                position,
                palette,
            })
        })
        .can_drag(|event, _, _| !event.modifiers.modified())
        .external_drag_payload(
            move |drag: &crate::features::transfers::drag_export::DraggedSelection, window, cx| {
                resolve_app.upgrade()?.update(cx, |app, cx| {
                    app.resolve_transfer_drag(
                        drag,
                        window.supports_virtual_file_drag(),
                        window.supports_file_promise_drag(),
                        cx,
                    )
                })
            },
        )
}
