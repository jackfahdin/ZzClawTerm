use rust_i18n::t;

use gpui::{Context, Window};
use nyaterm_core::ConnectionType;
use nyaterm_store::{StoreDomain, store_request};

use crate::features::NyaTermApp;
use crate::models::{SessionLaunchConfig, StartupCommandAction};

use super::state::RenameSessionSubmission;

impl NyaTermApp {
    pub(in crate::features) fn open_rename_session(
        &mut self,
        session_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current_name) = self.session.display_name(&session_id) else {
            self.shell
                .set_status("session no longer exists".to_string());
            cx.notify();
            return;
        };
        self.session.dialogs.open_rename(session_id, &current_name);
        self.forget_text_inputs("session.rename");
        self.shell.set_status("rename tab opened".to_string());
        self.open_form_dialog(
            (
                t!("tabCtx.renameTitle").to_string(),
                320.,
                t!("common.save").to_string(),
                |app, _, cx| app.rename_session_dialog_content(cx),
                |app, _, cx| app.submit_rename_session(cx),
                |app, cx| app.close_rename_session(cx),
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn close_rename_session(&mut self, cx: &mut Context<Self>) {
        self.session.dialogs.cancel_rename();
        self.forget_text_inputs("session.rename");
        self.shell.set_status("rename tab cancelled".to_string());
        cx.notify();
    }

    pub(in crate::features) fn submit_rename_session(&mut self, cx: &mut Context<Self>) -> bool {
        let (session_id, trimmed) = match self.session.dialogs.take_rename_submission() {
            RenameSessionSubmission::Inactive => {
                self.shell.set_status("no tab rename is active".to_string());
                cx.notify();
                return true;
            }
            RenameSessionSubmission::Empty => {
                self.shell
                    .set_status("tab name cannot be empty".to_string());
                cx.notify();
                return false;
            }
            RenameSessionSubmission::Ready { session_id, name } => (session_id, name),
        };
        self.forget_text_inputs("session.rename");
        self.session
            .set_custom_name(session_id.clone(), trimmed.clone());
        self.shell.set_status(format!("renamed tab to {trimmed}"));
        cx.notify();
        true
    }

    pub(in crate::features) fn open_startup_command_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_startup_command_dialog_for(StartupCommandAction::Duplicate, window, cx);
    }

    pub(in crate::features) fn open_startup_command_dialog_for(
        &mut self,
        action: StartupCommandAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.active_id().is_none() {
            self.shell.set_status(
                match action {
                    StartupCommandAction::Duplicate => {
                        "select a session before duplicating with a command"
                    }
                    StartupCommandAction::Multiplex => {
                        "select an SSH session before multiplexing with a command"
                    }
                }
                .to_string(),
            );
            cx.notify();
            return;
        }
        if action == StartupCommandAction::Multiplex
            && self
                .session
                .active_id()
                .and_then(|session_id| self.session.metadata(session_id))
                .is_none_or(|metadata| {
                    !matches!(metadata.launch_config, SessionLaunchConfig::Ssh(_))
                })
        {
            self.shell
                .set_status("active session is not SSH".to_string());
            cx.notify();
            return;
        }
        let delay_ms = u64::from(
            self.settings
                .summary()
                .interaction_duplicate_session_command_delay_ms,
        );
        self.session.dialogs.open_startup_command(action, delay_ms);
        self.forget_text_inputs("session.startup-command");
        self.shell.set_status(action.status_opened().to_string());
        let title = t!(match action {
            StartupCommandAction::Duplicate => "tabCtx.runCommandTitle",
            StartupCommandAction::Multiplex => "tabCtx.multiplexSshWithCommand",
        })
        .to_string();
        self.open_form_dialog(
            (
                title,
                448.,
                t!("common.confirm").to_string(),
                |app, _, cx| app.startup_command_dialog_content(cx),
                |app, window, cx| app.submit_startup_command_dialog(window, cx),
                |app, cx| app.close_startup_command_dialog(cx),
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn close_startup_command_dialog(&mut self, cx: &mut Context<Self>) {
        let action = self.session.dialogs.cancel_startup_command();
        self.forget_text_inputs("session.startup-command");
        self.shell.set_status(action.status_cancelled().to_string());
        cx.notify();
    }

    pub(in crate::features) fn set_startup_command_delay(
        &mut self,
        delay_ms: u64,
        cx: &mut Context<Self>,
    ) {
        self.session.dialogs.set_startup_command_delay(delay_ms);
        cx.notify();
    }

    pub(in crate::features) fn submit_startup_command_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((action, startup_command)) = self.session.dialogs.take_startup_command() else {
            self.shell
                .set_status("startup command cannot be empty".to_string());
            cx.notify();
            return false;
        };
        self.forget_text_inputs("session.startup-command");
        match action {
            StartupCommandAction::Duplicate => {
                self.duplicate_active_session_with_startup(Some(startup_command), window, cx);
            }
            StartupCommandAction::Multiplex => {
                self.multiplex_active_ssh_session_with_startup(Some(startup_command), window, cx);
            }
        }
        true
    }

    pub(in crate::features) fn apply_session_text_input(
        &mut self,
        field: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if !self.session.dialogs.apply_text_input(field, text) {
            return;
        }
        cx.notify();
    }

    fn flush_session_asset_monitoring(&mut self, session_id: &str, cx: &mut Context<Self>) {
        let expected_connection_id = self
            .session
            .metadata(session_id)
            .and_then(|metadata| metadata.source_connection_id.as_deref())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned);
        let Some(entry) = self.start_workspace.monitoring_mut().take(session_id) else {
            return;
        };
        let Some(connection_id) = expected_connection_id.filter(|id| id == &entry.connection_id)
        else {
            return;
        };
        if !self
            .connection_state
            .connections()
            .iter()
            .any(|connection| {
                connection.id == connection_id
                    && matches!(connection.config, ConnectionType::Ssh { .. })
            })
        {
            return;
        }

        let persisted_id = connection_id.clone();
        let patch = entry.last_asset_patch;
        self.submit_store_request(
            0,
            store_request(StoreDomain::Connections, move |store| {
                if !store.merge_connection_asset_from_monitoring(&persisted_id, patch)? {
                    return Ok(None);
                }
                store.get_connection(&persisted_id)
            }),
            move |this, event, cx| match event.outcome {
                Ok(Some(updated)) => {
                    this.connection_state.update_connection(updated);
                    cx.notify();
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "nyaterm::assets",
                        connection_id = %connection_id,
                        category = error.category(),
                        "failed to persist session monitoring asset snapshot"
                    );
                    this.settings.update_store_status(
                        format!("failed to save monitoring snapshot: {error}"),
                        false,
                    );
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn remove_session_state(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.flush_session_asset_monitoring(session_id, cx);
        self.remote_ops.clear_stats_sample(session_id);
        self.clear_terminal_selection_state_for_session(session_id);
        self.clear_terminal_mouse_report_for_session(session_id);
        self.session.start.clear_reconnect_failure(session_id);
        // If this leaf was a tab root, drop its pane tree (prune will rekey survivors).
        self.shell.remove_workspace_session(session_id);
        let multiplex_key = self.session.remove_session_catalog(session_id);
        self.session.clear_event_bridge_session(session_id);
        self.terminal.remove_frame_session(session_id);
        self.remove_terminal_surface(session_id);
        self.terminal.remove_session_surface_bounds(session_id);
        self.transfer.remove_browser_session_cache(session_id);
        self.transfer.clear_external_sync_for_session(session_id);
        self.transfer
            .close_properties_dialog_for_session(session_id);
        self.transfer.remove_editor_tabs_for_session(session_id);
        self.transfer.remove_preview_tabs_for_session(session_id);
        self.sync_input.purge_session(session_id);
        self.reconcile_terminal_windows();
        if self.session.restore_is_complete() {
            self.persist_open_tabs();
        }
        if let Some(multiplex_key) = multiplex_key
            && let Some(handle) = self
                .session
                .take_multiplex_handle_if_unreferenced(&multiplex_key)
        {
            self.session.disconnect_multiplex_handle(handle);
        }
    }
}
