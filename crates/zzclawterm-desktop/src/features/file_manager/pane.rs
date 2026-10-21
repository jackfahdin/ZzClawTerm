//! Independent file-manager pane state. Not shared with the sidebar browser.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use gpui::{FocusHandle, ScrollHandle, UniformListScrollHandle};
use zzclawterm_transport::{RemoteFilePath, SftpFileEntry};

use crate::features::transfers::{BrowserFilterCache, BrowserFilterKey, record_browser_history};
use crate::models::{
    TransferBrowserColumnWidths, TransferBrowserSortColumn, TransferBrowserSortDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::features) enum PaneId {
    Left,
    Right,
}

impl PaneId {
    pub(in crate::features) fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    pub(in crate::features) fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::features) enum PaneEndpoint {
    Local,
    Session(String),
}

impl PaneEndpoint {
    pub(in crate::features) fn session_id(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::Session(id) => Some(id.as_str()),
        }
    }
}

#[derive(Clone)]
pub(in crate::features) struct PaneNavigationSnapshot {
    pub path: RemoteFilePath,
    pub home_dir: String,
    pub entries: Arc<Vec<SftpFileEntry>>,
    pub loading: bool,
    pub error: Option<String>,
    pub status: String,
    pub history: VecDeque<String>,
    pub history_index: usize,
    pub selected_path: Option<String>,
    pub selected_paths: HashSet<String>,
}

pub(in crate::features) struct BrowserPaneState {
    pub endpoint: PaneEndpoint,
    pub path: RemoteFilePath,
    pub home_dir: String,
    pub home_dir_pending: bool,
    pub entries: Arc<Vec<SftpFileEntry>>,
    pub loading: bool,
    pub error: Option<String>,
    pub status: String,
    pub disconnected: bool,
    pub search: String,
    pub sort_column: TransferBrowserSortColumn,
    pub sort_direction: TransferBrowserSortDirection,
    pub column_widths: TransferBrowserColumnWidths,
    pub history: VecDeque<String>,
    pub history_index: usize,
    pub selected_path: Option<String>,
    pub selected_paths: HashSet<String>,
    pub list_scroll: UniformListScrollHandle,
    pub horizontal_scroll: ScrollHandle,
    pub focus: FocusHandle,
    pub generation: u64,
    filter: BrowserFilterCache,
}

impl BrowserPaneState {
    pub(in crate::features) fn new(endpoint: PaneEndpoint, focus: FocusHandle) -> Self {
        Self {
            endpoint,
            path: RemoteFilePath::new(String::new()),
            home_dir: String::new(),
            home_dir_pending: true,
            entries: Arc::new(Vec::new()),
            loading: false,
            error: None,
            status: String::new(),
            disconnected: false,
            search: String::new(),
            sort_column: TransferBrowserSortColumn::Name,
            sort_direction: TransferBrowserSortDirection::Ascending,
            column_widths: TransferBrowserColumnWidths::default(),
            history: VecDeque::new(),
            history_index: 0,
            selected_path: None,
            selected_paths: HashSet::new(),
            list_scroll: UniformListScrollHandle::new(),
            horizontal_scroll: ScrollHandle::new(),
            focus,
            generation: 0,
            filter: BrowserFilterCache::default(),
        }
    }

    pub(in crate::features) fn visible_entries(
        &mut self,
        show_hidden: bool,
    ) -> Arc<[SftpFileEntry]> {
        let key = BrowserFilterKey::new(
            &self.entries,
            &self.search,
            show_hidden,
            self.sort_column,
            self.sort_direction,
        );
        self.filter.entries(key, &self.entries)
    }

    pub(in crate::features) fn capture_navigation(&self) -> PaneNavigationSnapshot {
        PaneNavigationSnapshot {
            path: self.path.clone(),
            home_dir: self.home_dir.clone(),
            entries: self.entries.clone(),
            loading: self.loading,
            error: self.error.clone(),
            status: self.status.clone(),
            history: self.history.clone(),
            history_index: self.history_index,
            selected_path: self.selected_path.clone(),
            selected_paths: self.selected_paths.clone(),
        }
    }

