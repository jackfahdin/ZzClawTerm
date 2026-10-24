use std::collections::VecDeque;

use gpui::{AppContext as _, ClipboardItem, TestAppContext};
use zzclawterm_core::ActionLinksMatcherSettings;
use zzclawterm_terminal::selection::{TerminalBufferCellPos, TerminalSelection};

use crate::features::ZzClawTermApp;
use crate::features::terminal::state::TerminalSelectionTextStatus;
use crate::features::test_support::app_with_visible_local_session;
use crate::models::{TerminalFrameEvent, TerminalSelectionTextError};
use crate::test_support::{TestAppCleanup, TestConfigDir};

fn seed_cross_screen_selection(app: &mut ZzClawTermApp) {
    let output = (0..600)
        .map(|i| format!("line {i:04}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    let pipeline = &app.terminal.view.frame_pipeline;
    pipeline.seed_session("s1", output, "UTF-8", 1000);
    pipeline.request_snapshot("s1", 0, false, ActionLinksMatcherSettings::default());
    pipeline.flush_for_test();
    let mut events = VecDeque::new();
    pipeline.drain_events_into(&mut events, usize::MAX);
    let snapshot = events
        .into_iter()
        .find_map(|event| match event {
            TerminalFrameEvent::Snapshot(frame) => Some(frame.snapshot),
            _ => None,
        })
        .expect("worker snapshot");
    assert!(snapshot.row_count() < 500);
    app.terminal.selection.source_history = Some(snapshot.history);
    let view = app.terminal.view.views.get_mut("s1").unwrap();
    assert!(view.screen.all_text().is_none());
    view.frame_snapshot = Some(snapshot);
    app.terminal.selection.session_id = Some("s1".into());
    app.terminal.selection.selection = Some(TerminalSelection::from_range(
        TerminalBufferCellPos::new(0, 0),
        TerminalBufferCellPos::new(499, 8),
    ));
}

#[test]
fn pending_menu_keeps_copy_enabled_and_copies_the_complete_selection_once() {
    let dir = TestConfigDir::new("zzclawterm-cross-screen-copy");
    let mut cx = TestAppContext::single();
    let _cleanup = TestAppCleanup::new(&cx);
    let app = app_with_visible_local_session(&mut cx, dir.path(), "s1");
    let (key, generation) = cx.update_entity(&app, |app, cx| {
        seed_cross_screen_selection(app);
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        let items = app.terminal_context_menu_items_for_session("s1".into(), cx);
        let disabled = |label| {
            items
                .iter()
                .find(|item| item.test_label() == label)
                .unwrap()
                .test_presentation()
                .3
        };
        assert!(!disabled("Copy"));
        assert!(disabled("Find..."));
        assert!(disabled("Save as Quick Command"));
        assert!(!disabled("Paste"));
        assert!(matches!(
            app.terminal.selection.text.status,
            TerminalSelectionTextStatus::Pending
        ));
        let key = app.terminal.selection.text.key.clone().unwrap();
        let generation = app.terminal.selection.text.generation;
        // Automatic copy and repeated explicit copies coalesce on the same reply.
        app.copy_terminal_selection(cx);
        app.copy_terminal_selection_request(&key, generation, cx);
        app.copy_terminal_selection_or_visible(cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("unchanged")
        );
        app.terminal.view.frame_pipeline.flush_for_test();
        (key, generation)
    });
    cx.run_until_parked();
    cx.update_entity(&app, |app, cx| {
        let expected = (0..500)
            .map(|i| format!("line {i:04}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            app.selected_terminal_text().as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some(expected.as_str())
        );
        let items = app.terminal_context_menu_items_for_session("s1".into(), cx);
        assert!(
            !items
                .iter()
                .find(|item| item.test_label() == "Find...")
                .unwrap()
                .test_presentation()
                .3
        );
        let other = app.terminal_context_menu_items_for_session("other".into(), cx);
        assert!(!other.iter().any(|item| item.test_label() == "Copy"));
        cx.write_to_clipboard(ClipboardItem::new_string("new clipboard".into()));
        app.copy_terminal_selection_request(&key, generation, cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("new clipboard")
        );
    });
}

#[test]
fn clearing_replacing_or_switching_selection_discards_a_pending_copy() {
    for action in 0..4 {
        let dir = TestConfigDir::new("zzclawterm-cross-screen-cancel");
        let mut cx = TestAppContext::single();
        let _cleanup = TestAppCleanup::new(&cx);
        let app = app_with_visible_local_session(&mut cx, dir.path(), "s1");
        cx.update_entity(&app, |app, cx| {
            seed_cross_screen_selection(app);
            cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
            app.copy_terminal_selection_or_visible(cx);
            app.terminal.view.frame_pipeline.flush_for_test();
            match action {
                0 => app.clear_terminal_selection(cx),
                1 => {
                    // The reply is already queued, but belongs to the old endpoints.
                    app.terminal
                        .selection
                        .selection
                        .as_mut()
                        .unwrap()
                        .head
                        .position
                        .line = 400;
                    app.ensure_terminal_selection_text(cx);
                    app.terminal.view.frame_pipeline.flush_for_test();
                }
                2 => app.clear_terminal_selection_state_for_session("s1"),
                _ => {
                    app.terminal.clear_activation_interaction();
                }
            }
        });
        cx.run_until_parked();
        cx.update_entity(&app, |_, cx| {
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some("unchanged")
            );
        });
    }
}

#[test]
fn window_transfer_cancels_the_source_copy_and_reextracts_in_the_destination() {
    let source_dir = TestConfigDir::new("zzclawterm-selection-transfer-source");
    let target_dir = TestConfigDir::new("zzclawterm-selection-transfer-target");
    let mut cx = TestAppContext::single();
    let _cleanup = TestAppCleanup::new(&cx);
    let source = app_with_visible_local_session(&mut cx, source_dir.path(), "s1");
    let target = app_with_visible_local_session(&mut cx, target_dir.path(), "s1");
    let bundle = cx.update_entity(&source, |app, cx| {
        seed_cross_screen_selection(app);
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        app.copy_terminal_selection_or_visible(cx);
        app.terminal.view.frame_pipeline.flush_for_test();
        let frames = app
            .terminal
            .prepare_sessions_for_transfer(&["s1".into()])
            .unwrap();
        app.terminal.detach_sessions_for_transfer(frames)
    });
    cx.update_entity(&target, |app, cx| {
        assert!(app.terminal.attach_sessions_from_transfer(bundle).is_ok());
        app.ensure_terminal_selection_text(cx);
        app.terminal.view.frame_pipeline.flush_for_test();
    });
    cx.run_until_parked();
    cx.update_entity(&source, |app, cx| {
        assert!(app.terminal.selection.selection.is_none());
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("unchanged")
        );
    });
    cx.update_entity(&target, |app, _| {
        let expected = (0..500)
            .map(|i| format!("line {i:04}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            app.selected_terminal_text().as_deref(),
            Some(expected.as_str())
        );
    });
}

#[test]
fn stale_history_keeps_the_selection_menu_and_never_copies_visible_text() {
    let dir = TestConfigDir::new("zzclawterm-cross-screen-stale");
    let mut cx = TestAppContext::single();
    let _cleanup = TestAppCleanup::new(&cx);
    let app = app_with_visible_local_session(&mut cx, dir.path(), "s1");
    cx.update_entity(&app, |app, cx| {
        seed_cross_screen_selection(app);
        app.terminal
            .view
            .frame_pipeline
            .submit_decoded_output("s1".into(), "\x1bcnew screen".into());
        app.terminal.view.frame_pipeline.flush_for_test();
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        app.copy_terminal_selection_or_visible(cx);
        app.terminal.view.frame_pipeline.flush_for_test();
    });
    cx.run_until_parked();
    cx.update_entity(&app, |app, cx| {
        assert!(matches!(
            app.terminal.selection.text.status,
            TerminalSelectionTextStatus::Failed(TerminalSelectionTextError::StaleHistory)
        ));
        assert!(app.selected_terminal_text().is_none());
        let items = app.terminal_context_menu_items_for_session("s1".into(), cx);
        assert!(
            items
                .iter()
                .find(|item| item.test_label() == "Copy")
                .unwrap()
                .test_presentation()
                .3
        );
        app.copy_terminal_selection_or_visible(cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("unchanged")
        );
    });
}
