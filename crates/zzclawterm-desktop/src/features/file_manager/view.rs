use gpui::{
    AnyElement, App, ClickEvent, Context, CursorStyle, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*, px, relative, rgb, svg,
    uniform_list,
};
use rust_i18n::t;
use zzclawterm_transport::{RemoteFilePath, SftpFileEntry};
use zzclawterm_ui::{
    ZzClawButton, ZzClawButtonVariant, ZzClawContextMenu, ZzClawMenuItem, ZzClawScrollable,
    ZzClawTooltip, ZzClawUniformListScrollbar,
};

use super::pane::{PaneEndpoint, PaneId};
use super::workspace::{FileManagerQueueFilter, FileManagerWorkspace};
use crate::features::ZzClawTermApp;
use crate::features::formatting::format_permissions_octal;
use crate::features::pages::transfers::{format_browser_file_size, format_sftp_modified};
use crate::features::view_widgets::transfer_entry_icon;
use crate::models::TransferJobStatus;
use crate::theme::ThemePalette;

const MIN_PANE_PX: f32 = 280.;
const ROW_HEIGHT: f32 = 30.;

#[derive(Clone)]
pub(in crate::features) struct FileManagerDragPayload {
    pub manager_id: String,
    pub pane_id: PaneId,
    pub endpoint: PaneEndpoint,
    pub paths: Vec<RemoteFilePath>,
}

impl FileManagerWorkspace {
    fn read_app<R: Default>(&self, cx: &App, f: impl FnOnce(&ZzClawTermApp) -> R) -> R {
        let Some(app) = self.app.upgrade() else {
            return R::default();
        };
        f(app.read(cx))
    }

    fn dispatch_app(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ZzClawTermApp, &mut Window, &mut Context<ZzClawTermApp>) + 'static,
    ) {
        let app = self.app.clone();
        // Listeners lease this workspace. App commands may read or update it again,
        // so release the listener's lease before entering the application.
        window.defer(cx, move |window, cx| {
            let _ = app.update(cx, |app, cx| f(app, window, cx));
        });
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let unmodified = !event.keystroke.modifiers.modified();
        match key {
            "f5" if unmodified => {
                cx.stop_propagation();
                let id = self.id.clone();
                self.dispatch_app(window, cx, move |app, _, cx| {
                    app.start_file_manager_copy(id, false, cx)
                });
            }
            "f6" if unmodified => {
                cx.stop_propagation();
                let id = self.id.clone();
                self.dispatch_app(window, cx, move |app, window, cx| {
                    app.start_file_manager_move(id, window, cx)
                });
            }
            "f7" if unmodified => {
                cx.stop_propagation();
                let id = self.id.clone();
                let pane = self.active;
                self.dispatch_app(window, cx, move |app, window, cx| {
                    app.open_file_manager_name_dialog(
                        id,
                        pane,
                        FileManagerNameKind::Folder,
                        window,
                        cx,
                    )
                });
            }
            "delete" | "backspace" if unmodified => {
                if key == "backspace" {
                    let id = self.id.clone();
                    let pane = self.active;
                    self.dispatch_app(window, cx, move |app, _, cx| {
                        app.navigate_file_manager_parent(id, pane, cx)
                    });
                } else {
                    let id = self.id.clone();
                    self.dispatch_app(window, cx, move |app, window, cx| {
                        app.confirm_file_manager_delete(id, window, cx)
                    });
                }
                cx.stop_propagation();
            }
            "tab" if unmodified => {
                self.active = self.active.other();
                cx.notify();
            }
            _ => {}
        }
    }
}

impl Render for FileManagerWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        let palette = app.read(cx).theme_palette();
        let show_hidden = app
            .read(cx)
            .settings
            .summary()
            .ui_file_explorer_show_hidden_files;
        let available = window.viewport_size().width;
        self.single_pane = f32::from(available) < MIN_PANE_PX * 2. + 8.;
        let manager_id = self.id.clone();
        let left_entries = self.left.visible_entries(show_hidden);
        let right_entries = self.right.visible_entries(show_hidden);
        let jobs = app.read(cx).transfer.transfer_jobs().to_vec();
        let owned_jobs = app.read(cx).file_manager.job_ids().clone();

        let mut surface = div()
            .id(SharedString::from(format!("file-manager-{}", self.id)))
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx);
            }))
            .child(self.toolbar(palette, cx))
            .child(self.panes(palette, &left_entries, &right_entries, window, cx));
        surface = surface.child(self.queue(palette, &jobs, &owned_jobs, cx));
        let _ = manager_id;
        surface.into_any_element()
    }
}

