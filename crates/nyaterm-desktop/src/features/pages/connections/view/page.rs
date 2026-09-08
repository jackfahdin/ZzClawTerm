use rust_i18n::t;

use gpui::{
    AnyElement, Context, IntoElement, KeyDownEvent, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, SharedString, UniformListScrollHandle, div,
    prelude::{InteractiveElement, ParentElement, StatefulInteractiveElement, Styled},
    px, rgb, svg, uniform_list,
};
use nyaterm_ui::{
    NyaContextMenu, NyaDropdownMenu, NyaScrollable, NyaScrollbarAxis, NyaSearchInput,
};

use crate::features::connections::{ConnectionDragKind, ConnectionDragPayload};
use crate::models::ConnectionSortMode;

use super::super::list::{
    ConnectionListRow, connection_tree_indent_px, icon_action_button, icon_action_button_styled,
};
use super::super::panel::ConnectionPanel;
use super::CONNECTION_LIST_ROW_HEIGHT_PX;
use super::actions::ConnectionRowActionsDecoration;
use super::rows::{connection_inline_group_editor_row, connection_section, saved_connection_row};

/// The panel body.
///
/// Takes no `NyaTermApp`. GPUI records every entity read during a draw, so an app
/// read here -- even a diagnostic one -- would put the panel back on the app's
/// invalidation path and undo the isolation this shape exists for. Everything it
/// draws comes from the snapshot; everything it changes goes back through
/// `ConnectionPanel::with_app`.
pub(in crate::features::pages::connections) fn connections_panel(
    panel: &mut ConnectionPanel,
    window: &mut gpui::Window,
    cx: &mut Context<ConnectionPanel>,
) -> AnyElement {
    // Before the first flush there is nothing to draw and nothing to draw it from.
    // The catalog arrives on a store reply, which flushes on its way out.
    let Some(snapshot) = panel.snapshot() else {
        return div().into_any_element();
    };
    let chrome = snapshot.chrome;
    let palette = chrome.palette;

    let empty_connections_label = t!("savedConnections.empty");
    let empty_connections_hint = t!("savedConnections.emptyHint");
    let no_results_label = t!("savedConnections.noResults");
    let empty_group_label = t!("savedConnections.emptyGroup");

    let flat_rows = snapshot.rows.clone();
    let store_is_empty = snapshot.store_is_empty;
    let nothing_matched = flat_rows.is_empty();
    let widest_row = snapshot.widest_row;

    let list_scroll = window
        .use_keyed_state(
            SharedString::from("connections-list-scroll-handle"),
            cx,
            |_, _| UniformListScrollHandle::new(),
        )
        .read(cx)
        .clone();

    let mut list = div()
        .id(SharedString::from("connections-list-scroll"))
        .debug_selector(|| "connections-list-scroll".to_string())
        .relative()
        .flex_1()
        .min_h_0()
        .p(px(6.))
        .flex()
        .flex_col()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|panel, _, _, cx| {
                // Click empty background clears multi-select (Tauri list onMouseDown).
                panel.with_app(cx, |app, cx| {
                    if app.connection_state.list_has_selection() {
                        app.clear_selected_connections(cx);
                    }
                });
            }),
        )
        .on_drop(
            cx.listener(|panel, payload: &ConnectionDragPayload, _, cx| {
                let payload = payload.clone();
                panel.with_app(cx, |app, cx| {
                    app.connection_state.clear_list_drop_target();
                    match payload.kind {
                        ConnectionDragKind::Connection => {
                            app.move_connection_into_group(payload.id.clone(), None, cx);
                        }
                        ConnectionDragKind::Group => {
                            app.move_group_into_group(payload.id.clone(), None, cx);
                        }
                    }
                });
            }),
        );
    if store_is_empty {
        list = list.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .px_4()
                .py_8()
                .gap_2()
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(palette.text_muted))
                        .child(empty_connections_label),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(empty_connections_hint),
                ),
        );
    } else if nothing_matched {
        list = list.child(
            div()
                .px_4()
                .py_8()
                .text_size(px(11.))
                .text_color(rgb(palette.text_dimmed))
                .child(no_results_label),
        );
    } else {
        let row_count = flat_rows.len();
        let row_actions =
            ConnectionRowActionsDecoration::new(flat_rows.clone(), cx.weak_entity(), palette);
        // `uniform_list` derives its scrollable width from one measured row, so
        // point it at the row most likely to be the widest or long names would
        // still be unreachable.
        list = list.child(
            uniform_list(
                "connections-list-rows",
                row_count,
                // Still only the visible range, and now built from the snapshot
                // rather than by re-entering the app for every row on screen.
                cx.processor(move |panel, range: std::ops::Range<usize>, _, cx| {
                    let mut items = Vec::with_capacity(range.len());
                    let Some(snapshot) = panel.snapshot() else {
                        return items;
                    };
                    for index in range {
                        let Some(row) = snapshot.rows.get(index).cloned() else {
                            continue;
                        };
                        // A definite width here, so the rows inside can resolve
                        // their `min_w(relative(1.))` and still overflow it when
                        // the name is long. It tracks the horizontal scroll.
                        let item = div()
                            .h(px(CONNECTION_LIST_ROW_HEIGHT_PX))
                            .w_full()
                            .flex_none()
                            .flex()
                            .items_center();
                        items.push(match row {
                            ConnectionListRow::Separator => {
                                item.child(div().mx_2().h(px(1.)).w_full().bg(rgb(palette.border)))
                            }
                            ConnectionListRow::GroupHeader(section) => item.child(
                                div()
                                    .w_full()
                                    .child(connection_section(snapshot, section, cx)),
                            ),
                            ConnectionListRow::InlineGroupEditor { parent_id, depth } => item
                                .child(div().w_full().child(connection_inline_group_editor_row(
                                    snapshot, parent_id, depth, cx,
                                ))),
                            ConnectionListRow::EmptyGroup { depth } => item.child(
                                div()
                                    .w_full()
                                    .px_2()
                                    .pl(px(connection_tree_indent_px(depth)))
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child(empty_group_label.clone()),
                            ),
                            ConnectionListRow::Connection {
                                connection_id,
                                depth,
                            } => {
                                let Some(connection) = snapshot.connection(&connection_id).cloned()
                                else {
                                    continue;
                                };
                                item.child(
                                    div().w_full().child(saved_connection_row(
                                        snapshot, connection, depth, cx,
                                    )),
                                )
                            }
                        });
                    }
                    #[cfg(test)]
                    panel.note_rows_built(items.len());
                    items
                }),
            )
            .debug_selector(|| "connections-list-rows".to_string())
            .with_decoration(row_actions)
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .with_width_from_item(widest_row)
            .flex_1()
            .min_h_0()
            .track_scroll(&list_scroll),
        );
    }

    // Both axes ride this one handle: rows are `Unconstrained` and their names
    // are never clipped, so a long or deeply nested name overflows sideways.
    // One `Scrollbar` for both is the right shape here - a shared reveal state
    // matches a single scroll surface, and the vendor skips painting an axis
    // whose content fits.
    let list = list.scrollbar(&list_scroll, NyaScrollbarAxis::Both);
    // One context menu for the whole list, aimed by whatever the press landed
    // on. Rows and group headers re-aim it from their own capture handlers,
    // which run inside this one, so the reset here is what a press on empty
    // space keeps. Nesting a menu per row instead opens both menus on a single
    // right-click, and the one that never receives the click goes on
    // re-focusing itself every layout pass - stranding any dialog opened
    // afterwards, because a dialog is dismissed through actions routed along
    // the focused element's path.
    //
    // The items are still built on the app, and still only when the menu opens.
    let menu_app = panel.app_handle();
    let list = NyaContextMenu::new_dynamic(
        list.capture_any_mouse_down(cx.listener(|panel, event: &MouseDownEvent, _, cx| {
            if event.button == MouseButton::Right {
                panel.with_app(cx, |app, _| {
                    app.connection_state.prepare_list_context_menu();
                });
            }
        })),
        move |_, cx| {
            menu_app
                .update(cx, |app, cx| {
                    app.connection_list_target_context_menu_items(cx)
                })
                .unwrap_or_default()
        },
    );

    // Tauri: PanelHeader (shared stack) + search/action strip + flat tree list.
    // Count is shown in the shared panel header via meta; strip hosts search + icons.
    div()
        .relative()
        .key_context(crate::shortcuts::SAVED_CONNECTIONS_KEY_CONTEXT)
        .track_focus(panel.focus_handle())
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .bg(chrome.transparent_surface)
        .child(connections_search_bar(panel, cx))
        .child(list)
        .into_any_element()
}

