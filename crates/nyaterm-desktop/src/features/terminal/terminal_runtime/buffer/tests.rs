use crate::models::WorkspaceSplitDirection;
use crate::models::{
    TerminalFrameSearchKey, TerminalFrameSearchPurpose, TerminalFrameSearchResult,
    TerminalPresentation, TerminalSearchMode, TerminalViewState, TerminalWindowNode,
    TerminalWorkPolicy,
};
use nyaterm_terminal::{TerminalClipboardLoad, TerminalEffects};
use std::sync::Arc;

use super::{
    MAX_OSC52_REPLY_CHARS, TERMINAL_FRAME_EVENT_DRAIN_BATCH,
    TERMINAL_FRAME_EVENT_DRAIN_WALL_BUDGET, TERMINAL_FRAME_INPUT_WAKE_EVENT_DRAIN_BATCH,
    TERMINAL_FRAME_INPUT_WAKE_EVENT_DRAIN_WALL_BUDGET, TerminalFrameSearchKeys,
    TerminalSurfaceFrameNotify, limit_osc52_clipboard_reply_text,
    queue_osc52_clipboard_load_replies, terminal_apply_search_result_to_view,
    terminal_effects_need_ui_apply, terminal_local_log_text,
    terminal_output_frame_needs_chrome_notify, terminal_output_frame_surface_notify,
    terminal_search_frame_apply_result, terminal_selected_occurrence_frame_is_current,
    terminal_snapshot_priority_session_ids, terminal_window_node_visible_tab_ids,
};

fn search_key(query: &str) -> TerminalFrameSearchKey {
    TerminalFrameSearchKey {
        query: query.to_string(),
        case_sensitive: false,
        regex: false,
        whole_word: false,
        limit: 1000,
        request_generation: 0,
    }
}

#[test]
fn terminal_frame_input_wake_budget_prioritizes_ui_latency() {
    const {
        assert!(TERMINAL_FRAME_INPUT_WAKE_EVENT_DRAIN_BATCH < TERMINAL_FRAME_EVENT_DRAIN_BATCH);
    }
    assert!(
        TERMINAL_FRAME_INPUT_WAKE_EVENT_DRAIN_WALL_BUDGET < TERMINAL_FRAME_EVENT_DRAIN_WALL_BUDGET
    );
}

#[test]
fn osc52_clipboard_reply_limit_borrows_small_text() {
    let text = "small clipboard";
    let limited = limit_osc52_clipboard_reply_text(text);
    assert!(matches!(limited, std::borrow::Cow::Borrowed(_)));
    assert_eq!(limited.as_ref(), text);
}

#[test]
fn osc52_clipboard_reply_limit_preserves_utf8_boundary() {
    let text = format!("{}界", "好".repeat(MAX_OSC52_REPLY_CHARS));
    let limited = limit_osc52_clipboard_reply_text(&text);
    assert!(matches!(limited, std::borrow::Cow::Owned(_)));
    assert_eq!(limited.chars().count(), MAX_OSC52_REPLY_CHARS);
    assert!(limited.chars().all(|ch| ch == '好'));
}

#[test]
fn osc52_clipboard_load_reply_uses_empty_text_when_clipboard_unavailable() {
    let mut formatters: Vec<TerminalClipboardLoad> = vec![Arc::new(|text| format!("reply:{text}"))];
    let mut replies = Vec::new();

    queue_osc52_clipboard_load_replies(&mut formatters, "", &mut replies);

    assert!(formatters.is_empty());
    assert_eq!(replies, vec![b"reply:".to_vec()]);
}

#[test]
fn terminal_effects_skip_ui_apply_for_plain_output() {
    assert!(!terminal_effects_need_ui_apply(&TerminalEffects::default()));

    let effects = TerminalEffects {
        bell: true,
        ..TerminalEffects::default()
    };
    assert!(terminal_effects_need_ui_apply(&effects));

    let mut effects = TerminalEffects::default();
    effects.pty_write.push(b"\x1b[6n".to_vec());
    assert!(terminal_effects_need_ui_apply(&effects));

    let effects = TerminalEffects {
        shell_command_finished: true,
        ..TerminalEffects::default()
    };
    assert!(terminal_effects_need_ui_apply(&effects));
}

