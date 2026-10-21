use rust_i18n::t;

use std::time::Duration;

use gpui::{Context, Window};

use crate::features::formatting::{normalize_startup_command, short_id};
use crate::features::{AppLifecycleEvent, ZzClawTermApp};
use crate::models::StartupCommandRequest;
use crate::terminal::INITIAL_TERMINAL_BANNER;
use crate::terminal::initial_terminal_screen;

impl ZzClawTermApp {
    pub(in crate::features) fn schedule_startup_command(
        &mut self,
        session_id: String,
        startup_command: StartupCommandRequest,
        cx: &mut Context<Self>,
    ) {
        let command = normalize_startup_command(&startup_command.command);
        if command.trim().is_empty() {
            return;
        }
        let delay_ms = startup_command.delay_ms.min(60_000);
        self.shell.set_status(format!(
            "scheduled startup command for {}",
            short_id(&session_id)
        ));
        cx.spawn(async move |this, cx| {
            if delay_ms > 0 {
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;
            }
            let _ = this.update(cx, |this, cx| {
                if this.send_terminal_input_to_session(session_id, command.into_bytes(), cx) {
                    this.shell.set_status("startup command sent".to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::features) fn close_active_session(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.active_workspace_tab()
            && !crate::models::WorkspaceTabId::from_layout_key(&key).is_session()
        {
            self.request_workspace_tabs_close(vec![key], None, cx);
            return;
        }
        let Some(session_id) = self.session.active_id_owned() else {
            self.shell.set_status("no active session".to_string());
            cx.notify();
            return;
        };
        self.close_session(self.tab_root_for_session(&session_id), cx);
    }

    pub(in crate::features) fn close_session(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.hide_referenced_session_tab(&session_id, cx) {
            return;
        }
        if self.tab_tree_is_locked(&session_id) {
            self.notify_locked_tab_close_blocked(cx);
            return;
        }
        let preferred_next = self.shell.terminal_window_tree().and_then(|mut root| {
            let group = root.leaf_for_tab(&session_id).map(str::to_string)?;
            let groups = root.leaf_ids();
            let index = groups.iter().position(|id| id == &group).unwrap_or(0);
            let next = root.remove_tab(&session_id)?;
            let surviving = next.leaf_ids();
            let group = if next.leaf_tabs(&group).is_some() {
                group
            } else {
                surviving
                    .get(index.min(surviving.len().saturating_sub(1)))?
                    .clone()
            };
            next.leaf_tabs(&group)?.1.map(str::to_string)
        });
        // Tauri: closing a strip tab closes the whole tab tree; closing a secondary leaf
        // only removes that pane. Strip close uses the tab-root id.
        let close_ids = if !self.is_secondary_pane_session(&session_id) {
            if let Some(root) = self.shell.workspace_pane_root(&session_id) {
                root.session_ids()
            } else {
                vec![session_id.clone()]
            }
        } else {
            vec![session_id.clone()]
        };
        let was_active = self
            .session
            .active_id()
            .is_some_and(|active_id| close_ids.iter().any(|id| id == active_id));
        // A pane that fails to close must not abandon the rest of the tab tree half
        // torn down: the remaining panes still close and the layout is still pruned.
        let mut close_error = None;
        let mut closed_any = false;
        for close_id in &close_ids {
            let disconnected = self.session.is_disconnected(close_id);
            let close_result = if self.remote_desktop.is_session(close_id) {
                self.close_remote_desktop_runtime(close_id)
            } else {
                self.session
                    .manager()
                    .close_detached(close_id)
                    .map_err(anyhow::Error::from)
            };
            match close_result {
                Ok(()) => {}
                Err(_) if disconnected => {}
                Err(error) if !disconnected && !self.session.has_session(close_id) => {
                    close_error.get_or_insert(error);
                    continue;
                }
                Err(error) => {
                    close_error.get_or_insert(error);
                    continue;
                }
            }
            self.cleanup_recording_for_session(close_id);
            self.remove_session_state(close_id, cx);
            closed_any = true;
        }
        if !closed_any && let Some(error) = close_error.take() {
            self.shell.set_status(format!("close failed: {error}"));
            cx.notify();
            return;
        }
        self.prune_workspace_split();
        if was_active {
            self.ai.reset_agent_runtime();
            self.sync_session_event_bridge_policy();
            if let Some(next_tab) = preferred_next
                .filter(|id| self.workspace_tab_ids().contains(id))
                .or_else(|| {
                    self.session
                        .next_session_after(&session_id)
                        .filter(|id| !self.shell.session_tab_is_hidden(id))
                })
                .or_else(|| self.workspace_tab_ids().first().cloned())
            {
                self.activate_workspace_tab(&next_tab, cx);
                self.shell.set_status("session closed".to_string());
            } else {
                self.session.clear_active_session();
                self.sync_ai_active_scope(cx);
                self.terminal.view.output = String::from(INITIAL_TERMINAL_BANNER);
                self.terminal.view.output_decoder.reset_decoder();
                self.terminal.view.screen = initial_terminal_screen();
                if let Err(error) = self
                    .terminal
                    .view
                    .screen
                    .set_encoding(&self.settings.summary().interaction_default_encoding)
                {
                    self.shell
                        .set_status(crate::features::terminal::encoding_error_text(error));
                }
                self.shell.set_status("session closed".to_string());
            }
        } else {
            self.shell
                .set_status(format!("closed {}", short_id(&session_id)));
        }
        if let Some(error) = close_error {
            self.shell.set_status(format!("close failed: {error}"));
        }
        cx.notify();
    }

    pub(in crate::features) fn close_tab_active_pane(
        &mut self,
        tab_root_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.tab_tree_is_locked(tab_root_id) {
            self.notify_locked_tab_close_blocked(cx);
            return;
        }
        let pane_id = self.active_pane_for_tab_root(tab_root_id);
        if self.tab_tree_session_ids(tab_root_id).len() <= 1 {
            self.close_session(tab_root_id.to_string(), cx);
            return;
        }

        if pane_id == tab_root_id
            && let Some(survivor_id) = self
                .tab_tree_session_ids(tab_root_id)
                .into_iter()
                .find(|id| id != &pane_id)
        {
            self.session
                .migrate_tab_root_presentation(tab_root_id, &survivor_id);
            self.shell
                .rekey_workspace_pane_root(tab_root_id, survivor_id);
        }

        if self.workspace_references_session(&pane_id, cx) {
            let next = self
                .tab_tree_session_ids(&pane_id)
                .into_iter()
                .find(|id| id != &pane_id)
                .or_else(|| {
                    self.tab_tree_session_ids(tab_root_id)
                        .into_iter()
                        .find(|id| id != &pane_id)
                });
            self.shell.hide_workspace_session(&pane_id);
            self.prune_workspace_split();
            self.reconcile_terminal_windows();
            if let Some(next) = next {
                self.activate_session_id(&next, cx);
            }
            self.persist_open_tabs();
            cx.notify();
            return;
        }
        let was_active = self.session.active_id() == Some(pane_id.as_str());
        let disconnected = self.session.is_disconnected(&pane_id);
        let close_result = if self.remote_desktop.is_session(&pane_id) {
            self.close_remote_desktop_runtime(&pane_id)
        } else {
            self.session
                .manager()
                .close(&pane_id)
                .map_err(anyhow::Error::from)
        };
        match close_result {
            Ok(()) => {}
            Err(_) if disconnected => {}
            Err(error) if !disconnected && !self.session.has_session(&pane_id) => {
                self.shell.set_status(format!("close failed: {error}"));
                cx.notify();
                return;
            }
            Err(_) => {}
        }
        self.cleanup_recording_for_session(&pane_id);
        self.remove_session_state(&pane_id, cx);
        self.prune_workspace_split();
        if was_active {
            self.ai.reset_agent_runtime();
            self.sync_session_event_bridge_policy();
            let next = self
                .tab_tree_session_ids(tab_root_id)
                .into_iter()
                .find(|id| self.session.has_session(id))
                .or_else(|| self.session.next_session_after(&pane_id));
            if let Some(next_session_id) = next {
                self.activate_session_id(&next_session_id, cx);
            } else {
                self.session.clear_active_session();
                self.sync_ai_active_scope(cx);
            }
        }
        self.shell
            .set_status(format!("closed {}", short_id(&pane_id)));
        cx.notify();
    }

    pub(in crate::features) fn handle_window_minimize(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.summary().minimize_to_tray {
            if self.minimize_to_system_tray(window, cx) {
                return;
            }
            self.notify_operation(
                "tray-unavailable",
                zzclawterm_ui::notification::ZzClawNotificationKind::Warning,
                rust_i18n::t!("tray.unavailable").to_string(),
                cx,
            );
        }
        window.minimize_window();
    }

    pub(crate) fn handle_window_close_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_window_close_request_with_count(
            self.session.ordered_sessions().len(),
            window,
            cx,
        );
    }

    pub(crate) fn handle_window_close_request_with_count(
        &mut self,
        open_sessions: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.theme_designer().submitting {
            cx.emit(AppLifecycleEvent::ShutdownCancelled);
            return;
        }
        if self.settings.theme_designer().dirty() {
            self.activate_settings_window(cx);
            self.theme_designer_action(
                crate::features::settings::theme_designer::ThemeIntent::Quit(open_sessions),
                cx,
            );
            return;
        }
        if self.settings.summary().confirm_on_close && open_sessions > 0 {
            // Reuse the close-all confirmation as the quit-with-sessions gate.
            // A title-bar close control can also produce the native window-close
            // callback, so retain the existing modal rather than stacking a
            // second confirmation for the same close request.
            if self.session.dialog_should_quit_after_close_all() {
                return;
            }
            self.session.dialog_request_quit_after_close_all();
            self.open_close_all_sessions_confirm(window, cx);
            self.shell.set_status(format!(
                "confirm close: {open_sessions} session(s) still open"
            ));
            cx.notify();
            return;
        }
        if self.has_unconfirmed_editor_changes() {
            let targets = self
                .transfer
                .editor_documents()
                .iter()
                .map(|doc| crate::models::WorkspaceTabId::Editor(doc.id.clone()).layout_key())
                .collect();
            self.request_workspace_tabs_close(targets, Some(open_sessions), cx);
            return;
        }
        // Persist workspace before exit when startup restore is enabled.
        if self.settings.summary().startup_restore {
            self.flush_open_tabs_now(cx);
        }
        if self.prepare_native_update_exit(cx) {
            cx.emit(AppLifecycleEvent::ShutdownRequested);
        }
    }

    pub(in crate::features) fn open_close_all_sessions_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.ordered_sessions().is_empty() {
            self.shell.set_status("no sessions to close".to_string());
            cx.notify();
            return;
        }
        self.session.dialog_open_close_all_sessions_confirm();
        self.shell
            .set_status("close all sessions confirmation opened".to_string());
        let title_key = if self.session.dialog_should_quit_after_close_all() {
            "dialog.confirmClose"
        } else {
            "tabCtx.closeAll"
        };
        let description_key = if self.session.dialog_should_quit_after_close_all() {
            "dialog.confirmCloseDesc"
        } else {
            "tabCtx.closeAllConfirm"
        };
        let action_key = if self.session.dialog_should_quit_after_close_all() {
            "dialog.confirmCloseAction"
        } else {
            "tabCtx.closeAll"
        };
        self.open_confirm_dialog_with_cancel(
            (
                t!(title_key).to_string(),
                t!(description_key).to_string(),
                t!(action_key).to_string(),
                true,
                |app, window, cx| app.confirm_close_all_sessions(window, cx),
                |app, cx| app.cancel_close_all_sessions_confirm(cx),
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn cancel_close_all_sessions_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.session.dialog_should_quit_after_close_all() {
            self.cancel_editor_exit_approval();
            cx.emit(AppLifecycleEvent::ShutdownCancelled);
        }
        self.update
            .update(cx, |update, _| update.install_requested = false);
        self.session.dialog_cancel_close_all_sessions_confirm();
        self.shell
            .set_status("close all sessions cancelled".to_string());
        cx.notify();
    }

    pub(in crate::features) fn confirm_close_all_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let quit_after = self.session.dialog_take_close_all_sessions_confirm();
        if quit_after {
            self.handle_window_close_request_with_count(0, window, cx);
            return true;
        }
        self.close_all_sessions(cx);
        cx.notify();
        true
    }

    pub(in crate::features) fn close_all_sessions(&mut self, cx: &mut Context<Self>) {
        self.request_workspace_tabs_close(self.workspace_tab_ids(), None, cx);
    }

    pub(in crate::features) fn close_inactive_sessions(
        &mut self,
        keep_session_id: String,
        cx: &mut Context<Self>,
    ) {
        let targets = self
            .workspace_tab_ids()
            .into_iter()
            .filter(|key| key != &keep_session_id)
            .collect();
        self.activate_workspace_tab(&keep_session_id, cx);
        self.request_workspace_tabs_close(targets, None, cx);
    }

    pub(in crate::features) fn close_sessions_to_right(
        &mut self,
        anchor_session_id: String,
        cx: &mut Context<Self>,
    ) {
        let tabs = self
            .shell
            .terminal_group_for_tab(&anchor_session_id)
            .as_deref()
            .and_then(|group| self.shell.terminal_group_tabs(group))
            .map(|(tabs, _)| tabs)
            .unwrap_or_else(|| self.workspace_tab_ids());
        let Some(index) = tabs.iter().position(|key| key == &anchor_session_id) else {
            return;
        };
        self.request_workspace_tabs_close(tabs.into_iter().skip(index + 1).collect(), None, cx);
    }

    pub(in crate::features) fn navigate_terminal_command(
        &mut self,
        action: zzclawterm_terminal::command_navigation::CommandNavigationAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_id) = self.session.active_id_owned() {
            self.terminal
                .view
                .frame_pipeline
                .navigate_command(session_id, action);
            self.terminal.view.frame_pipeline.arm_event_wakes();
        }
        cx.notify();
    }

    pub(in crate::features) fn clear_terminal(&mut self, cx: &mut Context<Self>) {
        self.clear_terminal_selection(cx);
        self.terminal.menus.actions_open = false;
        if let Some(session_id) = self.session.active_id() {
            self.terminal
                .view
                .frame_pipeline
                .clear_session_except_input(session_id.to_string());
            self.terminal.view.frame_pipeline.arm_event_wakes();
            self.shell
                .set_status("terminal clear requested".to_string());
        }
        cx.notify();
    }

    pub(in crate::features) fn append_terminal_log(&mut self, text: impl AsRef<str>) {
        let session_id = self.session.active_id_owned();
        self.append_terminal_log_for_session(session_id.as_deref(), text.as_ref(), false);
    }
}
