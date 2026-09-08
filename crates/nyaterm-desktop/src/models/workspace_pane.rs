use gpui::Pixels;
use nyaterm_core::RestorableWorkspacePaneNode;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceSplitDirection {
    Horizontal,
    Vertical,
}

impl WorkspaceSplitDirection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
        }
    }
}

/// Tauri smart-split / tile modes for multi-leaf tab windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SmartSplitMode {
    Auto,
    Horizontal,
    Vertical,
}

impl SmartSplitMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "Smart Split",
            Self::Horizontal => "Tile Horizontally",
            Self::Vertical => "Tile Vertically",
        }
    }
}

/// Recursive workspace pane tree (Tauri PaneNode / SplitPane).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspacePaneNode {
    Leaf {
        session_id: String,
    },
    Split {
        id: String,
        direction: WorkspaceSplitDirection,
        ratio_percent: u8,
        first: Box<WorkspacePaneNode>,
        second: Box<WorkspacePaneNode>,
    },
}

impl WorkspacePaneNode {
    pub(crate) const DEFAULT_RATIO_PERCENT: u8 = 50;
    pub(crate) const MIN_RATIO_PERCENT: u8 = 20;
    pub(crate) const MAX_RATIO_PERCENT: u8 = 80;

    pub(crate) fn leaf(session_id: impl Into<String>) -> Self {
        Self::Leaf {
            session_id: session_id.into(),
        }
    }

    pub(crate) fn clamped_ratio_percent(value: u8) -> u8 {
        value.clamp(Self::MIN_RATIO_PERCENT, Self::MAX_RATIO_PERCENT)
    }

    pub(crate) fn primary_weight(ratio_percent: u8) -> f32 {
        Self::clamped_ratio_percent(ratio_percent) as f32
    }

    pub(crate) fn secondary_weight(ratio_percent: u8) -> f32 {
        (100 - Self::clamped_ratio_percent(ratio_percent)) as f32
    }

    pub(crate) fn contains_session(&self, session_id: &str) -> bool {
        match self {
            Self::Leaf { session_id: id } => id == session_id,
            Self::Split { first, second, .. } => {
                first.contains_session(session_id) || second.contains_session(session_id)
            }
        }
    }

    pub(crate) fn replace_session_id(&mut self, old_id: &str, new_id: &str) -> bool {
        match self {
            Self::Leaf { session_id } => {
                if session_id != old_id {
                    return false;
                }
                *session_id = new_id.to_string();
                true
            }
            Self::Split { first, second, .. } => {
                first.replace_session_id(old_id, new_id)
                    || second.replace_session_id(old_id, new_id)
            }
        }
    }