    pub(in crate::features) fn restore_navigation(&mut self, snapshot: PaneNavigationSnapshot) {
        self.path = snapshot.path;
        self.home_dir = snapshot.home_dir;
        self.entries = snapshot.entries;
        self.loading = snapshot.loading;
        self.error = snapshot.error;
        self.status = snapshot.status;
        self.history = snapshot.history;
        self.history_index = snapshot
            .history_index
            .min(self.history.len().saturating_sub(1));
        self.selected_path = snapshot.selected_path;
        self.selected_paths = snapshot.selected_paths;
    }

    pub(in crate::features) fn begin_load(&mut self, path: RemoteFilePath) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.path = path;
        self.loading = true;
        self.error = None;
        self.status = "Loading directory...".to_string();
        self.selected_path = None;
        self.selected_paths.clear();
        self.list_scroll = UniformListScrollHandle::new();
        self.horizontal_scroll = ScrollHandle::new();
        self.generation
    }

    pub(in crate::features) fn apply_listing(
        &mut self,
        generation: u64,
        endpoint: &PaneEndpoint,
        path: &RemoteFilePath,
        entries: Vec<SftpFileEntry>,
    ) -> bool {
        if self.generation != generation || &self.endpoint != endpoint {
            return false;
        }
        if self.path.display_path != path.display_path
            || self.path.raw_path_token != path.raw_path_token
        {
            return false;
        }
        self.entries = Arc::new(entries);
        self.loading = false;
        self.error = None;
        self.disconnected = false;
        self.status = format!("{} item(s)", self.entries.len());
        self.record_history(self.path.display_path.clone());
        true
    }

    pub(in crate::features) fn apply_error(
        &mut self,
        generation: u64,
        endpoint: &PaneEndpoint,
        snapshot: PaneNavigationSnapshot,
        error: String,
    ) -> bool {
        if self.generation != generation || &self.endpoint != endpoint {
            return false;
        }
        self.restore_navigation(snapshot);
        self.loading = false;
        self.status = format!("directory load failed: {error}");
        if self.entries.is_empty() {
            self.error = Some(error);
        }
        true
    }

    pub(in crate::features) fn set_endpoint(&mut self, endpoint: PaneEndpoint) {
        if self.endpoint == endpoint {
            return;
        }
        self.endpoint = endpoint;
        self.generation = self.generation.wrapping_add(1);
        self.home_dir.clear();
        self.home_dir_pending = true;
        self.path = RemoteFilePath::new(String::new());
        self.entries = Arc::new(Vec::new());
        self.history.clear();
        self.history_index = 0;
        self.selected_path = None;
        self.selected_paths.clear();
        self.error = None;
        self.disconnected = false;
        self.search.clear();
    }

    pub(in crate::features) fn replace_session_id(&mut self, old_id: &str, new_id: &str) -> bool {
        if self.endpoint.session_id() != Some(old_id) {
            return false;
        }
        self.endpoint = PaneEndpoint::Session(new_id.to_string());
        self.generation = self.generation.wrapping_add(1);
        true
    }

    pub(in crate::features) fn select_entry(&mut self, path: String) {
        self.selected_path = Some(path.clone());
        self.selected_paths.clear();
        self.selected_paths.insert(path);
    }

    pub(in crate::features) fn toggle_mark(&mut self, path: String) {
        if !self.selected_paths.remove(&path) {
            self.selected_paths.insert(path.clone());
        }
        self.selected_path = Some(path);
    }

    pub(in crate::features) fn selected_entries(&self) -> Vec<SftpFileEntry> {
        self.entries
            .iter()
            .filter(|entry| self.selected_paths.contains(&entry.identity_key()))
            .cloned()
            .collect()
    }

    fn record_history(&mut self, path: String) {
        record_browser_history(&mut self.history, &mut self.history_index, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserPaneState, PaneEndpoint};
    use gpui::TestAppContext;
    use zzclawterm_transport::{RemoteFilePath, SftpFileEntry, SftpFileType};

    fn entry(name: &str, path: &str) -> SftpFileEntry {
        SftpFileEntry {
            name: name.to_string(),
            path: path.to_string(),
            file_type: SftpFileType::File,
            size: Some(1),
            permissions: None,
            owner: String::new(),
            group: String::new(),
            modified_at: None,
            raw_path_token: None,
            symlink_target_is_directory: false,
        }
    }

    #[test]
    fn panes_keep_independent_search_sort_and_selection() {
        let cx = TestAppContext::single();
        cx.update(|cx| {
            let mut left = BrowserPaneState::new(PaneEndpoint::Local, cx.focus_handle());
            let mut right =
                BrowserPaneState::new(PaneEndpoint::Session("ssh-a".into()), cx.focus_handle());
            left.entries = std::sync::Arc::new(vec![entry("alpha", "/a"), entry("beta", "/b")]);
            right.entries = std::sync::Arc::new(vec![entry("gamma", "/g"), entry("delta", "/d")]);
            left.select_entry("/a".into());
            left.search = "al".into();
            right.select_entry("/g".into());
            right.search = "de".into();
            let left_visible = left.visible_entries(true);
            let right_visible = right.visible_entries(true);
            assert_eq!(left_visible.len(), 1);
            assert_eq!(left_visible[0].name, "alpha");
            assert_eq!(right.selected_path.as_deref(), Some("/g"));
            assert_eq!(left.selected_path.as_deref(), Some("/a"));
            assert_eq!(right_visible.len(), 1);
            assert_eq!(right_visible[0].name, "delta");
        });
    }

    #[test]
    fn stale_generation_or_endpoint_replies_are_discarded() {
        let cx = TestAppContext::single();
        cx.update(|cx| {
            let mut pane = BrowserPaneState::new(PaneEndpoint::Local, cx.focus_handle());
            let generation = pane.begin_load(RemoteFilePath::new("/one"));
            pane.endpoint = PaneEndpoint::Session("ssh-b".into());
            assert!(!pane.apply_listing(
                generation,
                &PaneEndpoint::Local,
                &RemoteFilePath::new("/one"),
                vec![entry("stale", "/one/stale")],
            ));
            let generation = pane.begin_load(RemoteFilePath::new("/two"));
            assert!(!pane.apply_listing(
                generation.wrapping_sub(1),
                &PaneEndpoint::Session("ssh-b".into()),
                &RemoteFilePath::new("/two"),
                vec![entry("old", "/two/old")],
            ));
            assert!(pane.apply_listing(
                generation,
                &PaneEndpoint::Session("ssh-b".into()),
                &RemoteFilePath::new("/two"),
                vec![entry("fresh", "/two/fresh")],
            ));
            assert_eq!(pane.entries.len(), 1);
            assert_eq!(pane.entries[0].name, "fresh");
        });
    }

    #[test]
    fn swapping_panes_exchanges_endpoint_search_and_selection() {
        let cx = TestAppContext::single();
        cx.update(|cx| {
            let mut left = BrowserPaneState::new(PaneEndpoint::Local, cx.focus_handle());
            let mut right =
                BrowserPaneState::new(PaneEndpoint::Session("ssh-a".into()), cx.focus_handle());
            left.select_entry("/a".into());
            left.search = "alpha".into();
            right.select_entry("/g".into());
            right.search = "gamma".into();
            std::mem::swap(&mut left, &mut right);
            assert_eq!(left.endpoint, PaneEndpoint::Session("ssh-a".into()));
            assert_eq!(right.endpoint, PaneEndpoint::Local);
            assert_eq!(left.selected_path.as_deref(), Some("/g"));
            assert_eq!(right.selected_path.as_deref(), Some("/a"));
            assert_eq!(left.search, "gamma");
            assert_eq!(right.search, "alpha");
        });
    }
}
