//! Asset record building, filtering, and the seven-way sort.
//!
//! Mirrors the pure logic embedded in `AssetView.tsx` and the enums in
//! `src/components/app/start-workspace/types.ts`: it builds display records,
//! applies the AND filter set, and sorts with a stable default fallback so rows
//! keep their group/sort-order/name ordering when a key ties.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::assets::formatters::{
    AssetDisplayLabels, build_asset_search_text, compare_asset_address, format_accelerators,
    format_asset_address, get_asset_connection_time_ms, get_disk_total_bytes, has_gpu, has_npu,
    is_linux_asset, is_windows_asset,
};
use crate::assets::groups::{connections_for_asset_group, group_path_label};
use crate::models::{Group, SavedConnection};
use crate::natural_order::natural_compare;

/// Which start-workspace surface opens: the classic workbench or the asset
/// table. Matches the Tauri `StartWorkspaceMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartWorkspaceMode {
    #[default]
    Workbench,
    Assets,
}

impl StartWorkspaceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workbench => "workbench",
            Self::Assets => "assets",
        }
    }
}

/// Parses the persisted `ui_start_workspace_mode`, defaulting to workbench.
pub fn parse_start_workspace_mode(value: &str) -> StartWorkspaceMode {
    match value {
        "assets" => StartWorkspaceMode::Assets,
        _ => StartWorkspaceMode::Workbench,
    }
}

/// List (table) or card grid layout for the asset surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetViewMode {
    #[default]
    List,
    Cards,
}

/// Parses a persisted view-mode string, defaulting to list.
pub fn parse_asset_view_mode(value: &str) -> AssetViewMode {
    match value {
        "cards" => AssetViewMode::Cards,
        _ => AssetViewMode::List,
    }
}

/// The four AND-combined asset filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetFilterKey {
    Linux,
    Windows,
    Gpu,
    Npu,
}

/// The seven sortable columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetSortKey {
    Name,
    Address,
    ConnectionTime,
    Cpu,
    Memory,
    Storage,
    Accelerators,
}

impl AssetSortKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Address => "address",
            Self::ConnectionTime => "connectionTime",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Storage => "storage",
            Self::Accelerators => "accelerators",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "name" => Some(Self::Name),
            "address" => Some(Self::Address),
            "connectionTime" => Some(Self::ConnectionTime),
            "cpu" => Some(Self::Cpu),
            "memory" => Some(Self::Memory),
            "storage" => Some(Self::Storage),
            "accelerators" => Some(Self::Accelerators),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetSortDirection {
    Asc,
    Desc,
}

impl AssetSortDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// A resolved sort: which column and which direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetSortState {
    pub key: AssetSortKey,
    pub direction: AssetSortDirection,
}

/// Resolves the persisted sort key/direction into a state, or `None` (default
/// order) when the key is unrecognized.
pub fn normalize_asset_sort_state(
    key: Option<&str>,
    direction: Option<&str>,
) -> Option<AssetSortState> {
    let key = AssetSortKey::parse(key?)?;
    let direction = match direction {
        Some("desc") => AssetSortDirection::Desc,
        _ => AssetSortDirection::Asc,
    };
    Some(AssetSortState { key, direction })
}

/// A display row: the connection plus the precomputed values the table needs.
#[derive(Debug, Clone)]
pub struct AssetRecord {
    pub connection: SavedConnection,
    pub group_path: String,
    pub group_sort_order: i32,
    pub connection_time_ms: Option<u64>,
    pub search_text: String,
}

/// A searchable group entry for the breadcrumb group picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetGroupOption {
    pub group: Group,
    pub path: String,
    pub search_text: String,
}

/// Builds one record per connection, resolving each group path once.
pub fn build_asset_records(
    connections: &[SavedConnection],
    groups: &[Group],
    root_label: &str,
) -> Vec<AssetRecord> {
    let group_sort_by_id: HashMap<&str, i32> = groups
        .iter()
        .map(|group| (group.id.as_str(), group.sort_order))
        .collect();

    connections
        .iter()
        .map(|connection| {
            let group_path = group_path_label(groups, connection.group_id.as_deref(), root_label);
            let group_sort_order = connection
                .group_id
                .as_deref()
                .and_then(|id| group_sort_by_id.get(id).copied())
                .unwrap_or(0);
            let search_text = build_asset_search_text(connection, &group_path);
            AssetRecord {
                connection: connection.clone(),
                group_path,
                group_sort_order,
                connection_time_ms: get_asset_connection_time_ms(connection.last_used_at_ms),
                search_text,
            }
        })
        .collect()
}

/// Builds the sorted, searchable group options for the breadcrumb picker.
pub fn build_group_options(groups: &[Group], root_label: &str) -> Vec<AssetGroupOption> {
    let mut options: Vec<AssetGroupOption> = groups
        .iter()
        .map(|group| {
            let path = group_path_label(groups, Some(group.id.as_str()), root_label);
            let search_text = format!("{} {}", group.name, path).to_lowercase();
            AssetGroupOption {
                group: group.clone(),
                path,
                search_text,
            }
        })
        .collect();
    options.sort_by(|left, right| natural_compare(&left.path, &right.path));
    options
}

/// Whether a connection passes every active filter (AND semantics).
pub fn connection_matches_filters(
    connection: &SavedConnection,
    filters: &[AssetFilterKey],
) -> bool {
    if filters.is_empty() {
        return true;
    }
    let asset = connection.asset.as_ref();
    for filter in filters {
        let matches = match filter {
            AssetFilterKey::Linux => is_linux_asset(asset),
            AssetFilterKey::Windows => is_windows_asset(asset),
            AssetFilterKey::Gpu => has_gpu(asset),
            AssetFilterKey::Npu => has_npu(asset),
        };
        if !matches {
            return false;
        }
    }
    true
}