impl FileManagerWorkspace {
    fn toolbar(&self, palette: ThemePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id.clone();
        let copy_id = id.clone();
        let move_id = id.clone();
        let swap_id = id.clone();
        let folder_id = id.clone();
        let delete_id = id.clone();
        let active = self.active;
        div()
            .h(px(36.))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgb(palette.border))
            .child(toolbar_button(
                palette,
                "fm-copy",
                "icons/copy.svg",
                t!("fileManager.copyToOther"),
                cx.listener(move |this, _, window, cx| {
                    let id = copy_id.clone();
                    this.dispatch_app(window, cx, move |app, _, cx| {
                        app.start_file_manager_copy(id, false, cx)
                    });
                }),
            ))
            .child(toolbar_button(
                palette,
                "fm-move",
                "icons/fe/forward.svg",
                t!("fileManager.moveToOther"),
                cx.listener(move |this, _, window, cx| {
                    let id = move_id.clone();
                    this.dispatch_app(window, cx, move |app, window, cx| {
                        app.start_file_manager_move(id, window, cx)
                    });
                }),
            ))
            .child(toolbar_button(
                palette,
                "fm-swap",
                "icons/swap-horiz.svg",
                t!("fileManager.swapPanes"),
                cx.listener(move |this, _, window, cx| {
                    this.swap_panes();
                    let id = swap_id.clone();
                    this.dispatch_app(window, cx, move |app, _, cx| {
                        app.persist_file_manager_settings(cx);
                        let _ = id;
                    });
                    cx.notify();
                }),
            ))
            .child(toolbar_button(
                palette,
                "fm-new-folder",
                "icons/fe/new-folder.svg",
                t!("fileExplorer.newFolder"),
                cx.listener(move |this, _, window, cx| {
                    let id = folder_id.clone();
                    this.dispatch_app(window, cx, move |app, window, cx| {
                        app.open_file_manager_name_dialog(
                            id,
                            active,
                            FileManagerNameKind::Folder,
                            window,
                            cx,
                        )
                    });
                }),
            ))
            .child(toolbar_button(
                palette,
                "fm-delete",
                "icons/fe/delete.svg",
                t!("fileExplorer.delete"),
                cx.listener(move |this, _, window, cx| {
                    let id = delete_id.clone();
                    this.dispatch_app(window, cx, move |app, window, cx| {
                        app.confirm_file_manager_delete(id, window, cx)
                    });
                }),
            ))
    }

    fn panes(
        &self,
        palette: ThemePalette,
        left_entries: &[SftpFileEntry],
        right_entries: &[SftpFileEntry],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut body = div().flex_1().min_h_0().min_w_0().flex();
        if self.single_pane {
            let pane = self.active;
            let entries = if pane == PaneId::Left {
                left_entries
            } else {
                right_entries
            };
            body = body
                .child(self.pane_view(palette, pane, entries, true, window, cx))
                .child(
                    ZzClawButton::new("fm-toggle-pane", t!("fileManager.otherPane").to_string())
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.active = this.active.other();
                            cx.notify();
                        })),
                );
        } else {
            let left_flex = f32::from(self.split_percent.clamp(20, 80));
            let right_flex = 100. - left_flex;
            body = body
                .child(
                    div()
                        .flex()
                        .min_w(px(MIN_PANE_PX))
                        .flex_grow(1.)
                        .flex_shrink(1.)
                        .flex_basis(relative(left_flex / 100.))
                        .child(self.pane_view(
                            palette,
                            PaneId::Left,
                            left_entries,
                            true,
                            window,
                            cx,
                        )),
                )
                .child(self.splitter(palette, cx))
                .child(
                    div()
                        .flex()
                        .min_w(px(MIN_PANE_PX))
                        .flex_grow(1.)
                        .flex_shrink(1.)
                        .flex_basis(relative(right_flex / 100.))
                        .child(self.pane_view(
                            palette,
                            PaneId::Right,
                            right_entries,
                            true,
                            window,
                            cx,
                        )),
                );
        }
        body
    }

    fn splitter(&self, palette: ThemePalette, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("file-manager-splitter")
            .w(px(6.))
            .h_full()
            .flex_none()
            .cursor(CursorStyle::ResizeLeftRight)
            .bg(rgb(if self.dragging_split {
                palette.primary
            } else {
                palette.border
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    if event.click_count >= 2 {
                        this.split_percent = 50;
                        this.dragging_split = false;
                    } else {
                        this.dragging_split = true;
                    }
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.dragging_split = false;
                    this.dispatch_app(window, cx, move |app, _, cx| {
                        app.persist_file_manager_settings(cx)
                    });
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if !this.dragging_split {
                        return;
                    }
                    let width = f32::from(window.viewport_size().width).max(1.);
                    let ratio = (f32::from(event.position.x) / width * 100.).clamp(20., 80.);
                    this.split_percent = ratio.round() as u8;
                    cx.notify();
                }),
            )
    }

    fn pane_view(
        &self,
        palette: ThemePalette,
        pane_id: PaneId,
        entries: &[SftpFileEntry],
        _full: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane = self.pane(pane_id);
        let manager_id = self.id.clone();
        let active = self.active == pane_id;
        let endpoint = pane.endpoint.clone();
        let path = pane.path.display_path.clone();
        let disconnected = pane.disconnected;
        let loading = pane.loading;
        let error = pane.error.clone();
        let status = pane.status.clone();
        let selected = pane.selected_path.clone();
        let selected_paths = pane.selected_paths.clone();
        let list_scroll = pane.list_scroll.clone();
        let focus = pane.focus.clone();
        let column_widths = pane.column_widths;
        let available = f32::from(window.viewport_size().width);
        let pane_width = if self.single_pane {
            available
        } else if pane_id == PaneId::Left {
            available * f32::from(self.split_percent) / 100.
        } else {
            available * f32::from(100 - self.split_percent) / 100.
        };
        let hide_permissions = pane_width < 520.;
        let hide_owner_group = pane_width < 640.;
        let source_label = self.read_app(cx, |app| app.endpoint_label(&endpoint));
        let choices = self.read_app(cx, |app| app.file_manager_source_choices());
        let mut source_items = Vec::new();
        for (choice, label) in choices {
            let id = manager_id.clone();
            let selected_endpoint = choice.clone();
            source_items.push(ZzClawMenuItem::action(label).on_click(cx.listener(
                move |this, _, window, cx| {
                    this.pane_mut(pane_id)
                        .set_endpoint(selected_endpoint.clone());
                    let id = id.clone();
                    this.dispatch_app(window, cx, move |app, _, cx| {
                        app.refresh_file_manager_pane(id, pane_id, cx);
                        app.persist_file_manager_settings(cx);
                    });
                    cx.notify();
                },
            )));
        }
        let header = ZzClawContextMenu::new(
            div()
                .id(SharedString::from(format!(
                    "fm-source-{}-{}",
                    self.id,
                    pane_id.as_str()
                )))
                .px_2()
                .h(px(28.))
                .flex()
                .items_center()
                .text_xs()
                .cursor_pointer()
                .child(source_label),
            source_items,
        );
        let up_id = manager_id.clone();
        let home_id = manager_id.clone();
        let refresh_id = manager_id.clone();
        let reconnect_id = manager_id.clone();
        let mut chrome = div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(rgb(if active {
                palette.primary
            } else {
                palette.border
            }))
            .track_focus(&focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_active(pane_id);
                    window.focus(&this.pane(pane_id).focus, cx);
                    cx.notify();
                }),
            )
            .child(header)
            .child(
                div()
                    .px_2()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(if path.is_empty() {
                                "/".to_string()
                            } else {
                                path.clone()
                            }),
                    )
                    .child(toolbar_button(
                        palette,
                        format!("fm-up-{}", pane_id.as_str()),
                        "icons/fe/up.svg",
                        t!("fileExplorer.goUp"),
                        cx.listener(move |this, _, window, cx| {
                            let id = up_id.clone();
                            this.dispatch_app(window, cx, move |app, _, cx| {
                                app.navigate_file_manager_parent(id, pane_id, cx)
                            });
                        }),
                    ))
                    .child(toolbar_button(
                        palette,
                        format!("fm-home-{}", pane_id.as_str()),
                        "icons/fe/locate.svg",
                        t!("fileManager.goHome"),
                        cx.listener(move |this, _, window, cx| {
                            let id = home_id.clone();
                            this.dispatch_app(window, cx, move |app, _, cx| {
                                app.navigate_file_manager_home(id, pane_id, cx)
                            });
                        }),
                    ))
                    .child(toolbar_button(
                        palette,
                        format!("fm-refresh-{}", pane_id.as_str()),
                        "icons/fe/refresh.svg",
                        t!("fileExplorer.refresh"),
                        cx.listener(move |this, _, window, cx| {
                            let id = refresh_id.clone();
                            this.dispatch_app(window, cx, move |app, _, cx| {
                                app.refresh_file_manager_pane(id, pane_id, cx)
                            });
                        }),
                    )),
            );
        if disconnected {
            let session_id = endpoint.session_id().unwrap_or_default().to_string();
            chrome = chrome.child(
                div()
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(palette.warning))
                    .child(t!("fileManager.disconnected").to_string())
                    .child(
                        ZzClawButton::new(
                            format!("fm-reconnect-{}", pane_id.as_str()),
                            t!("fileManager.reconnect").to_string(),
                        )
                        .small()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let session_id = session_id.clone();
                                let id = reconnect_id.clone();
                                this.dispatch_app(window, cx, move |app, window, cx| {
                                    app.reconnect_session(session_id, window, cx);
                                    app.refresh_file_manager_pane(id, pane_id, cx);
                                });
                            },
                        )),
                    ),
            );
        }
        if let Some(error) = error {
            chrome = chrome.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(palette.warning))
                    .child(error),
            );
        }
        let entries = entries.to_vec();
        let row_entries = entries.clone();
        let drop_manager = manager_id.clone();
        let list_id = SharedString::from(format!("fm-list-{}-{}", self.id, pane_id.as_str()));
        let count = entries.len() + 1;
        let mut list = uniform_list(
            list_id.clone(),
            count,
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                let mut items = Vec::with_capacity(range.len());
                let palette = this
                    .app
                    .upgrade()
                    .map(|app| app.read(cx).theme_palette())
                    .unwrap_or(palette);
                for index in range {
                    if index == 0 {
                        items.push(parent_row(palette, pane_id, cx).into_any_element());
                        continue;
                    }
                    let Some(entry) = row_entries.get(index - 1).cloned() else {
                        continue;
                    };
                    items.push(
                        entry_row(
                            FileManagerEntryRow {
                                palette,
                                manager_id: this.id.clone(),
                                pane_id,
                                endpoint: endpoint.clone(),
                                entry,
                                selected: selected.clone(),
                                selected_paths: &selected_paths,
                                widths: column_widths,
                                hide_permissions,
                                hide_owner_group,
                            },
                            cx,
                        )
                        .into_any_element(),
                    );
                }
                items
            }),
        );
        list.style().restrict_scroll_to_axis = Some(true);
        chrome = chrome.child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .child(
                    list.h_full()
                        .min_h_0()
                        .track_scroll(&list_scroll)
                        .can_drop(|drag, _, _| drag.is::<FileManagerDragPayload>())
                        .on_drop(cx.listener(
                            move |this, payload: &FileManagerDragPayload, window, cx| {
                                if payload.manager_id != this.id
                                    || payload.pane_id == pane_id
                                    || payload.paths.is_empty()
                                    || this.pane(payload.pane_id).endpoint != payload.endpoint
                                {
                                    return;
                                }
                                this.set_active(payload.pane_id);
                                let id = drop_manager.clone();
                                this.dispatch_app(window, cx, move |app, _, cx| {
                                    app.start_file_manager_copy(id, false, cx)
                                });
                            },
                        )),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .child(ZzClawUniformListScrollbar::new(
                            format!("fm-scroll-{}-{}", self.id, pane_id.as_str()),
                            &list_scroll,
                        )),
                ),
        );
        chrome = chrome.child(
            div()
                .h(px(22.))
                .px_2()
                .flex()
                .items_center()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(if loading {
                    t!("fileManager.loading").to_string()
                } else {
                    status
                }),
        );
        let _ = window;
        chrome.into_any_element()
    }

    fn queue(
        &self,
        palette: ThemePalette,
        jobs: &[crate::models::TransferJobState],
        owned: &std::collections::HashSet<String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let filtered: Vec<_> = jobs
            .iter()
            .filter(|job| self.show_all_jobs || owned.contains(&job.id))
            .filter(|job| match self.queue_filter {
                FileManagerQueueFilter::All => true,
                FileManagerQueueFilter::Active => matches!(
                    job.status,
                    TransferJobStatus::Running
                        | TransferJobStatus::Paused
                        | TransferJobStatus::Cancelling
                ),
                FileManagerQueueFilter::Completed => job.status == TransferJobStatus::Completed,
                FileManagerQueueFilter::Failed => {
                    job.status == TransferJobStatus::Failed
                        || job.status == TransferJobStatus::Cancelled
                }
            })
            .cloned()
            .collect();
        let mut header = div()
            .h(px(28.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .border_t_1()
            .border_color(rgb(palette.border))
            .child(
                ZzClawButton::new(
                    "fm-queue-toggle",
                    if self.queue_expanded {
                        t!("fileManager.hideQueue").to_string()
                    } else {
                        t!("fileManager.showQueue").to_string()
                    },
                )
                .small()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.queue_expanded = !this.queue_expanded;
                    cx.notify();
                })),
            )
            .child(queue_filter_button(
                palette,
                "fm-q-mine",
                t!("fileManager.thisManager"),
                !self.show_all_jobs,
                cx.listener(|this, _, _, cx| {
                    this.show_all_jobs = false;
                    cx.notify();
                }),
            ))
            .child(queue_filter_button(
                palette,
                "fm-q-all",
                t!("fileManager.allJobs"),
                self.show_all_jobs,
                cx.listener(|this, _, _, cx| {
                    this.show_all_jobs = true;
                    cx.notify();
                }),
            ));
        for (filter, label) in [
            (FileManagerQueueFilter::All, t!("fileManager.filterAll")),
            (
                FileManagerQueueFilter::Active,
                t!("fileManager.filterActive"),
            ),
            (
                FileManagerQueueFilter::Completed,
                t!("fileManager.filterCompleted"),
            ),
            (
                FileManagerQueueFilter::Failed,
                t!("fileManager.filterFailed"),
            ),
        ] {
            header = header.child(queue_filter_button(
                palette,
                format!("fm-qf-{filter:?}"),
                label,
                self.queue_filter == filter,
                cx.listener(move |this, _, _, cx| {
                    this.queue_filter = filter;
                    cx.notify();
                }),
            ));
        }
        let mut body = div().flex_none().flex().flex_col().child(header);
        if self.queue_expanded {
            let mut list = div()
                .h(px(self.queue_height))
                .overflow_y_scrollbar()
                .px_2()
                .py_1();
            if filtered.is_empty() {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette.text_muted))
                        .child(t!("fileTransfer.noTransfers").to_string()),
                );
            } else {
                for job in filtered {
                    list = list.child(
                        div()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .text_xs()
                            .gap_2()
                            .child(job.detail.clone()),
                    );
                }
            }
            body = body.child(list);
        }
        body
    }
}

