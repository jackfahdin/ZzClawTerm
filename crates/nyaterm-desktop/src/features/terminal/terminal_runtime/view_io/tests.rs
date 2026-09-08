use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, KeyDownEvent, MouseButton, TestAppContext};
use nyaterm_terminal::TerminalScreen;

use crate::features::terminal::terminal_surface_entity::terminal_snapshot_anchor_row_for_display_offset;
use crate::features::test_support::app_with_visible_local_session;
use crate::models::{
    TerminalBufferCellPos, TerminalFrameActionLinks, TerminalSelection, TerminalViewState,
};
use crate::terminal::{TerminalBufferMatch, TerminalKeyMode};
use crate::test_support::TestConfigDir;

use super::{
    TERMINAL_INPUT_LATENCY_WINDOW, TERMINAL_USER_SCROLL_ACTIVE_WINDOW,
    lost_mouse_report_release_button, terminal_cursor_position_changed,
    terminal_cursor_visible_for_display_offset, terminal_input_latency_active,
    terminal_key_bytes_for_mode_and_settings, terminal_keyword_highlight_updates_allowed,
    terminal_live_action_link_enrichment_allowed, terminal_mouse_report_button,
    terminal_paint_snapshot_for_view, terminal_paint_window_snapshot_for_view,
    terminal_retained_snapshot_matches_view, terminal_scroll_retained_window_extra_rows,
    terminal_scroll_snapshot_request_offset, terminal_scroll_text_first_decorations,
    terminal_selection_for_session, terminal_session_write_failure_log,
    terminal_should_defer_key_text_to_input_handler_for_state,
    terminal_should_track_command_suggestion_input, terminal_snapshot_covers_display_offset,
    terminal_snapshot_with_newer_edge_row, terminal_snapshot_with_retained_scroll_window,
    terminal_status_changed, terminal_user_scroll_active, terminal_visual_display_offset,
};

#[test]
fn terminal_mouse_report_buttons_map_to_their_gpui_capture_button() {
    assert_eq!(terminal_mouse_report_button(0), Some(MouseButton::Left));
    assert_eq!(terminal_mouse_report_button(1), Some(MouseButton::Middle));
    assert_eq!(terminal_mouse_report_button(2), Some(MouseButton::Right));
    assert_eq!(terminal_mouse_report_button(3), None);
}

#[test]
fn lost_mouse_report_capture_releases_the_button_that_is_no_longer_pressed() {
    for (captured, button) in [
        (0, MouseButton::Left),
        (1, MouseButton::Middle),
        (2, MouseButton::Right),
    ] {
        assert_eq!(
            lost_mouse_report_release_button(captured, Some(button)),
            None
        );
        assert_eq!(
            lost_mouse_report_release_button(captured, None),
            Some(button)
        );
        assert_eq!(
            lost_mouse_report_release_button(
                captured,
                Some(MouseButton::Navigate(gpui::NavigationDirection::Back))
            ),
            Some(button)
        );
    }
    assert_eq!(lost_mouse_report_release_button(3, None), None);
}

#[test]
fn terminal_selection_visual_is_isolated_to_its_owner_session() {
    let selection = TerminalSelection::from_range(
        TerminalBufferCellPos::new(10, 2),
        TerminalBufferCellPos::new(11, 4),
    );

    assert_eq!(
        terminal_selection_for_session(Some(selection), Some("left"), Some("right"), "left"),
        Some(selection)
    );
    assert_eq!(
        terminal_selection_for_session(Some(selection), Some("left"), Some("right"), "right"),
        None
    );
    assert_eq!(
        terminal_selection_for_session(Some(selection), None, Some("right"), "right"),
        Some(selection)
    );
}

fn key_event(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        },
        is_held: false,
        prefer_character_input: false,
    }
}

fn terminal_output_lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index:03}\n"))
        .collect::<String>()
}

const CURSOR_POSITION_SESSION_ID: &str = "cursor-position-session";

#[test]
fn terminal_paint_snapshot_waits_without_authoritative_scrollback_snapshot() {
    let view = TerminalViewState::from_output(terminal_output_lines(40));

    assert!(terminal_paint_snapshot_for_view(Some(&view), 4, None).is_none());
}

