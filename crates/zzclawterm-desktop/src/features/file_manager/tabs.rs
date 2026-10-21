use gpui::{Context, div, prelude::*};
use rust_i18n::t;
use zzclawterm_core::{FileManagerPaneRestore, FileManagerPaneSide, FileManagerSourceRestore};
use zzclawterm_transport::RemoteFilePath;

use super::pane::{PaneEndpoint, PaneId};
use super::state::FileManagerSidebarRestore;
use super::workspace::{FileManagerSeed, FileManagerWorkspace};
use crate::features::ZzClawTermApp;
use crate::models::{MainMode, NavItem, PanelSide, WorkspaceTabId};

impl ZzClawTermApp {
    pub(in crate::features) fn open_file_manager_tab(
        &mut self,
        seed: Option<FileManagerSeed>,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.file_manager.active_id().map(str::to_string) {
            let key = WorkspaceTabId::FileManager(id).layout_key();
            if self.workspace_tab_ids().contains(&key) {
                self.activate_workspace_tab(&key, cx);
                return;
            }
        }
        let id = self.file_manager.next_id();
        let seed = seed.unwrap_or_else(|| self.default_file_manager_seed());
        let app = cx.entity();
        let workspace = cx.new(|cx| FileManagerWorkspace::new(app, id.clone(), seed, cx));
        workspace.update(cx, |workspace, _| {
            workspace.restore_settings(&self.settings.summary().ui_file_manager);
        });
        self.file_manager.insert(id.clone(), workspace);
        let key = WorkspaceTabId::FileManager(id.clone()).layout_key();
        let current = self.active_workspace_tab();
        self.ensure_terminal_windows_root();
        if let Some(mut tree) = self.shell.terminal_window_tree() {
            let group = current
                .as_deref()
                .and_then(|key| tree.leaf_for_tab(key))
                .map(str::to_string)
                .or_else(|| tree.first_leaf_id());
            tree.ensure_tab(&key, group.as_deref());
            self.shell.restore_terminal_window_tree(Some(tree));
        }
        self.activate_workspace_tab(&key, cx);
        self.refresh_file_manager_pane(id.clone(), PaneId::Left, cx);
        self.refresh_file_manager_pane(id, PaneId::Right, cx);
        self.persist_file_manager_settings(cx);
        cx.notify();
    }

    pub(in crate::features) fn default_file_manager_seed(&self) -> FileManagerSeed {
        let restore = &self.settings.summary().ui_file_manager;
        let (right, right_path) = self
            .seed_from_browser()
            .unwrap_or_else(|| self.resolve_pane_restore(&restore.right));
        let (left, left_path) = self.resolve_pane_restore(&restore.left);
        FileManagerSeed {
            left,
            left_path,
            right,
            right_path,
        }
    }

    fn seed_from_browser(&self) -> Option<(PaneEndpoint, Option<RemoteFilePath>)> {
        let session_id = self.session.active_id()?;
        self.session
            .file_browser_backend_support_for_session(session_id)?;
        let path = self.transfer.browser_view().path.clone();
        let path = (!path.is_empty()).then(|| RemoteFilePath::new(path));
        Some((PaneEndpoint::Session(session_id.to_string()), path))
    }

    fn resolve_pane_restore(
        &self,
        restore: &FileManagerPaneRestore,
    ) -> (PaneEndpoint, Option<RemoteFilePath>) {
        let path = (!restore.path.is_empty()).then(|| RemoteFilePath::new(restore.path.clone()));
        match &restore.source {
            FileManagerSourceRestore::Local => (PaneEndpoint::Local, path),
            FileManagerSourceRestore::Connection { id } if !id.is_empty() => {
                if let Some(session_id) = self.session_id_for_connection(id) {
                    (PaneEndpoint::Session(session_id), path)
                } else {
                    (PaneEndpoint::Local, None)
                }
            }
            FileManagerSourceRestore::Connection { .. } => (PaneEndpoint::Local, None),
        }
    }

    pub(in crate::features) fn session_id_for_connection(
        &self,
        connection_id: &str,
    ) -> Option<String> {
        self.session
            .metadata_entries()
            .find_map(|(session_id, metadata)| {
                (metadata.source_connection_id.as_deref() == Some(connection_id)
                    && self
                        .session
                        .file_browser_backend_support_for_session(session_id)
                        .is_some())
                .then(|| session_id.to_string())
            })
    }

    pub(in crate::features) fn connection_id_for_session(
        &self,
        session_id: &str,
    ) -> Option<String> {
        self.session
            .metadata(session_id)
            .and_then(|metadata| metadata.source_connection_id.clone())
            .filter(|id| !id.is_empty())
    }