fn connections_search_bar(
    panel: &ConnectionPanel,
    cx: &mut Context<ConnectionPanel>,
) -> impl IntoElement {
    let snapshot = panel
        .snapshot()
        .expect("the caller returns early without a snapshot");
    let chrome = snapshot.chrome;
    let palette = chrome.palette;
    let search_empty = snapshot.search_is_empty;
    let search_field = snapshot.search_field.clone();
    // Tauri swaps the glyph, flips it for Z-A and tints it while a name sort is
    // active, so the current mode is readable without hovering for the tooltip.
    let sort_mode = snapshot.sort_mode;
    let sort_label = match sort_mode {
        ConnectionSortMode::Default => "icons/conn/sort.svg",
        ConnectionSortMode::NameAsc | ConnectionSortMode::NameDesc => "icons/conn/sort-alpha.svg",
    };
    let sort_tint = (sort_mode != ConnectionSortMode::Default).then_some(palette.primary);
    let sort_flipped = sort_mode == ConnectionSortMode::NameDesc;
    let sort_tooltip = t!(match sort_mode {
        ConnectionSortMode::Default => "savedConnections.sortDefault",
        ConnectionSortMode::NameAsc => "savedConnections.sortNameAsc",
        ConnectionSortMode::NameDesc => "savedConnections.sortNameDesc",
    });
    // Built when the menu opens, not on every frame that draws this strip. The
    // items and their twenty-one handlers stay on the app, reached weakly.
    let more_menu_app = panel.app_handle();
    let more_menu = NyaDropdownMenu::new("connections-more")
        .icon("icons/conn/more.svg")
        .icon_size(px(14.))
        .tooltip(t!("common.more"))
        .min_width(px(180.))
        .items_dynamic(move |_, cx| {
            more_menu_app
                .update(cx, |app, cx| app.connection_more_menu_items(cx))
                .unwrap_or_default()
        })
        .on_trigger(|_, _, cx| cx.stop_propagation());
    let mut search_input = NyaSearchInput::new("connection-search-input", &search_field)
        .on_key_down(cx.listener(|panel, event: &KeyDownEvent, window, cx| {
            let event = event.clone();
            panel.with_app(cx, |app, cx| {
                app.handle_connection_search_key_down(&event, window, cx);
            });
        }));
    if !search_empty {
        search_input = search_input.trailing(
            div()
                .id(SharedString::from("connection-search-clear"))
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .cursor_pointer()
                .hover(move |this| {
                    this.bg(rgb(palette.surface_elevated))
                        .text_color(rgb(palette.text))
                })
                .on_click(cx.listener(|panel, _, window, cx| {
                    panel.with_app(cx, |app, cx| {
                        app.clear_connection_search(window, cx);
                    });
                }))
                .child(
                    svg()
                        .size(px(13.))
                        .path("icons/window/close.svg")
                        .text_color(rgb(palette.text_muted)),
                ),
        );
    }

    // Tauri search strip: px-2 py-1.5, input h-7.
    div()
        .h(px(36.))
        .px_2()
        .flex()
        .items_center()
        .gap_1()
        .border_b_1()
        .border_color(rgb(palette.border))
        .bg(chrome.transparent_section_header)
        .child(div().flex_1().min_w_0().child(search_input))
        // Count lives in PanelHeader (Tauri).
        .child(icon_action_button_styled(
            palette,
            "connections-sort",
            sort_label,
            sort_tooltip,
            sort_tint,
            sort_flipped,
            cx.listener(|panel, _, _, cx| {
                panel.with_app(cx, |app, cx| {
                    app.cycle_connection_sort_mode(cx);
                });
            }),
        ))
        .child(icon_action_button(
            palette,
            "connections-temp-ssh",
            "icons/conn/flash.svg",
            t!("temporarySsh.title"),
            cx.listener(|panel, _, window, cx| {
                panel.with_app(cx, |app, cx| {
                    app.open_temporary_ssh_link_dialog(window, cx);
                });
            }),
        ))
        .child(icon_action_button(
            palette,
            "connections-new-group",
            "icons/fe/new-folder.svg",
            t!("savedConnections.newFolder"),
            cx.listener(|panel, _, window, cx| {
                panel.with_app(cx, |app, cx| {
                    app.open_connection_group_editor(None, None, window, cx);
                });
            }),
        ))
        .child(icon_action_button(
            palette,
            "connections-new",
            "icons/conn/add.svg",
            t!("savedConnections.newConnection"),
            cx.listener(|panel, _, window, cx| {
                panel.with_app(cx, |app, cx| {
                    app.open_connection_editor(None, None, false, window, cx);
                });
            }),
        ))
        .child(more_menu)
}
