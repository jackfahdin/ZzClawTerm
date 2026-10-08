//! Grouped send-command bar state.
//!
//! The bar composes one payload, chooses how to send it, and then reports
//! progress while sending. Those are three distinct phases and the twenty-four
//! `send_command_*` fields interleaved them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{App, AppContext as _, Entity};
use zzclawterm_core::command_draft::{DraftOrigin, DraftProvenance};
use zzclawterm_core::hex_document::{HexCopyFormat, format_hex};
use zzclawterm_transport::SessionKind;
use zzclawterm_ui::hex_editor::ZzClawHexEditorState;

use crate::send_command::{
    SendCommandControlFocus, SendCommandDataType, SendCommandLineEnding, SendCommandMode,
    SendCommandTarget, build_send_command_units_for,
};

pub(in crate::features) struct SendCommandFeatureState {
    composer: SendCommandComposerState,
    options: SendCommandOptionsState,
    progress: SendCommandProgressState,
}

/// The payload being composed and where the caret is.
struct SendCommandComposerState {
    text_draft: String,
    provenance: DraftProvenance,
    hex: Entity<ZzClawHexEditorState>,
    viewport_width: f32,
}

/// How the payload is interpreted and delivered.
struct SendCommandOptionsState {
    data_type: SendCommandDataType,
    text_mode: SendCommandMode,
    hex_mode: SendCommandMode,
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

#[derive(Clone)]
pub(in crate::features) struct SendCommandPresentationState {
    pub draft: String,
    pub provenance: DraftProvenance,
    pub hex: Entity<ZzClawHexEditorState>,
    pub viewport_width: f32,
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
    pub(in crate::features) fn new(cx: &mut App) -> Self {
        Self {
            composer: SendCommandComposerState {
                text_draft: String::new(),
                provenance: DraftProvenance::default(),
                hex: cx.new(ZzClawHexEditorState::new),
                viewport_width: 0.,
            },
            options: SendCommandOptionsState {
                data_type: SendCommandDataType::Text,
                text_mode: SendCommandMode::Line,
                hex_mode: SendCommandMode::Byte,
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

    pub(in crate::features) fn presentation(&self, cx: &App) -> SendCommandPresentationState {
        SendCommandPresentationState {
            draft: self.draft_for_send(false, cx),
            provenance: if self.options.data_type == SendCommandDataType::Text {
                self.composer.provenance.clone()
            } else {
                DraftProvenance::default()
            },
            hex: self.composer.hex.clone(),
            viewport_width: self.composer.viewport_width,
            data_type: self.options.data_type,
            mode: self.options.mode(),
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

    pub(in crate::features) fn set_viewport_width(&mut self, width: f32) -> bool {
        if (self.composer.viewport_width - width).abs() <= 1. {
            return false;
        }
        self.composer.viewport_width = width;
        true
    }

    pub(in crate::features) fn convert_to_hex(&mut self, cx: &mut App) -> bool {
        if self.is_sending() {
            return false;
        }
        self.composer.hex.update(cx, |hex, cx| {
            hex.set_bytes(self.composer.text_draft.as_bytes().to_vec(), cx)
        });
        self.set_data_type(SendCommandDataType::Hex);
        true
    }

    pub(in crate::features) fn decode_utf8(&mut self, cx: &App) -> Result<(), ()> {
        if self.is_sending()
            || self
                .composer
                .hex
                .read(cx)
                .document()
                .pending_nibble()
                .is_some()
        {
            return Err(());
        }
        let text =
            std::str::from_utf8(self.composer.hex.read(cx).document().bytes()).map_err(|_| ())?;
        self.composer.text_draft = text.to_string();
        self.composer.provenance = DraftProvenance::default();
        self.set_data_type(SendCommandDataType::Text);
        Ok(())
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

    pub(in crate::features) fn clear_draft(&mut self, cx: &mut App) {
        match self.options.data_type {
            SendCommandDataType::Text => {
                self.composer.text_draft.clear();
                self.composer.provenance = DraftProvenance::default();
            }
            SendCommandDataType::Hex => self.composer.hex.update(cx, |hex, cx| hex.clear(cx)),
        }
    }

    /// Never discard edits made while an earlier payload was being sent.
    pub(in crate::features) fn clear_sent_draft(
        &mut self,
        enabled: bool,
        completed_successfully: bool,
        sent_draft: &str,
        cx: &mut App,
    ) -> bool {
        if !enabled
            || !completed_successfully
            || self.draft_for_send(false, cx) != sent_draft
            || (self.options.data_type == SendCommandDataType::Hex
                && self
                    .composer
                    .hex
                    .read(cx)
                    .document()
                    .pending_nibble()
                    .is_some())
        {
            return false;
        }
        self.clear_draft(cx);
        true
    }

    pub(in crate::features) fn apply_draft(&mut self, text: String) {
        if self.composer.text_draft != text {
            self.composer.provenance.edit(text.is_empty());
        }
        self.composer.text_draft = text;
    }

    pub(in crate::features) fn fill_plugin_draft(
        &mut self,
        text: String,
        origin: DraftOrigin,
        replace: bool,
    ) {
        self.composer
            .provenance
            .fill(origin, replace, self.composer.text_draft.is_empty());
        self.composer.text_draft = text;
    }

    pub(in crate::features) fn draft_for_send(&self, append_enter: bool, cx: &App) -> String {
        let mut draft = match self.options.data_type {
            SendCommandDataType::Text => self.composer.text_draft.clone(),
            SendCommandDataType::Hex => format_hex(
                self.composer.hex.read(cx).document().bytes(),
                HexCopyFormat::Hex,
            ),
        };
        if append_enter && self.options.data_type == SendCommandDataType::Text {
            draft.push('\n');
        }
        draft
    }

    pub(in crate::features) fn build_units(
        &self,
        draft: &str,
        session_kind: Option<SessionKind>,
        cx: &App,
    ) -> Result<Vec<Vec<u8>>, String> {
        if self.options.data_type == SendCommandDataType::Hex
            && self.composer.hex.read(cx).error().is_some()
        {
            return Err("invalid paste: expected hexadecimal bytes".to_string());
        }
        if self.options.data_type == SendCommandDataType::Hex
            && self
                .composer
                .hex
                .read(cx)
                .document()
                .pending_nibble()
                .is_some()
        {
            return Err("incomplete hexadecimal byte".to_string());
        }
        build_send_command_units_for(
            draft,
            self.options.data_type,
            self.options.mode(),
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
        Some(self.reset_default_interval())
    }

    pub(in crate::features) fn set_mode(&mut self, mode: SendCommandMode) -> Option<String> {
        if self.progress.sending {
            return None;
        }
        match (self.options.data_type, mode) {
            (SendCommandDataType::Text, SendCommandMode::Line | SendCommandMode::Character) => {
                self.options.text_mode = mode
            }
            (SendCommandDataType::Hex, SendCommandMode::Byte | SendCommandMode::Packet) => {
                self.options.hex_mode = mode
            }
            _ => return None,
        }
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

    fn sync_count_input(&mut self) -> String {
        self.options.count_input = self.options.count_label();
        self.options.count_input.clone()
    }

    fn sync_interval_input(&mut self) -> String {
        self.options.sync_interval_input();
        self.options.interval_input.clone()
    }
}

/// Option edits that only touch how the payload is interpreted and delivered.
impl SendCommandOptionsState {
    fn mode(&self) -> SendCommandMode {
        match self.data_type {
            SendCommandDataType::Text => self.text_mode,
            SendCommandDataType::Hex => self.hex_mode,
        }
    }

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
        self.interval_seconds = match (self.data_type, self.mode()) {
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

    use super::SendCommandFeatureState;

    fn send_command_state(cx: &TestAppContext) -> SendCommandFeatureState {
        cx.update(SendCommandFeatureState::new)
    }

    #[test]
    fn send_command_owner_keeps_data_and_mode_compatible() {
        let cx = TestAppContext::single();
        let mut state = send_command_state(&cx);

        assert_eq!(
            state.set_data_type(SendCommandDataType::Hex),
            Some("0.02".to_string())
        );
        let presentation = cx.update(|cx| state.presentation(cx));
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
        assert_eq!(
            cx.update(|cx| state.presentation(cx).mode),
            SendCommandMode::Line
        );
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
    fn drafts_and_modes_survive_switching_and_clear_only_sent_payload() {
        let cx = TestAppContext::single();
        cx.update(|cx| {
            let mut state = SendCommandFeatureState::new(cx);
            state.apply_draft("AT+CSQ".to_string());
            state.set_mode(SendCommandMode::Character);
            state.set_data_type(SendCommandDataType::Hex);
            state
                .composer
                .hex
                .update(cx, |hex, cx| hex.paste("01 FF", cx));
            state.set_mode(SendCommandMode::Packet);
            state.set_data_type(SendCommandDataType::Text);
            assert_eq!(state.presentation(cx).draft, "AT+CSQ");
            assert_eq!(state.presentation(cx).mode, SendCommandMode::Character);
            state.set_data_type(SendCommandDataType::Hex);
            assert_eq!(state.presentation(cx).draft, "01 FF");
            assert_eq!(state.presentation(cx).mode, SendCommandMode::Packet);
            assert!(!state.clear_sent_draft(false, true, "01 FF", cx));
            assert!(!state.clear_sent_draft(true, false, "01 FF", cx));
            state.composer.hex.update(cx, |hex, cx| hex.paste("AB", cx));
            assert!(!state.clear_sent_draft(true, true, "01 FF", cx));
            assert!(state.clear_sent_draft(true, true, "01 FF AB", cx));
            state.set_data_type(SendCommandDataType::Text);
            assert_eq!(state.presentation(cx).draft, "AT+CSQ");
        });
    }

    #[test]
    fn explicit_utf8_conversion_validates_before_replacing_text() {
        let cx = TestAppContext::single();
        cx.update(|cx| {
            let mut state = SendCommandFeatureState::new(cx);
            state.apply_draft("中a".to_string());
            assert!(state.convert_to_hex(cx));
            assert_eq!(
                state.composer.hex.read(cx).document().bytes(),
                "中a".as_bytes()
            );
            state
                .composer
                .hex
                .update(cx, |hex, cx| hex.set_bytes(vec![255], cx));
            assert!(state.decode_utf8(cx).is_err());
            state.set_data_type(SendCommandDataType::Text);
            assert_eq!(state.presentation(cx).draft, "中a");
            state.set_data_type(SendCommandDataType::Hex);
            state
                .composer
                .hex
                .update(cx, |hex, cx| hex.set_bytes(b"Hello".to_vec(), cx));
            assert!(state.decode_utf8(cx).is_ok());
            assert_eq!(state.presentation(cx).draft, "Hello");
        });
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
