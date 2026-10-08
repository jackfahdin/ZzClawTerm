use crate::features::ZzClawTermApp;
use crate::models::TerminalWindowNode;
use gpui::{Context, Window};
use zzclawterm_transport::SessionInfo;

enum GroupTabTarget {
    Session(String),
    Pending(String),
    Failed(String),
}

impl ZzClawTermApp {
    fn terminal_group_tab_targets(&self) -> Vec<GroupTabTarget> {
        let group = self.current_terminal_group();
        let belongs = |placement: Option<crate::features::session::SessionStartTabPlacement>| {
            group.is_none()
                || placement
                    .and_then(|p| {
                        self.terminal
                            .terminal_start_display_group(p.request_sequence)
                    })
                    .or_else(|| self.terminal.first_terminal_group())
                    == group
        };
        let mut transient = self
            .session
            .start_pending_entries()
            .filter(|(_, pending)| {
                pending.reconnect_session_id.is_none() && belongs(pending.tab_placement)
            })
            .map(|(id, pending)| {
                let key = pending
                    .tab_placement
                    .map(|p| (p.insert_index, p.request_sequence))
                    .unwrap_or((usize::MAX, u64::MAX));
                (key, GroupTabTarget::Pending(id.clone()))
            })
            .chain(
                self.session
                    .start_failed_entries()
                    .filter(|(_, failed)| belongs(failed.pending.tab_placement))
                    .map(|(id, failed)| {
                        let key = failed
                            .pending
                            .tab_placement
                            .map(|p| (p.insert_index, p.request_sequence))
                            .unwrap_or((usize::MAX, u64::MAX));
                        (key, GroupTabTarget::Failed(id.clone()))
                    }),
            )
            .collect::<Vec<_>>();
        transient.sort_by_key(|(key, _)| *key);
        let mut transient = transient.into_iter().peekable();
        let mut targets = Vec::new();
        for (index, session) in self
            .active_terminal_group_sessions()
            .into_iter()
            .enumerate()
        {
            let key = self
                .session
                .session_start_tab_placement(&session.id)
                .map(|p| (p.insert_index, p.request_sequence))
                .unwrap_or((index, u64::MAX));
            while transient.peek().is_some_and(|(next, _)| *next < key) {
                targets.push(transient.next().unwrap().1);
            }
            targets.push(GroupTabTarget::Session(session.id));
        }
        targets.extend(transient.map(|(_, target)| target));
        targets
    }

    pub(in crate::features) fn select_terminal_group_tab(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let targets = self.terminal_group_tab_targets();
        let Some(target) = targets.get(index.min(targets.len().saturating_sub(1))) else {
            return;
        };
        match target {
            GroupTabTarget::Session(id) => self.select_session(id.clone(), cx),
            GroupTabTarget::Pending(id) => self.select_pending_session_start(id.clone(), cx),
            GroupTabTarget::Failed(id) => self.select_failed_session_start(id.clone(), cx),
        }
        self.sync_terminal_frame_snapshot_priority();
    }

    pub(in crate::features) fn select_relative_terminal_group_tab(
        &mut self,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let targets = self.terminal_group_tab_targets();
        if targets.is_empty() {
            return;
        }
        let current = targets
            .iter()
            .position(|target| match target {
                GroupTabTarget::Session(id) => {
                    !self.session.start_has_active_pending()
                        && !self.session.start_has_active_failed()
                        && self.session.active_id() == Some(id.as_str())
                }
                GroupTabTarget::Pending(id) | GroupTabTarget::Failed(id) => {
                    self.session.start_request_is_active(id)
                }
            })
            .unwrap_or(0);
        let next = (current as isize + offset).rem_euclid(targets.len() as isize) as usize;
        self.select_terminal_group_tab(next, cx);
    }
    pub(in crate::features) fn terminal_group_tab_count(&self) -> usize {
        self.terminal_group_tab_targets().len()
    }
    pub(in crate::features) fn terminal_group_can_split(
        &self,
        group: &str,
        direction: crate::models::WorkspaceSplitDirection,
    ) -> bool {
        let Some(bounds) = self.terminal.terminal_split_bounds(group) else {
            return true;
        };
        match direction {
            crate::models::WorkspaceSplitDirection::Horizontal => {
                f32::from(bounds.size.height) >= 324.
            }
            crate::models::WorkspaceSplitDirection::Vertical => {
                f32::from(bounds.size.width) >= 484.
            }
        }
    }

