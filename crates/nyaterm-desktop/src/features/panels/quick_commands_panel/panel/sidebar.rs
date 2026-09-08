use rust_i18n::t;

use gpui::{AppContext as _, Context, FontWeight, SharedString, div, prelude::*, px, rgb, rgba};
use nyaterm_ui::{NyaContextMenu, NyaMenuItem, NyaScrollable};

use super::super::super::QuickCommandCategoryOption;
use crate::features::{
    NyaTermApp, commands::QuickCommandDropPosition, commands::QuickCommandDropTarget,
};

use super::{QuickCommandDragKind, QuickCommandDragPayload, QuickCommandDragPreview};

impl NyaTermApp {
    pub(super) fn quick_command_category_sidebar(
        &mut self,
        categories: Vec<QuickCommandCategoryOption>,
        palette: crate::theme::ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut category_sidebar = div()
            .id(SharedString::from("quick-command-category-scroll"))
            .w(px(176.))
            .h_full()
            .flex_shrink_0()
            // Vertical only. Scrolling both axes lets the rows size to their
            // intrinsic width, which pushed every count pill past the 176px clip.
            .overflow_y_scrollbar()
            .p(px(6.))
            .border_r_1()
            .border_color(rgb(palette.border))
            .flex()
            .flex_col()
            .gap_1();
        for option in categories {
            let id = option.id.clone();
            let drag_option_id = option.id.clone();
            let drag_option_label = option.label.clone();
            let selected = self.commands.quick_selected_category() == option.id;
            let manageable = option.manageable;
            let depth = option.depth;
            // Real categories get the full menu; `all` / `uncategorized` get the two
            // add actions only, since there is nothing there to rename or delete.
            let menu_items = if manageable {
                self.quick_command_category_menu_items(option.id.clone(), cx)
            } else {
                self.quick_command_pseudo_category_menu_items(cx)
            };
            let row = div()
                .id(SharedString::from(format!(
                    "quick-command-category-{}",
                    option.id
                )))
                .relative()
                .h(px(32.))
                .w_full()
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .pl(px(8. + depth as f32 * 12.))
                .rounded_md()
                .bg(if selected {
                    rgb(palette.hover)
                } else {
                    rgba(0x00000000)
                })
                .text_xs()
                .text_color(if selected {
                    rgb(palette.link)
                } else {
                    rgb(palette.text)
                })
                .cursor_pointer()
                .hover(move |this| this.bg(rgb(palette.hover)))
                .child(
                    div()
                        .size(px(6.))
                        .flex_none()
                        .rounded_full()
                        .when(!selected, |this| this.opacity(0.6))
                        .bg(if selected {
                            rgb(palette.link)
                        } else {
                            rgb(palette.text_dimmed)
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .font_weight(FontWeight(500.))
                        .child(option.label),
                )
                .child(
                    div()
                        .flex_none()
                        .rounded_sm()
                        .px(px(6.))
                        .py(px(2.))
                        .bg(if selected {
                            rgba((palette.primary << 8) | 0x24)
                        } else {
                            rgb(palette.hover)
                        })
                        .text_size(px(10.))
                        .line_height(px(10.))
                        .text_color(if selected {
                            rgb(palette.primary)
                        } else {
                            rgb(palette.text_dimmed)
                        })
                        .child(option.count.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.commands.select_quick_category(id.clone());
                    cx.notify();
                }))
                .when(manageable, |this| {
                    let drag_id = drag_option_id.clone();
                    let drag_label = drag_option_label.clone();
                    let move_target = drag_option_id.clone();
                    let drop_target = drag_option_id.clone();
                    this.cursor_move()
                        .on_drag(
                            QuickCommandDragPayload {
                                kind: QuickCommandDragKind::Category,
                                id: drag_id,
                                label: drag_label,
                            },
                            |payload, position, _, cx| {
                                cx.new(|_| QuickCommandDragPreview {
                                    payload: payload.clone(),
                                    position,
                                })
                            },
                        )
                        .on_drag_move(cx.listener(
                            move |this,
                                  event: &gpui::DragMoveEvent<QuickCommandDragPayload>,
                                  _,
                                  cx| {
                                let _ = event.drag(cx);
                                let relative = if event.bounds.size.height > px(0.) {
                                    ((event.event.position.y - event.bounds.origin.y)
                                        / event.bounds.size.height)
                                        .clamp(0., 1.)
                                } else {
                                    0.5
                                };
                                let position = if relative < 0.25 {
                                    QuickCommandDropPosition::Before
                                } else if relative > 0.75 {
                                    QuickCommandDropPosition::After
                                } else {
                                    QuickCommandDropPosition::Inside
                                };
                                if this.commands.set_quick_drop_target(QuickCommandDropTarget {
                                    id: move_target.clone(),
                                    position,
                                }) {
                                    cx.notify();
                                }
                            },
                        ))
                        .on_drop(cx.listener(
                            move |this, payload: &QuickCommandDragPayload, _, cx| {
                                let position = this
                                    .commands
                                    .quick_drop_target()
                                    .filter(|target| target.id == drop_target)
                                    .map(|target| target.position)
                                    .unwrap_or(QuickCommandDropPosition::Inside);
                                let config = match payload.kind {
                                    QuickCommandDragKind::Command => {
                                        this.commands.move_quick_command_to_category(
                                            &payload.id,
                                            Some(drop_target.clone()),
                                        )
                                    }
                                    QuickCommandDragKind::Category => this
                                        .commands
                                        .move_quick_category(&payload.id, &drop_target, position),
                                };
                                this.finish_quick_command_reorder(config, cx);
                            },
                        ))
                })
                .when(option.id == "uncategorized", |this| {
                    this.on_drop(cx.listener(
                        move |this, payload: &QuickCommandDragPayload, _, cx| {
                            let config = (payload.kind == QuickCommandDragKind::Command)
                                .then(|| {
                                    this.commands
                                        .move_quick_command_to_category(&payload.id, None)
                                })
                                .flatten();
                            this.finish_quick_command_reorder(config, cx);
                        },
                    ))
                });
            category_sidebar =
                category_sidebar.child(NyaContextMenu::new(row, menu_items).into_any_element());
        }
        category_sidebar.into_any_element()
    }

    /// Group menu for a real category, mirroring Tauri's `QuickCommands.tsx`
    /// category `ContextMenuContent`: add, then reorder, then edit/delete.
    fn quick_command_category_menu_items(
        &self,
        category_id: String,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let add_category_parent = category_id.clone();
        let add_command_id = category_id.clone();
        let move_up_id = category_id.clone();
        let move_down_id = category_id.clone();
        let rename_id = category_id.clone();
        let delete_id = category_id.clone();
        let can_move_up = self
            .commands
            .quick_category_move_neighbor(&category_id, true)
            .is_some();
        let can_move_down = self
            .commands
            .quick_category_move_neighbor(&category_id, false)
            .is_some();
        vec![
            NyaMenuItem::action(t!("quickCommands.addCategory"))
                .icon("icons/fe/new-folder.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_new_quick_command_category(
                        Some(add_category_parent.clone()),
                        window,
                        cx,
                    );
                })),
            NyaMenuItem::action(t!("quickCommands.addCommand"))
                .icon("icons/conn/terminal.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_new_quick_command_editor_in_category(
                        Some(add_command_id.clone()),
                        window,
                        cx,
                    );
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("dialog.moveUp"))
                .icon("icons/chevron-up.svg")
                .disabled(!can_move_up)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.move_quick_command_category(move_up_id.clone(), true, cx);
                })),
            NyaMenuItem::action(t!("dialog.moveDown"))
                .icon("icons/chevron-down.svg")
                .disabled(!can_move_down)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.move_quick_command_category(move_down_id.clone(), false, cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("quickCommands.edit"))
                .icon("icons/net/edit.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_rename_quick_command_category(rename_id.clone(), window, cx);
                })),
            NyaMenuItem::action(t!("common.delete"))
                .icon("icons/net/delete.svg")
                .danger()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_delete_quick_command_category_confirm(delete_id.clone(), window, cx);
                })),
        ]
    }

    /// Group menu for the synthetic `all` / `uncategorized` rows. Neither can be
    /// renamed or deleted, but Tauri still offers both add actions from them: a root
    /// category, and a command with no category.
    fn quick_command_pseudo_category_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        vec![
            NyaMenuItem::action(t!("quickCommands.addCategory"))
                .icon("icons/fe/new-folder.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_new_quick_command_category(None, window, cx);
                })),
            NyaMenuItem::action(t!("quickCommands.addCommand"))
                .icon("icons/conn/terminal.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_new_quick_command_editor_in_category(None, window, cx);
                })),
        ]
    }

    #[cfg(test)]
    pub(crate) fn quick_command_category_menu_items_for_test(
        &self,
        category_id: String,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        self.quick_command_category_menu_items(category_id, cx)
    }

    #[cfg(test)]
    pub(crate) fn quick_command_pseudo_category_menu_items_for_test(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        self.quick_command_pseudo_category_menu_items(cx)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AppRuntime, QuickCommandCategory, RuntimeMode, uuid};
    use nyaterm_ui::NyaMenuItem;

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;

    fn menu_app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and a
        // nanosecond timestamp can repeat, which would share one config dir.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-quick-group-menu-{}-{}",
            std::process::id(),
            uuid()
        ));
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.clone(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    fn category(id: &str, order: i32) -> QuickCommandCategory {
        QuickCommandCategory {
            id: id.to_string(),
            name: id.to_string(),
            parent_id: None,
            sort_order: order,
        }
    }

    fn labels(items: &[NyaMenuItem]) -> Vec<&str> {
        items.iter().map(NyaMenuItem::test_label).collect()
    }

    /// Mirrors Tauri's category `ContextMenuContent`: two add actions, the reorder
    /// pair, then edit/delete, each group separated.
    #[test]
    fn group_menu_matches_tauri_structure_and_marks_delete_dangerous() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        cx.update_entity(&app, |app, _| {
            app.commands.replace_quick_command_catalog(
                Vec::new(),
                vec![
                    category("first", 0),
                    category("middle", 1),
                    category("last", 2),
                ],
            );
        });
        let items = cx.update_entity(&app, |app, cx| {
            app.quick_command_category_menu_items_for_test("middle".to_string(), cx)
        });

        assert_eq!(
            labels(&items),
            vec![
                "Add Category",
                "Add Command",
                "",
                "Move up",
                "Move down",
                "",
                "Edit",
                "Delete",
            ]
        );
        // (label, shortcut, icon, disabled, checked, danger)
        assert!(items[7].test_presentation().5, "delete should be dangerous");
        assert!(items.iter().all(|item| item.children().is_none()));
    }

    #[test]
    fn group_menu_disables_the_move_that_would_leave_the_sibling_run() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        cx.update_entity(&app, |app, _| {
            app.commands.replace_quick_command_catalog(
                Vec::new(),
                vec![
                    category("first", 0),
                    category("middle", 1),
                    category("last", 2),
                ],
            );
        });

        let disabled = |app: &gpui::Entity<NyaTermApp>, cx: &mut TestAppContext, id: &str| {
            let items = cx.update_entity(app, |app, cx| {
                app.quick_command_category_menu_items_for_test(id.to_string(), cx)
            });
            (
                items[3].test_presentation().3,
                items[4].test_presentation().3,
            )
        };

        assert_eq!(disabled(&app, &mut cx, "first"), (true, false));
        assert_eq!(disabled(&app, &mut cx, "middle"), (false, false));
        assert_eq!(disabled(&app, &mut cx, "last"), (false, true));
    }

    /// `all` / `uncategorized` cannot be renamed or deleted, so they offer the add
    /// actions only.
    #[test]
    fn pseudo_group_menu_offers_only_the_add_actions() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        let items = cx.update_entity(&app, |app, cx| {
            app.quick_command_pseudo_category_menu_items_for_test(cx)
        });

        assert_eq!(labels(&items), vec!["Add Category", "Add Command"]);
        assert!(items.iter().all(|item| !item.test_presentation().5));
    }
}
