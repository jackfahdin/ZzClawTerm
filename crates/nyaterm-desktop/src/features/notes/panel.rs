use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Render, ScrollStrategy, SharedString, Styled as _,
    Subscription, UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, rgb, rgba,
    uniform_list,
};

use nyaterm_core::{NoteFolder, NoteNodeKind};
use nyaterm_ui::{
    NyaButton, NyaButtonVariant, NyaContextMenu, NyaDropdownMenu, NyaInputEvent, NyaInputShell,
    NyaInputState, NyaMenuItem, NyaScrollable as _, NyaSearchInput,
};
use rust_i18n::t;

use crate::features::{NyaTermApp, view_widgets::mono_icon};

use super::NoteTreeRow;

#[derive(Clone)]
struct NotesDragPayload {
    id: String,
    label: String,
}

struct NotesDragPreview {
    payload: NotesDragPayload,
    position: gpui::Point<gpui::Pixels>,
    palette: crate::theme::ThemePalette,
}

impl Render for NotesDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x - px(70.))
            .pt(self.position.y - px(16.))
            .child(
                div()
                    .w(px(140.))
                    .h(px(32.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(self.palette.border))
                    .bg(rgb(self.palette.surface_elevated))
                    .text_color(rgb(self.palette.text))
                    .child(self.payload.label.clone()),
            )
    }
}

pub(in crate::features) struct NotesPanel {
    app: WeakEntity<NyaTermApp>,
    search: Entity<NyaInputState>,
    rename: Entity<NyaInputState>,
    renaming_id: Option<String>,
    context_target: Rc<RefCell<Option<String>>>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    #[cfg(test)]
    rows_built: usize,
    _app_subscription: Subscription,
    _search_subscription: Subscription,
    _rename_subscription: Subscription,
}

impl NotesPanel {
    pub(in crate::features) fn new(app: WeakEntity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            NyaInputState::new(cx, String::new()).placeholder(t!("notes.searchPlaceholder"))
        });
        let rename = cx.new(|cx| NyaInputState::new(cx, String::new()).max_chars(Some(120)));
        let app_subscription = cx.observe(
            &app.upgrade().expect("Notes panel is created with its app"),
            |_, _, cx| cx.notify(),
        );
        let search_subscription = cx.subscribe(&search, |_, _, _: &NyaInputEvent, cx| cx.notify());
        let rename_subscription = cx.subscribe(&rename, |this, _, event: &NyaInputEvent, cx| {
            if let NyaInputEvent::Submitted(value) | NyaInputEvent::Blurred(value) = event {
                let value = value.clone();
                let Some(id) = this.renaming_id.take() else {
                    return;
                };
                this.with_app(cx, |app, cx| app.rename_note_node(id, value, cx));
                cx.notify();
            }
        });
        Self {
            app,
            search,
            rename,
            renaming_id: None,
            context_target: Rc::new(RefCell::new(None)),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            #[cfg(test)]
            rows_built: 0,
            _app_subscription: app_subscription,
            _search_subscription: search_subscription,
            _rename_subscription: rename_subscription,
        }
    }

    fn with_app(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut NyaTermApp, &mut Context<NyaTermApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, f);
        }
    }

    fn move_selection(&self, delta: isize, cx: &mut Context<Self>) {
        let search = self.search.read(cx).value(cx);
        let scroll = self.scroll.clone();
        self.with_app(cx, move |app, cx| {
            let rows = app.notes.visible_rows(&search);
            if rows.is_empty() {
                return;
            }
            let current = app
                .notes
                .selected_node_id()
                .and_then(|id| rows.iter().position(|row| row.id == id));
            let next = match current {
                Some(index) => (index as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
                None if delta < 0 => rows.len() - 1,
                None => 0,
            };
            app.select_note_node(Some(rows[next].id.clone()), cx);
            scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        });
    }

    fn activate_selected(&self, cx: &mut Context<Self>) {
        self.with_app(cx, |app, cx| {
            let Some(id) = app.notes.selected_node_id().map(str::to_string) else {
                return;
            };
            match app.notes.node(&id).map(|node| node.0) {
                Some(NoteNodeKind::Folder) => app.toggle_note_folder(&id, cx),
                Some(NoteNodeKind::Note) => app.open_note_editor(id, cx),
                None => {}
            }
        });
    }

    fn begin_rename(
        &mut self,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.renaming_id = Some(id);
        self.rename.update(cx, |field, cx| {
            field.set_content(&name, cx);
            field.select_all(window, cx);
        });
        cx.notify();
    }

    fn rename_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.app.upgrade().and_then(|app| {
            app.read_with(cx, |app, _| {
                let id = app.notes.selected_node_id()?.to_string();
                let (_, _, name) = app.notes.node(&id)?;
                Some((id, name))
            })
        });
        if let Some((id, name)) = selected {
            self.begin_rename(id, name, window, cx);
        }
    }
}