#[test]
fn terminal_output_frame_notify_tracks_visible_unread_or_effects() {
    assert!(!terminal_output_frame_needs_chrome_notify(false, false));
    assert!(!terminal_output_frame_needs_chrome_notify(false, false));
    assert!(terminal_output_frame_needs_chrome_notify(true, false));
    assert!(terminal_output_frame_needs_chrome_notify(false, true));
    assert_eq!(
        terminal_output_frame_surface_notify(TerminalPresentation::VisibleActive, 0, 8),
        Some(TerminalSurfaceFrameNotify::Full(String::new()))
    );
    assert_eq!(
        terminal_output_frame_surface_notify(TerminalPresentation::VisibleInactive, 5, 8),
        Some(TerminalSurfaceFrameNotify::ScrollPositionOnly(String::new()))
    );
    assert_eq!(
        terminal_output_frame_surface_notify(TerminalPresentation::Background, 0, 8),
        None
    );
    assert_eq!(
        terminal_output_frame_surface_notify(TerminalPresentation::VisibleActive, 0, 0),
        None
    );
}

#[test]
fn terminal_search_frame_notify_targets_active_visible_buffer_query() {
    let key = search_key("needle");

    let result = terminal_search_frame_apply_result(
        "active".to_string(),
        true,
        true,
        Some("active"),
        true,
        TerminalSearchMode::Buffer,
        TerminalFrameSearchKeys {
            current: Some(&key),
            result: &key,
        },
    );

    assert!(result.chrome_dirty);
    assert_eq!(
        result.surface_notify,
        Some(TerminalSurfaceFrameNotify::Full("active".to_string()))
    );
}

#[test]
fn terminal_search_frame_notify_ignores_non_visible_or_stale_queries() {
    let key = search_key("needle");
    let other = search_key("other");

    for result in [
        terminal_search_frame_apply_result(
            "active".to_string(),
            false,
            true,
            Some("active"),
            true,
            TerminalSearchMode::Buffer,
            TerminalFrameSearchKeys {
                current: Some(&key),
                result: &key,
            },
        ),
        terminal_search_frame_apply_result(
            "active".to_string(),
            true,
            false,
            Some("active"),
            true,
            TerminalSearchMode::Buffer,
            TerminalFrameSearchKeys {
                current: Some(&key),
                result: &key,
            },
        ),
        terminal_search_frame_apply_result(
            "background".to_string(),
            true,
            true,
            Some("active"),
            true,
            TerminalSearchMode::Buffer,
            TerminalFrameSearchKeys {
                current: Some(&key),
                result: &key,
            },
        ),
        terminal_search_frame_apply_result(
            "active".to_string(),
            true,
            true,
            Some("active"),
            true,
            TerminalSearchMode::History,
            TerminalFrameSearchKeys {
                current: Some(&key),
                result: &key,
            },
        ),
        terminal_search_frame_apply_result(
            "active".to_string(),
            true,
            true,
            Some("active"),
            true,
            TerminalSearchMode::Buffer,
            TerminalFrameSearchKeys {
                current: Some(&other),
                result: &key,
            },
        ),
    ] {
        assert!(!result.chrome_dirty);
        assert_eq!(result.surface_notify, None);
    }
}

#[test]
fn selected_occurrence_frames_are_rejected_after_selection_clear() {
    let key = search_key("needle");
    let next_generation = TerminalFrameSearchKey {
        request_generation: 1,
        ..key.clone()
    };

    assert!(terminal_selected_occurrence_frame_is_current(
        Some("session"),
        Some("needle"),
        Some(&key),
        None,
        "session",
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &key,
    ));
    assert!(!terminal_selected_occurrence_frame_is_current(
        None,
        None,
        None,
        None,
        "session",
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &key,
    ));
    assert!(!terminal_selected_occurrence_frame_is_current(
        Some("session"),
        Some("needle"),
        None,
        None,
        "session",
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &key,
    ));
    assert!(!terminal_selected_occurrence_frame_is_current(
        Some("other"),
        Some("needle"),
        Some(&key),
        None,
        "session",
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &key,
    ));
    assert!(!terminal_selected_occurrence_frame_is_current(
        Some("session"),
        Some("needle"),
        Some(&next_generation),
        None,
        "session",
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &key,
    ));
}