#[test]
fn terminal_paint_snapshot_can_retain_matching_surface_snapshot() {
    let view = TerminalViewState::from_output(terminal_output_lines(40));
    let retained = std::sync::Arc::new(view.screen.viewport_snapshot(4));

    let snapshot = terminal_paint_snapshot_for_view(Some(&view), 4, Some(retained))
        .expect("matching retained surface snapshot should be usable");

    assert_eq!(snapshot.display_offset, 4);
}

#[test]
fn terminal_paint_snapshot_does_not_use_ui_screen_fallback() {
    let view = TerminalViewState::from_output(terminal_output_lines(40));

    assert!(terminal_paint_snapshot_for_view(Some(&view), 4, None).is_none());
}

#[test]
fn terminal_paint_window_snapshot_prefers_latest_live_frame_over_retained_surface() {
    let mut old_view = TerminalViewState::from_output(terminal_output_lines(40));
    old_view.frame_snapshot = Some(std::sync::Arc::new(old_view.screen.viewport_snapshot(0)));
    let mut view = TerminalViewState::from_output(terminal_output_lines(42));
    view.frame_snapshot = Some(std::sync::Arc::new(view.screen.viewport_snapshot(0)));
    let retained = old_view.frame_snapshot.clone();
    let latest = view
        .frame_snapshot
        .clone()
        .expect("view should have live frame snapshot");

    let snapshot = terminal_paint_window_snapshot_for_view(
        Some(&view),
        0,
        view.viewport_rows_for_ui(),
        retained,
    )
    .expect("live frame snapshot should be available");

    assert!(std::sync::Arc::ptr_eq(&snapshot, &latest));
}

#[test]
fn terminal_paint_window_keeps_worker_snapshot_when_ui_screen_is_stale() {
    let view = TerminalViewState::from_output(terminal_output_lines(8));
    let mut worker_screen = TerminalScreen::default();
    worker_screen.advance_decoded_text(&terminal_output_lines(160));
    let worker_snapshot = std::sync::Arc::new(worker_screen.snapshot());
    let mut view = view;
    view.frame_snapshot = Some(worker_snapshot.clone());

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), 0, view.viewport_rows_for_ui(), None)
            .expect("authoritative worker snapshot should be paintable");

    assert!(std::sync::Arc::ptr_eq(&snapshot, &worker_snapshot));
    assert!(
        snapshot
            .rows()
            .iter()
            .any(|row| row.text.contains("line 159"))
    );
}

#[test]
fn terminal_retained_snapshot_uses_worker_scrollback_length() {
    let view = TerminalViewState::from_output(terminal_output_lines(8));
    let mut worker_screen = TerminalScreen::default();
    worker_screen.advance_decoded_text(&terminal_output_lines(160));
    let worker_snapshot = std::sync::Arc::new(worker_screen.snapshot());
    let mut view = view;
    view.frame_snapshot = Some(worker_snapshot.clone());

    assert!(terminal_retained_snapshot_matches_view(
        worker_snapshot.as_ref(),
        &view,
        0,
        view.viewport_rows_for_ui(),
    ));
}

#[test]
fn terminal_paint_window_waits_for_worker_after_height_resize() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(80));
    let old_snapshot = std::sync::Arc::new(view.screen.viewport_snapshot(0));
    let old_rows = old_snapshot.row_count();
    view.frame_snapshot = Some(old_snapshot.clone());

    view.screen
        .resize(view.screen.cols() as u16, (old_rows + 16) as u16);
    view.grid_resize_pending = true;
    let viewport_rows = view.viewport_rows_for_ui();
    assert!(viewport_rows > old_rows);

    let snapshot = terminal_paint_window_snapshot_for_view(Some(&view), 0, viewport_rows, None);

    assert!(snapshot.is_none());
    assert!(std::sync::Arc::ptr_eq(
        view.frame_snapshot.as_ref().unwrap(),
        &old_snapshot,
    ));
}

#[test]
fn terminal_paint_window_waits_for_worker_after_width_resize() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(80));
    let old_snapshot = std::sync::Arc::new(view.screen.viewport_snapshot(0));
    let old_cols = old_snapshot.cols;
    view.frame_snapshot = Some(old_snapshot.clone());

    view.screen
        .resize((old_cols + 24) as u16, view.screen.rows() as u16);
    view.grid_resize_pending = true;
    let viewport_rows = view.viewport_rows_for_ui();

    let snapshot = terminal_paint_window_snapshot_for_view(Some(&view), 0, viewport_rows, None);

    assert!(snapshot.is_none());
    assert_eq!(view.frame_snapshot.as_ref().unwrap().cols, old_cols);
}

