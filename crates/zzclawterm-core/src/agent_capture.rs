use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use base64::{Engine as _, engine::general_purpose};

use crate::AiExecutionProfile;

const MARKER_PREFIX: &str = "__DF_CMD_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCapturedOutput {
    pub marker_id: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCaptureProcessResult {
    pub visible_text: String,
    pub completed: Vec<AgentCapturedOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCaptureCancelResult {
    pub marker_id: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapturePhase {
    WaitingForStart,
    Capturing,
    PostCapture,
}

#[derive(Debug)]
struct ActiveCapture {
    buffer: String,
    phase: CapturePhase,
    start_time: Instant,
}

#[derive(Debug, Default)]
pub struct AgentOutputCaptureProcessor {
    active: HashMap<String, ActiveCapture>,
    pending_marker_tail: String,
    completed: VecDeque<AgentCapturedOutput>,
}

pub fn build_agent_capture_command(
    profile: AiExecutionProfile,
    marker_id: &str,
    command: &str,
) -> Option<String> {
    match profile {
        AiExecutionProfile::Posix => Some(build_posix_capture_command(marker_id, command)),
        AiExecutionProfile::Powershell => {
            Some(build_powershell_capture_command(marker_id, command))
        }
        AiExecutionProfile::Cmd => Some(build_cmd_capture_command(marker_id, command)),
        AiExecutionProfile::Auto | AiExecutionProfile::SendOnly | AiExecutionProfile::Disabled => {
            None
        }
    }
}

fn build_posix_capture_command(marker_id: &str, command: &str) -> String {
    format!(
        " printf '\\n{MARKER_PREFIX}''START_{marker_id}__\\n'; {{ {command}; }}; _dfec=$?; printf '\\n{MARKER_PREFIX}''END_{marker_id}_'\"$_dfec\"'__\\n'; unset _dfec\n",
    )
}

fn build_powershell_capture_command(marker_id: &str, command: &str) -> String {
    let encoded_command = general_purpose::STANDARD.encode(command.as_bytes());
    format!(
        concat!(
            "$nyaiEc = 0; ",
            "$nyaiSuccess = $true; ",
            "$nyaiLastExit = 0; ",
            "$global:LASTEXITCODE = 0; ",
            "Write-Output (\"`n{MARKER_PREFIX}\" + \"START_{marker_id}__\"); ",
            "try {{ ",
            "$nyaiScript = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String(\"{encoded_command}\")); ",
            "$nyaiScript = $nyaiScript + \"`r`n`$nyaiSuccess = `$?; `$nyaiLastExit = `$LASTEXITCODE\"; ",
            ". ([scriptblock]::Create($nyaiScript)); ",
            "if (($nyaiLastExit -is [int]) -and $nyaiLastExit -ne 0) {{ $nyaiEc = $nyaiLastExit }} ",
            "elseif ($nyaiSuccess) {{ $nyaiEc = 0 }} else {{ $nyaiEc = 1 ",
            "}} ",
            "}} catch {{ Write-Error $_; $nyaiEc = 1 }}; ",
            "Write-Output (\"`n{MARKER_PREFIX}\" + \"END_{marker_id}_\" + $nyaiEc + \"__\"); ",
            "Remove-Variable nyaiEc,nyaiSuccess,nyaiLastExit,nyaiScript -ErrorAction SilentlyContinue\r\n",
        ),
        MARKER_PREFIX = MARKER_PREFIX,
        marker_id = marker_id,
        encoded_command = encoded_command,
    )
}

fn build_cmd_capture_command(marker_id: &str, command: &str) -> String {
    let command = command.replace("\r\n", " & ").replace(['\n', '\r'], " & ");
    let command = command.trim();
    let command_segment = if command.is_empty() {
        String::new()
    } else {
        format!(" & {command}")
    };

    format!(
        concat!(
            "echo {MARKER_PREFIX}^START_{marker_id}__",
            "{command_segment}",
            " & call echo {MARKER_PREFIX}^END_{marker_id}_^%ERRORLEVEL^%__\r\n",
        ),
        MARKER_PREFIX = MARKER_PREFIX,
        marker_id = marker_id,
        command_segment = command_segment,
    )
}

impl AgentOutputCaptureProcessor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, marker_id: String) {
        self.active.insert(
            marker_id,
            ActiveCapture {
                buffer: String::new(),
                phase: CapturePhase::WaitingForStart,
                start_time: Instant::now(),
            },
        );
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn cancel(&mut self, marker_id: &str) -> Option<AgentCaptureCancelResult> {
        let capture = self.active.remove(marker_id)?;
        if self.active.is_empty() {
            self.pending_marker_tail.clear();
        }
        Some(AgentCaptureCancelResult {
            marker_id: marker_id.to_string(),
            output: capture.buffer,
            exit_code: None,
            duration_ms: capture.start_time.elapsed().as_millis() as u64,
        })
    }

    pub fn process(&mut self, text: &str) -> AgentCaptureProcessResult {
        if self.active.is_empty() {
            return AgentCaptureProcessResult {
                visible_text: text.to_string(),
                completed: self.drain_completed(),
            };
        }

        let combined;
        let mut remaining = if self.pending_marker_tail.is_empty() {
            text
        } else {
            combined = format!("{}{}", self.pending_marker_tail, text);
            self.pending_marker_tail.clear();
            combined.as_str()
        };
        let mut visible_text = String::with_capacity(text.len());

        while !remaining.is_empty() {
            if let Some(result) = self.try_match_start(remaining) {
                remaining = result.after;
                continue;
            }

            if let Some(result) = self.try_match_end(remaining) {
                visible_text.push_str(result.before);
                remaining = result.after;
                continue;
            }

            if let Some(capture_id) = self.any_in_phase(CapturePhase::Capturing) {
                if let Some(pos) = remaining.find(MARKER_PREFIX) {
                    if let Some(cap) = self.active.get_mut(&capture_id) {
                        cap.buffer.push_str(&remaining[..pos]);
                    }
                    let candidate = &remaining[pos..];
                    if self.is_possible_marker_prefix(candidate) {
                        self.pending_marker_tail.push_str(candidate);
                        remaining = "";
                    } else if pos == 0 {
                        if let Some(cap) = self.active.get_mut(&capture_id) {
                            cap.buffer.push_str(MARKER_PREFIX);
                        }
                        remaining = &remaining[MARKER_PREFIX.len()..];
                    } else {
                        remaining = &remaining[pos..];
                    }
                } else {
                    if let Some(cap) = self.active.get_mut(&capture_id) {
                        cap.buffer.push_str(remaining);
                    }
                    remaining = "";
                }
            } else if let Some(capture_id) = self.any_in_phase(CapturePhase::PostCapture) {
                self.active.remove(&capture_id);
                if self.active.is_empty() {
                    self.pending_marker_tail.clear();
                }
                remaining = "";
            } else if self.any_in_phase(CapturePhase::WaitingForStart).is_some() {
                if let Some(tail_start) = self.possible_marker_tail_start(remaining) {
                    self.pending_marker_tail.push_str(&remaining[tail_start..]);
                }
                remaining = "";
            } else if let Some(pos) = remaining.find(MARKER_PREFIX) {
                visible_text.push_str(&remaining[..pos]);
                if pos == 0 {
                    visible_text.push_str(MARKER_PREFIX);
                    remaining = &remaining[MARKER_PREFIX.len()..];
                } else {
                    remaining = &remaining[pos..];
                }
            } else {
                visible_text.push_str(remaining);
                remaining = "";
            }
        }

        AgentCaptureProcessResult {
            visible_text,
            completed: self.drain_completed(),
        }
    }

    fn drain_completed(&mut self) -> Vec<AgentCapturedOutput> {
        self.completed.drain(..).collect()
    }

    fn any_in_phase(&self, target: CapturePhase) -> Option<String> {
        self.active
            .iter()
            .find(|(_, cap)| cap.phase == target)
            .map(|(id, _)| id.clone())
    }

    fn try_match_start<'a>(&mut self, text: &'a str) -> Option<MatchResult<'a>> {
        let prefix = format!("{MARKER_PREFIX}START_");
        let start_pos = text.find(&prefix)?;
        let after_prefix = &text[start_pos + prefix.len()..];
        let end_suffix = "__";
        let suffix_pos = after_prefix.find(end_suffix)?;
        let marker_id = &after_prefix[..suffix_pos];
        if !self.active.contains_key(marker_id) {
            return None;
        }
        if let Some(cap) = self.active.get_mut(marker_id) {
            cap.phase = CapturePhase::Capturing;
        }

        let marker_end = start_pos + prefix.len() + suffix_pos + end_suffix.len();
        let after_marker = &text[marker_end..];
        let after = after_marker
            .strip_prefix("\r\n")
            .or_else(|| after_marker.strip_prefix('\n'))
            .unwrap_or(after_marker);
        Some(MatchResult { before: "", after })
    }

    fn try_match_end<'a>(&mut self, text: &'a str) -> Option<MatchResult<'a>> {
        let prefix = format!("{MARKER_PREFIX}END_");
        let start_pos = text.find(&prefix)?;
        let after_prefix = &text[start_pos + prefix.len()..];
        let end_suffix = "__";
        let suffix_pos = after_prefix.find(end_suffix)?;
        let inner = &after_prefix[..suffix_pos];
        let last_underscore = inner.rfind('_')?;
        let marker_id = &inner[..last_underscore];
        let code_str = &inner[last_underscore + 1..];
        let exit_code = code_str.parse::<i32>().ok();

        let capture = self.active.get_mut(marker_id)?;
        let before = &text[..start_pos];
        let mut output = std::mem::take(&mut capture.buffer);
        output.push_str(before);
        let output = output.trim().to_string();
        self.completed.push_back(AgentCapturedOutput {
            marker_id: marker_id.to_string(),
            output,
            exit_code,
            duration_ms: capture.start_time.elapsed().as_millis() as u64,
        });
        capture.phase = CapturePhase::PostCapture;

        Some(MatchResult {
            before: "",
            after: "",
        })
    }

    fn possible_marker_tail_start(&self, text: &str) -> Option<usize> {
        text.char_indices()
            .filter_map(|(idx, _)| self.is_possible_marker_prefix(&text[idx..]).then_some(idx))
            .min_by_key(|idx| *idx)
    }

    fn is_possible_marker_prefix(&self, value: &str) -> bool {
        if value.is_empty() {
            return false;
        }
        if MARKER_PREFIX.starts_with(value) {
            return true;
        }

        self.active.keys().any(|marker_id| {
            let start_marker = format!("{MARKER_PREFIX}START_{marker_id}__");
            if start_marker.starts_with(value) {
                return true;
            }

            let end_prefix = format!("{MARKER_PREFIX}END_{marker_id}_");
            if end_prefix.starts_with(value) {
                return true;
            }
            value.starts_with(&end_prefix)
                && value[end_prefix.len()..]
                    .chars()
                    .all(|ch| ch.is_ascii_digit() || ch == '-')
        })
    }
}