fn toolbar_button(
    palette: ThemePalette,
    id: impl Into<String>,
    icon: &'static str,
    tooltip: impl Into<String>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    div()
        .id(SharedString::from(id.into()))
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)))
        .tooltip(move |window, cx| ZzClawTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(16.))
                .path(icon)
                .text_color(rgb(palette.text_muted)),
        )
        .on_click(on_click)
}

fn queue_filter_button(
    _palette: ThemePalette,
    id: impl Into<String>,
    label: impl Into<String>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    ZzClawButton::new(id.into(), label.into())
        .small()
        .variant(if active {
            ZzClawButtonVariant::Primary
        } else {
            ZzClawButtonVariant::Ghost
        })
        .on_click(on_click)
}

fn parent_row(
    palette: ThemePalette,
    pane_id: PaneId,
    cx: &mut Context<FileManagerWorkspace>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!(
            "fm-parent-{}",
            pane_id.as_str()
        )))
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .px_2()
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)))
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            if event.click_count() >= 2 {
                let id = this.id.clone();
                this.dispatch_app(window, cx, move |app, _, cx| {
                    app.navigate_file_manager_parent(id, pane_id, cx)
                });
            }
        }))
        .child(transfer_entry_icon(palette, "..", true, false, false))
        .child(" ..")
}