#[test]
fn terminal_retained_snapshot_accepts_current_scroll_window_geometry() {
    let view = TerminalViewState::from_output(terminal_output_lines(80));
    let display_offset = 4;
    let snapshot = view.snapshot_with_scroll_window(display_offset);
    let viewport_rows = view.viewport_rows_for_ui();

    assert!(terminal_retained_snapshot_matches_view(
        snapshot.as_ref(),
        &view,
        display_offset,
        viewport_rows,
    ));
}

#[test]
fn terminal_retained_snapshot_rejects_pre_resize_geometry() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(80));
    let display_offset = 4;
    let snapshot = view.snapshot_with_scroll_window(display_offset);
    let viewport_rows = view.viewport_rows_for_ui();
    let resized_cols = view.screen.cols().saturating_add(8) as u16;
    let resized_rows = view.screen.rows().saturating_add(6) as u16;

    view.screen.resize(resized_cols, resized_rows);

    assert!(!terminal_retained_snapshot_matches_view(
        snapshot.as_ref(),
        &view,
        display_offset,
        viewport_rows,
    ));
}

#[test]
fn terminal_visual_display_offset_keeps_text_window_stable_for_fractional_scroll() {
    assert_eq!(terminal_visual_display_offset(0, 0.0, 10), 0);
    assert_eq!(terminal_visual_display_offset(0, 0.25, 10), 0);
    assert_eq!(terminal_visual_display_offset(0, 0.5, 10), 0);
    assert_eq!(terminal_visual_display_offset(0, 0.95, 10), 0);
    assert_eq!(terminal_visual_display_offset(4, -0.25, 10), 4);
    assert_eq!(terminal_visual_display_offset(4, -0.6, 10), 4);
    assert_eq!(terminal_visual_display_offset(10, 0.5, 10), 10);
}

#[test]
fn terminal_scroll_snapshot_request_offset_waits_for_stable_text_offset() {
    assert_eq!(terminal_scroll_snapshot_request_offset(0, 0.0, 10), None);
    assert_eq!(terminal_scroll_snapshot_request_offset(0, 0.49, 10), None);
    assert_eq!(terminal_scroll_snapshot_request_offset(0, 0.5, 10), None);
    assert_eq!(
        terminal_scroll_snapshot_request_offset(4, -0.25, 10),
        Some(4)
    );
    assert_eq!(
        terminal_scroll_snapshot_request_offset(10, 0.5, 10),
        Some(10)
    );
}

#[test]
fn terminal_cursor_visibility_uses_display_offset_not_raw_scroll_offset() {
    assert!(terminal_cursor_visible_for_display_offset(
        true, false, 0, true, false, false
    ));
    assert!(!terminal_cursor_visible_for_display_offset(
        true, false, 1, true, false, false
    ));
    assert!(!terminal_cursor_visible_for_display_offset(
        true, false, 0, true, true, false
    ));
}

#[test]
fn terminal_cursor_position_change_is_detected_for_a_new_live_position() {
    assert!(terminal_cursor_position_changed(Some((4, 7)), Some((4, 8)),));
}

#[test]
fn moving_the_live_cursor_resets_a_hidden_blink_phase_before_paint() {
    let root = TestConfigDir::new("nyaterm-cursor-position");
    let mut cx = TestAppContext::single();
    let app = app_with_visible_local_session(&mut cx, root.path(), CURSOR_POSITION_SESSION_ID);

    cx.update_entity(&app, |app, cx| {
        let mut screen = TerminalScreen::default();
        screen.advance_decoded_text("prompt");
        let old_snapshot = Arc::new(screen.snapshot());
        screen.advance(b"\x1b[1C");
        let new_snapshot = Arc::new(screen.snapshot());

        app.terminal
            .view
            .views
            .get_mut(CURSOR_POSITION_SESSION_ID)
            .expect("fixture session view should exist")
            .frame_snapshot = Some(old_snapshot);
        app.sync_terminal_surface_paint(CURSOR_POSITION_SESSION_ID, cx);

        app.shell.set_cursor_blink_on(false);
        app.terminal
            .view
            .views
            .get_mut(CURSOR_POSITION_SESSION_ID)
            .expect("fixture session view should exist")
            .frame_snapshot = Some(new_snapshot);
        app.sync_terminal_surface_paint(CURSOR_POSITION_SESSION_ID, cx);

        assert!(app.shell.cursor_blink_on());
        let surface = app
            .terminal
            .view
            .surfaces
            .get(CURSOR_POSITION_SESSION_ID)
            .expect("fixture terminal surface should exist");
        assert!(surface.read(cx).cursor_is_shown());
    });
}

