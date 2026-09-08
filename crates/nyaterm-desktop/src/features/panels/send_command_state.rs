//! Grouped send-command bar state.
//!
//! The bar composes one payload, chooses how to send it, and then reports
//! progress while sending. Those are three distinct phases and the twenty-four
//! `send_command_*` fields interleaved them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::FocusHandle;
use nyaterm_transport::SessionKind;

use crate::send_command::{
    SendCommandControlFocus, SendCommandDataType, SendCommandLineEnding, SendCommandMode,
    SendCommandTarget, build_send_command_units_for, format_send_command_hex_display,
};

pub(in crate::features) struct SendCommandFeatureState {
    composer: SendCommandComposerState,
    options: SendCommandOptionsState,
    progress: SendCommandProgressState,
}

/// Focus handles the send-command bar needs at construction time.
pub(in crate::features) struct SendCommandFeatureFocus {
    pub editor: FocusHandle,
}

/// The payload being composed and where the caret is.
struct SendCommandComposerState {
    draft: String,
    focus: FocusHandle,
    hex_scroll_x: f32,
    hex_scroll_y: f32,
}

/// How the payload is interpreted and delivered.
struct SendCommandOptionsState {
    data_type: SendCommandDataType,
    mode: SendCommandMode,
    line_ending: SendCommandLineEnding,
    target: SendCommandTarget,
    count: Option<u32>,
    count_input: String,
    interval_seconds: f64,
    interval_input: String,
}

/// In-flight send: cancellation flag and the counters shown in the bar.
struct SendCommandProgressState {
    sending: bool,
    cancel: Option<Arc<AtomicBool>>,
    completed: u32,
    total: u32,
    round: u32,
    rounds: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::features) struct SendCommandPresentationState {
    pub draft: String,
    pub hex_scroll_x: f32,
    pub hex_scroll_y: f32,
    pub data_type: SendCommandDataType,
    pub mode: SendCommandMode,
    pub line_ending: SendCommandLineEnding,
    pub target: SendCommandTarget,
    pub count_input: String,
    pub interval_input: String,
    pub sending: bool,
    pub completed: u32,
    pub total: u32,
    pub round: u32,
    pub rounds: u32,
}

pub(in crate::features) struct SendCommandRunState {
    pub cancel: Arc<AtomicBool>,
    pub infinite: bool,
    pub rounds: u32,
    pub interval_seconds: f64,
    pub raw_units: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) struct SendCommandProgressResult {
    pub completed: u32,
    pub total: u32,
    pub round: u32,
}

impl SendCommandFeatureState {
    pub(in crate::features) fn new(focus: SendCommandFeatureFocus) -> Self {
        Self {
            composer: SendCommandComposerState {
                draft: String::new(),
                focus: focus.editor,
                hex_scroll_x: 0.,
                hex_scroll_y: 0.,
            },
            options: SendCommandOptionsState {
                data_type: SendCommandDataType::Text,
                mode: SendCommandMode::Line,
                line_ending: SendCommandLineEnding::Crlf,
                target: SendCommandTarget::Current,
                count: Some(1),
                count_input: "1".to_string(),
                interval_seconds: 1.0,
                interval_input: "1.00".to_string(),
            },
            progress: SendCommandProgressState {
                sending: false,
                cancel: None,
                completed: 0,
                total: 0,
                round: 0,
                rounds: 0,
            },
        }
    }

    pub(in crate::features) fn presentation(&self) -> SendCommandPresentationState {
        SendCommandPresentationState {
            draft: self.composer.draft.clone(),
            hex_scroll_x: self.composer.hex_scroll_x,
            hex_scroll_y: self.composer.hex_scroll_y,
            data_type: self.options.data_type,
            mode: self.options.mode,
            line_ending: self.options.line_ending,
            target: self.options.target.clone(),
            count_input: self.options.count_input.clone(),
            interval_input: self.options.interval_input.clone(),
            sending: self.progress.sending,
            completed: self.progress.completed,
            total: self.progress.total,
            round: self.progress.round,
            rounds: self.progress.rounds,
        }
    }