struct FileManagerEntryRow<'a> {
    palette: ThemePalette,
    manager_id: String,
    pane_id: PaneId,
    endpoint: PaneEndpoint,
    entry: SftpFileEntry,
    selected: Option<String>,
    selected_paths: &'a std::collections::HashSet<String>,
    widths: crate::models::TransferBrowserColumnWidths,
    hide_permissions: bool,
    hide_owner_group: bool,
}

fn entry_row(
    row: FileManagerEntryRow<'_>,
    cx: &mut Context<FileManagerWorkspace>,
) -> impl IntoElement {
    let FileManagerEntryRow {
        palette,
        manager_id,
        pane_id,
        endpoint,
        entry,
        selected,
        selected_paths,
        widths,
        hide_permissions,
        hide_owner_group,
    } = row;
    let identity = entry.identity_key();
    let marked =
        selected.as_deref() == Some(identity.as_str()) || selected_paths.contains(&identity);
    let is_dir = entry.is_directory();
    let name = entry.name.clone();
    let drag_entry = entry.clone();
    let click_entry = entry.clone();
    let menu_entry = entry.clone();
    let modified = format_sftp_modified(entry.modified_at);
    let size = if is_dir {
        "-".to_string()
    } else {
        format_browser_file_size(entry.size)
    };
    let payload = FileManagerDragPayload {
        manager_id: manager_id.clone(),
        pane_id,
        endpoint,
        paths: vec![entry.remote_path()],
    };
    let mut row = div()
        .id(SharedString::from(format!("fm-row-{identity}")))
        .h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(rgb(if marked { palette.hover } else { palette.bg }))
        .hover(|this| this.bg(rgb(palette.hover)))
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            this.set_active(pane_id);
            if event.modifiers().control || event.modifiers().platform {
                this.pane_mut(pane_id).toggle_mark(identity.clone());
            } else {
                this.pane_mut(pane_id).select_entry(identity.clone());
            }
            if event.click_count() >= 2 && click_entry.is_directory() {
                let id = this.id.clone();
                let path = click_entry.remote_path();
                this.dispatch_app(window, cx, move |app, _, cx| {
                    app.navigate_file_manager_pane(id, pane_id, path, cx)
                });
            }
            let _ = window;
            cx.notify();
        }))
        .on_drag(payload, move |payload, _, _, cx| {
            cx.new(|_| FileManagerDragPreview {
                label: drag_entry.name.clone(),
                _payload: payload.clone(),
            })
        })
        .child(
            div()
                .w(widths.name)
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .text_size(px(12.))
                .child(transfer_entry_icon(palette, &name, is_dir, false, false))
                .child(name.clone()),
        )
        .child(
            div()
                .w(widths.modified)
                .flex_none()
                .px_2()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(modified),
        )
        .child(
            div()
                .w(widths.size)
                .flex_none()
                .px_2()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(size),
        );
    if !hide_permissions {
        row = row.child(
            div()
                .w(widths.permissions)
                .flex_none()
                .px_2()
                .text_xs()
                .child(
                    entry
                        .permissions
                        .map(format_permissions_octal)
                        .unwrap_or_else(|| "-".to_string()),
                ),
        );
    }
    if !hide_owner_group {
        row = row
            .child(div().w(widths.owner).flex_none().px_2().text_xs().child(
                if entry.owner.is_empty() {
                    "-".to_string()
                } else {
                    entry.owner.clone()
                },
            ))
            .child(div().w(widths.group).flex_none().px_2().text_xs().child(
                if entry.group.is_empty() {
                    "-".to_string()
                } else {
                    entry.group.clone()
                },
            ));
    }
    let items = pane_entry_menu(manager_id, pane_id, menu_entry, cx);
    ZzClawContextMenu::new(row, items)
}