    pub(crate) fn session_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        self.collect_session_ids(&mut ids);
        ids
    }

    fn collect_session_ids(&self, out: &mut Vec<String>) {
        match self {
            Self::Leaf { session_id } => out.push(session_id.clone()),
            Self::Split { first, second, .. } => {
                first.collect_session_ids(out);
                second.collect_session_ids(out);
            }
        }
    }

    pub(crate) fn is_split(&self) -> bool {
        matches!(self, Self::Split { .. })
    }

    pub(crate) fn set_ratio_for_split(&mut self, split_id: &str, value: u8) -> bool {
        match self {
            Self::Leaf { .. } => false,
            Self::Split {
                id,
                ratio_percent,
                first,
                second,
                ..
            } => {
                if id == split_id {
                    *ratio_percent = Self::clamped_ratio_percent(value);
                    true
                } else {
                    first.set_ratio_for_split(split_id, value)
                        || second.set_ratio_for_split(split_id, value)
                }
            }
        }
    }

    pub(crate) fn direction_for_split(&self, split_id: &str) -> Option<WorkspaceSplitDirection> {
        match self {
            Self::Leaf { .. } => None,
            Self::Split {
                id,
                direction,
                first,
                second,
                ..
            } => {
                if id == split_id {
                    Some(*direction)
                } else {
                    first
                        .direction_for_split(split_id)
                        .or_else(|| second.direction_for_split(split_id))
                }
            }
        }
    }

    pub(crate) fn ratio_for_split(&self, split_id: &str) -> Option<u8> {
        match self {
            Self::Leaf { .. } => None,
            Self::Split {
                id,
                ratio_percent,
                first,
                second,
                ..
            } => {
                if id == split_id {
                    Some(*ratio_percent)
                } else {
                    first
                        .ratio_for_split(split_id)
                        .or_else(|| second.ratio_for_split(split_id))
                }
            }
        }
    }

    /// Split the leaf holding `target_session_id` by replacing it with
    /// Split(leaf(target), leaf(new_session_id)).
    pub(crate) fn split_leaf(
        &mut self,
        target_session_id: &str,
        new_session_id: String,
        direction: WorkspaceSplitDirection,
        split_id: String,
    ) -> bool {
        match self {
            Self::Leaf { session_id } if session_id == target_session_id => {
                let first = Box::new(Self::leaf(session_id.clone()));
                let second = Box::new(Self::leaf(new_session_id));
                *self = Self::Split {
                    id: split_id,
                    direction,
                    ratio_percent: Self::DEFAULT_RATIO_PERCENT,
                    first,
                    second,
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_leaf(
                    target_session_id,
                    new_session_id.clone(),
                    direction,
                    split_id.clone(),
                ) || second.split_leaf(target_session_id, new_session_id, direction, split_id)
            }
        }
    }

    /// Remove dead sessions and collapse unnecessary nodes.
    pub(crate) fn prune(self, live_ids: &HashSet<String>) -> Option<Self> {
        match self {
            Self::Leaf { session_id } => {
                if live_ids.contains(&session_id) {
                    Some(Self::Leaf { session_id })
                } else {
                    None
                }
            }
            Self::Split {
                id,
                direction,
                ratio_percent,
                first,
                second,
            } => {
                let first = first.prune(live_ids);
                let second = second.prune(live_ids);
                match (first, second) {
                    (Some(first), Some(second)) => Some(Self::Split {
                        id,
                        direction,
                        ratio_percent: Self::clamped_ratio_percent(ratio_percent),
                        first: Box::new(first),
                        second: Box::new(second),
                    }),
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                }
            }
        }
    }

    /// Serialize global workspace pane splits using ordered open-tab indexes.
    pub(crate) fn serialize_layout(
        &self,
        ordered_tab_ids: &[String],
    ) -> Option<RestorableWorkspacePaneNode> {
        if ordered_tab_ids.is_empty() {
            return None;
        }
        let index_by_id: std::collections::HashMap<&str, usize> = ordered_tab_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        self.serialize_layout_with(&index_by_id)
    }

    fn serialize_layout_with(
        &self,
        index_by_id: &std::collections::HashMap<&str, usize>,
    ) -> Option<RestorableWorkspacePaneNode> {
        match self {
            Self::Leaf { session_id } => {
                let tab_index = *index_by_id.get(session_id.as_str())?;
                Some(RestorableWorkspacePaneNode::Leaf { tab_index })
            }
            Self::Split {
                id,
                direction,
                ratio_percent,
                first,
                second,
            } => {
                let first = first.serialize_layout_with(index_by_id);
                let second = second.serialize_layout_with(index_by_id);
                match (first, second) {
                    (None, None) => None,
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (Some(first), Some(second)) => {
                        let ratio = (Self::clamped_ratio_percent(*ratio_percent) as f64) / 100.0;
                        Some(RestorableWorkspacePaneNode::Split {
                            id: id.clone(),
                            direction: match direction {
                                WorkspaceSplitDirection::Horizontal => "horizontal".to_string(),
                                WorkspaceSplitDirection::Vertical => "vertical".to_string(),
                            },
                            ratio: ratio.clamp(0.2, 0.8),
                            first: Box::new(first),
                            second: Box::new(second),
                        })
                    }
                }
            }
        }
    }

    /// Restore a global workspace pane tree from ordered open-tab ids.
    pub(crate) fn restore_layout(
        layout: &RestorableWorkspacePaneNode,
        ordered_tab_ids: &[String],
    ) -> Option<Self> {
        if ordered_tab_ids.is_empty() {
            return None;
        }
        let mut used = std::collections::HashSet::new();
        let restored = Self::restore_layout_inner(layout, ordered_tab_ids, &mut used)?;
        if !restored.is_split() {
            return None;
        }
        Some(restored)
    }

    fn restore_layout_inner(
        layout: &RestorableWorkspacePaneNode,
        ordered_tab_ids: &[String],
        used: &mut std::collections::HashSet<String>,
    ) -> Option<Self> {
        match layout {
            RestorableWorkspacePaneNode::Leaf { tab_index } => {
                let session_id = ordered_tab_ids.get(*tab_index)?.clone();
                if !used.insert(session_id.clone()) {
                    return None;
                }
                Some(Self::leaf(session_id))
            }
            RestorableWorkspacePaneNode::Split {
                id,
                direction,
                ratio,
                first,
                second,
            } => {
                let first = Self::restore_layout_inner(first, ordered_tab_ids, used);
                let second = Self::restore_layout_inner(second, ordered_tab_ids, used);
                match (first, second) {
                    (None, None) => None,
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (Some(first), Some(second)) => {
                        let direction = match direction.to_ascii_lowercase().as_str() {
                            "horizontal" | "row" => WorkspaceSplitDirection::Horizontal,
                            _ => WorkspaceSplitDirection::Vertical,
                        };
                        let ratio_percent = ((*ratio * 100.0).round() as u8)
                            .clamp(Self::MIN_RATIO_PERCENT, Self::MAX_RATIO_PERCENT);
                        let split_id = if id.trim().is_empty() {
                            format!("pane-split-{}", uuid_v4_like())
                        } else {
                            id.clone()
                        };
                        Some(Self::Split {
                            id: split_id,
                            direction,
                            ratio_percent,
                            first: Box::new(first),
                            second: Box::new(second),
                        })
                    }
                }
            }
        }
    }
}

/// Compatibility alias used by older dual-pane helpers.
pub(crate) type WorkspaceSplitState = WorkspacePaneNode;

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceSplitResizeState {
    pub(crate) split_id: String,
    pub(crate) direction: WorkspaceSplitDirection,
    pub(crate) start_pos: Pixels,
    pub(crate) start_ratio: u8,
    pub(crate) container_size: f32,
}

pub(crate) fn uuid_v4_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