    pub(in crate::features) fn current_terminal_group(&self) -> Option<String> {
        self.session
            .start_pending_entries()
            .filter(|(id, _)| self.session.start_request_is_active(id))
            .find_map(|(_, pending)| {
                pending.tab_placement.and_then(|placement| {
                    self.terminal
                        .terminal_start_display_group(placement.request_sequence)
                })
            })
            .or_else(|| {
                self.session
                    .start_failed_entries()
                    .filter(|(id, _)| self.session.start_request_is_active(id))
                    .find_map(|(_, failed)| {
                        failed.pending.tab_placement.and_then(|placement| {
                            self.terminal
                                .terminal_start_display_group(placement.request_sequence)
                        })
                    })
            })
            .or_else(|| {
                self.session
                    .active_id()
                    .and_then(|id| self.terminal.terminal_group_for_tab(id))
            })
    }

    pub(in crate::features) fn active_terminal_group_sessions(&self) -> Vec<SessionInfo> {
        if let Some(group) = self.current_terminal_group()
            && let Some((tabs, _)) = self.terminal.terminal_group_tabs(&group)
        {
            return tabs
                .iter()
                .filter_map(|id| self.session.session_info(id))
                .collect();
        }
        self.ordered_tab_sessions()
    }

    pub(crate) fn focus_terminal_session(
        &self,
        session: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.remote_desktop.is_session(session) {
            window.focus(self.remote_desktop.focus(), cx);
        } else {
            window.focus(self.terminal.input_focus(), cx);
        }
    }

    pub(in crate::features) fn select_relative_terminal_group(
        &mut self,
        offset: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.terminal.terminal_window_tree() else {
            return;
        };
        let groups = root.leaf_ids();
        if groups.is_empty() {
            return;
        }
        let index = self
            .current_terminal_group()
            .and_then(|id| groups.iter().position(|group| group == &id))
            .unwrap_or(0);
        let next = (index as isize + offset).rem_euclid(groups.len() as isize) as usize;
        if let Some((_, Some(tab))) = self.terminal.terminal_group_tabs(&groups[next]) {
            self.select_session(tab.clone(), cx);
            self.focus_terminal_session(&tab, window, cx);
        }
    }

