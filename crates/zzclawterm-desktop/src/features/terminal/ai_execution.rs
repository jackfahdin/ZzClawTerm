use zzclawterm_terminal::recording_sanitizer::RecordingSanitizer;

use crate::features::ZzClawTermApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::features) enum TerminalExecutionStatus {
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Disconnected,
}

pub(in crate::features) enum TerminalExecutionEvent {
    Started {
        id: String,
        session_id: String,
        source: String,
        command: String,
        step_index: u16,
        max_lines: u16,
    },
    Finished {
        id: String,
        session_id: String,
        source: String,
        command: String,
        step_index: u16,
        max_lines: u16,
        output: String,
        exit_code: Option<i32>,
        duration_ms: u64,
        source_truncated: bool,
        status: TerminalExecutionStatus,
    },
}

const RESET: &str = "\x1b[0m";
const ACCENT: &str = "\x1b[2;38;5;141m";

fn render_execution(event: TerminalExecutionEvent) -> (String, String) {
    match event {
        TerminalExecutionEvent::Started {
            id,
            session_id,
            source,
            command,
            step_index,
            max_lines,
        } => {
            debug_assert!(!id.is_empty());
            if max_lines == 0 {
                return (session_id, String::new());
            }
            let command = RecordingSanitizer::default().feed(&command.replace(['\r', '\n'], " ; "));
            let source = RecordingSanitizer::default()
                .feed(&source)
                .replace('\n', " ");
            (
                session_id,
                format!(
                    "\r\n\x1b[1m{command}{RESET}\r\n{ACCENT}┌ AI #{} · {source} ────────────────────{RESET}\r\n",
                    u32::from(step_index) + 1
                ),
            )
        }
        TerminalExecutionEvent::Finished {
            id,
            session_id,
            source,
            command,
            step_index,
            max_lines,
            output,
            exit_code,
            duration_ms,
            source_truncated,
            status,
        } => {
            debug_assert!(!id.is_empty());
            // The end event carries the same identity even if the view changed meanwhile.
            let _ = (source, command, step_index);
            if max_lines == 0 {
                return (session_id, String::new());
            }
            let output = RecordingSanitizer::default().feed(&output);
            let mut text = String::new();
            let mut lines = output.lines();
            for line in lines.by_ref().take(usize::from(max_lines)) {
                text.push_str(&format!("{ACCENT}│{RESET} {line}\r\n"));
            }
            if lines.next().is_some() || source_truncated {
                text.push_str(&format!("{ACCENT}│{RESET} …\r\n"));
            }
            let label = match status {
                TerminalExecutionStatus::TimedOut => {
                    rust_i18n::t!("ai.executionTimedOut").to_string()
                }
                TerminalExecutionStatus::Cancelled => {
                    rust_i18n::t!("ai.executionCancelled").to_string()
                }
                TerminalExecutionStatus::Disconnected => {
                    rust_i18n::t!("ai.executionDisconnected").to_string()
                }
                TerminalExecutionStatus::Failed => rust_i18n::t!("ai.executionFailed").to_string(),
                TerminalExecutionStatus::Completed => match exit_code {
                    Some(0) => "✓ exit 0".into(),
                    Some(code) => format!("✗ exit {code}"),
                    None => format!("? {}", rust_i18n::t!("ai.executionUnknownExit")),
                },
            };
            text.push_str(&format!("{ACCENT}└{RESET} {label} · {duration_ms}ms\r\n"));
            (session_id, text)
        }
    }
}

impl ZzClawTermApp {
    pub(in crate::features) fn present_terminal_execution(&self, event: TerminalExecutionEvent) {
        let (session_id, text) = render_execution(event);
        if !text.is_empty() {
            // Presentation goes directly to the ordered frame queue, never through I/O.
            self.terminal.view.frame_pipeline.ensure_session(
                session_id.clone(),
                self.effective_session_encoding(&session_id),
                self.terminal_scrollback_line_limit(),
            );
            self.terminal
                .view
                .frame_pipeline
                .submit_synthetic_output(session_id, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TerminalExecutionEvent, TerminalExecutionStatus, render_execution};

    fn end(lines: u16, output: &str) -> TerminalExecutionEvent {
        TerminalExecutionEvent::Finished {
            id: "task".into(),
            session_id: "terminal".into(),
            source: "MCP".into(),
            command: "pwd".into(),
            step_index: 0,
            max_lines: lines,
            output: output.into(),
            exit_code: Some(0),
            duration_ms: 10,
            source_truncated: false,
            status: TerminalExecutionStatus::Completed,
        }
    }

    #[test]
    fn presentation_truncates_without_interpreting_remote_control_sequences() {
        let (_, text) = render_execution(end(1, "\x1b[2Jfirst\x1b]52;c;secret\x07\nsecond"));
        assert!(text.contains("first"));
        assert!(!text.contains("second"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("\x1b[2J"));
        assert!(text.contains('…'));
        assert!(text.contains("exit 0"));
    }

    #[test]
    fn zero_lines_disables_both_start_and_end_blocks() {
        assert!(render_execution(end(0, "output")).1.is_empty());
        let start = TerminalExecutionEvent::Started {
            id: "task".into(),
            session_id: "terminal".into(),
            source: "AI".into(),
            command: "pwd".into(),
            step_index: 0,
            max_lines: 0,
        };
        assert!(render_execution(start).1.is_empty());
    }
}
