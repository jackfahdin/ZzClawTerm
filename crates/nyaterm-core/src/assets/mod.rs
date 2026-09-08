//! UI-independent asset-workspace logic.
//!
//! This module mirrors the Tauri edition's asset workspace helpers
//! (`src/lib/assetGroups.ts`, `src/components/app/start-workspace/
//! assetFormatters.ts`, `src/lib/assetMonitoring.ts`, and the sort/filter logic
//! in `AssetView.tsx`) so the GPUI home workspace produces identical grouping,
//! formatting, search text, filtering, sorting, and monitoring-cache behavior.
//!
//! It owns no GPUI or transport types. Monitoring snapshots reach these
//! functions through the small, transport-neutral input structs in
//! [`snapshot`]; the desktop layer adapts `nyaterm-transport` values into them.

mod formatters;
mod groups;
mod monitoring;
mod snapshot;
mod sort;

#[cfg(test)]
mod tests;

pub use formatters::{
    AssetDisplayLabels, build_asset_search_text, compare_asset_address, format_accelerators,
    format_asset_address, format_asset_connection_time, format_asset_system,
    format_asset_updated_at, format_bytes, format_cpu_summary, format_disk_summary,
    get_asset_connection_time_ms, get_disk_total_bytes, has_gpu, has_npu, is_linux_asset,
    is_windows_asset,
};
pub use groups::{
    ASSET_ROOT_SEGMENT_KEY, AssetGroupPathSegment, build_group_index, build_group_path,
    collect_descendant_group_ids, connections_for_asset_group, group_path_label,
    is_ungrouped_connection,
};
pub use monitoring::{
    AssetMonitoringCache, AssetMonitoringCacheEntry, RecordAssetMonitoringPatch,
    build_asset_patch_from_accelerator_snapshot, build_asset_patch_from_stats_snapshot,
};
pub use snapshot::{AssetAcceleratorSnapshot, AssetDiskSnapshot, AssetStatsSnapshot};
pub use sort::{
    AssetFilterKey, AssetGroupOption, AssetRecord, AssetSortDirection, AssetSortKey,
    AssetSortState, AssetViewMode, StartWorkspaceMode, build_asset_records, build_group_options,
    connection_matches_filters, normalize_asset_sort_state, parse_asset_view_mode,
    parse_start_workspace_mode, records_in_group, sort_asset_records,
};