    pub(in crate::features) fn editor_focus(&self) -> &FocusHandle {
        &self.composer.focus
    }

    pub(in crate::features) fn is_sending(&self) -> bool {
        self.progress.sending
    }

    pub(in crate::features) fn target(&self) -> &SendCommandTarget {
        &self.options.target
    }

    pub(in crate::features) fn apply_control_input(
        &mut self,
        control: SendCommandControlFocus,
        value: String,
    ) -> bool {
        if self.progress.sending {
            return false;
        }
        match control {
            SendCommandControlFocus::Count => {
                self.options.count_input = value;
                self.options.apply_count_input(true);
            }
            SendCommandControlFocus::Interval => {
                self.options.interval_input = value;
                self.options.apply_interval_input(true);
            }
        }
        true
    }

    pub(in crate::features) fn synced_control_input(
        &mut self,
        control: SendCommandControlFocus,
    ) -> String {
        match control {
            SendCommandControlFocus::Count => self.sync_count_input(),
            SendCommandControlFocus::Interval => self.sync_interval_input(),
        }
    }

    pub(in crate::features) fn clear_draft(&mut self) {
        self.composer.draft.clear();
    }

    pub(in crate::features) fn apply_draft(&mut self, text: String) -> Option<String> {
        if self.options.data_type == SendCommandDataType::Hex {
            let cleaned: String = text.chars().filter(|ch| ch.is_ascii_hexdigit()).collect();
            let formatted = format_send_command_hex_display(&cleaned);
            self.composer.draft = formatted.clone();
            self.composer.clamp_hex_scroll();
            (formatted != text).then_some(formatted)
        } else {
            self.composer.draft = text;
            None
        }
    }

    pub(in crate::features) fn draft_for_send(&self, append_enter: bool) -> String {
        let mut draft = self.composer.draft.clone();
        if append_enter && self.options.data_type == SendCommandDataType::Text {
            draft.push('\n');
        }
        draft
    }

    pub(in crate::features) fn build_units(
        &self,
        draft: &str,
        session_kind: Option<SessionKind>,
    ) -> Result<Vec<Vec<u8>>, String> {
        build_send_command_units_for(
            draft,
            self.options.data_type,
            self.options.mode,
            self.options.line_ending,
            session_kind,
        )
    }

    pub(in crate::features) fn begin_send(&mut self, units_per_round: u32) -> SendCommandRunState {
        let infinite = self.options.count.is_none();
        let rounds = self.options.count.unwrap_or(1).max(1);
        let cancel = Arc::new(AtomicBool::new(false));
        self.progress = SendCommandProgressState {
            sending: true,
            cancel: Some(cancel.clone()),
            completed: 0,
            total: if infinite {
                0
            } else {
                units_per_round.saturating_mul(rounds)
            },
            round: 0,
            rounds: if infinite { 0 } else { rounds },
        };
        SendCommandRunState {
            cancel,
            infinite,
            rounds,
            interval_seconds: self.options.interval_seconds.max(0.0),
            raw_units: self.options.data_type == SendCommandDataType::Hex,
        }
    }

    pub(in crate::features) fn set_progress_round(&mut self, round: u32) {
        if self.progress.sending {
            self.progress.round = round;
        }
    }

    pub(in crate::features) fn complete_progress_unit(&mut self) {
        if self.progress.sending {
            self.progress.completed = self.progress.completed.saturating_add(1);
        }
    }

    pub(in crate::features) fn finish_send(&mut self) -> SendCommandProgressResult {
        self.progress.sending = false;
        self.progress.cancel = None;
        SendCommandProgressResult {
            completed: self.progress.completed,
            total: self.progress.total,
            round: self.progress.round,
        }
    }

    pub(in crate::features) fn request_cancel(&self) -> bool {
        let Some(cancel) = self.progress.cancel.as_ref() else {
            return false;
        };
        cancel.store(true, Ordering::SeqCst);
        true
    }

    pub(in crate::features) fn set_target(&mut self, target: SendCommandTarget) -> bool {
        if self.progress.sending {
            return false;
        }
        self.options.target = target;
        true
    }