    pub(in crate::features) fn activate_file_manager_tab(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(id) else {
            return;
        };
        self.session.clear_active_start_selection();
        let key = WorkspaceTabId::FileManager(id.to_string()).layout_key();
        self.shell.workspace.focused_group = self.shell.sync_terminal_windows_active_tab(&key);
        self.shell.set_pane_focus_mode(false);
        self.shell.navigation.selected_nav = NavItem::Workspace;
        self.shell.navigation.main_mode = MainMode::Workspace;
        self.shell.request_session_tab_scroll_into_view();
        self.collapse_sidebars_for_file_manager(cx);
        workspace.update(cx, |workspace, cx| workspace.request_focus(cx));
        cx.notify();
    }

    pub(in crate::features) fn remove_workspace_file_manager_tab(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let key = WorkspaceTabId::FileManager(id.to_string()).layout_key();
        self.file_manager.remove(id);
        if let Some(mut tree) = self.shell.terminal_window_tree() {
            let next = tree.remove_tab(&key);
            self.shell.restore_terminal_window_tree(next);
        }
        self.restore_sidebars_from_file_manager(cx);
        self.release_unreferenced_hidden_sessions(cx);
        self.persist_file_manager_settings(cx);
    }

    pub(in crate::features) fn open_file_manager_for_session(
        &mut self,
        session_id: &str,
        pane: Option<PaneId>,
        cx: &mut Context<Self>,
    ) {
        if self
            .session
            .file_browser_backend_support_for_session(session_id)
            .is_none()
        {
            self.shell
                .set_status(t!("fileManager.sessionUnavailable").to_string());
            cx.notify();
            return;
        }
        if let Some(id) = self.file_manager.active_id().map(str::to_string)
            && let Some(workspace) = self.file_manager.workspace(&id)
        {
            let pane = pane.unwrap_or(PaneId::Right);
            workspace.update(cx, |workspace, _| {
                workspace
                    .pane_mut(pane)
                    .set_endpoint(PaneEndpoint::Session(session_id.to_string()));
                workspace.set_active(pane);
            });
            let key = WorkspaceTabId::FileManager(id.clone()).layout_key();
            if !self.workspace_tab_ids().contains(&key) {
                self.open_file_manager_tab(None, cx);
            } else {
                self.activate_workspace_tab(&key, cx);
            }
            self.refresh_file_manager_pane(id, pane, cx);
            return;
        }
        let mut seed = self.default_file_manager_seed();
        let endpoint = PaneEndpoint::Session(session_id.to_string());
        match pane {
            Some(PaneId::Left) | None => {
                seed.left = endpoint;
                seed.left_path = None;
            }
            Some(PaneId::Right) => {
                seed.right = endpoint;
                seed.right_path = None;
            }
        }
        self.open_file_manager_tab(Some(seed), cx);
    }

    fn collapse_sidebars_for_file_manager(&mut self, cx: &mut Context<Self>) {
        if !self
            .settings
            .summary()
            .ui_file_manager
            .auto_collapse_sidebar
        {
            return;
        }
        if self.file_manager.sidebar_restore_pending() {
            return;
        }
        let left_item = self.current_left_panel();
        let right_item = self.current_right_panel();
        let left_is_transfers = left_item == Some(NavItem::Transfers);
        let right_is_transfers = right_item == Some(NavItem::Transfers);
        if !left_is_transfers && !right_is_transfers {
            return;
        }
        self.file_manager
            .set_sidebar_restore(FileManagerSidebarRestore {
                left_open: left_is_transfers,
                left_item,
                left_collapsed: self.shell.panels.left_collapsed,
                right_open: right_is_transfers,
                right_item,
                right_collapsed: self.shell.panels.right_collapsed,
            });
        if left_is_transfers {
            self.shell.panels.left_collapsed = true;
        }
        if right_is_transfers {
            self.shell.panels.right_collapsed = true;
        }
        cx.notify();
    }

    pub(in crate::features) fn restore_sidebars_from_file_manager(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(restore) = self.file_manager.take_sidebar_restore() else {
            return;
        };
        if restore.left_open {
            self.shell.panels.left_collapsed = restore.left_collapsed;
            if let Some(item) = restore.left_item
                && !restore.left_collapsed
            {
                self.shell.panels.active_left = Some(item);
            }
        }
        if restore.right_open {
            self.shell.panels.right_collapsed = restore.right_collapsed;
            if let Some(item) = restore.right_item
                && !restore.right_collapsed
            {
                self.shell.panels.active_right = Some(item);
            }
        }
        let _ = PanelSide::Left;
        cx.notify();
    }