struct MatchResult<'a> {
    before: &'a str,
    after: &'a str,
}

#[cfg(test)]
mod tests {
    use super::{AgentOutputCaptureProcessor, build_agent_capture_command};
    use crate::AiExecutionProfile;

    #[test]
    fn builders_do_not_embed_matchable_markers_in_input_text() {
        for profile in [
            AiExecutionProfile::Posix,
            AiExecutionProfile::Powershell,
            AiExecutionProfile::Cmd,
        ] {
            let command = build_agent_capture_command(profile, "marker-1", "echo ok").unwrap();
            assert!(!command.contains("__DF_CMD_START_marker-1__"));
            assert!(!command.contains("__DF_CMD_END_marker-1_0__"));
        }
    }

    #[test]
    fn powershell_builder_is_single_logical_input_line() {
        let command = build_agent_capture_command(
            AiExecutionProfile::Powershell,
            "marker-1",
            "Write-Output 'ok'\r\n# comment",
        )
        .unwrap();
        let command = command.strip_suffix("\r\n").unwrap();

        assert!(!command.contains('\r'));
        assert!(!command.contains('\n'));
        assert!(command.contains("[scriptblock]::Create($nyaiScript)"));
        assert!(!command.contains("Write-Output 'ok'"));
    }

    #[test]
    fn cmd_builder_is_single_logical_input_line() {
        let command = build_agent_capture_command(
            AiExecutionProfile::Cmd,
            "marker-1",
            "echo one\r\necho two",
        )
        .unwrap();
        let command = command.strip_suffix("\r\n").unwrap();

        assert!(!command.contains('\r'));
        assert!(!command.contains('\n'));
        assert!(command.contains("echo one & echo two"));
        assert!(command.contains("call echo"));
        assert!(command.contains("^%ERRORLEVEL^%"));
    }