    pub(in crate::features) fn reset_default_interval(&mut self) -> String {
        self.options.apply_default_interval();
        self.sync_interval_input()
    }

    pub(in crate::features) fn set_data_type(
        &mut self,
        data_type: SendCommandDataType,
    ) -> Option<String> {
        if self.progress.sending {
            return None;
        }
        self.options.data_type = data_type;
        match data_type {
            SendCommandDataType::Hex
                if matches!(
                    self.options.mode,
                    SendCommandMode::Line | SendCommandMode::Character
                ) =>
            {
                self.options.mode = SendCommandMode::Byte;
            }
            SendCommandDataType::Text
                if matches!(
                    self.options.mode,
                    SendCommandMode::Packet | SendCommandMode::Byte
                ) =>
            {
                self.options.mode = SendCommandMode::Line;
            }
            _ => {}
        }
        self.composer.hex_scroll_x = 0.;
        self.composer.hex_scroll_y = 0.;
        Some(self.reset_default_interval())
    }

    pub(in crate::features) fn set_mode(&mut self, mode: SendCommandMode) -> Option<String> {
        if self.progress.sending {
            return None;
        }
        self.options.mode = mode;
        Some(self.reset_default_interval())
    }

    pub(in crate::features) fn set_line_ending(
        &mut self,
        line_ending: SendCommandLineEnding,
    ) -> bool {
        if self.progress.sending {
            return false;
        }
        self.options.line_ending = line_ending;
        true
    }

    pub(in crate::features) fn scroll_hex_by(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> bool {
        let next_y = (self.composer.hex_scroll_y - delta_y).clamp(0., max_scroll_y);
        let next_x = (self.composer.hex_scroll_x - delta_x).clamp(0., max_scroll_x);
        let changed = (next_y - self.composer.hex_scroll_y).abs() > 0.01
            || (next_x - self.composer.hex_scroll_x).abs() > 0.01;
        if changed {
            self.composer.hex_scroll_y = next_y;
            self.composer.hex_scroll_x = next_x;
        }
        changed
    }

    fn sync_count_input(&mut self) -> String {
        self.options.count_input = self.options.count_label();
        self.options.count_input.clone()
    }

    fn sync_interval_input(&mut self) -> String {
        self.options.sync_interval_input();
        self.options.interval_input.clone()
    }
}

/// Composer edits that only touch the payload and its hex viewport.
impl SendCommandComposerState {
    /// Keeps the hex guide overlay's scroll offsets inside the rendered text.
    ///
    /// The viewport constants approximate the Tauri textarea this replaced;
    /// they are unchanged.
    fn clamp_hex_scroll(&mut self) {
        const HEX_LINE_PX: f32 = 15.;
        const HEX_CHAR_PX: f32 = 7.2;
        const VIEWPORT_LINES: f32 = 5.;
        const VIEWPORT_CHARS: f32 = 48.;

        let display = format_send_command_hex_display(&self.draft);
        let lines: Vec<&str> = display.lines().collect();
        let line_count = lines.len().max(1) as f32;
        let max_line_chars = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32;

        let max_scroll_y = ((line_count - VIEWPORT_LINES).max(0.)) * HEX_LINE_PX;
        let max_scroll_x = ((max_line_chars - VIEWPORT_CHARS).max(0.)) * HEX_CHAR_PX;

        self.hex_scroll_y = self.hex_scroll_y.clamp(0., max_scroll_y);
        self.hex_scroll_x = self.hex_scroll_x.clamp(0., max_scroll_x);
    }
}

/// Option edits that only touch how the payload is interpreted and delivered.
impl SendCommandOptionsState {
    /// Parses the repeat-count field.
    ///
    /// `live` means the user is still typing, so an unparsable value is left
    /// alone rather than snapped back to 1.
    fn apply_count_input(&mut self, live: bool) {
        let trimmed = self.count_input.trim();
        if trimmed == "∞" || trimmed.eq_ignore_ascii_case("inf") {
            self.count = None;
            return;
        }
        if let Ok(value) = trimmed.parse::<u32>() {
            self.count = Some(value.clamp(1, 9999));
        } else if !live {
            self.count = Some(1);
        }
    }