impl Focusable for NotesPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NotesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().size_full().into_any_element();
        };
        let search = self.search.read(cx).value(cx);
        let (palette, panel_bg, toolbar_bg, rows, loading, error, selected, catalog_empty) = app
            .read_with(cx, |app, _| {
                let rows = app.notes.visible_rows(&search);
                let palette = app.theme_palette();
                (
                    palette,
                    app.shell_transparent_color(palette.surface),
                    app.shell_transparent_color(palette.section_header),
                    rows,
                    app.notes.loading(),
                    app.notes.error().map(str::to_string),
                    app.notes.selected_node_id().map(str::to_string),
                    app.notes.folders().is_empty() && app.notes.notes().is_empty(),
                )
            });
        let row_count = rows.len();
        let app_for_rows = self.app.clone();
        let selected_for_rows = selected.clone();
        let context_target_for_rows = self.context_target.clone();
        let list = uniform_list(
            "notes-tree-rows",
            row_count,
            cx.processor(move |panel, range: std::ops::Range<usize>, _, cx| {
                let items = range
                    .filter_map(|index| rows.get(index).cloned())
                    .map(|row| {
                        note_row(
                            panel,
                            row,
                            selected_for_rows.as_deref(),
                            palette,
                            app_for_rows.clone(),
                            context_target_for_rows.clone(),
                            cx,
                        )
                    })
                    .collect::<Vec<_>>();
                #[cfg(test)]
                {
                    panel.rows_built = panel.rows_built.saturating_add(items.len());
                }
                items
            }),
        )
        .flex_1()
        .min_h_0()
        .track_scroll(&self.scroll);

        let search_state = self.search.clone();
        let mut search_input = NyaSearchInput::new("notes-search", &search_state);
        if !search.is_empty() {
            let clear = self.search.clone();
            search_input = search_input.trailing(
                div()
                    .id("notes-clear-search")
                    .px_1()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| clear.update(cx, |field, cx| field.clear(cx)))
                    .child("×"),
            );
        }
        let mut root = div()
            .id("notes-panel")
            .key_context("NotesPanel")
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(panel_bg)
            .on_key_down(
                cx.listener(|panel, event: &gpui::KeyDownEvent, window, cx| {
                    if panel.search.read(cx).has_focus() || panel.rename.read(cx).has_focus() {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "up" => panel.move_selection(-1, cx),
                        "down" => panel.move_selection(1, cx),
                        "enter" => panel.activate_selected(cx),
                        "f2" => panel.rename_selected(window, cx),
                        "delete" => panel.with_app(cx, |app, cx| {
                            app.request_delete_selected_note_node(window, cx)
                        }),
                        _ => (),
                    }
                }),
            )
            .child(
                div()
                    .px_2()
                    .py(px(6.))
                    .bg(toolbar_bg)
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().min_w_0().flex_1().h(px(28.)).child(search_input))
                    .child(toolbar_button(
                        "notes-new-folder",
                        "icons/fe/new-folder.svg",
                        t!("notes.newFolder"),
                        palette,
                        self.app.clone(),
                        |app, cx| app.create_note_folder_in_selected_folder(cx),
                    ))
                    .child(toolbar_button(
                        "notes-new-note",
                        "icons/conn/add.svg",
                        t!("notes.newNote"),
                        palette,
                        self.app.clone(),
                        |app, cx| app.create_note_in_selected_folder(cx),
                    ))
                    .child(notes_more_menu(self.app.clone())),
            );
        if loading && catalog_empty {
            root = root.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("common.loading").to_string()),
            );
        } else if let Some(error) = error {
            root = root.child(
                div()
                    .p_3()
                    .text_size(px(11.))
                    .text_color(rgb(palette.danger))
                    .child(error),
            );
        } else if catalog_empty {
            let new_note_app = self.app.clone();
            let new_folder_app = self.app.clone();
            root = root.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .px_4()
                    .text_center()
                    .text_color(rgb(palette.text_muted))
                    .child(mono_icon(
                        "icons/file/description.svg",
                        rgb(palette.text_dimmed).into(),
                        24.,
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(palette.text))
                            .child(t!("notes.emptyTitle").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .child(t!("notes.emptyDescription").to_string()),
                    )
                    .child(
                        div()
                            .pt_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                NyaButton::new(
                                    "notes-empty-new-note",
                                    t!("notes.newNote").to_string(),
                                )
                                .small()
                                .variant(NyaButtonVariant::Primary)
                                .on_click(move |_, _, cx| {
                                    if let Some(app) = new_note_app.upgrade() {
                                        app.update(cx, |app, cx| app.create_root_note(cx));
                                    }
                                }),
                            )
                            .child(
                                NyaButton::new(
                                    "notes-empty-new-folder",
                                    t!("notes.newFolder").to_string(),
                                )
                                .small()
                                .on_click(move |_, _, cx| {
                                    if let Some(app) = new_folder_app.upgrade() {
                                        app.update(cx, |app, cx| app.create_root_note_folder(cx));
                                    }
                                }),
                            ),
                    ),
            );
        } else {
            let context_target_for_reset = self.context_target.clone();
            let context_target_for_menu = self.context_target.clone();
            let menu_app = self.app.clone();
            let menu_panel = cx.weak_entity();
            let tree = div()
                .flex_1()
                .min_h_0()
                .relative()
                .flex()
                .flex_col()
                .overflow_hidden()
                .p(px(6.))
                .capture_any_mouse_down(move |event: &MouseDownEvent, _, _| {
                    if event.button == MouseButton::Right {
                        *context_target_for_reset.borrow_mut() = None;
                    }
                })
                .on_drop(cx.listener(|panel, payload: &NotesDragPayload, _, cx| {
                    cx.stop_propagation();
                    let id = payload.id.clone();
                    panel.with_app(cx, |app, cx| app.move_note_node_to(id, None, cx));
                }))
                .child(list)
                .vertical_scrollbar(&self.scroll);
            root = root.child(
                NyaContextMenu::new_dynamic(tree, move |_, cx| {
                    note_context_menu_items(
                        context_target_for_menu.borrow().clone(),
                        menu_app.clone(),
                        menu_panel.clone(),
                        cx,
                    )
                })
                .min_width(px(160.)),
            );
        }
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        root.into_any_element()
    }
}

