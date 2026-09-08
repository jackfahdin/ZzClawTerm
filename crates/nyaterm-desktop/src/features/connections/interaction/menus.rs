use rust_i18n::t;

use gpui::{Context, Window};
use nyaterm_core::SavedConnection;

use crate::features::{NyaTermApp, session::SavedConnectionStartOptions};
impl NyaTermApp {
    pub(in crate::features) fn prepare_connection_context_menu(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        self.connection_state
            .prepare_list_connection_context_menu(connection_id);
        cx.notify();
    }

    pub(in crate::features) fn copy_connection_by_id(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self
            .connection_state
            .connections()
            .iter()
            .find(|connection| connection.id == connection_id)
            .cloned()
        else {
            self.shell
                .set_status("connection is no longer available".to_string());
            cx.notify();
            return;
        };
        self.submit_connection_copies(vec![connection], cx);
    }

    pub(in crate::features) fn start_selected_saved_connections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.connection_state.selected_connections();
        if selected.is_empty() {
            self.shell
                .set_status("select saved connections before connecting".to_string());
            cx.notify();
            return;
        }
        let started = self.start_saved_connection_starts(selected, window, cx);
        self.shell
            .set_status(format!("starting {started} connection(s)"));
    }

    pub(in crate::features) fn start_group_connections(
        &mut self,
        group_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connections = self
            .connection_state
            .saved_connections_in_group_tree(&group_id);
        if connections.is_empty() {
            self.shell
                .set_status("group has no connections".to_string());
            cx.notify();
            return;
        }
        let started = self.start_saved_connection_starts(connections, window, cx);
        self.shell
            .set_status(format!("starting {started} connection(s) from group"));
    }

    pub(in crate::features) fn open_connection_group_open_confirm(
        &mut self,
        group_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self
            .connection_state
            .groups()
            .iter()
            .find(|group| group.id == group_id)
        else {
            return;
        };
        let connection_count = self
            .connection_state
            .saved_connections_in_group_tree(&group_id)
            .len();
        if connection_count == 0 {
            return;
        }
        let label = group.name.clone();
        let description = t!(
            "savedConnections.openAllConnectionsConfirm",
            name = label,
            count = connection_count
        )
        .to_string();
        self.open_confirm_dialog(
            (
                t!("savedConnections.openAllConnections").to_string(),
                description,
                t!("savedConnections.openAllConnections").to_string(),
                false,
                move |app, window, cx| {
                    app.start_group_connections(group_id.clone(), window, cx);
                    true
                },
            ),
            window,
            cx,
        );
    }

    fn start_saved_connection_starts(
        &mut self,
        connections: Vec<SavedConnection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let mut started = 0usize;
        for connection in connections {
            if self
                .session
                .start_saved_connection_is_pending_or_preparing(&connection)
            {
                continue;
            }
            self.start_saved_connection_with_options(
                connection,
                SavedConnectionStartOptions::default(),
                window,
                cx,
            );
            started = started.saturating_add(1);
        }
        started
    }
}
