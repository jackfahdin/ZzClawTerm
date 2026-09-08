use rust_i18n::t;

use gpui::Context;
use nyaterm_core::SavedConnection;
use nyaterm_ui::{NyaDialogWindowExt as _, NyaMenuItem};

use crate::features::NyaTermApp;
use crate::models::ConnectionListContextTarget;

use super::editor::ordered_connection_groups;

impl NyaTermApp {
    fn connection_move_to_group_menu_items(
        &mut self,
        move_ids: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let ungrouped_ids = move_ids.clone();
        let mut items = vec![
            NyaMenuItem::action(t!("savedConnections.ungroupedConnections"))
                .icon("icons/conn/connect.svg")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.move_connections_into_group(ungrouped_ids.clone(), None, cx);
                })),
        ];

        let groups = ordered_connection_groups(self.connection_state.groups());
        if !groups.is_empty() {
            items.push(NyaMenuItem::separator());
        }
        for (group, depth) in groups {
            let group_id = group.id.clone();
            let ids = move_ids.clone();
            let label = format!("{:indent$}{}", "", group.name, indent = depth * 2);
            items.push(
                NyaMenuItem::action(label)
                    .icon("icons/conn/folder.svg")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.move_connections_into_group(ids.clone(), Some(group_id.clone()), cx);
                    })),
            );
        }
        items
    }

    pub(in crate::features::pages::connections) fn connection_context_menu_items(
        &mut self,
        connection: SavedConnection,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let selected = self.connection_state.selected_connections();
        let selected_count = selected.len();
        let targets_selection = selected_count > 1
            && self
                .connection_state
                .list_contains_selected_id(&connection.id);
        let connect_label = if targets_selection {
            format!(
                "{} ({selected_count})",
                t!("savedConnections.connectSelected")
            )
        } else {
            t!("savedConnections.connect").to_string()
        };
        let copy_label = if targets_selection {
            t!("savedConnections.copySelected")
        } else {
            t!("savedConnections.copy")
        };
        let move_ids = if targets_selection {
            selected
                .into_iter()
                .map(|connection| connection.id)
                .collect()
        } else {
            vec![connection.id.clone()]
        };

        let connection_id = connection.id.clone();
        let connection_for_connect = connection.clone();
        let connection_for_edit = connection.id.clone();
        let connection_for_rename = connection.id.clone();
        let connection_for_copy = connection.id.clone();
        let connection_for_delete = connection.id;
        let move_items = self.connection_move_to_group_menu_items(move_ids, cx);

        vec![
            NyaMenuItem::action(connect_label)
                .icon("icons/conn/connect.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    let selected = this.connection_state.selected_connections();
                    if selected.len() > 1 && selected.iter().any(|item| item.id == connection_id) {
                        this.start_selected_saved_connections(window, cx);
                    } else {
                        this.start_saved_connection(connection_for_connect.clone(), window, cx);
                    }
                })),
            NyaMenuItem::action(t!("savedConnections.edit"))
                .icon("icons/net/edit.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_connection_editor(
                        Some(connection_for_edit.clone()),
                        None,
                        false,
                        window,
                        cx,
                    );
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("savedConnections.rename"))
                .icon("icons/session/rename.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.rename_connection(connection_for_rename.clone(), window, cx);
                })),
            NyaMenuItem::action(copy_label)
                .icon("icons/copy.svg")
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.connection_state.selected_connections().len() > 1 {
                        this.copy_selected_connections(cx);
                    } else {
                        this.copy_connection_by_id(connection_for_copy.clone(), cx);
                    }
                })),
            NyaMenuItem::submenu(t!("savedConnections.moveToGroup"), move_items)
                .icon("icons/net/move.svg"),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("savedConnections.delete"))
                .icon("icons/net/delete.svg")
                .danger()
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.connection_state.selected_connections().len() > 1 {
                        this.delete_selected_connections(window, cx);
                    } else {
                        this.open_connection_delete_confirm(
                            connection_for_delete.clone(),
                            window,
                            cx,
                        );
                    }
                })),
        ]
    }

    pub(in crate::features::pages::connections) fn connection_group_context_menu_items(
        &mut self,
        group_id: String,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let group_id_new = group_id.clone();
        let group_id_folder = group_id.clone();
        let group_id_open = group_id.clone();
        let group_id_edit = group_id.clone();
        let group_id_delete = group_id.clone();
        let total_in_group = self
            .connection_state
            .saved_connections_in_group_tree(&group_id)
            .len();

        let mut items = vec![
            NyaMenuItem::action(t!("savedConnections.newConnection"))
                .icon("icons/conn/add.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_connection_editor(
                        None,
                        Some(group_id_new.clone()),
                        false,
                        window,
                        cx,
                    );
                })),
            NyaMenuItem::action(t!("savedConnections.newFolder"))
                .icon("icons/fe/new-folder.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_connection_group_editor(
                        None,
                        Some(group_id_folder.clone()),
                        window,
                        cx,
                    );
                    this.defer_connection_panel_snapshot_flush(cx);
                })),
        ];
        if total_in_group > 0 {
            items.extend([
                NyaMenuItem::separator(),
                NyaMenuItem::action(t!("savedConnections.openAllConnections"))
                    .icon("icons/fe/forward.svg")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_connection_group_open_confirm(group_id_open.clone(), window, cx);
                    })),
            ]);
        }
        items.extend([
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("savedConnections.renameFolder"))
                .icon("icons/session/rename.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_connection_group_editor(
                        Some(group_id_edit.clone()),
                        None,
                        window,
                        cx,
                    );
                    this.defer_connection_panel_snapshot_flush(cx);
                })),
            NyaMenuItem::action(t!("savedConnections.deleteFolder"))
                .icon("icons/net/delete.svg")
                .danger()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_connection_group_delete_confirm(group_id_delete.clone(), window, cx);
                })),
        ]);
        items
    }

    fn connection_selected_menu_items(
        &mut self,
        selected: Vec<SavedConnection>,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let selected_count = selected.len();
        let move_ids = selected
            .into_iter()
            .map(|connection| connection.id)
            .collect::<Vec<_>>();
        let connect_label = if selected_count > 1 {
            format!(
                "{} ({selected_count})",
                t!("savedConnections.connectSelected")
            )
        } else {
            t!("savedConnections.connect").to_string()
        };
        let move_items = self.connection_move_to_group_menu_items(move_ids, cx);

        vec![
            NyaMenuItem::action(connect_label)
                .icon("icons/conn/connect.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.start_selected_saved_connections(window, cx);
                })),
            NyaMenuItem::action(t!("savedConnections.copySelected"))
                .icon("icons/copy.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.copy_selected_connections(cx);
                })),
            NyaMenuItem::submenu(t!("savedConnections.moveToGroup"), move_items)
                .icon("icons/net/move.svg"),
            NyaMenuItem::action(t!("savedConnections.delete"))
                .icon("icons/net/delete.svg")
                .danger()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.delete_selected_connections(window, cx);
                })),
        ]
    }

    /// Items for the list's single context menu, chosen by what was right-clicked.
    ///
    /// The list cannot nest a menu per row: one right-click would open the row's
    /// and the list's together, and the one that never receives the click keeps
    /// re-focusing itself, which leaves any dialog opened afterwards unable to
    /// see its own dismiss actions.
    pub(in crate::features::pages::connections) fn connection_list_target_context_menu_items(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        match self.connection_state.list_context_target().clone() {
            ConnectionListContextTarget::List => self.connection_list_context_menu_items(cx),
            ConnectionListContextTarget::Group(group_id) => {
                self.connection_group_context_menu_items(group_id, cx)
            }
            ConnectionListContextTarget::Connection(connection_id) => {
                let Some(connection) = self
                    .connection_state
                    .connection_by_id(&connection_id)
                    .cloned()
                else {
                    // The row went away between the press and the build.
                    return self.connection_list_context_menu_items(cx);
                };
                self.connection_context_menu_items(connection, cx)
            }
        }
    }

    pub(in crate::features::pages::connections) fn connection_list_context_menu_items(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let selected = self.connection_state.selected_connections();
        let mut items = if selected.is_empty() {
            Vec::new()
        } else {
            let mut items = self.connection_selected_menu_items(selected, cx);
            items.push(NyaMenuItem::separator());
            items
        };
        items.extend([
            NyaMenuItem::action(t!("savedConnections.newConnection"))
                .icon("icons/conn/add.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_connection_editor(None, None, false, window, cx);
                })),
            NyaMenuItem::action(t!("savedConnections.newFolder"))
                .icon("icons/fe/new-folder.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_connection_group_editor(None, None, window, cx);
                    this.defer_connection_panel_snapshot_flush(cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("settings.importConfig"))
                .icon("icons/import.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    window.close_nya_dialog(cx);
                    this.open_connection_import_dialog(window, cx);
                })),
        ]);
        items
    }

    pub(in crate::features::pages::connections) fn connection_more_menu_items(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let selected = self.connection_state.selected_connections();
        let mut items = vec![
            NyaMenuItem::action(t!("settings.exportConfig"))
                .icon("icons/menu/export.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    window.close_nya_dialog(cx);
                    this.prompt_encrypted_portable_snapshot_export(window, cx);
                })),
            NyaMenuItem::action(t!("settings.importConfig"))
                .icon("icons/import.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    window.close_nya_dialog(cx);
                    this.open_connection_import_dialog(window, cx);
                })),
        ];
        if !selected.is_empty() {
            items.push(NyaMenuItem::separator());
            items.extend(self.connection_selected_menu_items(selected, cx));
        }
        if !self.connection_state.connections().is_empty() {
            items.extend([
                NyaMenuItem::separator(),
                NyaMenuItem::action(t!("savedConnections.clearAll"))
                    .icon("icons/transfer/clear-all.svg")
                    .danger()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_connections_clear_all_confirm(window, cx);
                    })),
            ]);
        }
        items
    }
}
