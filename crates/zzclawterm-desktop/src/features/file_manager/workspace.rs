use std::sync::Arc;

use gpui::{Context, Entity, EventEmitter, FocusHandle, Focusable, Subscription, WeakEntity};
use zzclawterm_core::FileManagerPaneSide;
use zzclawterm_transport::{RemoteFilePath, SftpFileEntry};

use super::pane::{BrowserPaneState, PaneEndpoint, PaneId, PaneNavigationSnapshot};
use crate::features::ZzClawTermApp;

pub(in crate::features) struct FileManagerSeed {
    pub left: PaneEndpoint,
    pub left_path: Option<RemoteFilePath>,
    pub right: PaneEndpoint,
    pub right_path: Option<RemoteFilePath>,
}

pub(in crate::features) struct FileManagerWorkspace {
    pub(in crate::features) app: WeakEntity<ZzClawTermApp>,
    pub id: String,
    pub(in crate::features) left: BrowserPaneState,
    pub(in crate::features) right: BrowserPaneState,
    pub active: PaneId,
    pub split_percent: u8,
    pub queue_expanded: bool,
    pub queue_height: f32,
    pub queue_filter: FileManagerQueueFilter,
    pub show_all_jobs: bool,
    pub single_pane: bool,
    pub dragging_split: bool,
    pub(in crate::features) focus: FocusHandle,
    _app_observation: Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum FileManagerQueueFilter {
    All,
    Active,
    Completed,
    Failed,
}

pub(in crate::features) struct FileManagerListResult {
    pub pane: PaneId,
    pub generation: u64,
    pub endpoint: PaneEndpoint,
    pub path: RemoteFilePath,
    pub snapshot: PaneNavigationSnapshot,
    pub result: Result<Vec<SftpFileEntry>, String>,
}

impl FileManagerWorkspace {
    pub(in crate::features) fn new(
        app: Entity<ZzClawTermApp>,
        id: String,
        seed: FileManagerSeed,
        cx: &mut Context<Self>,
    ) -> Self {
        let observation = cx.observe(&app, |_, _, cx| cx.notify());
        let mut workspace = Self {
            app: app.downgrade(),
            id,
            left: BrowserPaneState::new(seed.left, cx.focus_handle()),
            right: BrowserPaneState::new(seed.right, cx.focus_handle()),
            active: PaneId::Left,
            split_percent: 50,
            queue_expanded: false,
            queue_height: 160.,
            queue_filter: FileManagerQueueFilter::All,
            show_all_jobs: false,
            single_pane: false,
            dragging_split: false,
            focus: cx.focus_handle(),
            _app_observation: observation,
        };
        if let Some(path) = seed.left_path {
            workspace.left.path = path;
            workspace.left.home_dir_pending = false;
        }
        if let Some(path) = seed.right_path {
            workspace.right.path = path;
            workspace.right.home_dir_pending = false;
        }
        workspace
    }

    pub(in crate::features) fn pane(&self, id: PaneId) -> &BrowserPaneState {
        match id {
            PaneId::Left => &self.left,
            PaneId::Right => &self.right,
        }
    }

    pub(in crate::features) fn pane_mut(&mut self, id: PaneId) -> &mut BrowserPaneState {
        match id {
            PaneId::Left => &mut self.left,
            PaneId::Right => &mut self.right,
        }
    }

    pub(in crate::features) fn active_pane(&self) -> &BrowserPaneState {
        self.pane(self.active)
    }

    pub(in crate::features) fn set_active(&mut self, id: PaneId) {
        self.active = id;
    }

    pub(in crate::features) fn swap_panes(&mut self) {
        std::mem::swap(&mut self.left, &mut self.right);
        self.active = self.active.other();
    }

    pub(in crate::features) fn pane_ids_for_session(&self, session_id: &str) -> Vec<PaneId> {
        [PaneId::Left, PaneId::Right]
            .into_iter()
            .filter(|id| self.pane(*id).endpoint.session_id() == Some(session_id))
            .collect()
    }

    pub(in crate::features) fn references_session(&self, session_id: &str) -> bool {
        self.left.endpoint.session_id() == Some(session_id)
            || self.right.endpoint.session_id() == Some(session_id)
    }

    pub(in crate::features) fn replace_session_id(&mut self, old_id: &str, new_id: &str) {
        self.left.replace_session_id(old_id, new_id);
        self.right.replace_session_id(old_id, new_id);
    }

    pub(in crate::features) fn restore_settings(
        &mut self,
        settings: &zzclawterm_core::FileManagerUiSettings,
    ) {
        if !settings.remember_layout {
            return;
        }
        self.split_percent = settings.split_percent.clamp(20, 80);
        self.active = match settings.active_pane {
            FileManagerPaneSide::Left => PaneId::Left,
            FileManagerPaneSide::Right => PaneId::Right,
        };
        self.queue_expanded = settings.queue_expanded;
        self.queue_height = settings.queue_height.max(80) as f32;
    }

    pub(in crate::features) fn apply_list_result(&mut self, result: FileManagerListResult) -> bool {
        let FileManagerListResult {
            pane,
            generation,
            endpoint,
            path,
            snapshot,
            result,
        } = result;
        let pane = self.pane_mut(pane);
        match result {
            Ok(entries) => pane.apply_listing(generation, &endpoint, &path, entries),
            Err(error) => pane.apply_error(generation, &endpoint, snapshot, error),
        }
    }

    pub(in crate::features) fn refresh_matching_directory(
        &mut self,
        endpoint: &PaneEndpoint,
        parent: &str,
        entries: Vec<SftpFileEntry>,
    ) -> bool {
        let mut changed = false;
        for pane in [&mut self.left, &mut self.right] {
            if &pane.endpoint == endpoint
                && zzclawterm_transport::file_browser_path_eq(
                    match pane.endpoint {
                        PaneEndpoint::Local => zzclawterm_transport::FileBrowserBackendKind::Local,
                        PaneEndpoint::Session(_) => {
                            zzclawterm_transport::FileBrowserBackendKind::Remote
                        }
                    },
                    &pane.path.display_path,
                    parent,
                )
            {
                pane.entries = Arc::new(entries.clone());
                pane.status = format!("{} item(s)", pane.entries.len());
                changed = true;
            }
        }
        changed
    }

    pub(in crate::features) fn mark_disconnected(&mut self, session_id: &str, disconnected: bool) {
        for pane in [&mut self.left, &mut self.right] {
            if pane.endpoint.session_id() == Some(session_id) {
                pane.disconnected = disconnected;
                if disconnected {
                    pane.loading = false;
                    pane.status = "Disconnected".to_string();
                }
            }
        }
    }
}

impl Focusable for FileManagerWorkspace {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EventEmitter<()> for FileManagerWorkspace {}
