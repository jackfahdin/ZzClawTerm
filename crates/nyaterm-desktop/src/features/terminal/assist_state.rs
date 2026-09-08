//! Authoritative inline command and credential assistance state.

use std::collections::HashMap;

use nyaterm_core::TerminalInputState as CommandInputState;

use super::state::TerminalFeatureState;
use crate::models::{
    CommandSuggestionState, CredentialAutofillMatchPipeline, CredentialAutofillMatchRequestKey,
    CredentialSuggestionState, PendingCredentialAutofill,
};

/// Inline command completion and terminal-output credential prompt assistance.
///
/// These states share the terminal input lifecycle: session switches, terminal
/// mode changes, and settings updates reset them together. The credential
/// matcher remains a background pipeline and never runs in a render path.
pub(super) struct TerminalAssistState {
    pub(super) command_suggestions: Option<CommandSuggestionState>,
    pub(super) command_input_tracker: CommandInputState,
    pub(super) command_suggestions_suppressed: bool,
    pub(super) pending_command_history_entry: Option<String>,
    pub(super) command_suggestion_search_gen: u64,
    pub(super) command_suggestion_refresh_task: Option<gpui::Task<()>>,
    pub(super) credential_suggestions: Option<CredentialSuggestionState>,
    pub(super) credential_autofill_buffer: String,
    pub(super) credential_autofill_recent: HashMap<String, u64>,
    pub(super) credential_autofill_pending: Option<PendingCredentialAutofill>,
    pub(super) credential_autofill_detection_pending: bool,
    pub(super) credential_autofill_next_request_id: u64,
    pub(super) credential_autofill_pending_request: Option<CredentialAutofillMatchRequestKey>,
    pub(super) credential_autofill_match_pipeline: CredentialAutofillMatchPipeline,
    pub(super) credential_autofill_sending: bool,
    pub(super) credential_prompt_input_until_ms: u64,
}

impl TerminalAssistState {
    pub(super) fn new() -> Self {
        Self {
            command_suggestions: None,
            command_input_tracker: CommandInputState::new(),
            command_suggestions_suppressed: false,
            pending_command_history_entry: None,
            command_suggestion_search_gen: 0,
            command_suggestion_refresh_task: None,
            credential_suggestions: None,
            credential_autofill_buffer: String::new(),
            credential_autofill_recent: HashMap::new(),
            credential_autofill_pending: None,
            credential_autofill_detection_pending: false,
            credential_autofill_next_request_id: 0,
            credential_autofill_pending_request: None,
            credential_autofill_match_pipeline: CredentialAutofillMatchPipeline::spawn(),
            credential_autofill_sending: false,
            credential_prompt_input_until_ms: 0,
        }
    }

    fn clear_command_tracking(&mut self) {
        self.command_suggestions = None;
        self.command_input_tracker = CommandInputState::new();
        self.command_suggestions_suppressed = false;
        self.pending_command_history_entry = None;
    }

    fn invalidate_command_suggestion_search(&mut self) {
        self.command_suggestion_search_gen = self.command_suggestion_search_gen.saturating_add(1);
    }

    fn reset_for_session_switch(&mut self) {
        self.credential_suggestions = None;
        self.credential_autofill_buffer.clear();
        self.credential_autofill_recent.clear();
        self.credential_autofill_pending = None;
        self.credential_autofill_sending = false;
        self.credential_prompt_input_until_ms = 0;
        self.clear_command_tracking();
        self.invalidate_command_suggestion_search();
    }

    pub(super) fn dismiss_credential_suggestions(&mut self) -> bool {
        let had_panel = self.credential_suggestions.take().is_some();
        self.credential_autofill_buffer.clear();
        self.credential_autofill_recent.clear();
        self.credential_autofill_detection_pending = false;
        self.credential_autofill_pending_request = None;
        had_panel
    }

    pub(super) fn credential_prompt_input_mode(&self, now_ms: u64) -> bool {
        self.credential_prompt_input_until_ms > now_ms
    }

    pub(super) fn shutdown_workers(&mut self) {
        self.credential_autofill_match_pipeline.shutdown();
    }
}

impl TerminalFeatureState {
    pub(in crate::features) fn command_suggestions_open(&self) -> bool {
        self.assist.command_suggestions.is_some()
    }

    pub(in crate::features) fn credential_suggestions_open(&self) -> bool {
        self.assist.credential_suggestions.is_some()
    }

    pub(in crate::features) fn take_pending_command_history_entry(&mut self) -> Option<String> {
        self.assist.pending_command_history_entry.take()
    }

    pub(in crate::features) fn invalidate_command_suggestion_search(&mut self) {
        self.assist.invalidate_command_suggestion_search();
    }

    pub(in crate::features) fn clear_command_tracking(&mut self) {
        self.assist.clear_command_tracking();
    }

    pub(in crate::features) fn reset_assist_for_session_switch(&mut self) {
        self.assist.reset_for_session_switch();
    }

    pub(in crate::features) fn clear_active_session_assist(&mut self) {
        self.assist.command_input_tracker = CommandInputState::new();
        self.assist.command_suggestions = None;
        self.assist.credential_suggestions = None;
    }
}