    fn sync_interval_input(&mut self) {
        self.interval_input = format!("{:.2}", self.interval_seconds);
    }

    fn count_label(&self) -> String {
        self.count
            .map(|count| count.to_string())
            .unwrap_or_else(|| "∞".to_string())
    }

    fn apply_interval_input(&mut self, live: bool) {
        let trimmed = self.interval_input.trim();
        if let Ok(value) = trimmed.parse::<f64>() {
            if value.is_finite() && value >= 0.0 {
                self.interval_seconds = value.clamp(0.0, 60.0);
            }
        } else if !live {
            self.apply_default_interval();
        }
    }

    fn apply_default_interval(&mut self) {
        self.interval_seconds = match (self.data_type, self.mode) {
            (SendCommandDataType::Hex, SendCommandMode::Byte) => 0.02,
            (SendCommandDataType::Hex, _) => 0.0,
            (SendCommandDataType::Text, SendCommandMode::Line) => 1.0,
            (SendCommandDataType::Text, _) => 0.02,
        };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use gpui::TestAppContext;

    use crate::send_command::{SendCommandControlFocus, SendCommandDataType, SendCommandMode};

    use super::{SendCommandFeatureFocus, SendCommandFeatureState};

    fn send_command_state(cx: &TestAppContext) -> SendCommandFeatureState {
        cx.update(|cx| {
            SendCommandFeatureState::new(SendCommandFeatureFocus {
                editor: cx.focus_handle(),
            })
        })
    }

    #[test]
    fn send_command_owner_keeps_data_and_mode_compatible() {
        let cx = TestAppContext::single();
        let mut state = send_command_state(&cx);

        assert_eq!(
            state.set_data_type(SendCommandDataType::Hex),
            Some("0.02".to_string())
        );
        let presentation = state.presentation();
        assert_eq!(presentation.data_type, SendCommandDataType::Hex);
        assert_eq!(presentation.mode, SendCommandMode::Byte);

        assert_eq!(
            state.set_mode(SendCommandMode::Packet),
            Some("0.00".to_string())
        );
        assert_eq!(
            state.set_data_type(SendCommandDataType::Text),
            Some("1.00".to_string())
        );
        assert_eq!(state.presentation().mode, SendCommandMode::Line);
    }

    #[test]
    fn send_command_owner_normalizes_control_edits_and_infinity_count() {
        let cx = TestAppContext::single();
        let mut state = send_command_state(&cx);

        assert!(state.apply_control_input(SendCommandControlFocus::Count, "∞".to_string()));
        assert_eq!(
            state.synced_control_input(SendCommandControlFocus::Count),
            "∞"
        );
        assert!(state.apply_control_input(SendCommandControlFocus::Count, "25".to_string()));
        assert_eq!(
            state.synced_control_input(SendCommandControlFocus::Count),
            "25"
        );

        assert!(state.apply_control_input(SendCommandControlFocus::Interval, "999".to_string()));
        assert_eq!(
            state.synced_control_input(SendCommandControlFocus::Interval),
            "60.00"
        );
    }

    #[test]
    fn send_command_owner_finishes_progress_and_releases_cancel_atomically() {
        let cx = TestAppContext::single();
        let mut state = send_command_state(&cx);

        let run = state.begin_send(3);
        assert!(!run.infinite);
        assert_eq!(run.rounds, 1);
        assert!(state.is_sending());
        assert!(!state.set_target(crate::send_command::SendCommandTarget::AllCompatible));
        assert_eq!(state.set_data_type(SendCommandDataType::Hex), None);
        state.set_progress_round(1);
        state.complete_progress_unit();
        state.complete_progress_unit();
        assert!(state.request_cancel());
        assert!(run.cancel.load(Ordering::SeqCst));

        let progress = state.finish_send();
        assert_eq!(progress.completed, 2);
        assert_eq!(progress.total, 3);
        assert_eq!(progress.round, 1);
        assert!(!state.is_sending());
        assert!(!state.request_cancel());
    }
}