#[test]
fn terminal_snapshot_edge_row_extends_fractional_scroll_window() {
    let view = TerminalViewState::from_output(terminal_output_lines(40));
    let base = std::sync::Arc::new(view.screen.viewport_snapshot(1));
    let newer = std::sync::Arc::new(view.screen.viewport_snapshot(0));
    let base_rows = base.row_count();
    let newer_tail = newer.rows().last().map(|row| row.text.clone());

    let snapshot = terminal_snapshot_with_newer_edge_row(base, newer);

    assert_eq!(snapshot.row_count(), base_rows + 1);
    assert_eq!(
        snapshot.rows().last().map(|row| row.text.clone()),
        newer_tail
    );
    assert!(
        snapshot
            .rows()
            .iter()
            .all(|row| row.cells.len() == snapshot.cols)
    );
}

#[test]
fn terminal_snapshot_edge_row_preserves_absolute_range_start() {
    let view = TerminalViewState::from_output(terminal_output_lines(40));
    let base = std::sync::Arc::new(view.screen.viewport_snapshot(1));
    let newer = std::sync::Arc::new(view.screen.viewport_snapshot(0));
    let (base_start, _) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(
            base.as_ref(),
        );

    let snapshot = terminal_snapshot_with_newer_edge_row(base, newer);
    let (start, end) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(
            snapshot.as_ref(),
        );

    assert_eq!(start, base_start);
    assert_eq!(
        end,
        snapshot.total_rows.saturating_sub(snapshot.display_offset)
    );
}

#[test]
fn terminal_paint_window_snapshot_reuses_cached_retained_window_for_neighbor_offsets() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(80));
    let display_offset = 6;
    let viewport_rows = view.viewport_rows_for_ui();
    let scrollback_len = view.scrollback_len_for_ui();
    let retained = terminal_snapshot_with_retained_scroll_window(
        &view,
        std::sync::Arc::new(view.screen.viewport_snapshot(display_offset)),
        display_offset,
        viewport_rows,
        scrollback_len,
    );
    view.scrollback_snapshots.insert(display_offset, retained);

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None)
            .expect("window snapshot should be available");

    assert!(snapshot.row_count() > viewport_rows);
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset,
        viewport_rows,
        scrollback_len
    ));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset - 1,
        viewport_rows,
        scrollback_len
    ));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset + 1,
        viewport_rows,
        scrollback_len
    ));
    assert!(
        terminal_snapshot_anchor_row_for_display_offset(
            snapshot.as_ref(),
            display_offset,
            viewport_rows,
            scrollback_len
        ) > 0
    );
}

#[test]
fn terminal_paint_window_snapshot_reuses_covering_cached_retained_window() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(160));
    let cached_offset = 40;
    let target_offset = cached_offset + 2;
    let viewport_rows = view.viewport_rows_for_ui();
    let scrollback_len = view.scrollback_len_for_ui();
    let retained = terminal_snapshot_with_retained_scroll_window(
        &view,
        std::sync::Arc::new(view.screen.viewport_snapshot(cached_offset)),
        cached_offset,
        viewport_rows,
        scrollback_len,
    );
    assert!(terminal_snapshot_covers_display_offset(
        retained.as_ref(),
        target_offset,
        viewport_rows,
        scrollback_len
    ));
    view.scrollback_snapshots
        .insert(cached_offset, retained.clone());

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), target_offset, viewport_rows, None)
            .expect("covering retained window should be reused");

    assert!(std::sync::Arc::ptr_eq(&snapshot, &retained));
    let anchor = terminal_snapshot_anchor_row_for_display_offset(
        snapshot.as_ref(),
        target_offset,
        viewport_rows,
        scrollback_len,
    );
    assert_eq!(
        snapshot.line(anchor),
        view.screen.viewport_snapshot(target_offset).line(0)
    );
}

