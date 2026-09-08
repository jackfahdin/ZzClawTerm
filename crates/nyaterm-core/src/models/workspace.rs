use serde::{Deserialize, Serialize};

/// Tauri per-tab pane tree node (`RestorablePaneNode` in ui.open_tabs[].root).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum RestorablePaneNode {
    #[serde(rename = "leaf")]
    Leaf {
        #[serde(default)]
        id: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        session_type: String,
        #[serde(default)]
        connection_id: Option<String>,
    },
    #[serde(rename = "split")]
    Split {
        #[serde(default)]
        id: String,
        direction: String,
        #[serde(default = "default_restorable_split_ratio")]
        ratio: f64,
        first: Box<RestorablePaneNode>,
        second: Box<RestorablePaneNode>,
    },
}

impl RestorablePaneNode {
    /// Flatten session leaves in left-to-right / top-to-bottom order.
    pub fn collect_leaves(&self) -> Vec<RestorablePaneLeaf> {
        let mut out = Vec::new();
        self.collect_leaves_into(&mut out);
        out
    }

    fn collect_leaves_into(&self, out: &mut Vec<RestorablePaneLeaf>) {
        match self {
            Self::Leaf {
                id,
                title,
                session_type,
                connection_id,
            } => out.push(RestorablePaneLeaf {
                id: id.clone(),
                title: title.clone(),
                session_type: session_type.clone(),
                connection_id: connection_id.clone(),
            }),
            Self::Split { first, second, .. } => {
                first.collect_leaves_into(out);
                second.collect_leaves_into(out);
            }
        }
    }

    /// Map this pane tree onto ordered open-tab indexes starting at `base_index`.
    pub fn to_workspace_pane_layout(
        &self,
        base_index: usize,
    ) -> Option<RestorableWorkspacePaneNode> {
        let mut next = base_index;
        self.to_workspace_pane_layout_inner(&mut next)
    }

    fn to_workspace_pane_layout_inner(
        &self,
        next_index: &mut usize,
    ) -> Option<RestorableWorkspacePaneNode> {
        match self {
            Self::Leaf { .. } => {
                let tab_index = *next_index;
                *next_index += 1;
                Some(RestorableWorkspacePaneNode::Leaf { tab_index })
            }
            Self::Split {
                id,
                direction,
                ratio,
                first,
                second,
            } => {
                let first = first.to_workspace_pane_layout_inner(next_index);
                let second = second.to_workspace_pane_layout_inner(next_index);
                match (first, second) {
                    (None, None) => None,
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (Some(first), Some(second)) => Some(RestorableWorkspacePaneNode::Split {
                        id: if id.trim().is_empty() {
                            format!("pane-{}", *next_index)
                        } else {
                            id.clone()
                        },
                        direction: direction.clone(),
                        ratio: (*ratio).clamp(0.2, 0.8),
                        first: Box::new(first),
                        second: Box::new(second),
                    }),
                }
            }
        }
    }

    pub fn leaf_session(
        title: impl Into<String>,
        session_type: impl Into<String>,
        connection_id: Option<String>,
    ) -> Self {
        Self::Leaf {
            id: String::new(),
            title: title.into(),
            session_type: session_type.into(),
            connection_id,
        }
    }
}

/// One restorable session leaf extracted from a tab pane tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorablePaneLeaf {
    pub id: String,
    pub title: String,
    pub session_type: String,
    pub connection_id: Option<String>,
}

/// Tauri `ui.open_tabs` entry (native restores connection/local leaf sessions).
/// Optional `root` preserves Tauri per-tab pane trees for interop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RestorableOpenTab {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub session_type: String,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub custom_name: Option<String>,
    #[serde(default)]
    pub tab_color: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    #[serde(default)]
    pub active_pane_id: Option<String>,
    #[serde(default)]
    pub root: Option<RestorablePaneNode>,
}

