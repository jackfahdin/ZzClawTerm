//! Group-tree navigation for the asset workspace.
//!
//! Mirrors `src/lib/assetGroups.ts`. The breadcrumb tolerates missing parents
//! and circular `parent_id` chains, and the descendant collection includes a
//! selected group plus every group reachable through valid parent links.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::{Group, SavedConnection};

/// Translation key the root breadcrumb segment resolves to, matching the Tauri
/// `ASSET_ROOT_SEGMENT.name`. The desktop layer swaps this for a localized
/// label; the pure logic keeps the key so tests stay locale-independent.
pub const ASSET_ROOT_SEGMENT_KEY: &str = "assets.root";

/// One breadcrumb segment. `id` is `None` for the synthetic root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetGroupPathSegment {
    pub id: Option<String>,
    pub name: String,
}

/// First-writer-wins index of groups by id, dropping empty or duplicate ids.
pub fn build_group_index(groups: &[Group]) -> HashMap<&str, &Group> {
    let mut index = HashMap::new();
    for group in groups {
        if group.id.is_empty() {
            continue;
        }
        index.entry(group.id.as_str()).or_insert(group);
    }
    index
}

/// Builds the root-first breadcrumb for a selected group.
///
/// Returns just the root segment when nothing is selected. Walks up through
/// `parent_id`, stopping on a missing parent or a cycle so the loop always
/// terminates.
pub fn build_group_path(
    groups: &[Group],
    selected_group_id: Option<&str>,
) -> Vec<AssetGroupPathSegment> {
    let root = AssetGroupPathSegment {
        id: None,
        name: ASSET_ROOT_SEGMENT_KEY.to_string(),
    };

    let Some(selected_group_id) = selected_group_id.filter(|id| !id.is_empty()) else {
        return vec![root];
    };

    let groups_by_id = build_group_index(groups);
    let mut segments = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = Some(selected_group_id);

    while let Some(id) = current_id {
        if !seen.insert(id.to_string()) {
            break;
        }
        let Some(group) = groups_by_id.get(id) else {
            break;
        };
        segments.push(AssetGroupPathSegment {
            id: Some(group.id.clone()),
            name: group.name.clone(),
        });
        current_id = group.parent_id.as_deref();
    }

    segments.reverse();
    let mut path = Vec::with_capacity(segments.len() + 1);
    path.push(root);
    path.extend(segments);
    path
}

/// Collects the selected group and every descendant reachable through valid
/// parent links. Returns an empty set when the selection is empty or unknown.
pub fn collect_descendant_group_ids(
    groups: &[Group],
    selected_group_id: Option<&str>,
) -> HashSet<String> {
    let mut result = HashSet::new();
    let Some(selected_group_id) = selected_group_id.filter(|id| !id.is_empty()) else {
        return result;
    };

    let groups_by_id = build_group_index(groups);
    if !groups_by_id.contains_key(selected_group_id) {
        return result;
    }

    let mut children_by_parent: HashMap<&str, Vec<&str>> = HashMap::new();
    for group in groups_by_id.values() {
        let Some(parent_id) = group.parent_id.as_deref() else {
            continue;
        };
        if !groups_by_id.contains_key(parent_id) {
            continue;
        }
        children_by_parent
            .entry(parent_id)
            .or_default()
            .push(group.id.as_str());
    }

    let mut queue = VecDeque::new();
    queue.push_back(selected_group_id);
    while let Some(group_id) = queue.pop_front() {
        if !result.insert(group_id.to_string()) {
            continue;
        }
        if let Some(children) = children_by_parent.get(group_id) {
            for child in children {
                queue.push_back(child);
            }
        }
    }

    result
}

/// Returns the connections belonging to a selected group and its descendants.
///
/// With no selection every connection is returned (the root view). An unknown
/// selection yields nothing, matching the Tauri behavior.
pub fn connections_for_asset_group<'a>(
    connections: &'a [SavedConnection],
    groups: &[Group],
    selected_group_id: Option<&str>,
) -> Vec<&'a SavedConnection> {
    let Some(selected_group_id) = selected_group_id.filter(|id| !id.is_empty()) else {
        return connections.iter().collect();
    };

    let selected_ids = collect_descendant_group_ids(groups, Some(selected_group_id));
    if selected_ids.is_empty() {
        return Vec::new();
    }

    connections
        .iter()
        .filter(|connection| {
            connection
                .group_id
                .as_deref()
                .is_some_and(|group_id| selected_ids.contains(group_id))
        })
        .collect()
}

/// Joins a group's breadcrumb into a display string, substituting `root_label`
/// for the synthetic root segment.
pub fn group_path_label(groups: &[Group], group_id: Option<&str>, root_label: &str) -> String {
    build_group_path(groups, group_id)
        .into_iter()
        .map(|segment| match segment.id {
            None => root_label.to_string(),
            Some(_) => segment.name,
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Whether a connection has no group, or names a group that no longer exists.
pub fn is_ungrouped_connection(connection: &SavedConnection, groups: &[Group]) -> bool {
    match connection.group_id.as_deref() {
        None => true,
        Some(group_id) => !build_group_index(groups).contains_key(group_id),
    }
}