#[test]
fn terminal_paint_window_snapshot_reuses_cached_retained_window_without_rewrapping() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(120));
    let display_offset = 12;
    let viewport_rows = view.viewport_rows_for_ui();
    let scrollback_len = view.scrollback_len_for_ui();
    let base = std::sync::Arc::new(view.screen.viewport_snapshot(display_offset));
    let retained = terminal_snapshot_with_retained_scroll_window(
        &view,
        base,
        display_offset,
        viewport_rows,
        scrollback_len,
    );
    let retained_rows = retained.row_count();
    view.scrollback_snapshots
        .insert(display_offset, retained.clone());

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None)
            .expect("cached retained window should be reusable");

    assert_eq!(snapshot.row_count(), retained_rows);
    assert!(std::sync::Arc::ptr_eq(&snapshot, &retained));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset,
        viewport_rows,
        scrollback_len
    ));
}

#[test]
fn terminal_paint_window_snapshot_covers_viewport_sized_scroll_runs() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(160));
    let display_offset = 40;
    let viewport_rows = view.viewport_rows_for_ui();
    let scrollback_len = view.scrollback_len_for_ui();
    let base = std::sync::Arc::new(view.screen.viewport_snapshot(display_offset));
    let retained = terminal_snapshot_with_retained_scroll_window(
        &view,
        base,
        display_offset,
        viewport_rows,
        scrollback_len,
    );
    view.scrollback_snapshots.insert(display_offset, retained);

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None)
            .expect("cached window snapshot should cover direct scroll runs");

    assert!(viewport_rows >= 16);
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset.saturating_sub(viewport_rows),
        viewport_rows,
        scrollback_len
    ));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset,
        viewport_rows,
        scrollback_len
    ));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset + viewport_rows,
        viewport_rows,
        scrollback_len
    ));
}

#[test]
fn terminal_paint_window_snapshot_covers_multi_viewport_fast_scroll_runs() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(240));
    let display_offset = 80;
    let viewport_rows = view.viewport_rows_for_ui();
    let scrollback_len = view.scrollback_len_for_ui();
    let fast_delta = viewport_rows.saturating_mul(2);
    let base = std::sync::Arc::new(view.screen.viewport_snapshot(display_offset));
    let retained = terminal_snapshot_with_retained_scroll_window(
        &view,
        base,
        display_offset,
        viewport_rows,
        scrollback_len,
    );
    view.scrollback_snapshots.insert(display_offset, retained);

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None)
            .expect("cached window snapshot should cover multi-viewport fast scroll runs");

    assert!(
        snapshot
            .rows()
            .iter()
            .all(|row| row.cells.len() == snapshot.cols)
    );
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset.saturating_sub(fast_delta),
        viewport_rows,
        scrollback_len
    ));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset,
        viewport_rows,
        scrollback_len
    ));
    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset + fast_delta,
        viewport_rows,
        scrollback_len
    ));

    for offset in [
        display_offset.saturating_sub(fast_delta),
        display_offset,
        display_offset + fast_delta,
    ] {
        let anchor = terminal_snapshot_anchor_row_for_display_offset(
            snapshot.as_ref(),
            offset,
            viewport_rows,
            scrollback_len,
        );
        assert_eq!(
            snapshot.line(anchor),
            view.screen.viewport_snapshot(offset).line(0)
        );
    }
}

#[test]
fn terminal_scroll_retained_window_extra_rows_covers_fast_scroll_runs() {
    assert_eq!(terminal_scroll_retained_window_extra_rows(12), 32);
    assert_eq!(terminal_scroll_retained_window_extra_rows(40), 80);
    assert_eq!(terminal_scroll_retained_window_extra_rows(120), 192);
}

#[test]
fn terminal_paint_window_snapshot_waits_without_cached_snapshot() {
    let view = TerminalViewState::from_output(terminal_output_lines(80));
    let display_offset = 6;
    let viewport_rows = view.viewport_rows_for_ui();

    assert!(
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None,)
            .is_none()
    );
}