/// Sorts records in place by the resolved sort state, applying the stable
/// default-order fallback on ties.
pub fn sort_asset_records(
    records: &mut [AssetRecord],
    sort_state: Option<AssetSortState>,
    labels: &AssetDisplayLabels,
) {
    records.sort_by(|left, right| match sort_state {
        None => compare_default_asset_order(left, right),
        Some(state) if state.key == AssetSortKey::ConnectionTime => {
            let diff = compare_connection_time(left, right, state.direction);
            if diff != Ordering::Equal {
                diff
            } else {
                compare_default_asset_order(left, right)
            }
        }
        Some(state) => {
            let diff = compare_by_sort_key(left, right, state.key, labels);
            let diff = match state.direction {
                AssetSortDirection::Asc => diff,
                AssetSortDirection::Desc => diff.reverse(),
            };
            if diff != Ordering::Equal {
                diff
            } else {
                compare_default_asset_order(left, right)
            }
        }
    });
}

fn compare_default_asset_order(left: &AssetRecord, right: &AssetRecord) -> Ordering {
    left.group_sort_order
        .cmp(&right.group_sort_order)
        .then_with(|| left.connection.sort_order.cmp(&right.connection.sort_order))
        .then_with(|| natural_compare(&left.connection.name, &right.connection.name))
}

fn compare_by_sort_key(
    left: &AssetRecord,
    right: &AssetRecord,
    key: AssetSortKey,
    labels: &AssetDisplayLabels,
) -> Ordering {
    match key {
        AssetSortKey::Name => natural_compare(&left.connection.name, &right.connection.name),
        AssetSortKey::Address => compare_asset_address(
            &format_asset_address(&left.connection, labels),
            &format_asset_address(&right.connection, labels),
        ),
        AssetSortKey::ConnectionTime => {
            compare_connection_time(left, right, AssetSortDirection::Asc)
        }
        AssetSortKey::Cpu => compare_nullable_number(
            cpu_sort_value(&left.connection),
            cpu_sort_value(&right.connection),
        ),
        AssetSortKey::Memory => compare_nullable_number(
            left.connection.asset.as_ref().and_then(|a| a.memory_bytes),
            right.connection.asset.as_ref().and_then(|a| a.memory_bytes),
        ),
        AssetSortKey::Storage => compare_nullable_number(
            get_disk_total_bytes(
                left.connection
                    .asset
                    .as_ref()
                    .and_then(|a| a.disks.as_deref()),
            ),
            get_disk_total_bytes(
                right
                    .connection
                    .asset
                    .as_ref()
                    .and_then(|a| a.disks.as_deref()),
            ),
        ),
        AssetSortKey::Accelerators => {
            let count_diff = compare_nullable_number(
                left.connection
                    .asset
                    .as_ref()
                    .and_then(|a| a.accelerators.as_ref())
                    .map(|list| list.len() as u64),
                right
                    .connection
                    .asset
                    .as_ref()
                    .and_then(|a| a.accelerators.as_ref())
                    .map(|list| list.len() as u64),
            );
            if count_diff != Ordering::Equal {
                return count_diff;
            }
            natural_compare(
                &format_accelerators(
                    left.connection
                        .asset
                        .as_ref()
                        .and_then(|a| a.accelerators.as_deref()),
                    labels,
                    None,
                ),
                &format_accelerators(
                    right
                        .connection
                        .asset
                        .as_ref()
                        .and_then(|a| a.accelerators.as_deref()),
                    labels,
                    None,
                ),
            )
        }
    }
}

/// Compares last-connection times, sorting known times ahead of unknown ones
/// and applying the direction only between two known times — matching Tauri.
fn compare_connection_time(
    left: &AssetRecord,
    right: &AssetRecord,
    direction: AssetSortDirection,
) -> Ordering {
    match (left.connection_time_ms, right.connection_time_ms) {
        (Some(left_ms), Some(right_ms)) => {
            let base = left_ms.cmp(&right_ms);
            match direction {
                AssetSortDirection::Asc => base,
                AssetSortDirection::Desc => base.reverse(),
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn cpu_sort_value(connection: &SavedConnection) -> Option<u64> {
    let asset = connection.asset.as_ref()?;
    asset
        .cpu_cores
        .or(asset.cpu_threads)
        .map(|value| value as u64)
}

/// Numeric comparison that sorts present values ahead of absent ones so
/// unfilled rows sink to the bottom regardless of direction handling upstream.
fn compare_nullable_number(left: Option<u64>, right: Option<u64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Filters records to a selected group and its descendants. With no selection
/// every record is kept.
pub fn records_in_group(
    records: Vec<AssetRecord>,
    connections: &[SavedConnection],
    groups: &[Group],
    selected_group_id: Option<&str>,
) -> Vec<AssetRecord> {
    let Some(_) = selected_group_id.filter(|id| !id.is_empty()) else {
        return records;
    };
    let allowed: std::collections::HashSet<&str> =
        connections_for_asset_group(connections, groups, selected_group_id)
            .into_iter()
            .map(|connection| connection.id.as_str())
            .collect();
    records
        .into_iter()
        .filter(|record| allowed.contains(record.connection.id.as_str()))
        .collect()
}