    /// Compatibility data is expanded only after both restoration sources have
    /// settled, so old outer-layout indexes still address their original tabs.
    pub(in crate::features) fn normalize_legacy_terminal_groups(&mut self) -> bool {
        if !self.session.restore_is_complete()
            || !self.shell.workspace.pane_layout_restored
            || self.shell.workspace.pane_layout_loading
            || !self.terminal.terminal_windows_restore_is_complete()
        {
            return false;
        }
        if self.shell.workspace.pane_roots.is_empty() {
            return true;
        }
        let active = self.session.active_id_owned();
        let ordered = self
            .ordered_tab_sessions()
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        let mut root = self
            .terminal
            .terminal_window_tree()
            .unwrap_or_else(|| TerminalWindowNode::leaf(ordered.clone(), active.clone()));
        let secondary = self
            .session
            .session_order()
            .iter()
            .filter(|id| self.is_secondary_pane_session(id))
            .cloned()
            .collect::<Vec<_>>();
        for tab in secondary {
            if let Some(next) = root.remove_tab(&tab) {
                root = next;
            } else {
                return false;
            }
        }
        let mut properties = Vec::new();
        for tab in ordered {
            if let Some(panes) = self.shell.workspace.pane_roots.get(&tab) {
                if !root.expand_pane_tab(&tab, panes) {
                    return false;
                }
                properties.push((tab, panes.session_ids()));
            }
        }
        let mut actual = root.collect_tab_ids();
        let mut expected = self
            .session
            .ordered_sessions()
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        if actual != expected || actual.windows(2).any(|ids| ids[0] == ids[1]) {
            return false;
        }
        for (tab, sessions) in properties {
            let name = self.session.custom_name(&tab).map(str::to_string);
            let color = self.session.tab_color(&tab);
            let locked = self.session.tab_is_locked(&tab);
            for session in sessions {
                if let Some(name) = &name {
                    self.session.set_custom_name(session.clone(), name.clone());
                }
                if let Some(color) = color {
                    self.session.set_tab_color(&session, Some(color));
                }
                if locked {
                    self.session.set_tab_locked(&session, true);
                }
            }
        }
        if let Some(active) = active {
            root.set_active_tab(&active);
        }
        self.terminal.restore_terminal_window_tree(Some(root));
        self.shell.workspace.pane_roots.clear();
        self.shell.workspace.tab_owner.clear();
        self.shell.workspace.split = None;
        self.sync_terminal_frame_snapshot_priority();
        self.persist_open_tabs();
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::session::PendingSessionStart;
    use crate::features::{ZzClawTermApp, session::SavedConnectionStartOptions};
    use crate::models::{
        SessionLaunchConfig, SessionRuntimeMetadata, SmartSplitMode, TabDockEdge, TabDockZone,
        TerminalWindowNode, WorkspacePaneNode, WorkspaceSplitDirection,
    };
    use gpui::{AppContext as _, Bounds, Entity, TestAppContext, point, px, size};
    use std::time::Instant;
    use zzclawterm_core::{AiExecutionProfile, AppRuntime, RuntimeMode, test_support::TestTempDir};
    use zzclawterm_transport::LocalSessionConfig;
    use zzclawterm_transport::SessionKind;

    fn pending(
        placement: Option<crate::features::session::SessionStartTabPlacement>,
    ) -> PendingSessionStart {
        PendingSessionStart {
            attempt: Default::default(),
            connection_name: "new local".into(),
            launch_config: None,
            requested_at: Instant::now(),
            kind: SessionKind::LocalPty,
            ai_execution_profile: AiExecutionProfile::Posix,
            custom_name: None,
            tab_color: None,
            locked: false,
            after_session_id: None,
            insert_index: None,
            seed_output: None,
            startup_command: None,
            multiplex_key: None,
            source_connection_id: None,
            workspace_split: None,
            tab_placement: placement,
            reconnect_session_id: None,
        }
    }

    #[test]
    fn failed_start_only_hides_its_group_and_preserves_layout() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            app.apply_smart_split(SmartSplitMode::Vertical, cx);
            let before = app.terminal.terminal_window_tree();
            let group = app.current_terminal_group().unwrap();
            let options = app.prepare_session_start_options(SavedConnectionStartOptions::default());
            app.session
                .simulate_pending_start("pending", pending(options.tab_placement));
            assert_eq!(app.current_terminal_group(), Some(group.clone()));
            assert!(!app.visible_terminal_session_ids().contains(&"a"));
            assert!(app.visible_terminal_session_ids().contains(&"b"));
            app.session.simulate_start_failure("pending");
            assert_eq!(app.current_terminal_group(), Some(group));
            assert_eq!(app.terminal.terminal_window_tree(), before);
            app.select_relative_session(1, cx);
            assert_eq!(app.session.active_id(), Some("a"));
            assert!(!app.session.start_has_active_failed());
            app.select_relative_session(1, cx);
            assert!(app.session.start_has_active_failed());
            app.select_session("b".into(), cx);
            assert_eq!(app.visible_terminal_session_ids().len(), 3);
        });
    }

    #[test]
    fn removing_source_group_does_not_retarget_an_in_flight_split() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            app.apply_smart_split(SmartSplitMode::Vertical, cx);
            let original = app.current_terminal_group().unwrap();
            let options = app.prepare_session_start_options(SavedConnectionStartOptions::default());
            app.close_session("a".into(), cx);
            assert!(!app.terminal.terminal_window_has_leaf(&original));
            assert_eq!(
                app.terminal
                    .terminal_start_group(options.tab_placement.unwrap().request_sequence),
                Some(original.as_str())
            );
            app.register_session_for_start(
                "new",
                SessionRuntimeMetadata {
                    ssh_config: None,
                    ssh_multiplex_key: None,
                    source_connection_id: None,
                    ai_execution_profile: AiExecutionProfile::Posix,
                    launch_config: SessionLaunchConfig::Local(LocalSessionConfig::default()),
                    disconnected: true,
                },
                options.tab_placement,
                None,
            );
            let before = app.terminal.terminal_window_tree();
            app.apply_workspace_split_for_duplicate(
                cx,
                Some((WorkspaceSplitDirection::Horizontal, "a".into())),
                "new",
            );
            assert_eq!(app.terminal.terminal_window_tree(), before);
            assert!(app.session.has_session("new"));
        });
    }

    #[test]
    fn reordered_and_merged_tabs_serialize_in_their_visual_order() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            app.reorder_session_relative("c".into(), "a".into(), false, cx);
            assert_eq!(
                app.ordered_tab_sessions()
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>(),
                ["c", "a", "b"]
            );
            let serialized = app.serialize_open_tabs();
            assert_eq!(serialized.len(), 3);
            assert_eq!(serialized[0].title, app.session.display_name("c").unwrap());
            app.apply_smart_split(SmartSplitMode::Auto, cx);
            app.unsplit_workspace(cx);
            assert_eq!(app.terminal.terminal_window_tab_ids(), ["c", "a", "b"]);
        });
    }

    #[test]
    fn background_connection_completion_keeps_the_input_session_visible_in_its_group() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            let group = app.current_terminal_group().unwrap();
            let options = app.prepare_session_start_options(SavedConnectionStartOptions::default());
            app.select_session("b".into(), cx);
            app.register_session_for_start(
                "new",
                SessionRuntimeMetadata {
                    ssh_config: None,
                    ssh_multiplex_key: None,
                    source_connection_id: None,
                    ai_execution_profile: AiExecutionProfile::Posix,
                    launch_config: SessionLaunchConfig::Local(LocalSessionConfig::default()),
                    disconnected: true,
                },
                options.tab_placement,
                None,
            );
            assert_eq!(app.session.active_id(), Some("b"));
            assert_eq!(
                app.terminal
                    .terminal_group_tabs(&group)
                    .unwrap()
                    .1
                    .as_deref(),
                Some("b")
            );
            assert_eq!(app.visible_terminal_session_ids(), ["b"]);
            assert_eq!(app.terminal.terminal_group_for_tab("new"), Some(group));
        });
    }

    fn fixture(cx: &mut TestAppContext) -> (Entity<ZzClawTermApp>, TestTempDir) {
        let root = TestTempDir::new("zzclawterm-terminal-groups");
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.path().to_path_buf(),
            root.path().join("config"),
            root.path().join("logs"),
            root.path().join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        let app = cx.new(|cx| ZzClawTermApp::new(runtime, stores, cx));
        app.update(cx, |app, _| {
            app.session.mark_restore_complete();
            app.shell.set_workspace_pane_layout_restored(true);
            app.terminal.complete_terminal_windows_restore();
            app.shell.show_workspace();
            for id in ["a", "b", "c"] {
                register(app, id);
            }
            app.session.select_active_session("a");
            app.reconcile_terminal_windows();
        });
        (app, root)
    }

    fn register(app: &mut ZzClawTermApp, id: &str) {
        app.session.register_session_metadata(
            id,
            SessionRuntimeMetadata {
                ssh_config: None,
                ssh_multiplex_key: None,
                source_connection_id: None,
                ai_execution_profile: AiExecutionProfile::Posix,
                launch_config: SessionLaunchConfig::Local(LocalSessionConfig::default()),
                disconnected: true,
            },
        );
    }

    #[test]
    fn starts_and_duplicate_splits_keep_the_group_reserved_before_focus_changes() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            app.apply_smart_split(SmartSplitMode::Vertical, cx);
            let original = app.terminal.terminal_group_for_tab("a").unwrap();
            let options = app.prepare_session_start_options(SavedConnectionStartOptions::default());
            app.select_session("b".into(), cx);
            app.register_session_for_start(
                "new",
                SessionRuntimeMetadata {
                    ssh_config: None,
                    ssh_multiplex_key: None,
                    source_connection_id: None,
                    ai_execution_profile: AiExecutionProfile::Posix,
                    launch_config: SessionLaunchConfig::Local(LocalSessionConfig::default()),
                    disconnected: true,
                },
                options.tab_placement,
                None,
            );
            assert_eq!(
                app.terminal.terminal_group_for_tab("new"),
                Some(original.clone())
            );
            app.apply_workspace_split_for_duplicate(
                cx,
                Some((WorkspaceSplitDirection::Horizontal, "a".into())),
                "new",
            );
            assert_ne!(app.terminal.terminal_group_for_tab("new"), Some(original));
            assert!(app.terminal.terminal_windows_is_multi_leaf());
            assert_eq!(app.session.active_id(), Some("b"));
            assert!(app.visible_terminal_session_ids().contains(&"b"));
        });
    }

    #[test]
    fn closing_tabs_and_merging_groups_preserves_other_connections_and_properties() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            let original = app.current_terminal_group().unwrap();
            app.dock_tab_on_terminal_window_leaf(
                "c".into(),
                original.clone(),
                TabDockZone::Edge(TabDockEdge::Bottom),
                cx,
            );
            app.select_session("b".into(), cx);
            app.close_session("b".into(), cx);
            assert_eq!(app.session.active_id(), Some("a"));
            app.session.set_tab_locked("c", true);
            app.close_session("c".into(), cx);
            assert!(app.session.has_session("c"));
            app.session.set_tab_color("c", Some(0x123456));
            app.unsplit_workspace(cx);
            assert!(!app.terminal_windows_is_multi_leaf());
            assert_eq!(
                app.terminal
                    .terminal_window_tree()
                    .unwrap()
                    .collect_tab_ids(),
                ["a", "c"]
            );
            assert_eq!(app.session.tab_color("c"), Some(0x123456));
            assert!(app.session.tab_is_locked("c"));
            assert_eq!(app.session.active_id(), Some("a"));
        });
    }

    #[test]
    fn keyboard_tab_switching_and_focus_mode_use_the_visible_group() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            let group = app.current_terminal_group().unwrap();
            app.dock_tab_on_terminal_window_leaf(
                "c".into(),
                group,
                TabDockZone::Edge(TabDockEdge::Bottom),
                cx,
            );
            app.select_session("a".into(), cx);
            app.select_relative_session(1, cx);
            assert_eq!(app.session.active_id(), Some("b"));
            app.select_relative_session(1, cx);
            assert_eq!(app.session.active_id(), Some("a"));
            app.select_session_index(99, cx);
            assert_eq!(app.session.active_id(), Some("b"));
            assert_eq!(app.visible_terminal_session_ids().len(), 2);
            app.shell.set_pane_focus_mode(true);
            assert_eq!(app.visible_terminal_session_ids(), ["b"]);
            app.shell.set_pane_focus_mode(false);
            assert_eq!(app.visible_terminal_session_ids().len(), 2);
        });
    }

    #[test]
    fn old_nested_tabs_convert_without_losing_sessions_or_their_outer_position() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            let panes = WorkspacePaneNode::Split {
                id: "legacy".into(),
                direction: WorkspaceSplitDirection::Horizontal,
                ratio_percent: 37,
                first: Box::new(WorkspacePaneNode::leaf("a")),
                second: Box::new(WorkspacePaneNode::leaf("b")),
            };
            app.shell.insert_workspace_pane_root("a".into(), panes);
            app.session
                .set_custom_name("a".into(), "legacy name".into());
            app.session.set_tab_locked("a", true);
            let outer = TerminalWindowNode::tile_in_bounds(
                &["a".into(), "c".into()],
                SmartSplitMode::Vertical,
                1000.,
                700.,
            )
            .unwrap();
            app.terminal.restore_terminal_window_tree(Some(outer));
            app.select_session("b".into(), cx);
            app.normalize_legacy_terminal_groups();
            assert!(app.shell.workspace.pane_roots.is_empty());
            assert_eq!(
                app.terminal
                    .terminal_window_tree()
                    .unwrap()
                    .collect_tab_ids(),
                ["a", "b", "c"]
            );
            assert_eq!(app.session.active_id(), Some("b"));
            assert!(app.session.tab_is_locked("b"));
            assert_eq!(app.session.custom_name("b"), Some("legacy name"));
            let saved = app
                .terminal
                .serialize_terminal_window_layout(&["a".into(), "b".into(), "c".into()])
                .unwrap();
            let restored =
                TerminalWindowNode::restore_layout(&saved, &["a".into(), "b".into(), "c".into()])
                    .unwrap();
            assert_eq!(restored.collect_tab_ids(), ["a", "b", "c"]);
        });
    }

    #[test]
    fn invalid_legacy_conversion_preserves_original_data() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, _| {
            let panes = WorkspacePaneNode::Split {
                id: "legacy".into(),
                direction: WorkspaceSplitDirection::Horizontal,
                ratio_percent: 50,
                first: Box::new(WorkspacePaneNode::leaf("a")),
                second: Box::new(WorkspacePaneNode::leaf("missing")),
            };
            app.shell
                .insert_workspace_pane_root("a".into(), panes.clone());
            let before = app.terminal.terminal_window_tree();
            app.normalize_legacy_terminal_groups();
            assert_eq!(app.terminal.terminal_window_tree(), before);
            assert_eq!(app.shell.workspace_pane_root("a"), Some(&panes));
        });
    }

    #[test]
    fn resizing_uses_the_local_split_extent_instead_of_a_fixed_viewport_estimate() {
        let mut cx = TestAppContext::single();
        let (app, _root) = fixture(&mut cx);
        app.update(&mut cx, |app, cx| {
            let group = app.current_terminal_group().unwrap();
            app.dock_tab_on_terminal_window_leaf(
                "c".into(),
                group,
                TabDockZone::Edge(TabDockEdge::Right),
                cx,
            );
            let TerminalWindowNode::Split { id, .. } = app.terminal.terminal_window_tree().unwrap()
            else {
                panic!("split");
            };
            app.terminal.record_terminal_split_bounds(
                id.clone(),
                Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1004.), px(600.)),
                },
            );
            app.start_workspace_split_resize(
                id.clone(),
                &gpui::MouseDownEvent {
                    position: point(px(500.), px(0.)),
                    ..Default::default()
                },
                cx,
            );
            app.update_workspace_split_resize(
                &gpui::MouseMoveEvent {
                    position: point(px(600.), px(0.)),
                    ..Default::default()
                },
                cx,
            );
            assert_eq!(
                app.terminal.terminal_window_split_geometry(&id).unwrap().1,
                60
            );
        });
    }
}