#[test]
fn terminal_paint_window_snapshot_reuses_cached_authoritative_snapshot() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(80));
    let display_offset = 6;
    let viewport_rows = view.viewport_rows_for_ui();
    let scrollback_len = view.scrollback_len_for_ui();
    let retained = terminal_snapshot_with_retained_scroll_window(
        &view,
        std::sync::Arc::new(view.screen.viewport_snapshot(display_offset)),
        display_offset,
        viewport_rows,
        scrollback_len,
    );
    view.scrollback_snapshots.insert(display_offset, retained);

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None)
            .expect("cached scrolled paint window should be used");

    assert!(terminal_snapshot_covers_display_offset(
        snapshot.as_ref(),
        display_offset,
        viewport_rows,
        scrollback_len
    ));
    assert_eq!(
        snapshot.line(terminal_snapshot_anchor_row_for_display_offset(
            snapshot.as_ref(),
            display_offset,
            viewport_rows,
            scrollback_len
        )),
        view.screen.viewport_snapshot(display_offset).line(0)
    );
}

#[test]
fn terminal_paint_window_snapshot_preserves_view_absolute_start() {
    let mut view = TerminalViewState::from_output(terminal_output_lines(80));
    let display_offset = 4;
    let viewport_rows = view.viewport_rows_for_ui();
    let base = std::sync::Arc::new(view.screen.viewport_snapshot(display_offset));
    view.scrollback_snapshots
        .insert(display_offset, base.clone());
    let (base_start, _) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(
            base.as_ref(),
        );

    let snapshot =
        terminal_paint_window_snapshot_for_view(Some(&view), display_offset, viewport_rows, None)
            .expect("window snapshot should be available");
    let anchor = terminal_snapshot_anchor_row_for_display_offset(
        snapshot.as_ref(),
        display_offset,
        viewport_rows,
        view.scrollback_len_for_ui(),
    );
    let (window_start, _) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(
            snapshot.as_ref(),
        );

    assert_eq!(window_start + anchor, base_start);
}

#[test]
fn terminal_scroll_text_first_decorations_keep_search_overlay_without_links() {
    let view = TerminalViewState::from_output(terminal_output_lines(80));
    let snapshot = view.screen.viewport_snapshot(6);
    let (abs_start, _) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(&snapshot);
    let matches = vec![TerminalBufferMatch {
        line_index: abs_start + 1,
        start_col: 2,
        end_col: 6,
    }];

    let decorations = terminal_scroll_text_first_decorations(
        &snapshot,
        Some(matches.as_slice()),
        &[],
        true,
        true,
    );

    assert_eq!(decorations.len(), snapshot.row_count());
    assert_eq!(decorations[1].search_ranges, vec![(2, 6)]);
    assert!(decorations[1].active_search_ranges.is_empty());
    assert!(decorations[1].link_ranges.is_empty());
}

#[test]
fn terminal_scroll_text_first_decorations_include_links_for_current_snapshot() {
    let mut screen = TerminalScreen::new(40, 3);
    screen.advance(b"\x1b]8;;https://example.com\x07click\x1b]8;;\x07 plain");
    let snapshot = screen.viewport_snapshot(0);
    let (absolute_start_row, absolute_end_row) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(&snapshot);
    let mut links = TerminalFrameActionLinks {
        matcher_key: 1,
        absolute_start_row,
        absolute_end_row,
        row_signatures: snapshot.rows().iter().map(|row| row.signature).collect(),
        matches_by_line: vec![Vec::new(); snapshot.row_count()],
        cell_ranges_by_line: vec![Vec::new(); snapshot.row_count()],
    };
    links.cell_ranges_by_line[0].push((6, 11));

    let decorations = terminal_scroll_text_first_decorations(
        &snapshot,
        None,
        std::slice::from_ref(&links),
        true,
        true,
    );

    assert_eq!(decorations.len(), snapshot.row_count());
    assert_eq!(decorations[0].link_ranges, vec![(6, 11), (0, 5)]);
    assert!(decorations[0].search_ranges.is_empty());
}

