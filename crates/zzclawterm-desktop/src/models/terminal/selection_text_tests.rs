use futures::executor::block_on;
use zzclawterm_terminal::TerminalScreen;
use zzclawterm_terminal::selection::{TerminalBufferCellPos, TerminalSelection};

use super::{TerminalFramePipeline, TerminalSelectionTextError};
use crate::models::recording::RecordingWritePipeline;

#[test]
fn worker_extracts_a_range_larger_than_any_paint_snapshot() {
    let mut recording = RecordingWritePipeline::spawn(4096);
    let mut pipeline = TerminalFramePipeline::spawn(recording.writer());
    let output = (0..600)
        .map(|i| format!("line {i:04}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    pipeline.seed_session("s1", &output, "UTF-8", 1000);
    pipeline.flush_for_test();
    let frame = pipeline.take_session_for_transfer("s1").unwrap().unwrap();
    let history = frame.screen.history_state();
    pipeline
        .insert_sessions_from_transfer(vec![("s1".into(), frame)])
        .unwrap();
    let selection = TerminalSelection::from_range(
        TerminalBufferCellPos::new(0, 0),
        TerminalBufferCellPos::new(499, 8),
    );
    let text = block_on(pipeline.request_selection_text("s1".into(), selection, history))
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        text,
        (0..500)
            .map(|i| format!("line {i:04}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(
        block_on(pipeline.request_selection_text("missing".into(), selection, history)).unwrap(),
        Err(TerminalSelectionTextError::MissingSession)
    );
    pipeline.shutdown();
    recording.shutdown();
}

#[test]
fn worker_accepts_growth_but_rejects_eviction_reset_reflow_and_screen_switch() {
    let mut recording = RecordingWritePipeline::spawn(4096);
    let mut pipeline = TerminalFramePipeline::spawn(recording.writer());
    let selection = TerminalSelection::from_range(
        TerminalBufferCellPos::new(0, 0),
        TerminalBufferCellPos::new(0, 2),
    );
    for (id, change) in [("evict", 0), ("reset", 1), ("reflow", 2), ("alternate", 3)] {
        pipeline.seed_session(id, "abc\r\n", "UTF-8", 10);
        pipeline.flush_for_test();
        let mut frame = pipeline.take_session_for_transfer(id).unwrap().unwrap();
        let history = frame.screen.history_state();
        // Ordinary scrolling before the buffer fills keeps absolute row keys valid.
        frame.screen.advance(b"more\r\n");
        pipeline
            .insert_sessions_from_transfer(vec![(id.into(), frame)])
            .unwrap();
        assert_eq!(
            block_on(pipeline.request_selection_text(id.into(), selection, history))
                .unwrap()
                .unwrap()
                .as_deref(),
            Some("abc")
        );
        let mut frame = pipeline.take_session_for_transfer(id).unwrap().unwrap();
        match change {
            0 => frame.screen.advance("x\r\n".repeat(100).as_bytes()),
            1 => frame.screen.advance(b"\x1bc"),
            2 => frame.screen.resize(40, 24),
            _ => frame.screen.advance(b"\x1b[?1049h"),
        }
        pipeline
            .insert_sessions_from_transfer(vec![(id.into(), frame)])
            .unwrap();
        assert_eq!(
            block_on(pipeline.request_selection_text(id.into(), selection, history)).unwrap(),
            Err(TerminalSelectionTextError::StaleHistory)
        );
    }
    // A valid coordinate epoch still must not silently truncate an invalid range.
    let screen = TerminalScreen::default();
    pipeline.ensure_session("empty", "UTF-8", 100);
    let invalid = TerminalSelection::from_range(
        TerminalBufferCellPos::new(0, 0),
        TerminalBufferCellPos::new(1000, 0),
    );
    assert_eq!(
        block_on(pipeline.request_selection_text("empty".into(), invalid, screen.history_state()))
            .unwrap(),
        Err(TerminalSelectionTextError::InvalidRange)
    );
    pipeline.shutdown();
    recording.shutdown();
}
