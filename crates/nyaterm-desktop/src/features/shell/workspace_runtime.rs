use std::collections::HashSet;

use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, deferred,
};
use nyaterm_core::uuid;
use nyaterm_store::{StoreDomain, store_request};

use crate::features::{
    NyaTermApp, formatting::short_id, view_widgets::horizontal_resize_handle_visual,
    view_widgets::vertical_resize_handle_visual,
};
use crate::models::{
    MainMode, NavItem, WorkspacePaneNode, WorkspaceSplitDirection, WorkspaceSplitResizeState,
};

impl NyaTermApp {
    pub(in crate::features) fn live_session_ids(&self) -> HashSet<String> {
        self.session
            .metadata_entries()
            .filter(|(_, metadata)| !metadata.disconnected)
            .map(|(session_id, _)| session_id.to_string())
            .collect()
    }

    pub(in crate::features) fn prune_workspace_split(&mut self) {
        let live_ids = self.live_session_ids_with_disconnected();
        let before_roots = self.shell.workspace.pane_roots.clone();
        let root_keys: Vec<String> = self.shell.workspace.pane_roots.keys().cloned().collect();
        for tab_root in root_keys {
            let Some(root) = self.shell.workspace.pane_roots.remove(&tab_root) else {
                continue;
            };
            if let Some(node) = root.prune(&live_ids)
                && node.is_split()
            {
                // Keep map key as original tab root when still present; else rekey.
                let key = if node.contains_session(&tab_root) {
                    tab_root.clone()
                } else {
                    node.session_ids()
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| tab_root.clone())
                };
                self.shell.workspace.pane_roots.insert(key, node);
            }
            // Single leaf collapses to no stored tree for this tab.
        }
        // Also prune legacy workspace_split if roots empty (migration path).
        if self.shell.workspace.pane_roots.is_empty()
            && let Some(root) = self.shell.workspace.split.take()
            && let Some(node) = root.prune(&live_ids)
        {
            if node.is_split() {
                if let Some(first) = node.session_ids().into_iter().next() {
                    self.shell.workspace.pane_roots.insert(first.clone(), node);
                }
            } else if let WorkspacePaneNode::Leaf { session_id } = node {
                self.session.select_active_session_if_none(session_id);
            }
        }
        self.rebuild_session_tab_owners();
        self.sync_workspace_split_from_active_tab();
        if self.shell.workspace.pane_roots != before_roots {
            self.persist_workspace_pane_layout();
            if self.session.restore_is_complete() {
                self.persist_open_tabs();
            }
        }
    }

    fn live_session_ids_with_disconnected(&self) -> HashSet<String> {
        let mut live = self.live_session_ids();
        for (session_id, metadata) in self.session.metadata_entries() {
            if metadata.disconnected {
                live.insert(session_id.to_string());
            }
        }
        live
    }

    /// Rebuild leaf→tab-root ownership from `session_pane_roots`.
    pub(in crate::features) fn rebuild_session_tab_owners(&mut self) {
        self.shell.workspace.rebuild_tab_owners();
    }

    /// Expose the active tab's pane tree via `workspace_split` for existing renderers.
    pub(in crate::features) fn sync_workspace_split_from_active_tab(&mut self) {
        let Some(active) = self.session.active_id_owned() else {
            self.shell.workspace.split = None;
            self.sync_terminal_frame_snapshot_priority();
            return;
        };
        let tab_root = self.tab_root_for_session(&active);
        self.shell.workspace.split = self
            .shell
            .workspace
            .pane_roots
            .get(&tab_root)
            .filter(|root| root.is_split())
            .cloned();
        self.sync_terminal_frame_snapshot_priority();
    }

    fn write_back_active_tab_pane_root(&mut self) {
        let Some(active) = self.session.active_id_owned() else {
            return;
        };
        let tab_root = self.tab_root_for_session(&active);
        if let Some(root) = self.shell.workspace.split.clone()
            && root.is_split()
        {
            self.shell.workspace.pane_roots.insert(tab_root, root);
            self.rebuild_session_tab_owners();
        }
    }

    fn attach_workspace_split(
        &mut self,
        cx: &mut Context<Self>,
        direction: WorkspaceSplitDirection,
        primary_session_id: String,
        secondary_session_id: String,
    ) {
        let split_id = uuid();
        let tab_root = self.tab_root_for_session(&primary_session_id);
        if let Some(root) = self.shell.workspace.pane_roots.get_mut(&tab_root)
            && root.split_leaf(
                &primary_session_id,
                secondary_session_id.clone(),
                direction,
                split_id.clone(),
            )
        {
            self.rebuild_session_tab_owners();
            self.activate_session_id(&secondary_session_id, cx);
            self.sync_workspace_split_from_active_tab();
            self.shell.navigation.selected_nav = NavItem::Workspace;
            self.shell.navigation.main_mode = MainMode::Workspace;
            self.persist_workspace_pane_layout();
            if self.session.restore_is_complete() {
                self.persist_open_tabs();
            }
            return;
        }
        // Create a new per-tab dual split rooted at the primary session tab.
        let root = WorkspacePaneNode::Split {
            id: split_id,
            direction,
            ratio_percent: WorkspacePaneNode::DEFAULT_RATIO_PERCENT,
            first: Box::new(WorkspacePaneNode::leaf(primary_session_id.clone())),
            second: Box::new(WorkspacePaneNode::leaf(secondary_session_id.clone())),
        };
        self.shell.workspace.pane_roots.insert(tab_root, root);
        self.rebuild_session_tab_owners();
        self.activate_session_id(&secondary_session_id, cx);
        self.sync_workspace_split_from_active_tab();
        self.shell.navigation.selected_nav = NavItem::Workspace;
        self.shell.navigation.main_mode = MainMode::Workspace;
        self.persist_workspace_pane_layout();
        if self.session.restore_is_complete() {
            self.persist_open_tabs();
        }
    }

    pub(in crate::features) fn activate_workspace_pane(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.session.active_id() == Some(session_id.as_str()) {
            return;
        }
        self.activate_session_id_with_surface_sync(&session_id, cx);
        self.sync_workspace_split_from_active_tab();
        self.shell.navigation.selected_nav = NavItem::Workspace;
        self.shell.navigation.main_mode = MainMode::Workspace;
        self.shell
            .set_status(format!("focused pane {}", short_id(&session_id)));
        cx.notify();
    }

    pub(in crate::features) fn split_workspace_with_duplicate(
        &mut self,
        direction: WorkspaceSplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.start_has_active_pending() || self.session.start_has_active_failed() {
            self.shell
                .set_status("select a connected session before splitting".to_string());
            cx.notify();
            return;
        }
        let Some(source_session_id) = self.session.active_id_owned() else {
            self.shell
                .set_status("start a session before splitting".to_string());
            cx.notify();
            return;
        };
        if !self.session.has_session(&source_session_id) {
            self.shell
                .set_status("active session cannot be duplicated for split".to_string());
            cx.notify();
            return;
        }
        self.session
            .start_set_pending_workspace_split(direction, source_session_id);
        self.duplicate_active_session(window, cx);
    }

    pub(in crate::features) fn apply_workspace_split_for_duplicate(
        &mut self,
        cx: &mut Context<Self>,
        workspace_split: Option<(WorkspaceSplitDirection, String)>,
        new_session_id: &str,
    ) {
        let Some((direction, source_session_id)) = workspace_split else {
            return;
        };
        self.attach_workspace_split(cx, direction, source_session_id, new_session_id.to_string());
        self.shell.set_status(format!(
            "split {} pane duplicated",
            direction.label().to_lowercase()
        ));
    }

    pub(in crate::features) fn unsplit_workspace(&mut self, cx: &mut Context<Self>) {
        let Some(active_id) = self.session.active_id_owned() else {
            self.shell.set_status("workspace is not split".to_string());
            cx.notify();
            return;
        };
        let tab_root = self.tab_root_for_session(&active_id);
        let Some(root) = self.shell.workspace.pane_roots.remove(&tab_root) else {
            self.shell.workspace.split = None;
            self.shell.set_status("workspace is not split".to_string());
            cx.notify();
            return;
        };
        self.shell.workspace.split_resize = None;

        if let Some(collapsed) = collapse_around_session(root.clone(), &active_id) {
            match collapsed {
                WorkspacePaneNode::Split { .. } => {
                    self.shell.workspace.pane_roots.insert(tab_root, collapsed);
                    self.shell.set_status("collapsed focused split".to_string());
                }
                WorkspacePaneNode::Leaf { session_id } => {
                    self.activate_session_id_with_surface_sync(&session_id, cx);
                    self.shell.set_status("workspace split closed".to_string());
                }
            }
            self.rebuild_session_tab_owners();
            self.sync_workspace_split_from_active_tab();
            self.persist_workspace_pane_layout();
            if self.session.restore_is_complete() {
                self.persist_open_tabs();
            }
            cx.notify();
            return;
        }

        let _ = root;
        self.rebuild_session_tab_owners();
        self.sync_workspace_split_from_active_tab();
        self.shell.set_status("workspace split closed".to_string());
        self.persist_workspace_pane_layout();
        if self.session.restore_is_complete() {
            self.persist_open_tabs();
        }
        cx.notify();
    }

    pub(in crate::features) fn start_workspace_split_resize(
        &mut self,
        split_id: String,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        // Prefer multi-leaf tab-window splits when active; otherwise pane splits.
        let (direction, start_ratio) =
            if let Some(geometry) = self.terminal.terminal_window_split_geometry(&split_id) {
                geometry
            } else {
                let Some(root) = self.shell.workspace.split.as_ref() else {
                    return;
                };
                let Some(direction) = root.direction_for_split(&split_id) else {
                    return;
                };
                let Some(start_ratio) = root.ratio_for_split(&split_id) else {
                    return;
                };
                (direction, start_ratio)
            };
        let start_pos = match direction {
            WorkspaceSplitDirection::Horizontal => event.position.y,
            WorkspaceSplitDirection::Vertical => event.position.x,
        };
        self.shell.workspace.split_resize = Some(WorkspaceSplitResizeState {
            split_id,
            direction,
            start_pos,
            start_ratio,
            container_size: 0.,
        });
        self.shell
            .set_status("resizing workspace split".to_string());
        cx.notify();
    }

    pub(in crate::features) fn update_workspace_split_resize(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.shell.workspace.split_resize.clone() else {
            return;
        };
        let current = match state.direction {
            WorkspaceSplitDirection::Horizontal => event.position.y,
            WorkspaceSplitDirection::Vertical => event.position.x,
        };
        let delta_px = f32::from(current - state.start_pos);
        // Approximate container size from a stable heuristic when unknown: treat 4px ~ 1%.
        let container = if state.container_size > 1. {
            state.container_size
        } else {
            400.
        };
        let delta_ratio = ((delta_px / container) * 100.).round() as i16;
        let next = (state.start_ratio as i16 + delta_ratio).clamp(
            WorkspacePaneNode::MIN_RATIO_PERCENT as i16,
            WorkspacePaneNode::MAX_RATIO_PERCENT as i16,
        ) as u8;
        let mut applied = self
            .terminal
            .set_terminal_window_split_ratio(&state.split_id, next);
        if !applied
            && let Some(root) = self.shell.workspace.split.as_mut()
            && root.set_ratio_for_split(&state.split_id, next)
        {
            applied = true;
            self.write_back_active_tab_pane_root();
        }
        if applied {
            self.shell.set_status(format!("split ratio {next}%"));
            cx.notify();
        }
    }

    pub(in crate::features) fn finish_workspace_split_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.shell.workspace.split_resize.take() {
            let ratio = self
                .terminal
                .terminal_window_split_geometry(&state.split_id)
                .map(|(_, ratio)| ratio)
                .or_else(|| {
                    self.shell
                        .workspace
                        .split
                        .as_ref()
                        .and_then(|root| root.ratio_for_split(&state.split_id))
                });
            if let Some(ratio) = ratio {
                self.shell
                    .set_status(format!("split ratio set to {ratio}%"));
            }
            if self.terminal_windows_is_multi_leaf() {
                self.persist_terminal_window_layout();
            } else if self
                .shell
                .workspace
                .split
                .as_ref()
                .is_some_and(|root| root.is_split())
            {
                self.write_back_active_tab_pane_root();
                self.persist_workspace_pane_layout();
            }
            cx.notify();
        }
    }

    pub(in crate::features) fn workspace_split_resize_handle(
        &self,
        split_id: String,
        direction: WorkspaceSplitDirection,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let dragging = self
            .shell
            .workspace
            .split_resize
            .as_ref()
            .is_some_and(|resize| resize.split_id == split_id);
        let id = SharedString::from(format!("workspace-split-resize-{split_id}"));
        let hover_id = id.clone();
        let drag_id = id.clone();
        match direction {
            WorkspaceSplitDirection::Horizontal => deferred(
                horizontal_resize_handle_visual(
                    palette,
                    dragging,
                    self.shell.resize_handle_is_highlighted(&id),
                )
                .id(id.clone())
                .cursor_row_resize()
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.update_resize_handle_hover(hover_id.clone(), *hovered, cx);
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.activate_resize_handle_immediately(drag_id.clone(), cx);
                        this.start_workspace_split_resize(split_id.clone(), event, cx);
                    }),
                ),
            )
            .into_any_element(),
            WorkspaceSplitDirection::Vertical => deferred(
                vertical_resize_handle_visual(
                    palette,
                    dragging,
                    self.shell.resize_handle_is_highlighted(&id),
                )
                .id(id.clone())
                .cursor_col_resize()
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.update_resize_handle_hover(hover_id.clone(), *hovered, cx);
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.activate_resize_handle_immediately(drag_id.clone(), cx);
                        this.start_workspace_split_resize(split_id.clone(), event, cx);
                    }),
                ),
            )
            .into_any_element(),
        }
    }
    pub(in crate::features) fn persist_workspace_pane_layout(&mut self) {
        if !self.settings.summary().startup_restore
            || !self.settings.summary().startup_restore_window_layout
        {
            return;
        }
        if !self.session.restore_is_complete() {
            return;
        }
        self.shell.mark_window_layout_persist_dirty();
    }

    /// Install a restored pane tree, activating a session in it if the current one is
    /// not part of it. Reports whether the tree was actually installed.
    ///
    /// Split out of `try_restore_workspace_pane_layout`'s store callback so the
    /// activation behaviour is testable without a store round trip.
    ///
    /// **Activation goes through `activate_session_id`, not
    /// `session.select_active_session`.** This path used to call the latter directly,
    /// which skipped the entire session-switch protocol: releasing the previous
    /// session's RDP keys, caching its transfer browser, resetting terminal assist and
    /// credential-autofill state, resetting the transfer queue interaction, and
    /// resetting the remote runtime. The remote one is the visible bug -- restoring a
    /// split layout that changed the active host left the previous host's stats, GPU,
    /// NPU, process and Docker data on screen.
    ///
    /// Ordering matters and is deliberate: the pane root is inserted *before*
    /// activating, because `activate_session_id` ends with
    /// `sync_workspace_split_from_active_tab`, which derives the visible split from
    /// `pane_roots`. Activating first would sync against a tree that does not contain
    /// the restored root yet. Both that helper and `rebuild_session_tab_owners` are pure
    /// derives, so the trailing sync here is idempotent and covers the branch where no
    /// activation happens.
    fn apply_restored_workspace_pane_layout(
        &mut self,
        restored: WorkspacePaneNode,
        previously_active: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !restored.is_split() {
            return false;
        }
        let Some(first) = restored.session_ids().into_iter().next() else {
            return false;
        };
        let needs_activation =
            previously_active.is_none_or(|active| !restored.contains_session(active));

        self.shell
            .workspace
            .pane_roots
            .insert(first.clone(), restored);
        self.rebuild_session_tab_owners();
        if needs_activation {
            self.activate_session_id(&first, cx);
        }
        self.sync_workspace_split_from_active_tab();
        true
    }

    pub(in crate::features) fn try_restore_workspace_pane_layout(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.shell.workspace.pane_layout_restored {
            return;
        }
        if !self.settings.summary().startup_restore
            || !self.settings.summary().startup_restore_window_layout
        {
            self.shell.workspace.pane_layout_restored = true;
            return;
        }
        // Multi-leaf tab windows take visual precedence; skip pane restore when active.
        if self.terminal_windows_is_multi_leaf() {
            self.shell.workspace.pane_layout_restored = true;
            return;
        }
        let ordered = self
            .session
            .ordered_sessions()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        if ordered.len() < 2 {
            // After startup finishes, don't keep waiting forever for a second tab.
            if self.session.restore_is_complete() {
                self.shell.workspace.pane_layout_restored = true;
            }
            return;
        }
        self.shell.workspace.pane_layout_restored = true;
        let active = self.session.active_id_owned();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Sessions, |store| {
                store.load_workspace_pane_layout()
            }),
            move |this, event, cx| {
                let layout = match event.outcome {
                    Ok(Some(layout)) => layout,
                    Ok(None) => return,
                    Err(error) => {
                        this.shell
                            .set_status(format!("failed to restore pane layout: {error}"));
                        cx.notify();
                        return;
                    }
                };
                let Some(restored) = WorkspacePaneNode::restore_layout(&layout, &ordered) else {
                    return;
                };
                if !this.apply_restored_workspace_pane_layout(restored, active.as_deref(), cx) {
                    return;
                }
                this.shell.navigation.selected_nav = NavItem::Workspace;
                this.shell.navigation.main_mode = MainMode::Workspace;
                this.shell
                    .set_status("restored workspace pane layout".to_string());
                cx.notify();
            },
            cx,
        );
    }
}