#[test]
fn terminal_scroll_text_first_decorations_map_covering_action_link_window() {
    let view = TerminalViewState::from_output(terminal_output_lines(80));
    let snapshot = view.screen.viewport_snapshot(6);
    let (snapshot_start, snapshot_end) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(&snapshot);
    let mut links = TerminalFrameActionLinks {
        matcher_key: 1,
        absolute_start_row: snapshot_start.saturating_sub(2),
        absolute_end_row: snapshot_end.saturating_add(2),
        row_signatures: vec![0; snapshot.row_count() + 4],
        matches_by_line: vec![Vec::new(); snapshot.row_count() + 4],
        cell_ranges_by_line: vec![Vec::new(); snapshot.row_count() + 4],
    };
    let relative_row = snapshot_start + 1 - links.absolute_start_row;
    links.row_signatures[relative_row] = snapshot.rows()[1].signature;
    links.cell_ranges_by_line[relative_row].push((3, 8));

    let decorations = terminal_scroll_text_first_decorations(
        &snapshot,
        None,
        std::slice::from_ref(&links),
        true,
        true,
    );

    assert_eq!(decorations.len(), snapshot.row_count());
    assert!(decorations[0].link_ranges.is_empty());
    assert_eq!(decorations[1].link_ranges, vec![(3, 8)]);
}

#[test]
fn terminal_scroll_text_first_decorations_keep_partially_covered_bottom_links() {
    let view = TerminalViewState::from_output(terminal_output_lines(80));
    let snapshot = view.screen.viewport_snapshot(6);
    let (snapshot_start, snapshot_end) =
        crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range(&snapshot);
    let top_links = TerminalFrameActionLinks {
        matcher_key: 1,
        absolute_start_row: snapshot_start,
        absolute_end_row: snapshot_start + 1,
        row_signatures: vec![snapshot.rows()[0].signature],
        matches_by_line: vec![Vec::new()],
        cell_ranges_by_line: vec![vec![(1, 4)]],
    };
    let bottom_links = TerminalFrameActionLinks {
        matcher_key: 1,
        absolute_start_row: snapshot_end - 1,
        absolute_end_row: snapshot_end,
        row_signatures: vec![snapshot.rows()[snapshot.row_count() - 1].signature],
        matches_by_line: vec![Vec::new()],
        cell_ranges_by_line: vec![vec![(3, 8)]],
    };

    let decorations = terminal_scroll_text_first_decorations(
        &snapshot,
        None,
        &[top_links, bottom_links],
        true,
        true,
    );

    assert_eq!(decorations.len(), snapshot.row_count());
    assert_eq!(decorations[0].link_ranges, vec![(1, 4)]);
    assert_eq!(
        decorations[snapshot.row_count() - 1].link_ranges,
        vec![(3, 8)]
    );
}

#[test]
fn terminal_keyword_highlight_updates_stay_enabled_for_active_surface() {
    assert!(terminal_keyword_highlight_updates_allowed(true, false));
    assert!(!terminal_keyword_highlight_updates_allowed(true, true));
}

#[test]
fn terminal_keyword_highlight_updates_pause_for_inactive_surface() {
    assert!(!terminal_keyword_highlight_updates_allowed(false, false));
}

#[test]
fn live_action_link_enrichment_waits_for_input_and_output_to_calm() {
    assert!(terminal_live_action_link_enrichment_allowed(
        0, true, false, false
    ));
    assert!(!terminal_live_action_link_enrichment_allowed(
        0, true, true, false
    ));
    assert!(!terminal_live_action_link_enrichment_allowed(
        0, true, false, true
    ));
    assert!(!terminal_live_action_link_enrichment_allowed(
        1, true, false, false
    ));
}

#[test]
fn terminal_user_scroll_active_requires_scrolled_surface_and_recent_input() {
    let now = Instant::now();

    assert!(!terminal_user_scroll_active(0, true, Some(now), now));
    assert!(terminal_user_scroll_active(4, true, Some(now), now));
    assert!(!terminal_user_scroll_active(4, false, Some(now), now));
    assert!(!terminal_user_scroll_active(
        4,
        true,
        Some(now - TERMINAL_USER_SCROLL_ACTIVE_WINDOW - Duration::from_millis(1)),
        now,
    ));
    assert!(!terminal_user_scroll_active(4, true, None, now));
}