impl RestorableOpenTab {
    /// Expand this tab into one or more session restore descriptors.
    /// Split roots become multiple leaves (native global sessions).
    pub fn expanded_sessions(&self) -> Vec<RestorableOpenTabSession> {
        if let Some(root) = &self.root {
            let leaves = root.collect_leaves();
            if !leaves.is_empty() {
                return leaves
                    .into_iter()
                    .map(|leaf| {
                        let title = if leaf.title.trim().is_empty() {
                            self.title.clone()
                        } else {
                            leaf.title
                        };
                        let session_type = if leaf.session_type.trim().is_empty() {
                            self.session_type.clone()
                        } else {
                            leaf.session_type
                        };
                        let connection_id = leaf
                            .connection_id
                            .filter(|id| !id.is_empty())
                            .or_else(|| self.connection_id.clone());
                        RestorableOpenTabSession {
                            title,
                            session_type,
                            connection_id,
                            custom_name: self.custom_name.clone(),
                            tab_color: self.tab_color.clone(),
                            locked: self.locked,
                        }
                    })
                    .collect();
            }
        }

        vec![RestorableOpenTabSession {
            title: self.title.clone(),
            session_type: self.session_type.clone(),
            connection_id: self.connection_id.clone(),
            custom_name: self.custom_name.clone(),
            tab_color: self.tab_color.clone(),
            locked: self.locked,
        }]
    }

    /// If this tab carries a multi-pane root, map it onto ordered tab indexes
    /// starting at `base_index` (index of the first expanded leaf).
    pub fn workspace_pane_layout_from_root(
        &self,
        base_index: usize,
    ) -> Option<RestorableWorkspacePaneNode> {
        let root = self.root.as_ref()?;
        if matches!(root, RestorablePaneNode::Leaf { .. }) {
            return None;
        }
        let layout = root.to_workspace_pane_layout(base_index)?;
        match &layout {
            RestorableWorkspacePaneNode::Split { .. } => Some(layout),
            RestorableWorkspacePaneNode::Leaf { .. } => None,
        }
    }

    pub fn with_leaf_root(
        title: impl Into<String>,
        session_type: impl Into<String>,
        connection_id: Option<String>,
        custom_name: Option<String>,
        tab_color: Option<String>,
    ) -> Self {
        let title = title.into();
        let session_type = session_type.into();
        let root = RestorablePaneNode::leaf_session(
            title.clone(),
            session_type.clone(),
            connection_id.clone(),
        );
        Self {
            title,
            session_type,
            connection_id,
            custom_name,
            tab_color,
            locked: false,
            active_pane_id: None,
            root: Some(root),
        }
    }
}

/// Flattened session restore unit (one native tab / session).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorableOpenTabSession {
    pub title: String,
    pub session_type: String,
    pub connection_id: Option<String>,
    pub custom_name: Option<String>,
    pub tab_color: Option<String>,
    pub locked: bool,
}

/// Native workspace pane split tree (indexes into ordered open tabs).
/// Distinct from Tauri per-tab pane trees: native H/V splits arrange sessions globally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum RestorableWorkspacePaneNode {
    #[serde(rename = "leaf")]
    Leaf {
        #[serde(default)]
        tab_index: usize,
    },
    #[serde(rename = "split")]
    Split {
        #[serde(default)]
        id: String,
        direction: String,
        #[serde(default = "default_restorable_split_ratio")]
        ratio: f64,
        first: Box<RestorableWorkspacePaneNode>,
        second: Box<RestorableWorkspacePaneNode>,
    },
}

/// Tauri `ui.terminal_window_layout` node (indexes into ordered open tabs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum RestorableTerminalWindowNode {
    #[serde(rename = "leaf")]
    Leaf {
        #[serde(default)]
        tab_indexes: Vec<usize>,
        #[serde(default)]
        active_tab_index: Option<usize>,
    },
    #[serde(rename = "split")]
    Split {
        direction: String,
        #[serde(default = "default_restorable_split_ratio")]
        ratio: f64,
        first: Box<RestorableTerminalWindowNode>,
        second: Box<RestorableTerminalWindowNode>,
    },
}

fn default_restorable_split_ratio() -> f64 {
    0.5
}
