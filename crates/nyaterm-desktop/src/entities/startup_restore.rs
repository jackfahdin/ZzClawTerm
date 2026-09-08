use nyaterm_core::{RestorableOpenTab, RestorableWorkspacePaneNode};

#[derive(Debug, Default)]
pub struct StartupRestoreStore {
    started_after_window_open: bool,
    open_tabs_restored: bool,
    queue: Vec<RestorableOpenTab>,
    pending_pane_layouts: Vec<RestorableWorkspacePaneNode>,
    pending_active_pane_indexes: Vec<usize>,
    loaded_open_tabs: Option<Vec<RestorableOpenTab>>,
}

impl StartupRestoreStore {
    pub fn mark_started_after_window_open(&mut self) -> bool {
        if self.started_after_window_open {
            return false;
        }
        self.started_after_window_open = true;
        true
    }

    pub fn started_after_window_open(&self) -> bool {
        self.started_after_window_open
    }

    pub fn open_tabs_restored(&self) -> bool {
        self.open_tabs_restored
    }

    pub fn mark_open_tabs_restored(&mut self) -> bool {
        if self.open_tabs_restored {
            return false;
        }
        self.open_tabs_restored = true;
        true
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn set_queue(&mut self, queue: Vec<RestorableOpenTab>) {
        self.queue = queue;
    }

    pub fn set_loaded_open_tabs(&mut self, tabs: Vec<RestorableOpenTab>) {
        self.loaded_open_tabs = Some(tabs);
    }

    pub fn take_loaded_open_tabs(&mut self) -> Option<Vec<RestorableOpenTab>> {
        self.loaded_open_tabs.take()
    }

    pub fn queue_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn pop_next_tab(&mut self) -> Option<RestorableOpenTab> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    pub fn can_pump_queue(&self, pending_session_active: bool) -> bool {
        !pending_session_active && !self.queue.is_empty()
    }

    pub fn clear_pending_layouts(&mut self) {
        self.pending_pane_layouts.clear();
        self.pending_active_pane_indexes.clear();
    }

    pub fn push_pending_pane_layout(&mut self, layout: RestorableWorkspacePaneNode) {
        self.pending_pane_layouts.push(layout);
    }

    pub fn push_pending_active_pane_index(&mut self, index: usize) {
        self.pending_active_pane_indexes.push(index);
    }

    pub fn take_pending_pane_layouts(&mut self) -> Vec<RestorableWorkspacePaneNode> {
        std::mem::take(&mut self.pending_pane_layouts)
    }

    pub fn take_pending_active_pane_indexes(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.pending_active_pane_indexes)
    }
}