#[test]
fn terminal_input_latency_active_uses_short_idle_window() {
    let now = Instant::now();

    assert!(terminal_input_latency_active(Some(now), now));
    assert!(!terminal_input_latency_active(
        Some(now - TERMINAL_INPUT_LATENCY_WINDOW - Duration::from_millis(1)),
        now,
    ));
    assert!(!terminal_input_latency_active(None, now));
}

#[test]
fn terminal_command_suggestion_input_tracking_skips_low_latency_mode() {
    assert!(terminal_should_track_command_suggestion_input(
        true, false, true
    ));
    assert!(!terminal_should_track_command_suggestion_input(
        true, true, true
    ));
    assert!(!terminal_should_track_command_suggestion_input(
        false, false, true
    ));
}

#[test]
fn terminal_command_suggestion_input_tracking_skips_disabled_suggestions() {
    assert!(!terminal_should_track_command_suggestion_input(
        true, false, false
    ));
}

#[test]
fn terminal_session_write_failure_log_escapes_control_text() {
    let log = terminal_session_write_failure_log("input", "closed\r\n\x1b[31m");

    assert_eq!(
        log,
        "\n# session write failed (input): closed\\r\\n\\x1b[31m\n"
    );
}

#[test]
fn terminal_key_encoding_uses_target_session_mode() {
    let event = key_event("up", None, gpui::Modifiers::default());
    let normal =
        terminal_key_bytes_for_mode_and_settings(&event, TerminalKeyMode::default(), false)
            .unwrap();
    let application = terminal_key_bytes_for_mode_and_settings(
        &event,
        TerminalKeyMode {
            application_cursor: true,
            ..TerminalKeyMode::default()
        },
        false,
    )
    .unwrap();

    assert_eq!(normal, b"\x1b[A".to_vec());
    assert_eq!(application, b"\x1bOA".to_vec());
}

#[test]
fn terminal_key_encoding_sends_plain_tab_to_pty() {
    let event = key_event("tab", None, gpui::Modifiers::default());

    assert_eq!(
        terminal_key_bytes_for_mode_and_settings(&event, TerminalKeyMode::default(), false)
            .unwrap(),
        b"\t".to_vec()
    );
}

#[test]
fn terminal_key_encoding_sends_shift_tab_to_pty() {
    let event = key_event(
        "tab",
        None,
        gpui::Modifiers {
            shift: true,
            ..gpui::Modifiers::default()
        },
    );

    assert_eq!(
        terminal_key_bytes_for_mode_and_settings(&event, TerminalKeyMode::default(), false)
            .unwrap(),
        b"\x1b[Z".to_vec()
    );
}

#[test]
fn terminal_key_encoding_keeps_alt_meta_setting_outside_mode() {
    let event = key_event(
        "x",
        Some("x"),
        gpui::Modifiers {
            alt: true,
            ..gpui::Modifiers::default()
        },
    );

    assert_eq!(
        terminal_key_bytes_for_mode_and_settings(&event, TerminalKeyMode::default(), true,)
            .unwrap(),
        b"\x1bx".to_vec()
    );
    assert!(
        terminal_key_bytes_for_mode_and_settings(&event, TerminalKeyMode::default(), false,)
            .is_none()
    );
}

#[test]
fn ime_defer_does_not_swallow_plain_space_without_marked_text() {
    let event = key_event("space", None, gpui::Modifiers::default());

    assert!(!terminal_should_defer_key_text_to_input_handler_for_state(
        true, "", &event
    ));
}

#[test]
fn ime_defer_keeps_space_for_active_marked_text() {
    let event = key_event("space", None, gpui::Modifiers::default());

    assert!(terminal_should_defer_key_text_to_input_handler_for_state(
        true, "ni", &event
    ));
}

#[test]
fn ime_defer_keeps_non_ascii_text_for_input_handler() {
    let event = key_event("あ", Some("あ"), gpui::Modifiers::default());

    assert!(terminal_should_defer_key_text_to_input_handler_for_state(
        true, "", &event
    ));
}

#[test]
fn terminal_status_changed_detects_identical_text() {
    assert!(!terminal_status_changed("sent 1 byte(s)", "sent 1 byte(s)"));
    assert!(terminal_status_changed("idle", "sent 1 byte(s)"));
}