fn notes_more_menu(app: WeakEntity<NyaTermApp>) -> NyaDropdownMenu {
    let expand_app = app.clone();
    let collapse_app = app.clone();
    NyaDropdownMenu::new("notes-more")
        .icon("icons/session/more.svg")
        .icon_size(px(16.))
        .tooltip(t!("common.more"))
        .items([
            NyaMenuItem::action(t!("notes.expandAll").to_string()).on_click(move |_, _, cx| {
                if let Some(app) = expand_app.upgrade() {
                    app.update(cx, |app, cx| app.set_all_notes_expanded(true, cx));
                }
            }),
            NyaMenuItem::action(t!("notes.collapseAll").to_string()).on_click(move |_, _, cx| {
                if let Some(app) = collapse_app.upgrade() {
                    app.update(cx, |app, cx| app.set_all_notes_expanded(false, cx));
                }
            }),
            NyaMenuItem::action(t!("common.refresh").to_string())
                .icon("icons/fe/refresh.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = app.upgrade() {
                        app.update(cx, |app, cx| app.refresh_notes(cx));
                    }
                }),
        ])
}

fn toolbar_button(
    id: &'static str,
    icon: &'static str,
    tooltip: impl Into<SharedString>,
    palette: crate::theme::ThemePalette,
    app: WeakEntity<NyaTermApp>,
    action: impl Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static,
) -> gpui::AnyElement {
    let tooltip = tooltip.into();
    div()
        .id(id)
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(move |this| this.bg(rgb(palette.hover)))
        .tooltip(move |window, cx| nyaterm_ui::NyaTooltip::new(tooltip.clone()).build(window, cx))
        .on_click(move |_, _, cx| {
            if let Some(app) = app.upgrade() {
                app.update(cx, |app, cx| action(app, cx));
            }
        })
        .child(mono_icon(icon, rgb(palette.text_muted).into(), 14.))
        .into_any_element()
}

fn note_row_drop_parent(row: &NoteTreeRow, source_id: &str) -> Option<Option<String>> {
    if source_id == row.id {
        return None;
    }
    Some(match row.kind {
        NoteNodeKind::Folder => Some(row.id.clone()),
        NoteNodeKind::Note => row.parent_id.clone(),
    })
}

