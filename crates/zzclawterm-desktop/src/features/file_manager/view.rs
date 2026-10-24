mod interaction;
mod navigation;
mod queue;
mod table;

use gpui::{
    AnyElement, App, ClickEvent, Context, CursorStyle, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*,
    px, relative, rgb,
};
use rust_i18n::t;
use zzclawterm_ui::{ZzClawButton, ZzClawButtonVariant, ZzClawScrollable};

use super::pane::PaneId;
use super::workspace::FileManagerWorkspace;
use crate::features::ZzClawTermApp;
use crate::features::transfers::file_operations::FileTransferOperation;
use crate::theme::ThemePalette;

#[derive(Clone, Copy)]
pub(in crate::features) enum FileManagerNameKind {
    Folder,
    File,
    Rename,
}

impl FileManagerWorkspace {
    fn read_app<R: Default>(&self, cx: &App, f: impl FnOnce(&ZzClawTermApp) -> R) -> R {
        self.app
            .upgrade()
            .map(|app| f(app.read(cx)))
            .unwrap_or_default()
    }

    fn dispatch_app(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ZzClawTermApp, &mut Window, &mut Context<ZzClawTermApp>) + 'static,
    ) {
        let app = self.app.clone();
        // The workspace is leased during listeners. Enter the app only after
        // releasing that lease, with all file-operation arguments already frozen.
        window.defer(cx, move |window, cx| {
            let _ = app.update(cx, |app, cx| f(app, window, cx));
        });
    }

    fn finish_resize(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.dragging_split || self.column_resize.is_some() || self.queue_resize.is_some() {
            self.dragging_split = false;
            self.column_resize = None;
            self.queue_resize = None;
            self.save_layout(window, cx);
            cx.notify();
        }
    }

    fn save_layout(&self, window: &Window, cx: &mut Context<Self>) {
        self.dispatch_app(window, cx, |app, _, cx| {
            app.persist_file_manager_settings(cx)
        });
    }

    fn send_transfer(
        &self,
        operation: FileTransferOperation,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let intent = self.transfer_intent(operation);
        let id = self.id.clone();
        self.dispatch_app(window, cx, move |app, _, cx| {
            app.submit_file_transfer(id, intent, cx)
        });
    }

    fn open_name_dialog(
        &self,
        pane: PaneId,
        kind: FileManagerNameKind,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.selection(pane);
        let id = self.id.clone();
        self.dispatch_app(window, cx, move |app, window, cx| {
            app.open_file_manager_name_target(id, selection, kind, window, cx)
        });
    }

    fn delete_selection(&self, window: &Window, cx: &mut Context<Self>) {
        let selection = self.selection(self.active);
        let id = self.id.clone();
        self.dispatch_app(window, cx, move |app, window, cx| {
            app.confirm_file_manager_delete_selection(id, selection, window, cx)
        });
    }

    fn toolbar(&self, palette: ThemePalette, cx: &mut Context<Self>) -> AnyElement {
        let can_transfer = !self.active_pane().selected_paths.is_empty()
            && !self.active_pane().disconnected
            && !self.pane(self.active.other()).disconnected
            && !self.pane(self.active.other()).path.display_path.is_empty();
        let target = self.read_app(cx, |app| {
            app.endpoint_label(&self.pane(self.active.other()).endpoint)
        });
        let direction = if self.active == PaneId::Left {
            "→"
        } else {
            "←"
        };
        let toolbar = div()
            .h(px(36.))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgb(palette.border))
            .child(
                ZzClawButton::new(
                    "fm-copy",
                    format!("{} {direction} {target}", t!("fileManager.copy")),
                )
                .small()
                .disabled(!can_transfer)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.send_transfer(FileTransferOperation::Copy, window, cx)
                })),
            )
            .child(
                ZzClawButton::new("fm-move", t!("fileManager.move").to_string())
                    .small()
                    .disabled(!can_transfer)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.send_transfer(FileTransferOperation::Move, window, cx)
                    })),
            )
            .child(
                ZzClawButton::new("fm-swap", t!("fileManager.swapPanes").to_string())
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.swap_panes();
                        for pane in [PaneId::Left, PaneId::Right] {
                            let search = this.pane(pane).search.clone();
                            this.inputs(pane)
                                .search
                                .update(cx, |input, cx| input.set_content_silent(&search, cx));
                        }
                        let id = this.id.clone();
                        this.dispatch_app(window, cx, move |app, _, cx| {
                            app.refresh_file_manager_pane(id.clone(), PaneId::Left, cx);
                            app.refresh_file_manager_pane(id, PaneId::Right, cx);
                            app.persist_file_manager_settings(cx);
                        });
                        this.focus_pane(this.active, window, cx);
                    })),
            )
            .child(
                ZzClawButton::new("fm-folder", t!("fileExplorer.newFolder").to_string())
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_name_dialog(this.active, FileManagerNameKind::Folder, window, cx)
                    })),
            )
            .child(
                ZzClawButton::new("fm-delete", t!("fileExplorer.delete").to_string())
                    .small()
                    .disabled(self.active_pane().selected_paths.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| this.delete_selection(window, cx))),
            );
        if self.single_pane {
            let mut switches = div()
                .h(px(28.))
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .px_2();
            for pane in [PaneId::Left, PaneId::Right] {
                switches = switches.child(
                    ZzClawButton::new(
                        format!("fm-show-{}", pane.as_str()),
                        t!(if pane == PaneId::Left {
                            "fileManager.leftPane"
                        } else {
                            "fileManager.rightPane"
                        })
                        .to_string(),
                    )
                    .small()
                    .variant(if self.active == pane {
                        ZzClawButtonVariant::Primary
                    } else {
                        ZzClawButtonVariant::Ghost
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.focus_pane(pane, window, cx)),
                    ),
                );
            }
            return div()
                .flex_none()
                .min_w_0()
                .flex()
                .flex_col()
                .child(switches)
                .child(toolbar.overflow_x_scrollbar())
                .into_any_element();
        }
        toolbar.overflow_x_scrollbar().into_any_element()
    }

    fn measure(&self, pane: Option<PaneId>, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = cx.weak_entity();
        gpui::canvas(
            move |bounds, _, cx| {
                let Some(workspace) = workspace.upgrade() else {
                    return;
                };
                let previous = match pane {
                    None => workspace.read(cx).bounds,
                    Some(pane) => {
                        workspace.read(cx).pane_bounds[if pane == PaneId::Left { 0 } else { 1 }]
                    }
                };
                if previous == Some(bounds) {
                    return;
                }
                cx.defer(move |cx| {
                    workspace.update(cx, |this, cx| {
                        match pane {
                            None => {
                                this.bounds = Some(bounds);
                                this.single_pane = f32::from(bounds.size.width) < 568.;
                            }
                            Some(pane) => {
                                this.pane_bounds[if pane == PaneId::Left { 0 } else { 1 }] =
                                    Some(bounds)
                            }
                        }
                        cx.notify();
                    })
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    fn pane_width(&self, pane: PaneId) -> f32 {
        self.pane_bounds[if pane == PaneId::Left { 0 } else { 1 }]
            .map(|bounds| f32::from(bounds.size.width))
            .unwrap_or(400.)
    }

    fn panes(&mut self, palette: ThemePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex_1().min_h_0().min_w_0().flex();
        if self.single_pane {
            body = body.child(self.pane_view(palette, self.active, cx));
        } else {
            for pane in [PaneId::Left, PaneId::Right] {
                if pane == PaneId::Right {
                    body = body.child(self.splitter(palette, cx));
                }
                let fraction = if pane == PaneId::Left {
                    self.split_percent
                } else {
                    100 - self.split_percent
                };
                body = body.child(
                    div()
                        .min_w_0()
                        .min_h_0()
                        .flex_grow(1.)
                        .flex_shrink(1.)
                        .flex_basis(relative(f32::from(fraction) / 100.))
                        .child(self.pane_view(palette, pane, cx)),
                );
            }
        }
        body.into_any_element()
    }

    fn splitter(&self, palette: ThemePalette, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("file-manager-splitter")
            .w(px(6.))
            .h_full()
            .flex_none()
            .cursor(CursorStyle::ResizeLeftRight)
            .bg(rgb(palette.border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if event.click_count >= 2 {
                        this.split_percent = 50;
                        this.dragging_split = false;
                        this.save_layout(window, cx);
                    } else {
                        this.dragging_split = true;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }
}

impl Render for FileManagerWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_pending {
            self.focus_pending = false;
            self.focus_pane(self.active, window, cx);
        }
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        let palette = app.read(cx).theme_palette();
        div()
            .id(SharedString::from(format!("file-manager-{}", self.id)))
            .size_full()
            .min_w_0()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx)
            }))
            .on_modifiers_changed(|_, window, _| window.refresh())
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.dragging_split
                    && let Some(bounds) = this.bounds
                {
                    this.split_percent = ((f32::from(event.position.x - bounds.origin.x)
                        / f32::from(bounds.size.width).max(1.))
                        * 100.)
                        .clamp(20., 80.)
                        .round() as u8;
                    cx.notify();
                }
                if let Some((pane, column, x, width)) = this.column_resize {
                    this.pane_mut(pane).column_widths.set(
                        column,
                        px((f32::from(width) + f32::from(event.position.x) - x).clamp(72., 2000.)),
                    );
                    cx.notify();
                }
                if let Some((y, height)) = this.queue_resize {
                    this.queue_height = (height + y - f32::from(event.position.y)).clamp(80., 600.);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.finish_resize(window, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.finish_resize(window, cx)),
            )
            .child(self.measure(None, cx))
            .child(self.toolbar(palette, cx))
            .child(self.panes(palette, cx))
            .child(self.queue_view(palette, cx))
            .into_any_element()
    }
}

fn small_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> ZzClawButton {
    ZzClawButton::new(id, label)
        .small()
        .variant(ZzClawButtonVariant::Ghost)
        .on_click(on_click)
}

#[cfg(test)]
mod tests;