#[test]
fn terminal_search_results_keep_find_and_selected_occurrence_slots_independent() {
    let mut view = TerminalViewState::new();
    view.screen_revision = 7;
    let find = TerminalFrameSearchResult::new(search_key("find"), 7, Ok(Vec::new()));
    let selected = TerminalFrameSearchResult::new(
        TerminalFrameSearchKey {
            request_generation: 3,
            ..search_key("selected")
        },
        7,
        Ok(Vec::new()),
    );

    assert!(terminal_apply_search_result_to_view(
        &mut view,
        TerminalFrameSearchPurpose::Find,
        &find,
        false,
    ));
    assert!(terminal_apply_search_result_to_view(
        &mut view,
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &selected,
        true,
    ));
    assert_eq!(
        view.search_result.as_ref().map(|result| &result.key),
        Some(&find.key)
    );
    assert_eq!(
        view.selected_occurrence_result
            .as_ref()
            .map(|result| &result.key),
        Some(&selected.key)
    );

    let visible = TerminalFrameSearchResult::new(selected.key.clone(), 7, Ok(Vec::new()));
    assert!(terminal_apply_search_result_to_view(
        &mut view,
        TerminalFrameSearchPurpose::SelectedOccurrenceVisible {
            absolute_start: 0,
            absolute_end: 24,
        },
        &visible,
        true,
    ));
    assert_eq!(
        view.selected_occurrence_visible_result
            .as_ref()
            .map(|result| &result.key),
        Some(&selected.key)
    );

    let replacement_find = TerminalFrameSearchResult {
        key: search_key("replacement"),
        ..find.clone()
    };
    assert!(terminal_apply_search_result_to_view(
        &mut view,
        TerminalFrameSearchPurpose::Find,
        &replacement_find,
        false,
    ));
    assert_eq!(
        view.selected_occurrence_result
            .as_ref()
            .map(|result| &result.key),
        Some(&selected.key)
    );
}

#[test]
fn terminal_search_results_reject_stale_revision_and_occurrence_generation() {
    let mut view = TerminalViewState::new();
    view.screen_revision = 9;
    let stale_revision = TerminalFrameSearchResult::new(search_key("find"), 8, Ok(Vec::new()));
    let stale_generation = TerminalFrameSearchResult::new(
        TerminalFrameSearchKey {
            request_generation: 1,
            ..search_key("selected")
        },
        9,
        Ok(Vec::new()),
    );

    assert!(!terminal_apply_search_result_to_view(
        &mut view,
        TerminalFrameSearchPurpose::Find,
        &stale_revision,
        false,
    ));
    assert!(!terminal_apply_search_result_to_view(
        &mut view,
        TerminalFrameSearchPurpose::SelectedOccurrence,
        &stale_generation,
        false,
    ));
    assert!(view.search_result.is_none());
    assert!(view.selected_occurrence_result.is_none());
}

#[test]
fn terminal_window_visible_tab_ids_returns_leaf_active_tabs() {
    let root = TerminalWindowNode::Split {
        id: "split".to_string(),
        direction: WorkspaceSplitDirection::Vertical,
        ratio_percent: 50,
        first: Box::new(TerminalWindowNode::Leaf {
            id: "left".to_string(),
            tab_ids: vec!["a".to_string(), "b".to_string()],
            active_tab_id: Some("b".to_string()),
        }),
        second: Box::new(TerminalWindowNode::Leaf {
            id: "right".to_string(),
            tab_ids: vec!["c".to_string()],
            active_tab_id: Some("c".to_string()),
        }),
    };

    assert_eq!(terminal_window_node_visible_tab_ids(&root), vec!["b", "c"]);
}

#[test]
fn split_visible_inactive_is_not_background_hidden_tab() {
    let visible = ["b", "c"];
    let priority = terminal_snapshot_priority_session_ids(Some("b"), &visible);

    let active = TerminalPresentation::resolve(true, priority.iter().any(|id| id == "b"));
    let split_inactive = TerminalPresentation::resolve(false, priority.iter().any(|id| id == "c"));
    let hidden_tab = TerminalPresentation::resolve(false, priority.iter().any(|id| id == "a"));

    assert_eq!(active, TerminalPresentation::VisibleActive);
    assert_eq!(split_inactive, TerminalPresentation::VisibleInactive);
    assert_eq!(hidden_tab, TerminalPresentation::Background);
    assert!(TerminalWorkPolicy::for_presentation(split_inactive).live_snapshot);
    assert!(!TerminalWorkPolicy::for_presentation(split_inactive).active_decorations);
    assert!(!TerminalWorkPolicy::for_presentation(hidden_tab).live_snapshot);
}

#[test]
fn terminal_local_log_text_preserves_framing_but_escapes_controls() {
    let text = "\n# started evil\x1b]52;c;AAAA\x07\tpath\r\n";
    let escaped = terminal_local_log_text(text);

    assert_eq!(
        escaped.as_ref(),
        "\n# started evil\\x1b]52;c;AAAA\\u{7}\tpath\r\n"
    );
    assert!(!escaped.contains('\x1b'));
    assert!(!escaped.contains('\x07'));
    assert!(escaped.starts_with('\n'));
    assert!(escaped.ends_with("\r\n"));
}
