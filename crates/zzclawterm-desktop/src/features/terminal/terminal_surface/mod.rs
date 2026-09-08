mod decorations;
mod helpers;
pub(in crate::features) use decorations::{
    TerminalDecorationSources, build_terminal_line_decorations,
    terminal_absolute_line_for_snapshot_row, terminal_action_links_cover_all_snapshot_rows,
    terminal_action_links_for_paint_snapshot, terminal_action_links_have_ranges_for_snapshot,
    terminal_action_links_overlap_snapshot, terminal_line_decorations_cache_key,
    terminal_line_decorations_needed, terminal_snapshot_absolute_range,
};

mod scrollbar;
pub(in crate::features) use scrollbar::{
    TERMINAL_SCROLLBAR_COLUMN_WIDTH, TERMINAL_SCROLLBAR_MIN_THUMB_HEIGHT,
    TERMINAL_SCROLLBAR_TRACK_PADDING_RIGHT, TERMINAL_SCROLLBAR_TRACK_PADDING_Y,
    TerminalOverviewMarker, TerminalOverviewMarkerBucket, TerminalOverviewMarkerKind,
    TerminalScrollbarDragState, TerminalScrollbarInput, TerminalScrollbarMetrics,
    terminal_overview_marker_buckets, terminal_overview_marker_canvas,
    terminal_scroll_offset_from_pointer, terminal_scrollbar_grab_offset_for_pointer,
    terminal_scrollbar_metrics, terminal_scrollbar_thumb_element,
    terminal_scrollbar_track_bounds_tracker, terminal_scrollbar_track_color, track_height,
};

mod canvas;
mod chrome;
