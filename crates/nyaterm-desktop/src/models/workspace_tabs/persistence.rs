use nyaterm_core::RestorableTerminalWindowNode;

use super::{TerminalWindowNode, WorkspacePaneNode, WorkspaceSplitDirection, uuid_v4_like};

impl TerminalWindowNode {
    /// Serialize to Tauri `ui.terminal_window_layout` using ordered tab indexes.
    pub(crate) fn serialize_layout(
        &self,
        ordered_tab_ids: &[String],
    ) -> Option<RestorableTerminalWindowNode> {
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

    pub(super) fn serialize_layout_with(
        &self,
        index_by_id: &std::collections::HashMap<&str, usize>,
    ) -> Option<RestorableTerminalWindowNode> {
        match self {
            Self::Leaf {
                tab_ids,
                active_tab_id,
                ..
            } => {
                let mut tab_indexes = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for tab_id in tab_ids {
                    if let Some(&index) = index_by_id.get(tab_id.as_str())
                        && seen.insert(index)
                    {
                        tab_indexes.push(index);
                    }
                }
                if tab_indexes.is_empty() {
                    return None;
                }
                let active_tab_index = active_tab_id
                    .as_ref()
                    .and_then(|id| index_by_id.get(id.as_str()).copied())
                    .or_else(|| tab_indexes.first().copied());
                Some(RestorableTerminalWindowNode::Leaf {
                    tab_indexes,
                    active_tab_index,
                })
            }
            Self::Split {
                direction,
                ratio_percent,
                first,
                second,
                ..
            } => {
                let first = first.serialize_layout_with(index_by_id);
                let second = second.serialize_layout_with(index_by_id);
                match (first, second) {
                    (None, None) => None,
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (Some(first), Some(second)) => {
                        let ratio = (WorkspacePaneNode::clamped_ratio_percent(*ratio_percent)
                            as f64)
                            / 100.0;
                        Some(RestorableTerminalWindowNode::Split {
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

    /// Restore from Tauri layout; returns None if any ordered tab is missing.
    pub(crate) fn restore_layout(
        layout: &RestorableTerminalWindowNode,
        ordered_tab_ids: &[String],
    ) -> Option<Self> {
        if ordered_tab_ids.is_empty() {
            return None;
        }
        let mut used = std::collections::HashSet::new();
        let restored = Self::restore_layout_inner(layout, ordered_tab_ids, &mut used)?;
        if ordered_tab_ids.iter().any(|id| !used.contains(id)) {
            return None;
        }
        Some(restored)
    }

    pub(super) fn restore_layout_inner(
        layout: &RestorableTerminalWindowNode,
        ordered_tab_ids: &[String],
        used: &mut std::collections::HashSet<String>,
    ) -> Option<Self> {
        match layout {
            RestorableTerminalWindowNode::Leaf {
                tab_indexes,
                active_tab_index,
            } => {
                let mut tab_ids = Vec::new();
                for &index in tab_indexes {
                    let Some(tab_id) = ordered_tab_ids.get(index) else {
                        continue;
                    };
                    if used.insert(tab_id.clone()) {
                        tab_ids.push(tab_id.clone());
                    }
                }
                if tab_ids.is_empty() {
                    return None;
                }
                let active_tab_id = active_tab_index
                    .and_then(|index| ordered_tab_ids.get(index).cloned())
                    .filter(|id| tab_ids.iter().any(|tab| tab == id))
                    .or_else(|| tab_ids.first().cloned());
                Some(Self::leaf(tab_ids, active_tab_id))
            }
            RestorableTerminalWindowNode::Split {
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
                        let ratio_percent = ((*ratio * 100.0).round() as u8).clamp(
                            WorkspacePaneNode::MIN_RATIO_PERCENT,
                            WorkspacePaneNode::MAX_RATIO_PERCENT,
                        );
                        Some(Self::Split {
                            id: format!("tw-split-{}", uuid_v4_like()),
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
