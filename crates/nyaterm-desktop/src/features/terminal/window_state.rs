//! Authoritative split/tab window ownership for the terminal feature.

use nyaterm_core::RestorableTerminalWindowNode;

use super::state::TerminalFeatureState;
use crate::models::{SmartSplitMode, TabDockZone, TerminalWindowNode, WorkspaceSplitDirection};

/// Split/tab window tree and drag-and-drop targets over it.
pub(super) struct TerminalWindowState {
    pub(super) tree: Option<TerminalWindowNode>,
    pub(super) drop: Option<(String, TabDockZone)>,
    /// Whether we already attempted startup restore of multi-leaf layout.
    pub(super) restored: bool,
    pub(super) file_drop_hover: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features) enum TerminalWindowReconcileResult {
    Inactive,
    Cleared,
    Reconciled { focused_leaf_id: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features) enum TerminalWindowDockResult {
    MissingTree,
    UnknownTab,
    NoEffect,
    Docked { focused_leaf_id: Option<String> },
}

impl TerminalFeatureState {
    pub(in crate::features) fn reconcile_terminal_windows(
        &mut self,
        live_ids: &[String],
        preferred_leaf_id: Option<&str>,
        active_tab_id: Option<&str>,
    ) -> TerminalWindowReconcileResult {
        if self.windows.tree.is_none() {
            return TerminalWindowReconcileResult::Inactive;
        }
        if live_ids.is_empty() {
            self.windows.tree = None;
            return TerminalWindowReconcileResult::Cleared;
        }

        let mut tree_cleared = false;
        if let Some(root) = self.windows.tree.as_mut() {
            for tab_id in root.collect_tab_ids() {
                if !live_ids.iter().any(|id| id == &tab_id) {
                    if let Some(next) = root.remove_tab(&tab_id) {
                        *root = next;
                    } else {
                        tree_cleared = true;
                        break;
                    }
                }
            }
        }
        if tree_cleared {
            self.windows.tree = None;
            return TerminalWindowReconcileResult::Inactive;
        }

        let Some(root) = self.windows.tree.as_mut() else {
            return TerminalWindowReconcileResult::Inactive;
        };
        for tab_id in live_ids {
            root.ensure_tab(tab_id, preferred_leaf_id);
        }
        if let Some(active_tab_id) = active_tab_id {
            let _ = root.set_active_tab(active_tab_id);
        }
        let focused_leaf_id = preferred_leaf_id
            .filter(|preferred| root.leaf_ids().iter().any(|leaf| leaf == *preferred))
            .map(str::to_string)
            .or_else(|| root.first_leaf_id());
        TerminalWindowReconcileResult::Reconciled { focused_leaf_id }
    }

    pub(in crate::features) fn ensure_terminal_windows_root(
        &mut self,
        tab_ids: Vec<String>,
        active_tab_id: Option<String>,
    ) -> Option<String> {
        if self.windows.tree.is_some() || tab_ids.is_empty() {
            return None;
        }
        let root = TerminalWindowNode::leaf(tab_ids, active_tab_id);
        let focused_leaf_id = root.first_leaf_id();
        self.windows.tree = Some(root);
        focused_leaf_id
    }

    pub(in crate::features) fn activate_terminal_window_tab(
        &mut self,
        leaf_id: &str,
        session_id: &str,
    ) {
        if let Some(root) = self.windows.tree.as_mut() {
            let _ = set_terminal_window_leaf_active(root, leaf_id, session_id);
            let _ = root.set_active_tab(session_id);
        }
    }

    pub(in crate::features) fn terminal_windows_is_multi_leaf(&self) -> bool {
        matches!(self.windows.tree, Some(TerminalWindowNode::Split { .. }))
    }

    pub(in crate::features) fn multi_leaf_terminal_window_tree(
        &self,
    ) -> Option<TerminalWindowNode> {
        self.windows
            .tree
            .as_ref()
            .filter(|root| matches!(root, TerminalWindowNode::Split { .. }))
            .cloned()
    }

    pub(in crate::features) fn terminal_window_tree_is_some(&self) -> bool {
        self.windows.tree.is_some()
    }

    pub(in crate::features) fn sync_terminal_windows_active_tab(
        &mut self,
        tab_id: &str,
    ) -> Option<String> {
        let root = self.windows.tree.as_mut()?;
        let _ = root.set_active_tab(tab_id);
        terminal_window_leaf_with_tab(root, tab_id)
    }

    pub(in crate::features) fn place_tab_before_in_terminal_windows(
        &mut self,
        tab_id: &str,
        before_tab_id: &str,
    ) -> Option<Option<String>> {
        self.windows.drop = None;
        let root = self.windows.tree.as_mut()?;
        if !root.place_tab_before(tab_id, before_tab_id) {
            return None;
        }
        let _ = root.set_active_tab(tab_id);
        Some(terminal_window_leaf_with_tab(root, tab_id).or_else(|| root.first_leaf_id()))
    }

    pub(in crate::features) fn terminal_window_drop_for_leaf(
        &self,
        leaf_id: &str,
    ) -> Option<TabDockZone> {
        self.windows
            .drop
            .as_ref()
            .filter(|(leaf, _)| leaf == leaf_id)
            .map(|(_, zone)| *zone)
    }

    pub(in crate::features) fn set_terminal_window_drop(
        &mut self,
        leaf_id: String,
        zone: TabDockZone,
    ) -> bool {
        let next = Some((leaf_id, zone));
        if self.windows.drop == next {
            return false;
        }
        self.windows.drop = next;
        true
    }

    pub(in crate::features) fn clear_terminal_window_drop(&mut self) -> bool {
        self.windows.drop.take().is_some()
    }

    pub(in crate::features) fn dock_tab_on_terminal_window_leaf(
        &mut self,
        tab_id: &str,
        target_leaf_id: &str,
        zone: TabDockZone,
    ) -> TerminalWindowDockResult {
        self.windows.drop = None;
        let Some(root) = self.windows.tree.as_mut() else {
            return TerminalWindowDockResult::MissingTree;
        };
        if !root.contains_tab(tab_id) {
            return TerminalWindowDockResult::UnknownTab;
        }
        if !root.dock_tab(tab_id, target_leaf_id, zone) {
            return TerminalWindowDockResult::NoEffect;
        }
        let _ = root.set_active_tab(tab_id);
        TerminalWindowDockResult::Docked {
            focused_leaf_id: terminal_window_leaf_with_tab(root, tab_id)
                .or_else(|| root.first_leaf_id()),
        }
    }

    pub(in crate::features) fn apply_smart_split(
        &mut self,
        tab_ids: &[String],
        mode: SmartSplitMode,
        active_tab_id: Option<&str>,
    ) -> Option<Option<String>> {
        let mut root = TerminalWindowNode::build_smart_split_layout(tab_ids, mode)?;
        if let Some(active_tab_id) = active_tab_id {
            let _ = root.set_active_tab(active_tab_id);
        }
        let focused_leaf_id = active_tab_id
            .and_then(|active| terminal_window_leaf_with_tab(&root, active))
            .or_else(|| root.first_leaf_id());
        self.windows.tree = Some(root);
        Some(focused_leaf_id)
    }

    pub(in crate::features) fn terminal_window_split_geometry(
        &self,
        split_id: &str,
    ) -> Option<(WorkspaceSplitDirection, u8)> {
        let root = self.windows.tree.as_ref()?;
        Some((
            root.direction_for_split(split_id)?,
            root.ratio_for_split(split_id)?,
        ))
    }

    pub(in crate::features) fn set_terminal_window_split_ratio(
        &mut self,
        split_id: &str,
        ratio_percent: u8,
    ) -> bool {
        self.windows
            .tree
            .as_mut()
            .is_some_and(|root| root.set_ratio_for_split(split_id, ratio_percent))
    }

    pub(in crate::features) fn replace_terminal_window_tab_id(
        &mut self,
        old_id: &str,
        new_id: &str,
    ) -> bool {
        self.windows
            .tree
            .as_mut()
            .is_some_and(|root| root.replace_tab_id(old_id, new_id))
    }

    pub(in crate::features) fn serialize_terminal_window_layout(
        &self,
        ordered_tab_ids: &[String],
    ) -> Option<RestorableTerminalWindowNode> {
        self.windows
            .tree
            .as_ref()
            .filter(|root| matches!(root, TerminalWindowNode::Split { .. }))
            .and_then(|root| root.serialize_layout(ordered_tab_ids))
    }

    pub(in crate::features) fn terminal_windows_restore_is_complete(&self) -> bool {
        self.windows.restored
    }

    pub(in crate::features) fn mark_terminal_windows_restore_pending(&mut self) {
        self.windows.restored = false;
    }

    pub(in crate::features) fn complete_terminal_windows_restore(&mut self) {
        self.windows.restored = true;
    }

    pub(in crate::features) fn restore_terminal_window_layout(
        &mut self,
        layout: &RestorableTerminalWindowNode,
        ordered_tab_ids: &[String],
        active_tab_id: Option<&str>,
    ) -> Option<Option<String>> {
        let mut root = TerminalWindowNode::restore_layout(layout, ordered_tab_ids)?;
        if !matches!(root, TerminalWindowNode::Split { .. }) {
            return None;
        }
        if let Some(active_tab_id) = active_tab_id {
            let _ = root.set_active_tab(active_tab_id);
        }
        let focused_leaf_id = active_tab_id
            .and_then(|active| terminal_window_leaf_with_tab(&root, active))
            .or_else(|| root.first_leaf_id());
        self.windows.tree = Some(root);
        Some(focused_leaf_id)
    }

    pub(in crate::features) fn terminal_file_drop_hover_is_pending(&self) -> bool {
        self.windows.file_drop_hover.is_some()
    }

    pub(in crate::features) fn terminal_file_drop_hover_matches(&self, session_id: &str) -> bool {
        self.windows.file_drop_hover.as_deref() == Some(session_id)
    }

    pub(in crate::features) fn set_terminal_file_drop_hover(
        &mut self,
        session_id: Option<String>,
    ) -> bool {
        if self.windows.file_drop_hover == session_id {
            return false;
        }
        self.windows.file_drop_hover = session_id;
        true
    }

    pub(in crate::features) fn clear_terminal_file_drop_hover(&mut self) -> bool {
        self.windows.file_drop_hover.take().is_some()
    }

    pub(in crate::features) fn clear_terminal_file_drop_hover_for_session(
        &mut self,
        session_id: &str,
    ) -> bool {
        if self.windows.file_drop_hover.as_deref() != Some(session_id) {
            return false;
        }
        self.windows.file_drop_hover = None;
        true
    }
}

fn terminal_window_leaf_with_tab(node: &TerminalWindowNode, tab_id: &str) -> Option<String> {
    match node {
        TerminalWindowNode::Leaf { id, tab_ids, .. } => {
            tab_ids.iter().any(|id| id == tab_id).then(|| id.clone())
        }
        TerminalWindowNode::Split { first, second, .. } => {
            terminal_window_leaf_with_tab(first, tab_id)
                .or_else(|| terminal_window_leaf_with_tab(second, tab_id))
        }
    }
}

fn set_terminal_window_leaf_active(
    node: &mut TerminalWindowNode,
    leaf_id: &str,
    tab_id: &str,
) -> bool {
    match node {
        TerminalWindowNode::Leaf {
            id,
            tab_ids,
            active_tab_id,
        } => {
            if id == leaf_id && tab_ids.iter().any(|id| id == tab_id) {
                *active_tab_id = Some(tab_id.to_string());
                true
            } else {
                false
            }
        }
        TerminalWindowNode::Split { first, second, .. } => {
            set_terminal_window_leaf_active(first, leaf_id, tab_id)
                || set_terminal_window_leaf_active(second, leaf_id, tab_id)
        }
    }
}