    pub(in crate::features) fn note_file_manager_manual_sidebar_open(&mut self) {
        if self.file_manager.sidebar_restore_pending() {
            self.file_manager.suppress_sidebar_restore();
        }
    }

    pub(in crate::features) fn persist_file_manager_settings(&mut self, cx: &mut Context<Self>) {
        let open = self.file_manager.workspaces().next().is_some();
        let mut settings = self.settings.summary().ui_file_manager.clone();
        settings.open = open;
        if let Some(workspace) = self
            .file_manager
            .workspaces()
            .next()
            .map(|(_, workspace)| workspace.clone())
        {
            let snapshot = workspace.read(cx);
            settings.split_percent = snapshot.split_percent;
            settings.active_pane = match snapshot.active {
                PaneId::Left => FileManagerPaneSide::Left,
                PaneId::Right => FileManagerPaneSide::Right,
            };
            settings.queue_expanded = snapshot.queue_expanded;
            settings.queue_height = snapshot.queue_height.max(80.) as u32;
            settings.left = self.pane_restore_from(snapshot, PaneId::Left);
            settings.right = self.pane_restore_from(snapshot, PaneId::Right);
        }
        self.settings.set_file_manager_ui(settings);
        self.queue_settings_save(
            crate::features::settings::SettingsSaveKind::FileExplorer,
            cx,
        );
    }

    fn pane_restore_from(
        &self,
        workspace: &FileManagerWorkspace,
        id: PaneId,
    ) -> FileManagerPaneRestore {
        let pane = workspace.pane(id);
        FileManagerPaneRestore {
            source: match &pane.endpoint {
                PaneEndpoint::Local => FileManagerSourceRestore::Local,
                PaneEndpoint::Session(session_id) => {
                    match self.connection_id_for_session(session_id) {
                        Some(id) => FileManagerSourceRestore::Connection { id },
                        None => FileManagerSourceRestore::Local,
                    }
                }
            },
            path: pane.path.display_path.clone(),
        }
    }

    pub(in crate::features) fn restore_file_manager_after_sessions(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.summary().ui_file_manager.open {
            return;
        }
        if self.file_manager.workspaces().next().is_some() {
            if let Some(id) = self.file_manager.active_id().map(str::to_string) {
                self.refresh_file_manager_pane(id.clone(), PaneId::Left, cx);
                self.refresh_file_manager_pane(id, PaneId::Right, cx);
            }
            return;
        }
        self.open_file_manager_tab(None, cx);
    }

    pub(in crate::features) fn file_manager_tab_title(&self, id: &str, cx: &gpui::App) -> String {
        let Some(workspace) = self.file_manager.workspace(id) else {
            return t!("fileManager.tabTitle").to_string();
        };
        let workspace = workspace.read(cx);
        let left = self.endpoint_label(&workspace.pane(PaneId::Left).endpoint);
        let right = self.endpoint_label(&workspace.pane(PaneId::Right).endpoint);
        format!("{} · {left} | {right}", t!("fileManager.tabTitle"))
    }

    pub(in crate::features) fn endpoint_label(&self, endpoint: &PaneEndpoint) -> String {
        match endpoint {
            PaneEndpoint::Local => t!("fileManager.local").to_string(),
            PaneEndpoint::Session(id) => self
                .session
                .display_name(id)
                .unwrap_or_else(|| t!("fileManager.session").to_string()),
        }
    }

    pub(in crate::features) fn file_manager_source_choices(&self) -> Vec<(PaneEndpoint, String)> {
        let mut choices = vec![(PaneEndpoint::Local, t!("fileManager.local").to_string())];
        for session_id in self.session.session_ids() {
            if self
                .session
                .file_browser_backend_support_for_session(session_id)
                .is_none()
            {
                continue;
            }
            let label = self.endpoint_label(&PaneEndpoint::Session(session_id.to_string()));
            choices.push((PaneEndpoint::Session(session_id.to_string()), label));
        }
        choices
    }
}

impl FileManagerWorkspace {
    pub(in crate::features) fn request_focus(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl ZzClawTermApp {
    pub(in crate::features) fn workspace_file_manager_content(
        &mut self,
        id: &str,
        _cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.file_manager
            .workspace(id)
            .map(|view| view.into_any_element())
            .unwrap_or_else(|| div().into_any_element())
    }
}
