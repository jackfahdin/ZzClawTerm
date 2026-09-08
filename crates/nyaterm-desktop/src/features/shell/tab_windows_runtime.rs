use gpui::Context;
use nyaterm_store::{StoreDomain, store_request};

use crate::features::terminal::{TerminalWindowDockResult, TerminalWindowReconcileResult};
use crate::features::{NyaTermApp, formatting::short_id};
use crate::models::{MainMode, NavItem, SmartSplitMode, TabDockZone};

impl NyaTermApp {
    /// Ensure every live session appears in the multi-leaf layout once it is enabled.
    pub(in crate::features) fn reconcile_terminal_windows(&mut self) {
        // Flat strip mode (default): avoid allocating a full session list on
        // every residual call when the multi-leaf owner is inactive.
        if !self.terminal.terminal_window_tree_is_some() {
            return;
        }
        let live_ids = self
            .session
            .session_order()
            .iter()
            .filter(|session_id| {
                self.session.has_session(session_id) && !self.is_secondary_pane_session(session_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let preferred = self.shell.workspace.focused_terminal_leaf_id.clone();
        let active = self.session.active_id_owned();
        match self.terminal.reconcile_terminal_windows(
            &live_ids,
            preferred.as_deref(),
            active.as_deref(),
        ) {
            TerminalWindowReconcileResult::Inactive => {}
            TerminalWindowReconcileResult::Cleared => {
                self.shell.workspace.focused_terminal_leaf_id = None;
            }
            TerminalWindowReconcileResult::Reconciled { focused_leaf_id } => {
                self.shell.workspace.focused_terminal_leaf_id = focused_leaf_id;
            }
        }
    }

    pub(in crate::features) fn ensure_terminal_windows_root(&mut self) {
        let tab_ids = self
            .ordered_tab_sessions()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        let active = self.session.active_id_owned();
        if let Some(focused_leaf_id) = self.terminal.ensure_terminal_windows_root(tab_ids, active) {
            self.shell.workspace.focused_terminal_leaf_id = Some(focused_leaf_id);
        }
    }

    pub(in crate::features) fn activate_terminal_window_tab(
        &mut self,
        leaf_id: String,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        self.ensure_terminal_windows_root();
        self.terminal
            .activate_terminal_window_tab(&leaf_id, &session_id);
        self.shell.workspace.focused_terminal_leaf_id = Some(leaf_id);
        self.activate_session_id_with_surface_sync(&session_id, cx);
        self.shell.navigation.selected_nav = NavItem::Workspace;
        self.shell.navigation.main_mode = MainMode::Workspace;
        cx.notify();
    }

    pub(in crate::features) fn terminal_windows_is_multi_leaf(&self) -> bool {
        self.terminal.terminal_windows_is_multi_leaf()
    }

    pub(in crate::features) fn sync_terminal_windows_active_tab(&mut self, session_id: &str) {
        // Multi-leaf tab ids are tab roots; map secondary pane focus to its strip tab.
        let tab_id = self.tab_root_for_session(session_id);
        if let Some(leaf_id) = self.terminal.sync_terminal_windows_active_tab(&tab_id) {
            self.shell.workspace.focused_terminal_leaf_id = Some(leaf_id);
        }
    }

    pub(in crate::features) fn place_tab_before_in_terminal_windows(
        &mut self,
        tab_id: String,
        before_tab_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal.terminal_window_tree_is_some() {
            self.terminal.clear_terminal_window_drop();
            return;
        }
        if let Some(focused_leaf_id) = self
            .terminal
            .place_tab_before_in_terminal_windows(&tab_id, &before_tab_id)
        {
            self.shell.workspace.focused_terminal_leaf_id = focused_leaf_id;
            self.activate_session_id_with_surface_sync(&tab_id, cx);
            self.shell.set_status(format!(
                "moved tab {} before {}",
                short_id(&tab_id),
                short_id(&before_tab_id)
            ));
            self.persist_terminal_window_layout();
        }
        cx.notify();
    }

    pub(in crate::features) fn set_terminal_window_drop(
        &mut self,
        leaf_id: String,
        zone: TabDockZone,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.set_terminal_window_drop(leaf_id, zone) {
            cx.notify();
        }
    }

    pub(in crate::features) fn clear_terminal_window_drop(&mut self, cx: &mut Context<Self>) {
        if self.terminal.clear_terminal_window_drop() {
            cx.notify();
        }
    }

    pub(in crate::features) fn dock_tab_on_terminal_window_leaf(
        &mut self,
        tab_id: String,
        target_leaf_id: String,
        zone: TabDockZone,
        cx: &mut Context<Self>,
    ) {
        self.ensure_terminal_windows_root();
        let focused_leaf_id =
            match self
                .terminal
                .dock_tab_on_terminal_window_leaf(&tab_id, &target_leaf_id, zone)
            {
                TerminalWindowDockResult::MissingTree => {
                    cx.notify();
                    return;
                }
                TerminalWindowDockResult::UnknownTab => {
                    self.shell
                        .set_status(format!("unknown tab {}", short_id(&tab_id)));
                    cx.notify();
                    return;
                }
                TerminalWindowDockResult::NoEffect => {
                    self.shell.set_status("tab dock had no effect".to_string());
                    cx.notify();
                    return;
                }
                TerminalWindowDockResult::Docked { focused_leaf_id } => focused_leaf_id,
            };
        self.shell.workspace.focused_terminal_leaf_id = focused_leaf_id;
        self.activate_session_id_with_surface_sync(&tab_id, cx);
        self.shell.navigation.selected_nav = NavItem::Workspace;
        self.shell.navigation.main_mode = MainMode::Workspace;
        let zone_label = match zone {
            TabDockZone::Center => "merged into leaf".to_string(),
            TabDockZone::Edge(edge) => format!("split to {}", edge.label()),
        };
        self.shell
            .set_status(format!("docked tab {} ({})", short_id(&tab_id), zone_label));
        self.persist_terminal_window_layout();
        cx.notify();
    }

    /// Apply Tauri smart-split / tile layout: each open tab becomes its own multi-leaf window.
    pub(in crate::features) fn apply_smart_split(
        &mut self,
        mode: SmartSplitMode,
        cx: &mut Context<Self>,
    ) {
        let tab_ids = self
            .ordered_tab_sessions()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        if tab_ids.is_empty() {
            self.shell.set_status("no tabs to tile".to_string());
            cx.notify();
            return;
        }
        let active = self.session.active_id_owned();
        let Some(focused_leaf_id) =
            self.terminal
                .apply_smart_split(&tab_ids, mode, active.as_deref())
        else {
            self.shell
                .set_status("unable to build tile layout".to_string());
            cx.notify();
            return;
        };
        // Clear global pane splits so multi-leaf rendering takes precedence cleanly.
        self.shell.workspace.split = None;
        self.shell.workspace.split_resize = None;
        self.shell.workspace.focused_terminal_leaf_id = focused_leaf_id;
        self.shell.navigation.selected_nav = NavItem::Workspace;
        self.shell.navigation.main_mode = MainMode::Workspace;
        self.shell
            .set_status(format!("applied {}", mode.label().to_ascii_lowercase()));
        self.persist_terminal_window_layout();
        // Global pane layout is obsolete while multi-leaf is active.
        self.persist_workspace_pane_layout();
        cx.notify();
    }

    pub(in crate::features) fn persist_terminal_window_layout(&mut self) {
        if !self.settings.summary().startup_restore
            || !self.settings.summary().startup_restore_window_layout
        {
            return;
        }
        // Defer disk write — layout changes must not open redb on the UI hot path.
        self.shell.mark_window_layout_persist_dirty();
    }

    pub(in crate::features) fn try_restore_terminal_window_layout(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.terminal_windows_restore_is_complete() {
            return;
        }
        if !self.settings.summary().startup_restore
            || !self.settings.summary().startup_restore_window_layout
        {
            self.terminal.complete_terminal_windows_restore();
            return;
        }
        // Do not open the config DB during connect/register; wait for idle.
        if self.session.start_has_pending() || self.runtime_output_pressure_active() {
            return;
        }
        let ordered = self
            .ordered_tab_sessions()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        // Wait until startup restore has created sessions so tab indexes can map
        // correctly. Once startup restore is complete, an empty session list
        // means there is nothing to restore; mark this done so the runtime can
        // enter the quiet cadence.
        if ordered.is_empty() {
            if self.session.restore_is_complete() {
                self.terminal.complete_terminal_windows_restore();
            }
            return;
        }
        self.terminal.complete_terminal_windows_restore();
        let active = self.session.active_id_owned();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Sessions, |store| {
                store.load_terminal_window_layout()
            }),
            move |this, event, cx| {
                match event.outcome {
                    Ok(Some(layout)) => {
                        if let Some(focused_leaf_id) = this.terminal.restore_terminal_window_layout(
                            &layout,
                            &ordered,
                            active.as_deref(),
                        ) {
                            this.shell.workspace.focused_terminal_leaf_id = focused_leaf_id;
                            this.shell
                                .set_status("restored multi-leaf window layout".to_string());
                        }
                    }
                    Ok(None) => {}
                    Err(error) => this
                        .shell
                        .set_status(format!("failed to restore terminal layout: {error}")),
                }
                cx.notify();
            },
            cx,
        );
    }
}