    #[test]
    fn unsupported_profiles_do_not_build_capture_commands() {
        for profile in [
            AiExecutionProfile::Auto,
            AiExecutionProfile::SendOnly,
            AiExecutionProfile::Disabled,
        ] {
            assert!(build_agent_capture_command(profile, "marker-1", "echo ok").is_none());
        }
    }

    #[test]
    fn captures_crlf_output_with_prompt_before_markers() {
        let mut processor = AgentOutputCaptureProcessor::new();
        processor.register("m1".to_string());

        let result = processor.process(
            "C:\\>echo marker\r\n__DF_CMD_START_m1__\r\nok\r\n__DF_CMD_END_m1_7__\r\nC:\\>",
        );

        assert!(result.visible_text.is_empty());
        assert_eq!(result.completed.len(), 1);
        assert_eq!(result.completed[0].output, "ok");
        assert_eq!(result.completed[0].exit_code, Some(7));
    }

    #[test]
    fn captures_start_marker_split_across_chunks() {
        let mut processor = AgentOutputCaptureProcessor::new();
        processor.register("m2".to_string());

        assert!(processor.process("__DF_CMD_STA").visible_text.is_empty());
        assert!(
            processor
                .process("RT_m2__\nhello\n")
                .visible_text
                .is_empty()
        );
        let result = processor.process("__DF_CMD_END_m2_0__\n");

        assert!(result.visible_text.is_empty());
        assert_eq!(result.completed.len(), 1);
        assert_eq!(result.completed[0].output, "hello");
        assert_eq!(result.completed[0].exit_code, Some(0));
    }

    #[test]
    fn captures_end_marker_split_across_chunks() {
        let mut processor = AgentOutputCaptureProcessor::new();
        processor.register("m3".to_string());

        assert!(
            processor
                .process("__DF_CMD_START_m3__\nhello\n__DF_CMD_EN")
                .visible_text
                .is_empty()
        );
        let result = processor.process("D_m3_9__\n");

        assert!(result.visible_text.is_empty());
        assert_eq!(result.completed.len(), 1);
        assert_eq!(result.completed[0].output, "hello");
        assert_eq!(result.completed[0].exit_code, Some(9));
    }

    #[test]
    fn cancel_clears_pending_marker_tail() {
        let mut processor = AgentOutputCaptureProcessor::new();
        processor.register("m4".to_string());

        assert!(processor.process("__DF_CMD_STA").visible_text.is_empty());
        assert!(!processor.pending_marker_tail.is_empty());

        let cancelled = processor.cancel("m4").unwrap();
        assert_eq!(cancelled.marker_id, "m4");
        assert!(!processor.has_active());
        assert!(processor.pending_marker_tail.is_empty());
        let result = processor.process("plain output");

        assert_eq!(result.visible_text, "plain output");
        assert!(result.completed.is_empty());
    }
}
