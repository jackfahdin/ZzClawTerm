//! Authoritative split/tab window ownership for the terminal feature.

use gpui::{Bounds, Pixels, ScrollHandle};
use std::collections::HashMap;
use zzclawterm_core::RestorableTerminalWindowNode;

use super::state::TerminalFeatureState;
use crate::models::{SmartSplitMode, TabDockZone, TerminalWindowNode, WorkspaceSplitDirection};

/// Split/tab window tree and drag-and-drop targets over it.
pub(super) struct TerminalWindowState {
    pub(super) tree: Option<TerminalWindowNode>,
    pub(super) drop: Option<(String, TabDockZone)>,
    /// Whether we already attempted startup restore of multi-leaf layout.
    pub(super) restored: bool,
    pub(super) restore_in_flight: bool,
    pub(super) persistence_blocked: bool,
    pub(super) file_drop_hover: Option<String>,
    pub(super) group_scrolls: HashMap<String, ScrollHandle>,
    pub(super) start_groups: HashMap<u64, String>,
    pub(super) split_bounds: HashMap<String, Bounds<Pixels>>,
    pub(super) workspace_bounds: Option<Bounds<Pixels>>,
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
    pub(in crate::features) fn terminal_layout_persistence_is_supported(&self) -> bool {
        !self.windows.persistence_blocked
    }
    pub(in crate::features) fn preserve_unread_terminal_layout(&mut self) {
        self.windows.persistence_blocked = true;
    }
    fn prune_terminal_group_caches(&mut self) {
        let ids = self
            .windows
            .tree
            .as_ref()
            .map(TerminalWindowNode::node_ids)
            .unwrap_or_default();
        let leaves = self
            .windows
            .tree
            .as_ref()
            .map(TerminalWindowNode::leaf_ids)
            .unwrap_or_default();
        self.windows
            .group_scrolls
            .retain(|id, _| leaves.contains(id));
        self.windows.split_bounds.retain(|id, _| ids.contains(id));
    }
    pub(in crate::features) fn terminal_group_for_tab(&self, tab: &str) -> Option<String> {
        self.windows
            .tree
            .as_ref()?
            .leaf_for_tab(tab)
            .map(str::to_string)
    }

    pub(in crate::features) fn terminal_group_tabs(
        &self,
        leaf: &str,
    ) -> Option<(Vec<String>, Option<String>)> {
        self.windows
            .tree
            .as_ref()?
            .leaf_tabs(leaf)
            .map(|(tabs, active)| (tabs.to_vec(), active.map(str::to_string)))
    }

    pub(in crate::features) fn terminal_group_scroll(&mut self, leaf: &str) -> ScrollHandle {
        self.windows
            .group_scrolls
            .entry(leaf.to_string())
            .or_default()
            .clone()
    }

    pub(in crate::features) fn reserve_terminal_start_group(
        &mut self,
        sequence: u64,
        leaf: String,
    ) {
        self.windows.start_groups.insert(sequence, leaf);
    }

    pub(in crate::features) fn terminal_start_group(&self, sequence: u64) -> Option<&str> {
        self.windows.start_groups.get(&sequence).map(String::as_str)
    }
    pub(in crate::features) fn terminal_start_display_group(
        &self,
        sequence: u64,
    ) -> Option<String> {
        self.terminal_start_group(sequence)
            .filter(|group| self.terminal_window_has_leaf(group))
            .map(str::to_string)
            .or_else(|| self.first_terminal_group())
    }
    pub(in crate::features) fn first_terminal_group(&self) -> Option<String> {
        self.windows.tree.as_ref()?.first_leaf_id()
    }

    pub(in crate::features) fn clear_terminal_start_groups(&mut self) {
        self.windows.start_groups.clear();
    }

    pub(in crate::features) fn place_started_terminal_tab(&mut self, tab: &str, sequence: u64) {
        let group = self.windows.start_groups.get(&sequence).cloned();
        if let Some(root) = self.windows.tree.as_mut()
            && let Some(group) = group.filter(|group| root.leaf_tabs(group).is_some())
        {
            root.move_tab_to_leaf(tab, &group);
            root.set_active_tab(tab);
        }
    }