/// Keep only the branch that contains `session_id`, collapsing every split on the path
/// into that single branch (closes the sibling panes of the active leaf).
fn collapse_around_session(node: WorkspacePaneNode, session_id: &str) -> Option<WorkspacePaneNode> {
    match node {
        WorkspacePaneNode::Leaf { session_id: id } => {
            if id == session_id {
                Some(WorkspacePaneNode::Leaf { session_id: id })
            } else {
                None
            }
        }
        WorkspacePaneNode::Split { first, second, .. } => {
            let in_first = first.contains_session(session_id);
            let in_second = second.contains_session(session_id);
            if in_first && !in_second {
                collapse_around_session(*first, session_id)
            } else if in_second && !in_first {
                collapse_around_session(*second, session_id)
            } else if in_first && in_second {
                // Should not happen for unique session leaves; keep first match.
                collapse_around_session(*first, session_id)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod restore_activation_tests {
    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AiExecutionProfile, AppRuntime, RuntimeMode, uuid};
    use nyaterm_transport::{LocalSessionConfig, RemoteStats, SshSessionConfig};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::{
        SessionLaunchConfig, SessionRuntimeMetadata, WorkspacePaneNode, WorkspaceSplitDirection,
    };

    fn app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-restore-activation-{}-{}",
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

    fn register_ssh_session(app: &mut NyaTermApp, session_id: &str) {
        app.session.register_session_metadata(
            session_id,
            SessionRuntimeMetadata {
                ssh_config: Some(SshSessionConfig::default()),
                ssh_multiplex_key: None,
                source_connection_id: None,
                ai_execution_profile: AiExecutionProfile::Posix,
                launch_config: SessionLaunchConfig::Local(LocalSessionConfig::default()),
                disconnected: false,
            },
        );
    }

    fn split(first: &str, second: &str) -> WorkspacePaneNode {
        WorkspacePaneNode::Split {
            id: "split-1".to_string(),
            direction: WorkspaceSplitDirection::Horizontal,
            ratio_percent: WorkspacePaneNode::DEFAULT_RATIO_PERCENT,
            first: Box::new(WorkspacePaneNode::leaf(first)),
            second: Box::new(WorkspacePaneNode::leaf(second)),
        }
    }

    /// Restoring a layout that switches hosts must clear the remote presentation.
    ///
    /// This path used to call `session.select_active_session` directly, so none of the
    /// session-switch protocol ran -- the previous host stats, GPU and NPU data stayed on
    /// screen under a different session. The revision assertions are the load-bearing
    /// half: `reset_for_session_switch` clearing the data is not enough on its own,
    /// because the snapshot flush keys on the revision moving.
    #[test]
    fn workspace_restore_switching_hosts_clears_the_remote_presentation() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            register_ssh_session(app, "host-a");
            register_ssh_session(app, "host-b");
            register_ssh_session(app, "host-c");
            app.session.select_active_session("host-a".to_string());

            // Host A data on screen.
            app.remote_ops.apply_stats(RemoteStats::default());
            app.remote_ops.set_stats_status("loaded stats for host-a");
            assert!(app.remote_ops.stats_presentation().data.is_some());
            let before = (
                app.remote_ops.stats_revision(),
                app.remote_ops.gpu_revision(),
                app.remote_ops.npu_revision(),
            );

            // A layout that does not contain host A forces an activation.
            let restored = split("host-b", "host-c");
            assert!(app.apply_restored_workspace_pane_layout(restored, Some("host-a"), cx));

            assert_eq!(
                app.session.active_id(),
                Some("host-b"),
                "the restored layout has to take the active session with it"
            );
            assert!(
                app.remote_ops.stats_presentation().data.is_none(),
                "host A stats must not survive a switch to host B"
            );
            let after = (
                app.remote_ops.stats_revision(),
                app.remote_ops.gpu_revision(),
                app.remote_ops.npu_revision(),
            );
            assert_ne!(before.0, after.0, "the stats pane revision must advance");
            assert_ne!(before.1, after.1, "the GPU pane revision must advance");
            assert_ne!(before.2, after.2, "the NPU pane revision must advance");
        });
    }

    /// A layout that already contains the active session is not a switch, so nothing
    /// is reset. Without this the test above would pass for a version that reset the
    /// remote runtime unconditionally, which would throw away live data on every
    /// restore.
    #[test]
    fn workspace_restore_keeping_the_active_host_preserves_the_remote_presentation() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            register_ssh_session(app, "host-a");
            register_ssh_session(app, "host-b");
            app.session.select_active_session("host-a".to_string());
            app.remote_ops.apply_stats(RemoteStats::default());
            let before = app.remote_ops.stats_revision();

            let restored = split("host-a", "host-b");
            assert!(app.apply_restored_workspace_pane_layout(restored, Some("host-a"), cx));

            assert_eq!(app.session.active_id(), Some("host-a"));
            assert!(
                app.remote_ops.stats_presentation().data.is_some(),
                "no host change means no reset"
            );
            assert_eq!(app.remote_ops.stats_revision(), before);
        });
    }
}