fn note_row(
    panel: &mut NotesPanel,
    row: NoteTreeRow,
    selected_id: Option<&str>,
    palette: crate::theme::ThemePalette,
    app: WeakEntity<NyaTermApp>,
    context_target: Rc<RefCell<Option<String>>>,
    cx: &mut Context<NotesPanel>,
) -> gpui::AnyElement {
    let selected = selected_id == Some(row.id.as_str());
    let click_id = row.id.clone();
    let kind = row.kind;
    let app_for_click = app.clone();
    let drag_payload = NotesDragPayload {
        id: row.id.clone(),
        label: row.name.clone(),
    };
    if panel.renaming_id.as_deref() == Some(row.id.as_str()) {
        return div()
            .h(px(28.))
            .w_full()
            .flex_none()
            .pl(px(48. + row.depth as f32 * 14.))
            .pr_1()
            .flex()
            .items_center()
            .child(
                div()
                    .w_full()
                    .h(px(24.))
                    .child(NyaInputShell::new("notes-inline-rename", &panel.rename)),
            )
            .into_any_element();
    }
    let expander = if row.kind == NoteNodeKind::Folder {
        let toggle_app = app.clone();
        let toggle_id = row.id.clone();
        let mut chevron = mono_icon(
            "icons/menu/chevron-right.svg",
            rgb(palette.text_dimmed).into(),
            16.,
        );
        if row.expanded {
            chevron =
                chevron.with_transformation(gpui::Transformation::rotate(gpui::percentage(0.25)));
        }
        div()
            .id(SharedString::from(format!("notes-toggle-{}", row.id)))
            .size(px(20.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .hover(move |this| this.bg(rgb(palette.hover)))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                if let Some(app) = toggle_app.upgrade() {
                    app.update(cx, |app, cx| app.toggle_note_folder(&toggle_id, cx));
                }
            })
            .child(chevron)
            .into_any_element()
    } else {
        div().size(px(20.)).flex_none().into_any_element()
    };
    let icon_path = match (row.kind, row.expanded) {
        (NoteNodeKind::Folder, true) => "icons/session/folder-open.svg",
        (NoteNodeKind::Folder, false) => "icons/conn/folder.svg",
        (NoteNodeKind::Note, _) => "icons/notes.svg",
    };
    let icon_color = if row.kind == NoteNodeKind::Folder && row.expanded {
        palette.primary
    } else {
        palette.text_muted
    };
    let context_id = row.id.clone();
    let row_selector = format!("notes-row-{}", row.id);
    let drop_row_for_style = row.clone();
    let drop_row = row.clone();
    let drop_app = app.clone();
    let base = div()
        .id(SharedString::from(format!("notes-row-{}", row.id)))
        .debug_selector(move || row_selector.clone())
        .h(px(28.))
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .pr_1()
        .pl(px(4. + row.depth as f32 * 14.))
        .rounded_sm()
        .bg(if selected {
            rgba((palette.primary << 8) | 0x2e)
        } else {
            rgba(0x00000000)
        })
        .text_color(rgb(if selected {
            palette.text
        } else {
            palette.text_muted
        }))
        .hover(move |this| this.bg(rgb(palette.hover)).text_color(rgb(palette.text)))
        .cursor_pointer()
        .cursor_move()
        .capture_any_mouse_down(move |event: &MouseDownEvent, _, _| {
            if event.button == MouseButton::Right {
                *context_target.borrow_mut() = Some(context_id.clone());
            }
        })
        .on_drag(drag_payload, move |payload, position, _, cx| {
            cx.new(|_| NotesDragPreview {
                payload: payload.clone(),
                position,
                palette,
            })
        })
        .drag_over::<NotesDragPayload>(move |this, payload, _, cx| {
            let valid =
                note_row_drop_parent(&drop_row_for_style, &payload.id).is_some_and(|parent_id| {
                    drop_app.upgrade().is_some_and(|app| {
                        app.read(cx)
                            .notes
                            .can_move_to(&payload.id, parent_id.as_deref())
                    })
                });
            if valid {
                this.border_1().border_color(rgb(palette.primary))
            } else {
                this
            }
        })
        .on_drop(
            cx.listener(move |panel, payload: &NotesDragPayload, _, cx| {
                cx.stop_propagation();
                let id = payload.id.clone();
                let Some(parent_id) = note_row_drop_parent(&drop_row, &id) else {
                    return;
                };
                panel.with_app(cx, |app, cx| app.move_note_node_to(id, parent_id, cx));
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|panel, _, window, cx| {
                cx.stop_propagation();
                window.focus(&panel.focus, cx);
            }),
        )
        .on_click(move |event, _, cx| {
            if let Some(app) = app_for_click.upgrade() {
                app.update(cx, |app, cx| {
                    app.select_note_node(Some(click_id.clone()), cx);
                    if event.click_count() >= 2 {
                        match kind {
                            NoteNodeKind::Folder => app.toggle_note_folder(&click_id, cx),
                            NoteNodeKind::Note => app.open_note_editor(click_id.clone(), cx),
                        }
                    }
                });
            }
        })
        .child(expander)
        .child(
            div()
                .size(px(20.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(mono_icon(icon_path, rgb(icon_color).into(), 14.)),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(12.))
                .child(row.name),
        );
    base.into_any_element()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NoteFolderTarget {
    id: String,
    name: String,
    depth: usize,
}

fn note_folder_targets(folders: &[NoteFolder]) -> Vec<NoteFolderTarget> {
    let mut children = HashMap::<Option<&str>, Vec<&NoteFolder>>::new();
    for folder in folders {
        children
            .entry(folder.parent_id.as_deref())
            .or_default()
            .push(folder);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|left, right| {
            left.sort_order
                .cmp(&right.sort_order)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        });
    }

    fn append(
        parent_id: Option<&str>,
        depth: usize,
        children: &HashMap<Option<&str>, Vec<&NoteFolder>>,
        visited: &mut HashSet<String>,
        targets: &mut Vec<NoteFolderTarget>,
    ) {
        let Some(folders) = children.get(&parent_id) else {
            return;
        };
        for folder in folders {
            if !visited.insert(folder.id.clone()) {
                continue;
            }
            targets.push(NoteFolderTarget {
                id: folder.id.clone(),
                name: folder.name.clone(),
                depth,
            });
            append(
                Some(folder.id.as_str()),
                depth + 1,
                children,
                visited,
                targets,
            );
        }
    }

    let mut targets = Vec::with_capacity(folders.len());
    append(None, 0, &children, &mut HashSet::new(), &mut targets);
    targets
}

fn note_context_menu_items(
    target_id: Option<String>,
    app: WeakEntity<NyaTermApp>,
    panel: WeakEntity<NotesPanel>,
    cx: &mut gpui::App,
) -> Vec<NyaMenuItem> {
    let Some(app_entity) = app.upgrade() else {
        return Vec::new();
    };
    let Some(target_id) = target_id else {
        let note_app = app.clone();
        let folder_app = app.clone();
        return vec![
            NyaMenuItem::action(t!("notes.newNote").to_string())
                .icon("icons/conn/add.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = note_app.upgrade() {
                        app.update(cx, |app, cx| app.create_root_note(cx));
                    }
                }),
            NyaMenuItem::action(t!("notes.newFolder").to_string())
                .icon("icons/fe/new-folder.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = folder_app.upgrade() {
                        app.update(cx, |app, cx| app.create_root_note_folder(cx));
                    }
                }),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("common.refresh").to_string())
                .icon("icons/fe/refresh.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = app.upgrade() {
                        app.update(cx, |app, cx| app.refresh_notes(cx));
                    }
                }),
        ];
    };
    let Some((kind, _, name)) = app_entity.read(cx).notes.node(&target_id) else {
        return Vec::new();
    };
    let folder_targets = {
        let app = app_entity.read(cx);
        note_folder_targets(app.notes.folders())
            .into_iter()
            .filter(|folder| app.notes.can_move_to(&target_id, Some(&folder.id)))
            .collect::<Vec<_>>()
    };

    let mut items = Vec::new();
    if kind == NoteNodeKind::Note {
        let open_app = app.clone();
        let open_id = target_id.clone();
        items.push(
            NyaMenuItem::action(t!("notes.open").to_string())
                .icon("icons/menu/external.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = open_app.upgrade() {
                        let id = open_id.clone();
                        app.update(cx, |app, cx| app.open_note_editor(id, cx));
                    }
                }),
        );
    } else {
        let note_app = app.clone();
        let note_parent_id = target_id.clone();
        items.push(
            NyaMenuItem::action(t!("notes.newNote").to_string())
                .icon("icons/conn/add.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = note_app.upgrade() {
                        let parent_id = note_parent_id.clone();
                        app.update(cx, |app, cx| app.create_note_in_folder(Some(parent_id), cx));
                    }
                }),
        );
        let folder_app = app.clone();
        let folder_parent_id = target_id.clone();
        items.push(
            NyaMenuItem::action(t!("notes.newFolder").to_string())
                .icon("icons/fe/new-folder.svg")
                .on_click(move |_, _, cx| {
                    if let Some(app) = folder_app.upgrade() {
                        let parent_id = folder_parent_id.clone();
                        app.update(cx, |app, cx| {
                            app.create_note_folder_in_folder(Some(parent_id), cx)
                        });
                    }
                }),
        );
    }

    let rename_id = target_id.clone();
    items.push(
        NyaMenuItem::action(t!("notes.rename").to_string())
            .icon("icons/net/edit.svg")
            .on_click(move |_, window, cx| {
                let id = rename_id.clone();
                let name = name.clone();
                let _ = panel.update(cx, |panel, cx| panel.begin_rename(id, name, window, cx));
            }),
    );

    let mut move_items = Vec::with_capacity(folder_targets.len() + 1);
    let root_app = app.clone();
    let root_id = target_id.clone();
    move_items.push(
        NyaMenuItem::action(t!("notes.root").to_string()).on_click(move |_, _, cx| {
            if let Some(app) = root_app.upgrade() {
                let id = root_id.clone();
                app.update(cx, |app, cx| app.move_note_node_to(id, None, cx));
            }
        }),
    );
    for folder in folder_targets {
        let move_app = app.clone();
        let id = target_id.clone();
        let parent_id = folder.id;
        move_items.push(
            NyaMenuItem::action(folder.name)
                .label_indent(px(folder.depth as f32 * 10.))
                .on_click(move |_, _, cx| {
                    if let Some(app) = move_app.upgrade() {
                        let id = id.clone();
                        let parent_id = parent_id.clone();
                        app.update(cx, |app, cx| app.move_note_node_to(id, Some(parent_id), cx));
                    }
                }),
        );
    }
    items.push(
        NyaMenuItem::submenu(t!("notes.moveTo").to_string(), move_items)
            .icon("icons/net/move.svg")
            .submenu_min_width(px(176.))
            .submenu_max_height(px(288.))
            .submenu_scrollable(true),
    );

    let delete_app = app;
    let delete_id = target_id;
    items.push(
        NyaMenuItem::action(t!("common.delete").to_string())
            .icon("icons/net/delete.svg")
            .danger()
            .on_click(move |_, window, cx| {
                if let Some(app) = delete_app.upgrade() {
                    let id = delete_id.clone();
                    app.update(cx, |app, cx| app.request_delete_note_node(id, window, cx));
                }
            }),
    );
    items
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::Path, time::Duration};

    use gpui::{
        AppContext as _, Entity, IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
        ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, div, point, px,
    };
    use nyaterm_core::{
        AppRuntime, NoteFolder, NoteSummary, NoteTreePayload, NotesUiState, RuntimeMode,
    };

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::test_support::TestConfigDir;
    use nyaterm_ui::NyaDialogWindowExt as _;

    use super::{
        NoteFolderTarget, NoteTreeRow, note_context_menu_items, note_folder_targets,
        note_row_drop_parent,
    };

    struct NotesHost {
        app: Entity<NyaTermApp>,
    }

    impl Render for NotesHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let panel = self.app.read(cx).notes_panel.clone();
            div().w(px(360.)).h(px(600.)).child(panel)
        }
    }

    fn test_app(cx: &mut TestAppContext, root: &Path) -> Entity<NyaTermApp> {
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.to_path_buf(),
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

    fn draw(app: &Entity<NyaTermApp>, cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            app.update(cx, |_, cx| cx.notify());
            _ = window.draw(cx);
        });
        cx.run_until_parked();
    }

    fn right_click(cx: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Right,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Right,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.run_until_parked();
    }

    fn assert_dialog_closed(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            assert!(!window.has_active_nya_dialog(cx));
        });
    }

    fn open_note_delete_dialog(cx: &mut VisualTestContext) {
        let row = cx
            .debug_bounds("notes-row-note-1")
            .expect("note row should render");
        right_click(cx, row.center());
        cx.simulate_keystrokes("down down down down enter");
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            assert!(window.has_active_nya_dialog(cx));
        });
    }

    #[test]
    fn folder_targets_keep_tree_order_and_depth() {
        let folders = vec![
            NoteFolder {
                id: "root".into(),
                parent_id: None,
                name: "Root".into(),
                sort_order: 0,
                created_at_ms: 1,
                updated_at_ms: 1,
                extra: BTreeMap::new(),
            },
            NoteFolder {
                id: "child".into(),
                parent_id: Some("root".into()),
                name: "Child".into(),
                sort_order: 0,
                created_at_ms: 1,
                updated_at_ms: 1,
                extra: BTreeMap::new(),
            },
            NoteFolder {
                id: "sibling".into(),
                parent_id: None,
                name: "Sibling".into(),
                sort_order: 1,
                created_at_ms: 1,
                updated_at_ms: 1,
                extra: BTreeMap::new(),
            },
        ];

        assert_eq!(
            note_folder_targets(&folders),
            vec![
                NoteFolderTarget {
                    id: "root".into(),
                    name: "Root".into(),
                    depth: 0,
                },
                NoteFolderTarget {
                    id: "child".into(),
                    name: "Child".into(),
                    depth: 1,
                },
                NoteFolderTarget {
                    id: "sibling".into(),
                    name: "Sibling".into(),
                    depth: 0,
                },
            ]
        );
    }

    #[test]
    fn row_drop_targets_match_tauri_parent_rules() {
        let folder = NoteTreeRow {
            id: "folder".into(),
            kind: nyaterm_core::NoteNodeKind::Folder,
            parent_id: None,
            name: "Folder".into(),
            depth: 0,
            expanded: false,
            has_children: false,
        };
        let note = NoteTreeRow {
            id: "note".into(),
            kind: nyaterm_core::NoteNodeKind::Note,
            parent_id: Some("folder".into()),
            name: "Note".into(),
            depth: 1,
            expanded: false,
            has_children: false,
        };

        assert_eq!(
            note_row_drop_parent(&folder, "source"),
            Some(Some("folder".into()))
        );
        assert_eq!(
            note_row_drop_parent(&note, "source"),
            Some(Some("folder".into()))
        );
        assert_eq!(note_row_drop_parent(&folder, "folder"), None);
        assert_eq!(note_row_drop_parent(&note, "note"), None);
    }

    #[test]
    fn context_menu_items_match_blank_folder_and_note_targets() {
        let test_dir = TestConfigDir::new("nyaterm-notes-panel");
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx, test_dir.path());
        cx.update_entity(&app, |app, _| {
            let generation = app.notes.begin_load().expect("begin notes load");
            assert!(app.notes.apply_load(
                generation,
                NoteTreePayload {
                    folders: vec![
                        NoteFolder {
                            id: "root".into(),
                            parent_id: None,
                            name: "Root".into(),
                            sort_order: 0,
                            created_at_ms: 1,
                            updated_at_ms: 1,
                            extra: BTreeMap::new(),
                        },
                        NoteFolder {
                            id: "child".into(),
                            parent_id: Some("root".into()),
                            name: "Child".into(),
                            sort_order: 0,
                            created_at_ms: 1,
                            updated_at_ms: 1,
                            extra: BTreeMap::new(),
                        },
                        NoteFolder {
                            id: "sibling".into(),
                            parent_id: None,
                            name: "Sibling".into(),
                            sort_order: 1,
                            created_at_ms: 1,
                            updated_at_ms: 1,
                            extra: BTreeMap::new(),
                        },
                    ],
                    notes: vec![NoteSummary {
                        id: "note".into(),
                        parent_id: Some("root".into()),
                        title: "Note".into(),
                        sort_order: 1,
                        revision: 1,
                        created_at_ms: 1,
                        updated_at_ms: 1,
                        extra: BTreeMap::new(),
                    }],
                    ui: NotesUiState::default(),
                }
            ));
        });
        let panel = cx.read_entity(&app, |app, _| app.notes_panel.downgrade());
        let app_weak = app.downgrade();

        cx.update(|cx| {
            let blank = note_context_menu_items(None, app_weak.clone(), panel.clone(), cx);
            assert_eq!(blank.len(), 4);
            assert_eq!(
                blank
                    .iter()
                    .map(|item| item.test_presentation().1)
                    .collect::<Vec<_>>(),
                vec![
                    Some("icons/conn/add.svg".into()),
                    Some("icons/fe/new-folder.svg".into()),
                    None,
                    Some("icons/fe/refresh.svg".into()),
                ]
            );

            let folder =
                note_context_menu_items(Some("root".into()), app_weak.clone(), panel.clone(), cx);
            assert_eq!(folder.len(), 5);
            assert_eq!(
                folder
                    .iter()
                    .map(|item| item.test_presentation().1)
                    .collect::<Vec<_>>(),
                vec![
                    Some("icons/conn/add.svg".into()),
                    Some("icons/fe/new-folder.svg".into()),
                    Some("icons/net/edit.svg".into()),
                    Some("icons/net/move.svg".into()),
                    Some("icons/net/delete.svg".into()),
                ]
            );
            let folder_move_targets = folder[3].children().expect("move submenu");
            assert_eq!(folder_move_targets.len(), 2);
            assert_eq!(folder_move_targets[1].test_label(), "Sibling");

            let note =
                note_context_menu_items(Some("note".into()), app_weak.clone(), panel.clone(), cx);
            assert_eq!(note.len(), 4);
            assert_eq!(
                note[0].test_presentation().1.as_deref(),
                Some("icons/menu/external.svg")
            );
            let note_move_targets = note[2].children().expect("move submenu");
            assert_eq!(note_move_targets[2].test_label(), "Child");
            assert_eq!(note_move_targets[2].test_submenu_layout().3, Some(px(10.)));
        });
    }

    #[test]
    fn loaded_tauri_note_tree_builds_visible_rows() {
        let test_dir = TestConfigDir::new("nyaterm-notes-panel");
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx, test_dir.path());
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let generation = app.notes.begin_load().expect("begin notes load");
            assert!(app.notes.apply_load(
                generation,
                NoteTreePayload {
                    folders: vec![NoteFolder {
                        id: "folder-1".into(),
                        parent_id: None,
                        name: "NyaTerm".into(),
                        sort_order: 0,
                        created_at_ms: 1,
                        updated_at_ms: 1,
                        extra: BTreeMap::new(),
                    }],
                    notes: vec![NoteSummary {
                        id: "note-1".into(),
                        parent_id: Some("folder-1".into()),
                        title: "v1.1.19".into(),
                        sort_order: 0,
                        revision: 1,
                        created_at_ms: 1,
                        updated_at_ms: 1,
                        extra: BTreeMap::new(),
                    }],
                    ui: NotesUiState {
                        expanded_folder_ids: vec!["folder-1".into()],
                        last_selected_node_id: None,
                    },
                }
            ));
        });

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |_, _| NotesHost { app: host_app });
        let cx: &mut VisualTestContext = cx;
        draw(&app, cx);

        cx.update(|_, cx| {
            let rows_built = app.read(cx).notes_panel.read(cx).rows_built;
            assert!(
                rows_built >= 2,
                "the expanded Tauri folder and its child note must enter the paint pipeline"
            );
        });
    }

    #[test]
    fn note_delete_dialog_controls_remain_clickable_after_context_menu() {
        let test_dir = TestConfigDir::new("nyaterm-notes-panel");
        let mut cx = TestAppContext::single();
        let app = test_app(&mut cx, test_dir.path());
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let generation = app.notes.begin_load().expect("begin notes load");
            assert!(app.notes.apply_load(
                generation,
                NoteTreePayload {
                    folders: Vec::new(),
                    notes: vec![NoteSummary {
                        id: "note-1".into(),
                        parent_id: None,
                        title: "v1.1.19".into(),
                        sort_order: 0,
                        revision: 1,
                        created_at_ms: 1,
                        updated_at_ms: 1,
                        extra: BTreeMap::new(),
                    }],
                    ui: NotesUiState::default(),
                }
            ));
        });

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let host = cx.new(|_| NotesHost { app: host_app });
            nyaterm_ui::nya_root(host, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        draw(&app, cx);

        open_note_delete_dialog(cx);
        let cancel = cx
            .debug_bounds("nya-dialog-cancel-button")
            .expect("cancel button should render");
        cx.simulate_click(cancel.center(), Modifiers::default());
        assert_dialog_closed(cx);

        open_note_delete_dialog(cx);
        let content = cx
            .debug_bounds("notes-delete-dialog-content")
            .expect("dialog content should render");
        let title = cx
            .debug_bounds("notes-delete-dialog-title")
            .expect("dialog title should render");
        let card_left = content.origin.x - px(16.);
        let card_top = title.origin.y - px(17.);
        let close = point(card_left + px(420.) - px(18.), card_top + px(18.));
        cx.simulate_click(close, Modifiers::default());
        assert_dialog_closed(cx);

        open_note_delete_dialog(cx);
        let confirm = cx
            .debug_bounds("nya-dialog-action-button")
            .expect("confirm button should render");
        cx.simulate_click(confirm.center(), Modifiers::default());
        assert_dialog_closed(cx);
    }
}