    pub(in crate::features) fn terminal_split_bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.windows.split_bounds.get(id).copied()
    }

    pub(in crate::features) fn record_terminal_split_bounds(
        &mut self,
        id: String,
        bounds: Bounds<Pixels>,
    ) {
        self.windows.split_bounds.insert(id, bounds);
    }

    pub(in crate::features) fn terminal_workspace_size(&self) -> Option<(f32, f32)> {
        self.windows
            .workspace_bounds
            .map(|bounds| (f32::from(bounds.size.width), f32::from(bounds.size.height)))
    }

    pub(in crate::features) fn record_terminal_workspace_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.windows.workspace_bounds = Some(bounds);
    }

    pub(in crate::features) fn tile_terminal_groups(
        &mut self,
        tabs: &[String],
        mode: SmartSplitMode,
        size: (f32, f32),
        active: Option<&str>,
    ) -> Option<Option<String>> {
        let mut root = TerminalWindowNode::tile_in_bounds(tabs, mode, size.0, size.1)?;
        if let Some(active) = active {
            root.set_active_tab(active);
        }
        let group = active
            .and_then(|tab| root.leaf_for_tab(tab))
            .map(str::to_string)
            .or_else(|| root.first_leaf_id());
        self.windows.tree = Some(root);
        self.windows.drop = None;
        self.windows.split_bounds.clear();
        self.prune_terminal_group_caches();
        Some(group)
    }

    pub(in crate::features) fn merge_terminal_groups(&mut self, active: Option<String>) {
        if let Some(root) = self.windows.tree.take() {
            let id = root.first_leaf_id();
            let mut leaf = TerminalWindowNode::leaf(root.collect_tab_ids(), active);
            if let (Some(id), TerminalWindowNode::Leaf { id: leaf_id, .. }) = (id, &mut leaf) {
                *leaf_id = id;
            }
            self.windows.tree = Some(leaf);
        }
        self.windows.drop = None;
        self.windows.split_bounds.clear();
        self.prune_terminal_group_caches();
    }

    pub(in crate::features) fn reconcile_terminal_windows(
        &mut self,
        live_ids: &[String],
        preferred_leaf_id: Option<&str>,
        active_tab_id: Option<&str>,
    ) -> TerminalWindowReconcileResult {
        if self.windows.tree.is_none() {
            if live_ids.is_empty() {
                return TerminalWindowReconcileResult::Inactive;
            }
            self.windows.tree = Some(TerminalWindowNode::leaf(
                live_ids.to_vec(),
                active_tab_id.map(str::to_string),
            ));
        }
        if live_ids.is_empty() {
            self.windows.tree = None;
            self.prune_terminal_group_caches();
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
            // All old groups vanished, but new live sessions must still have a home.
            self.windows.tree = Some(TerminalWindowNode::leaf(
                live_ids.to_vec(),
                active_tab_id.map(str::to_string),
            ));
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
        self.prune_terminal_group_caches();
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

    pub(in crate::features) fn terminal_windows_is_multi_leaf(&self) -> bool {
        matches!(self.windows.tree, Some(TerminalWindowNode::Split { .. }))
    }

    pub(in crate::features) fn terminal_window_tree_is_some(&self) -> bool {
        self.windows.tree.is_some()
    }

    pub(in crate::features) fn terminal_window_tree(&self) -> Option<TerminalWindowNode> {
        self.windows.tree.clone()
    }
    pub(in crate::features) fn terminal_window_tab_ids(&self) -> Vec<String> {
        self.windows
            .tree
            .as_ref()
            .map(TerminalWindowNode::collect_tab_ids)
            .unwrap_or_default()
    }

    pub(in crate::features) fn restore_terminal_window_tree(
        &mut self,
        tree: Option<TerminalWindowNode>,
    ) {
        self.windows.tree = tree;
        self.prune_terminal_group_caches();
    }

    pub(in crate::features) fn terminal_window_has_leaf(&self, leaf_id: &str) -> bool {
        self.windows
            .tree
            .as_ref()
            .is_some_and(|tree| tree.leaf_ids().iter().any(|id| id == leaf_id))
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

    #[cfg(test)]
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
            .and_then(|root| root.serialize_layout(ordered_tab_ids))
    }

    pub(in crate::features) fn terminal_windows_restore_is_in_flight(&self) -> bool {
        self.windows.restore_in_flight
    }
    pub(in crate::features) fn begin_terminal_windows_restore(&mut self) {
        self.windows.restore_in_flight = true;
    }

    pub(in crate::features) fn terminal_windows_restore_is_complete(&self) -> bool {
        self.windows.restored
    }

    pub(in crate::features) fn mark_terminal_windows_restore_pending(&mut self) {
        self.windows.restored = false;
        self.windows.restore_in_flight = false;
    }

    pub(in crate::features) fn complete_terminal_windows_restore(&mut self) {
        self.windows.restored = true;
        self.windows.restore_in_flight = false;
    }

    pub(in crate::features) fn restore_terminal_window_layout(
        &mut self,
        layout: &RestorableTerminalWindowNode,
        ordered_tab_ids: &[String],
        active_tab_id: Option<&str>,
    ) -> Option<Option<String>> {
        let Some(root) = TerminalWindowNode::restore_layout(layout, ordered_tab_ids) else {
            self.preserve_unread_terminal_layout();
            return None;
        };
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