fn pane_entry_menu(
    manager_id: String,
    pane_id: PaneId,
    entry: SftpFileEntry,
    cx: &mut Context<FileManagerWorkspace>,
) -> Vec<ZzClawMenuItem> {
    let copy_id = manager_id.clone();
    let move_id = manager_id.clone();
    let delete_id = manager_id.clone();
    let open_id = manager_id.clone();
    let rename_id = manager_id.clone();
    let file_id = manager_id;
    vec![
        ZzClawMenuItem::action(t!("fileManager.copyToOther")).on_click({
            let identity = entry.identity_key();
            cx.listener(move |this, _, window, cx| {
                this.pane_mut(pane_id).select_entry(identity.clone());
                let id = copy_id.clone();
                this.dispatch_app(window, cx, move |app, _, cx| {
                    app.start_file_manager_copy(id, false, cx)
                });
            })
        }),
        ZzClawMenuItem::action(t!("fileManager.moveToOther")).on_click(cx.listener(
            move |this, _, window, cx| {
                let id = move_id.clone();
                this.dispatch_app(window, cx, move |app, window, cx| {
                    app.start_file_manager_move(id, window, cx)
                });
            },
        )),
        ZzClawMenuItem::action(t!("fileExplorer.cmRename")).on_click({
            let identity = entry.identity_key();
            cx.listener(move |this, _, window, cx| {
                this.pane_mut(pane_id).select_entry(identity.clone());
                let id = rename_id.clone();
                this.dispatch_app(window, cx, move |app, window, cx| {
                    app.open_file_manager_name_dialog(
                        id,
                        pane_id,
                        FileManagerNameKind::Rename,
                        window,
                        cx,
                    )
                });
            })
        }),
        ZzClawMenuItem::action(t!("fileExplorer.newFile")).on_click(cx.listener(
            move |this, _, window, cx| {
                let id = file_id.clone();
                this.dispatch_app(window, cx, move |app, window, cx| {
                    app.open_file_manager_name_dialog(
                        id,
                        pane_id,
                        FileManagerNameKind::File,
                        window,
                        cx,
                    )
                });
            },
        )),
        ZzClawMenuItem::action(t!("fileExplorer.delete")).on_click(cx.listener(
            move |this, _, window, cx| {
                let id = delete_id.clone();
                this.dispatch_app(window, cx, move |app, window, cx| {
                    app.confirm_file_manager_delete(id, window, cx)
                });
            },
        )),
        ZzClawMenuItem::action(t!("fileExplorer.cmOpen")).on_click({
            let entry = entry.clone();
            cx.listener(move |this, _, window, cx| {
                if entry.is_directory() {
                    let id = open_id.clone();
                    let path = entry.remote_path();
                    this.dispatch_app(window, cx, move |app, _, cx| {
                        app.navigate_file_manager_pane(id, pane_id, path, cx)
                    });
                }
            })
        }),
    ]
}

struct FileManagerDragPreview {
    label: String,
    _payload: FileManagerDragPayload,
}

impl Render for FileManagerDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px_2().py_1().text_xs().child(self.label.clone())
    }
}

#[derive(Clone, Copy)]
pub(in crate::features) enum FileManagerNameKind {
    Folder,
    File,
    Rename,
}

#[cfg(test)]
mod tests;
