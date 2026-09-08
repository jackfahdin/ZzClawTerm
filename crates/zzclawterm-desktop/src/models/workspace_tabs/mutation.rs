use super::{TerminalWindowNode, WorkspacePaneNode, WorkspaceSplitDirection};

impl TerminalWindowNode {
    pub(crate) fn remove_tab(&mut self, tab_id: &str) -> Option<Self> {
        match self {
            Self::Leaf {
                tab_ids,
                active_tab_id,
                id,
            } => {
                if !tab_ids.iter().any(|id| id == tab_id) {
                    return Some(Self::Leaf {
                        id: id.clone(),
                        tab_ids: tab_ids.clone(),
                        active_tab_id: active_tab_id.clone(),
                    });
                }
                tab_ids.retain(|id| id != tab_id);
                if active_tab_id.as_deref() == Some(tab_id) {
                    *active_tab_id = tab_ids.first().cloned();
                }
                if tab_ids.is_empty() {
                    None
                } else {
                    Some(Self::Leaf {
                        id: id.clone(),
                        tab_ids: tab_ids.clone(),
                        active_tab_id: active_tab_id.clone(),
                    })
                }
            }
            Self::Split {
                id,
                direction,
                ratio_percent,
                first,
                second,
            } => {
                let next_first = first.remove_tab(tab_id);
                let next_second = second.remove_tab(tab_id);
                match (next_first, next_second) {
                    (Some(a), Some(b)) => Some(Self::Split {
                        id: id.clone(),
                        direction: *direction,
                        ratio_percent: *ratio_percent,
                        first: Box::new(a),
                        second: Box::new(b),
                    }),
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                }
            }
        }
    }

    pub(crate) fn move_tab_to_leaf(&mut self, tab_id: &str, target_leaf_id: &str) -> bool {
        if !self.contains_tab(tab_id) {
            return false;
        }
        // Already on target leaf.
        if let Self::Leaf { id, tab_ids, .. } = self
            && id == target_leaf_id
        {
            return tab_ids.iter().any(|id| id == tab_id);
        }
        let Some(removed) = self.remove_tab(tab_id) else {
            return false;
        };
        *self = removed;
        if self.insert_tab_into_leaf(target_leaf_id, tab_id) {
            true
        } else {
            // Target gone; put back on first leaf.
            self.insert_tab_into_first_leaf(tab_id);
            false
        }
    }

    pub(crate) fn first_leaf_id(&self) -> Option<String> {
        match self {
            Self::Leaf { id, .. } => Some(id.clone()),
            Self::Split { first, .. } => first.first_leaf_id(),
        }
    }

    pub(crate) fn leaf_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_leaf_ids(&mut out);
        out
    }

    pub(super) fn collect_leaf_ids(&self, out: &mut Vec<String>) {
        match self {
            Self::Leaf { id, .. } => out.push(id.clone()),
            Self::Split { first, second, .. } => {
                first.collect_leaf_ids(out);
                second.collect_leaf_ids(out);
            }
        }
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
                    *ratio_percent = WorkspacePaneNode::clamped_ratio_percent(value);
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
}
