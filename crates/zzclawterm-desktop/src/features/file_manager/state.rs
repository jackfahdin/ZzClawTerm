use std::collections::{HashMap, HashSet};

use gpui::Entity;

use super::pane::PaneEndpoint;
use super::workspace::FileManagerWorkspace;
use crate::models::NavItem;

#[derive(Clone)]
pub(in crate::features) struct FileManagerSidebarRestore {
    pub left_open: bool,
    pub left_item: Option<NavItem>,
    pub left_collapsed: bool,
    pub right_open: bool,
    pub right_item: Option<NavItem>,
    pub right_collapsed: bool,
}

#[derive(Default)]
pub(in crate::features) struct FileManagerFeatureState {
    workspaces: HashMap<String, Entity<FileManagerWorkspace>>,
    job_ids: HashSet<String>,
    job_targets: HashMap<String, FileManagerJobTarget>,
    next_id: u64,
    sidebar_restore: Option<FileManagerSidebarRestore>,
    sidebar_restore_suppressed: bool,
}

#[derive(Clone, Debug)]
pub(in crate::features) struct FileManagerJobTarget {
    pub source: PaneEndpoint,
    pub source_parent: String,
    pub target: PaneEndpoint,
    pub target_parent: String,
}

impl FileManagerFeatureState {
    pub(in crate::features) fn workspace(&self, id: &str) -> Option<Entity<FileManagerWorkspace>> {
        self.workspaces.get(id).cloned()
    }

    pub(in crate::features) fn workspaces(
        &self,
    ) -> impl Iterator<Item = (&String, &Entity<FileManagerWorkspace>)> {
        self.workspaces.iter()
    }

    pub(in crate::features) fn active_id(&self) -> Option<&str> {
        self.workspaces.keys().next().map(String::as_str)
    }

    pub(in crate::features) fn insert(
        &mut self,
        id: String,
        workspace: Entity<FileManagerWorkspace>,
    ) {
        self.workspaces.insert(id, workspace);
    }

    pub(in crate::features) fn remove(&mut self, id: &str) -> Option<Entity<FileManagerWorkspace>> {
        self.workspaces.remove(id)
    }

    pub(in crate::features) fn next_id(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        format!("fm-{}", self.next_id)
    }

    pub(in crate::features) fn track_job(&mut self, job_id: String, target: FileManagerJobTarget) {
        self.job_ids.insert(job_id.clone());
        self.job_targets.insert(job_id, target);
    }

    #[allow(dead_code)]
    pub(in crate::features) fn owns_job(&self, job_id: &str) -> bool {
        self.job_ids.contains(job_id)
    }

    pub(in crate::features) fn job_ids(&self) -> &HashSet<String> {
        &self.job_ids
    }

    pub(in crate::features) fn job_target(&self, job_id: &str) -> Option<&FileManagerJobTarget> {
        self.job_targets.get(job_id)
    }

    pub(in crate::features) fn references_session(
        &self,
        session_id: &str,
        cx: &AppContext,
    ) -> bool {
        self.workspaces
            .values()
            .any(|workspace| workspace.read(cx).references_session(session_id))
            || self.job_targets.values().any(|target| {
                target.source.session_id() == Some(session_id)
                    || target.target.session_id() == Some(session_id)
            })
    }

    pub(in crate::features) fn replace_session_id(
        &mut self,
        old_id: &str,
        new_id: &str,
        cx: &mut AppContext,
    ) {
        for workspace in self.workspaces.values() {
            workspace.update(cx, |workspace, cx| {
                workspace.replace_session_id(old_id, new_id);
                cx.notify();
            });
        }
        for target in self.job_targets.values_mut() {
            if target.source.session_id() == Some(old_id) {
                target.source = PaneEndpoint::Session(new_id.to_string());
            }
            if target.target.session_id() == Some(old_id) {
                target.target = PaneEndpoint::Session(new_id.to_string());
            }
        }
    }

    pub(in crate::features) fn set_sidebar_restore(&mut self, restore: FileManagerSidebarRestore) {
        if self.sidebar_restore.is_none() {
            self.sidebar_restore = Some(restore);
        }
        self.sidebar_restore_suppressed = false;
    }

    pub(in crate::features) fn take_sidebar_restore(
        &mut self,
    ) -> Option<FileManagerSidebarRestore> {
        if self.sidebar_restore_suppressed {
            self.sidebar_restore = None;
            return None;
        }
        self.sidebar_restore.take()
    }

    pub(in crate::features) fn suppress_sidebar_restore(&mut self) {
        self.sidebar_restore_suppressed = true;
        self.sidebar_restore = None;
    }

    pub(in crate::features) fn sidebar_restore_pending(&self) -> bool {
        self.sidebar_restore.is_some() && !self.sidebar_restore_suppressed
    }
}

type AppContext = gpui::App;
