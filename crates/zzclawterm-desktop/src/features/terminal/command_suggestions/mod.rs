use rust_i18n::t;

use std::sync::Arc;
use std::time::{Duration, Instant};
use zzclawterm_ui::{ZzClawScrollable, ZzClawTooltip};

use gpui::{
    Bounds, Context, FontWeight, IntoElement, KeyDownEvent, Pixels, SharedString, div, prelude::*,
    px, rgb, rgba, svg,
};
use zzclawterm_core::{
    CommandHistoryEntry, QuickCommand, TerminalInputState, apply_terminal_input_data,
    apply_terminal_input_data_in_place, can_suggest_from_tracked_command,
    command_starts_suggestion_suppressing_program, get_tracked_command,
    get_tracked_submission_command, manual_empty_command_suggestions, resync_from_terminal_line,
    search_command_sources, terminal_input_tracker_below_min_chars,
};
use zzclawterm_store::{StoreDomain, store_request};

use crate::features::ZzClawTermApp;
use crate::features::terminal::terminal_runtime::TERMINAL_INPUT_LATENCY_WINDOW;
use crate::models::{CommandSuggestionItem, CommandSuggestionState};
use crate::terminal::terminal_byte_index_for_cell_col;

mod helpers;

use self::helpers::command_suggestion_highlight_parts;

const COMMAND_SUGGESTION_INPUT_SLOW_THRESHOLD: Duration = Duration::from_millis(4);
const COMMAND_SUGGESTION_REFRESH_SLOW_THRESHOLD: Duration = Duration::from_millis(8);
const COMMAND_SUGGESTION_REFRESH_DEBOUNCE: Duration = Duration::from_millis(80);
const COMMAND_SUGGESTION_REFRESH_PRESSURE_RETRY: Duration = Duration::from_millis(120);
pub(in crate::features) const SUGGESTION_OVERLAY_HEADER_HEIGHT: f32 = 24.0;
pub(in crate::features) const SUGGESTION_OVERLAY_FOOTER_HEIGHT: f32 = 24.0;
const SUGGESTION_OVERLAY_CHROME_HEIGHT: f32 =
    SUGGESTION_OVERLAY_HEADER_HEIGHT + SUGGESTION_OVERLAY_FOOTER_HEIGHT;
const SUGGESTION_OVERLAY_MAX_HEIGHT: f32 = 320.0;
const SUGGESTION_OVERLAY_CURSOR_GAP: f32 = 4.0;
const SUGGESTION_OVERLAY_VIEWPORT_MARGIN: f32 = 8.0;

#[derive(Clone, Copy, Default)]
struct CommandSuggestionInputTiming {
    utf8: Duration,
    submission: Duration,
    resync: Duration,
    apply_tracker: Duration,
    min_chars: Duration,
    pattern: Duration,
    pager: Duration,
    eligibility: Duration,
    hide_popup: Duration,
    schedule: Duration,
}

impl CommandSuggestionInputTiming {
    fn max_stage(self) -> Duration {
        [
            self.utf8,
            self.submission,
            self.resync,
            self.apply_tracker,
            self.min_chars,
            self.pattern,
            self.pager,
            self.eligibility,
            self.hide_popup,
            self.schedule,
        ]
        .into_iter()
        .max()
        .unwrap_or(Duration::ZERO)
    }
}

#[derive(Clone, Copy, Default)]
struct CommandSuggestionRefreshTiming {
    pattern: Duration,
    search: Duration,
    cursor: Duration,
    update_state: Duration,
    hide_popup: Duration,
}

struct CommandSuggestionSearchRequest {
    request_id: u64,
    session_id: String,
    pattern: String,
    pattern_chars: usize,
    min_chars: usize,
    max_chars: usize,
    history: Arc<[CommandHistoryEntry]>,
    quick_commands: Arc<[QuickCommand]>,
    started_at: Instant,
    popup_visible_at_start: bool,
    timing: CommandSuggestionRefreshTiming,
}

impl CommandSuggestionRefreshTiming {
    fn max_stage(self) -> Duration {
        [
            self.pattern,
            self.search,
            self.cursor,
            self.update_state,
            self.hide_popup,
        ]
        .into_iter()
        .max()
        .unwrap_or(Duration::ZERO)
    }
}

fn command_suggestion_refresh_input_delay(
    last_terminal_input_at: Option<Instant>,
    now: Instant,
) -> Option<Duration> {
    last_terminal_input_at.and_then(|last| {
        let elapsed = now.saturating_duration_since(last);
        (elapsed < TERMINAL_INPUT_LATENCY_WINDOW).then(|| TERMINAL_INPUT_LATENCY_WINDOW - elapsed)
    })
}

fn command_suggestion_input_can_defer_refresh(state: &TerminalInputState) -> bool {
    !state.desynced && !state.multiline && state.cursor == state.value.len()
}

fn command_suggestion_input_obvious_pager_prefix(state: &TerminalInputState) -> bool {
    let value = state.value.trim_start();
    value.starts_with('/') || value.starts_with('?') || value.starts_with(':')
}

fn command_suggestion_input_candidate_chars(state: &TerminalInputState) -> usize {
    state.value.trim_start().chars().count()
}

fn command_history_input_update(state: &mut TerminalInputState, text: &str) -> Option<String> {
    let submitted = if text.contains('\r') || text.contains('\n') {
        let submitted = get_tracked_submission_command(state);
        (!submitted.is_empty()).then_some(submitted)
    } else {
        None
    };
    apply_terminal_input_data_in_place(state, text);
    submitted
}

impl ZzClawTermApp {
    pub(in crate::features) fn dismiss_command_suggestions(&mut self, cx: &mut Context<Self>) {
        self.terminal.assist.command_suggestion_search_gen = self
            .terminal
            .assist
            .command_suggestion_search_gen
            .saturating_add(1);
        self.terminal.assist.command_suggestion_refresh_task = None;
        let mut changed = false;
        if self.terminal.assist.command_suggestions.take().is_some() {
            changed = true;
        }
        // Keep draft for continued typing; only clear list visibility.
        if changed {
            cx.notify();
        }
    }

    pub(in crate::features) fn clear_command_suggestion_draft(&mut self, cx: &mut Context<Self>) {
        self.terminal.assist.command_suggestion_search_gen = self
            .terminal
            .assist
            .command_suggestion_search_gen
            .saturating_add(1);
        self.terminal.assist.command_suggestion_refresh_task = None;
        let mut changed = false;
        if self.terminal.assist.command_input_tracker != TerminalInputState::new() {
            self.terminal.assist.command_input_tracker = TerminalInputState::new();
            changed = true;
        }
        if self.terminal.assist.command_suggestions.take().is_some() {
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::features) fn note_command_suggestion_input(
        &mut self,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) {
        let started_at = Instant::now();
        let byte_count = bytes.len();
        let popup_visible_at_start = self.terminal.assist.command_suggestions.is_some();
        let mut timing = CommandSuggestionInputTiming::default();
        macro_rules! finish {
            ($outcome:expr, $pattern_chars:expr) => {{
                self.log_command_suggestion_input_diagnostic(
                    started_at,
                    byte_count,
                    popup_visible_at_start,
                    $outcome,
                    $pattern_chars,
                    timing,
                );
                return;
            }};
        }

        if self.terminal.assist.credential_suggestions.is_some()
            || self.is_credential_prompt_input_mode()
        {
            return;
        }
        if self.settings.summary().terminal_low_latency_mode {
            self.clear_command_suggestion_draft(cx);
            finish!("low_latency_mode", 0);
        }
        if !self
            .settings
            .summary()
            .interaction_command_suggestions_enabled
        {
            self.clear_command_suggestion_draft(cx);
            return;
        }
        if self.session.active_id().is_none() {
            self.clear_command_suggestion_draft(cx);
            return;
        }
        let utf8_started_at = Instant::now();
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => {
                timing.utf8 = utf8_started_at.elapsed();
                text
            }
            Err(_) => {
                timing.utf8 = utf8_started_at.elapsed();
                self.clear_command_suggestion_draft(cx);
                finish!("non_utf8", 0);
            }
        };
        if text.is_empty() {
            finish!("empty_text", 0);
        }

        // Exit interactive suppression on Ctrl+C or q (Tauri resetCommandSuggestionSuppression).
        if self.terminal.assist.command_suggestions_suppressed
            && (text == "\u{0003}" || text == "q")
        {
            self.terminal.assist.command_suggestions_suppressed = false;
            self.terminal.assist.command_input_tracker = TerminalInputState::new();
            self.terminal.assist.command_suggestions = None;
            cx.notify();
            finish!("suppression_reset", 0);
        }

        // Capture submission command before tracker reset on Enter.
        let submission_started_at = Instant::now();
        if text.contains('\r') || text.contains('\n') {
            let submitted =
                get_tracked_submission_command(&self.terminal.assist.command_input_tracker);
            if !submitted.is_empty() {
                self.terminal.assist.pending_command_history_entry = Some(submitted.clone());
                if command_starts_suggestion_suppressing_program(&submitted) {
                    self.terminal.assist.command_suggestions_suppressed = true;
                }
            }
        }
        timing.submission = submission_started_at.elapsed();

        // Tab-desync recovery: before applying non-tab input, resync from terminal line.
        let resync_started_at = Instant::now();
        if text != "\t"
            && self.terminal.assist.command_input_tracker.desynced
            && self.terminal.assist.command_input_tracker.desync_reason == Some("tab")
            && let Some(line) = self.read_active_terminal_input_line()
            && let Some(recovered) =
                resync_from_terminal_line(&self.terminal.assist.command_input_tracker, &line)
        {
            self.terminal.assist.command_input_tracker = recovered;
        }
        timing.resync = resync_started_at.elapsed();

        let apply_started_at = Instant::now();
        apply_terminal_input_data_in_place(&mut self.terminal.assist.command_input_tracker, text);
        timing.apply_tracker = apply_started_at.elapsed();

        if self.terminal.assist.command_suggestions_suppressed {
            let hide_started_at = Instant::now();
            self.terminal.assist.command_suggestion_search_gen = self
                .terminal
                .assist
                .command_suggestion_search_gen
                .saturating_add(1);
            if self.terminal.assist.command_suggestions.take().is_some() {
                cx.notify();
            }
            timing.hide_popup = hide_started_at.elapsed();
            finish!("suppressed", 0);
        }

        let min_chars_started_at = Instant::now();
        let min_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_min_chars
            .max(1) as usize;
        let below_min_chars = terminal_input_tracker_below_min_chars(
            &self.terminal.assist.command_input_tracker,
            min_chars,
        );
        timing.min_chars = min_chars_started_at.elapsed();
        if below_min_chars {
            let hide_started_at = Instant::now();
            self.terminal.assist.command_suggestion_search_gen = self
                .terminal
                .assist
                .command_suggestion_search_gen
                .saturating_add(1);
            if self.terminal.assist.command_suggestions.take().is_some() {
                cx.notify();
            }
            timing.hide_popup = hide_started_at.elapsed();
            finish!("below_min_chars", 0);
        }

        let pattern_started_at = Instant::now();
        let pattern_chars =
            command_suggestion_input_candidate_chars(&self.terminal.assist.command_input_tracker);
        timing.pattern = pattern_started_at.elapsed();

        let pager_started_at = Instant::now();
        let pager_input = command_suggestion_input_obvious_pager_prefix(
            &self.terminal.assist.command_input_tracker,
        );
        timing.pager = pager_started_at.elapsed();
        if pager_input {
            let hide_started_at = Instant::now();
            self.terminal.assist.command_suggestion_search_gen = self
                .terminal
                .assist
                .command_suggestion_search_gen
                .saturating_add(1);
            if self.terminal.assist.command_suggestions.take().is_some() {
                cx.notify();
            }
            timing.hide_popup = hide_started_at.elapsed();
            finish!("pager_input", pattern_chars);
        }

        let eligibility_started_at = Instant::now();
        let can_suggest =
            command_suggestion_input_can_defer_refresh(&self.terminal.assist.command_input_tracker);
        timing.eligibility = eligibility_started_at.elapsed();
        if !can_suggest {
            let hide_started_at = Instant::now();
            self.terminal.assist.command_suggestion_search_gen = self
                .terminal
                .assist
                .command_suggestion_search_gen
                .saturating_add(1);
            if self.terminal.assist.command_suggestions.take().is_some() {
                cx.notify();
            }
            timing.hide_popup = hide_started_at.elapsed();
            finish!("not_eligible", pattern_chars);
        }
        let schedule_started_at = Instant::now();
        self.schedule_command_suggestion_refresh(cx);
        timing.schedule = schedule_started_at.elapsed();
        finish!("scheduled", pattern_chars);
    }

    pub(in crate::features) fn note_command_history_input(&mut self, bytes: &[u8]) {
        if self.terminal.assist.credential_suggestions.is_some()
            || self.is_credential_prompt_input_mode()
        {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.terminal.assist.command_input_tracker = TerminalInputState::new();
            return;
        };
        if text.is_empty() {
            return;
        }
        if self.terminal.assist.command_suggestions_suppressed {
            if text == "\u{0003}" || text == "q" {
                self.terminal.assist.command_suggestions_suppressed = false;
                self.terminal.assist.command_input_tracker = TerminalInputState::new();
            }
            return;
        }

        let submitted =
            command_history_input_update(&mut self.terminal.assist.command_input_tracker, text);
        if let Some(submitted) = submitted {
            if command_starts_suggestion_suppressing_program(&submitted) {
                self.terminal.assist.command_suggestions_suppressed = true;
            }
            self.terminal.assist.pending_command_history_entry = Some(submitted);
        }
    }

    pub(in crate::features) fn schedule_command_suggestion_refresh(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        // Tauri useCommandHistory: 80ms debounce before fuzzy search.
        self.schedule_command_suggestion_refresh_after(COMMAND_SUGGESTION_REFRESH_DEBOUNCE, cx);
    }

    fn schedule_command_suggestion_refresh_after(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        self.terminal.assist.command_suggestion_search_gen = self
            .terminal
            .assist
            .command_suggestion_search_gen
            .saturating_add(1);
        let request_id = self.terminal.assist.command_suggestion_search_gen;
        self.terminal.assist.command_suggestion_refresh_task =
            Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let request = this.update(cx, |this, cx| {
                    if this.terminal.assist.command_suggestion_search_gen != request_id {
                        return None;
                    }
                    if this.settings.summary().terminal_low_latency_mode {
                        this.hide_command_suggestions_if_present(cx);
                        return None;
                    }
                    let now = Instant::now();
                    if let Some(delay) = command_suggestion_refresh_input_delay(
                        this.shell.last_terminal_input_at(),
                        now,
                    ) {
                        this.schedule_command_suggestion_refresh_after(delay, cx);
                        return None;
                    }
                    if this.runtime_output_pressure_active() {
                        this.hide_command_suggestions_if_present(cx);
                        this.schedule_command_suggestion_refresh_after(
                            COMMAND_SUGGESTION_REFRESH_PRESSURE_RETRY,
                            cx,
                        );
                        return None;
                    }
                    this.prepare_command_suggestion_search(request_id, cx)
                });
                let Ok(Some(request)) = request else {
                    return;
                };
                let search_started_at = Instant::now();
                let pattern = request.pattern.clone();
                let min_chars = request.min_chars;
                let max_chars = request.max_chars;
                let history = request.history.clone();
                let quick_commands = request.quick_commands.clone();
                let search_task = cx.background_spawn(async move {
                    search_command_sources(
                        &history,
                        &quick_commands,
                        &pattern,
                        12,
                        Some(min_chars),
                        Some(max_chars),
                    )
                });
                let results = search_task.await;
                let search_duration = search_started_at.elapsed();
                let _ = this.update(cx, |this, cx| {
                    this.publish_command_suggestion_search(request, results, search_duration, cx);
                });
            }));
    }

    pub(in crate::features) fn read_active_terminal_input_line(&self) -> Option<String> {
        let offset = self.active_terminal_display_offset();
        let snapshot = self.terminal_snapshot_for_session(self.session.active_id(), offset);
        if snapshot.cursor.row == usize::MAX {
            return None;
        }
        let line = snapshot.line(snapshot.cursor.row)?;
        Some(terminal_line_prefix_for_cell_col(line, snapshot.cursor.col))
    }

    pub(in crate::features) fn refresh_command_suggestions(&mut self, cx: &mut Context<Self>) {
        self.schedule_command_suggestion_refresh_after(Duration::ZERO, cx);
    }

    pub(in crate::features) fn show_manual_command_suggestions(&mut self, cx: &mut Context<Self>) {
        if self.terminal.assist.credential_suggestions.is_some()
            || self.is_credential_prompt_input_mode()
            || self.terminal.assist.command_suggestions_suppressed
            || self.settings.summary().terminal_low_latency_mode
            || !self
                .settings
                .summary()
                .interaction_command_suggestions_enabled
        {
            self.hide_command_suggestions_if_present(cx);
            return;
        }
        let Some(session_id) = self
            .session
            .active_id()
            .filter(|session_id| !session_id.is_empty())
            .map(ToOwned::to_owned)
        else {
            self.hide_command_suggestions_if_present(cx);
            return;
        };
        if !command_suggestion_input_can_defer_refresh(&self.terminal.assist.command_input_tracker)
        {
            self.hide_command_suggestions_if_present(cx);
            return;
        }

        self.terminal.assist.command_suggestion_search_gen = self
            .terminal
            .assist
            .command_suggestion_search_gen
            .saturating_add(1);
        self.terminal.assist.command_suggestion_refresh_task = None;
        let min_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_min_chars
            .max(1) as usize;
        let max_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_max_chars
            .max(min_chars as u32) as usize;
        let pattern = get_tracked_command(&self.terminal.assist.command_input_tracker);
        let pattern_chars = pattern.chars().count();
        let results = if pattern.trim().is_empty() {
            manual_empty_command_suggestions(
                &self.commands.command_history_snapshot(),
                &self.commands.quick_commands_snapshot(),
                12,
                Some(min_chars),
                Some(max_chars),
            )
        } else if can_suggest_from_tracked_command(
            &self.terminal.assist.command_input_tracker,
            &pattern,
        ) {
            search_command_sources(
                &self.commands.command_history_snapshot(),
                &self.commands.quick_commands_snapshot(),
                &pattern,
                12,
                Some(min_chars),
                Some(max_chars),
            )
        } else {
            Vec::new()
        };
        if results.is_empty() {
            self.hide_command_suggestions_if_present(cx);
            return;
        }

        let (cursor_row, cursor_col) = self.active_terminal_cursor_cell();
        self.terminal.assist.command_suggestions = Some(CommandSuggestionState {
            session_id,
            draft: pattern,
            items: results
                .into_iter()
                .map(|item| CommandSuggestionItem {
                    command: item.command,
                    display: item.display,
                    source: item.source,
                    score: item.score,
                    indices: item.indices,
                })
                .collect(),
            selected_index: None,
            cursor_row,
            cursor_col,
        });
        self.shell
            .set_status(format!("showing {pattern_chars}-char command suggestions"));
        cx.notify();
    }

    fn prepare_command_suggestion_search(
        &mut self,
        request_id: u64,
        cx: &mut Context<Self>,
    ) -> Option<CommandSuggestionSearchRequest> {
        let started_at = Instant::now();
        let popup_visible_at_start = self.terminal.assist.command_suggestions.is_some();
        let mut timing = CommandSuggestionRefreshTiming::default();
        macro_rules! finish_refresh {
            ($outcome:expr, $pattern_chars:expr, $result_count:expr) => {{
                self.log_command_suggestion_refresh_diagnostic(
                    started_at,
                    popup_visible_at_start,
                    $outcome,
                    $pattern_chars,
                    $result_count,
                    timing,
                );
                return None;
            }};
        }

        let Some(session_id) = self
            .session
            .active_id()
            .filter(|session_id| !session_id.is_empty())
            .map(ToOwned::to_owned)
        else {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            finish_refresh!("no_active_session", 0, 0);
        };
        if self.terminal.assist.credential_suggestions.is_some()
            || self.is_credential_prompt_input_mode()
            || self.terminal.assist.command_suggestions_suppressed
        {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            finish_refresh!("suppressed_or_credential", 0, 0);
        }
        if !self
            .settings
            .summary()
            .interaction_command_suggestions_enabled
        {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            finish_refresh!("disabled", 0, 0);
        }
        let min_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_min_chars
            .max(1) as usize;
        let max_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_max_chars
            .max(min_chars as u32) as usize;
        let pattern_started_at = Instant::now();
        let pattern = get_tracked_command(&self.terminal.assist.command_input_tracker);
        let pattern_chars = pattern.chars().count();
        timing.pattern = pattern_started_at.elapsed();
        if !can_suggest_from_tracked_command(&self.terminal.assist.command_input_tracker, &pattern)
        {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            finish_refresh!("not_eligible", pattern_chars, 0);
        }
        if pattern_chars < min_chars {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            finish_refresh!("below_min_chars", pattern_chars, 0);
        }
        // Pager/search-like prefixes: hide suggestions.
        if pattern.starts_with('/') || pattern.starts_with('?') || pattern.starts_with(':') {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            finish_refresh!("pager_prefix", pattern_chars, 0);
        }
        Some(CommandSuggestionSearchRequest {
            request_id,
            session_id,
            pattern,
            pattern_chars,
            min_chars,
            max_chars,
            history: self.commands.command_history_snapshot(),
            quick_commands: self.commands.quick_commands_snapshot(),
            started_at,
            popup_visible_at_start,
            timing,
        })
    }

    fn publish_command_suggestion_search(
        &mut self,
        request: CommandSuggestionSearchRequest,
        results: Vec<zzclawterm_core::FuzzyResult>,
        search_duration: Duration,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.assist.command_suggestion_search_gen != request.request_id
            || self.session.active_id() != Some(request.session_id.as_str())
            || self.terminal.assist.credential_suggestions.is_some()
            || self.is_credential_prompt_input_mode()
            || self.terminal.assist.command_suggestions_suppressed
            || !self
                .settings
                .summary()
                .interaction_command_suggestions_enabled
            || get_tracked_command(&self.terminal.assist.command_input_tracker) != request.pattern
        {
            return;
        }
        let CommandSuggestionSearchRequest {
            session_id,
            pattern,
            pattern_chars,
            started_at,
            popup_visible_at_start,
            mut timing,
            ..
        } = request;
        timing.search = search_duration;
        let result_count = results.len();
        if results.is_empty() {
            let hide_started_at = Instant::now();
            self.hide_command_suggestions_if_present(cx);
            timing.hide_popup = hide_started_at.elapsed();
            self.log_command_suggestion_refresh_diagnostic(
                started_at,
                popup_visible_at_start,
                "search_empty",
                pattern_chars,
                result_count,
                timing,
            );
            return;
        }
        let cursor_started_at = Instant::now();
        let (cursor_row, cursor_col) = self.active_terminal_cursor_cell();
        timing.cursor = cursor_started_at.elapsed();
        let update_started_at = Instant::now();
        let next_state = CommandSuggestionState {
            session_id,
            draft: pattern,
            items: results
                .into_iter()
                .map(|item| CommandSuggestionItem {
                    command: item.command,
                    display: item.display,
                    source: item.source,
                    score: item.score,
                    indices: item.indices,
                })
                .collect(),
            selected_index: None,
            cursor_row,
            cursor_col,
        };
        if !command_suggestion_state_changed(
            self.terminal.assist.command_suggestions.as_ref(),
            &next_state,
        ) {
            timing.update_state = update_started_at.elapsed();
            self.log_command_suggestion_refresh_diagnostic(
                started_at,
                popup_visible_at_start,
                "unchanged",
                pattern_chars,
                result_count,
                timing,
            );
            return;
        }
        self.terminal.assist.command_suggestions = Some(next_state);
        timing.update_state = update_started_at.elapsed();
        cx.notify();
        self.log_command_suggestion_refresh_diagnostic(
            started_at,
            popup_visible_at_start,
            "shown",
            pattern_chars,
            result_count,
            timing,
        );
    }

    fn log_command_suggestion_input_diagnostic(
        &mut self,
        started_at: Instant,
        byte_count: usize,
        popup_visible_at_start: bool,
        outcome: &'static str,
        pattern_chars: usize,
        timing: CommandSuggestionInputTiming,
    ) {
        let total_duration = started_at.elapsed();
        if total_duration < COMMAND_SUGGESTION_INPUT_SLOW_THRESHOLD
            && timing.max_stage() < COMMAND_SUGGESTION_INPUT_SLOW_THRESHOLD
        {
            return;
        }
        if !self.should_log_slow_diagnostic("terminal_suggestion_input", Instant::now()) {
            return;
        }
        tracing::warn!(
            diagnostic = "terminal_suggestion_input",
            outcome,
            byte_count,
            pattern_chars,
            tracker_value_bytes = self.terminal.assist.command_input_tracker.value.len(),
            tracker_cursor = self.terminal.assist.command_input_tracker.cursor,
            tracker_desynced = self.terminal.assist.command_input_tracker.desynced,
            tracker_desync_reason = self
                .terminal
                .assist
                .command_input_tracker
                .desync_reason
                .unwrap_or(""),
            tracker_multiline = self.terminal.assist.command_input_tracker.multiline,
            tracker_paste_mode = self.terminal.assist.command_input_tracker.paste_mode,
            popup_visible_at_start,
            popup_visible = self.terminal.assist.command_suggestions.is_some(),
            total_us = total_duration.as_micros(),
            utf8_us = timing.utf8.as_micros(),
            submission_us = timing.submission.as_micros(),
            resync_us = timing.resync.as_micros(),
            apply_tracker_us = timing.apply_tracker.as_micros(),
            min_chars_us = timing.min_chars.as_micros(),
            pattern_us = timing.pattern.as_micros(),
            pager_us = timing.pager.as_micros(),
            eligibility_us = timing.eligibility.as_micros(),
            hide_popup_us = timing.hide_popup.as_micros(),
            schedule_us = timing.schedule.as_micros(),
            "slow terminal suggestion input"
        );
    }

    fn log_command_suggestion_refresh_diagnostic(
        &mut self,
        started_at: Instant,
        popup_visible_at_start: bool,
        outcome: &'static str,
        pattern_chars: usize,
        result_count: usize,
        timing: CommandSuggestionRefreshTiming,
    ) {
        let total_duration = started_at.elapsed();
        if total_duration < COMMAND_SUGGESTION_REFRESH_SLOW_THRESHOLD
            && timing.max_stage() < COMMAND_SUGGESTION_REFRESH_SLOW_THRESHOLD
        {
            return;
        }
        if !self.should_log_slow_diagnostic("terminal_suggestion_refresh", Instant::now()) {
            return;
        }
        tracing::warn!(
            diagnostic = "terminal_suggestion_refresh",
            outcome,
            pattern_chars,
            result_count,
            command_history_count = self.commands.command_history().len(),
            quick_command_count = self.commands.quick_commands().len(),
            tracker_value_bytes = self.terminal.assist.command_input_tracker.value.len(),
            tracker_desynced = self.terminal.assist.command_input_tracker.desynced,
            tracker_multiline = self.terminal.assist.command_input_tracker.multiline,
            popup_visible_at_start,
            popup_visible = self.terminal.assist.command_suggestions.is_some(),
            total_us = total_duration.as_micros(),
            pattern_us = timing.pattern.as_micros(),
            search_us = timing.search.as_micros(),
            cursor_us = timing.cursor.as_micros(),
            update_state_us = timing.update_state.as_micros(),
            hide_popup_us = timing.hide_popup.as_micros(),
            "slow terminal suggestion refresh"
        );
    }

    fn hide_command_suggestions_if_present(&mut self, cx: &mut Context<Self>) {
        if self.terminal.assist.command_suggestions.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::features) fn active_terminal_cursor_cell(&self) -> (usize, usize) {
        let offset = self.active_terminal_display_offset();
        let snapshot = self.terminal_snapshot_for_session(self.session.active_id(), offset);
        let row = if snapshot.cursor.row == usize::MAX {
            snapshot.row_count().saturating_sub(1)
        } else {
            snapshot.cursor.row
        };
        (row, snapshot.cursor.col)
    }

    /// Handle suggestion popup keys. Returns true when the key was consumed.
    pub(in crate::features) fn handle_command_suggestion_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.terminal.assist.command_suggestions.as_ref() else {
            return false;
        };
        if state.items.is_empty() {
            return false;
        }
        let session_id = state.session_id.clone();
        if self.session.active_id() != Some(session_id.as_str())
            || self
                .terminal_surface_bounds_for_session(Some(&session_id))
                .is_none()
        {
            self.terminal.assist.command_suggestions = None;
            cx.notify();
            return false;
        }
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.alt || keystroke.modifiers.control {
            return false;
        }
        match keystroke.key.as_str() {
            "escape" => {
                self.dismiss_command_suggestions(cx);
                true
            }
            "up" => {
                if let Some(state) = self.terminal.assist.command_suggestions.as_mut() {
                    state.selected_index = command_suggestion_step_selection(
                        state.selected_index,
                        state.items.len(),
                        -1,
                    );
                    cx.notify();
                }
                true
            }
            "down" => {
                if let Some(state) = self.terminal.assist.command_suggestions.as_mut() {
                    state.selected_index = command_suggestion_step_selection(
                        state.selected_index,
                        state.items.len(),
                        1,
                    );
                    cx.notify();
                }
                true
            }
            "tab" => self.apply_selected_command_suggestion(false, cx),
            "enter" => self.apply_selected_command_suggestion(true, cx),
            _ => false,
        }
    }

    pub(in crate::features) fn apply_selected_command_suggestion(
        &mut self,
        execute: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.terminal.assist.command_suggestions.clone() else {
            return false;
        };
        let Some(item) =
            command_suggestion_item_for_selection(&state.items, state.selected_index).cloned()
        else {
            return false;
        };
        let source = item.source.clone();
        let command = item.command;
        // Tauri replaceCurrentLine path: Ctrl+E (end) + Ctrl+U (kill line) + command.
        let mut payload = String::new();
        payload.push('\u{05}'); // Ctrl+E
        payload.push('\u{15}'); // Ctrl+U
        payload.push_str(&command);
        if execute {
            payload.push('\r');
        }
        self.terminal.assist.command_input_tracker = TerminalInputState::new();
        self.terminal.assist.command_suggestions = None;
        self.send_terminal_input_without_suggestion_track(payload.into_bytes(), cx);
        if !execute {
            // After fill, tracker becomes the filled command for continued typing.
            self.terminal.assist.command_input_tracker =
                apply_terminal_input_data(&TerminalInputState::new(), &command);
            self.refresh_command_suggestions(cx);
        }
        self.shell.set_status(if execute {
            format!("executed suggestion from {source}")
        } else {
            format!("filled suggestion from {source}")
        });
        cx.notify();
        true
    }

    pub(in crate::features) fn delete_command_suggestion_history(
        &mut self,
        command: String,
        cx: &mut Context<Self>,
    ) {
        let command = command.trim().to_string();
        if command.is_empty() {
            return;
        }
        let request_command = command.clone();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Commands, move |store| {
                store.delete_command_history(&request_command)?;
                store.list_command_history(64)
            }),
            move |this, event, cx| {
                match event.outcome {
                    Ok(history) => {
                        this.commands.replace_command_history(history);
                        this.session.remove_command_from_all_history(&command);
                        if let Some(state) = this.terminal.assist.command_suggestions.as_mut() {
                            state.items.retain(|item| {
                                !(item.source == "history" && item.command == command)
                            });
                            if state.items.is_empty() {
                                this.terminal.assist.command_suggestions = None;
                            } else {
                                state.selected_index = command_suggestion_clamp_selection(
                                    state.selected_index,
                                    state.items.len(),
                                );
                            }
                        } else {
                            this.refresh_command_suggestions(cx);
                        }
                        this.shell
                            .set_status(format!("deleted history command '{command}'"));
                    }
                    Err(error) => this
                        .shell
                        .set_status(format!("failed to delete history: {error}")),
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(in crate::features) fn command_suggestions_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let Some(state) = self.terminal.assist.command_suggestions.as_ref() else {
            return div().into_any_element();
        };
        if state.items.is_empty() {
            return div().into_any_element();
        }
        if self.session.active_id() != Some(state.session_id.as_str()) {
            return div().into_any_element();
        }
        let menu_w = 380.0_f32;
        let menu_h = suggestion_overlay_desired_height(state.items.len(), 28.0);
        let title = t!("suggestions.title");
        let match_label = t!(if state.items.len() == 1 {
            "suggestions.match"
        } else {
            "suggestions.matches"
        });
        let footer = format!(
            "↑↓ {} · Enter {} · Tab {} · Esc {}",
            t!("suggestions.select"),
            t!("suggestions.execute"),
            t!("suggestions.fill"),
            t!("suggestions.dismiss")
        );
        let Some(placement) = self.suggestion_overlay_position_for_session(
            Some(&state.session_id),
            state.cursor_row,
            state.cursor_col,
            menu_w,
            menu_h,
        ) else {
            return div().into_any_element();
        };
        let selected_preview = state
            .selected_index
            .and_then(|index| state.items.get(index))
            .filter(|item| item.command.chars().count() > 48)
            .map(|item| item.command.clone());

        let mut list = div()
            .id(SharedString::from("command-suggestions-list"))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar();
        for (index, item) in state.items.iter().enumerate() {
            let selected = state.selected_index == Some(index);
            let source_icon = match item.source.as_str() {
                "history" => "icons/history.svg",
                _ => "icons/commands.svg",
            };
            let label = if item.display.trim().is_empty() {
                item.command.clone()
            } else {
                item.display.clone()
            };
            let show_full_command = label.chars().count() > 48;
            let full_command = item.command.clone();
            let is_history = item.source == "history";
            let delete_command = item.command.clone();
            let mut row = div()
                .id(SharedString::from(format!("command-suggestion-{index}")))
                .h(px(28.))
                .flex_none()
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .border_l_2()
                .border_color(rgb(if selected {
                    palette.primary
                } else {
                    palette.surface
                }))
                .bg(rgb(if selected {
                    palette.hover
                } else {
                    palette.surface
                }))
                .text_size(px(11.))
                .text_color(rgb(palette.text))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(state) = this.terminal.assist.command_suggestions.as_mut() {
                        state.selected_index = Some(index);
                    }
                    let _ = this.apply_selected_command_suggestion(true, cx);
                }))
                .child(
                    svg()
                        .size(px(13.))
                        .flex_none()
                        .path(source_icon)
                        .text_color(if selected {
                            rgb(palette.accent)
                        } else {
                            rgb(palette.text_dimmed)
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .children(command_suggestion_highlight_parts(
                            &label,
                            &item.indices,
                            palette,
                            selected,
                        )),
                );
            if show_full_command {
                row = row.tooltip(move |window, cx| {
                    ZzClawTooltip::new(full_command.clone()).build(window, cx)
                });
            }
            if is_history {
                row = row.child(
                    div()
                        .id(SharedString::from(format!(
                            "command-suggestion-del-{index}"
                        )))
                        .flex_none()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_dimmed))
                        .hover(|this| {
                            this.bg(rgb(palette.surface_elevated))
                                .text_color(rgb(palette.danger))
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_command_suggestion_history(delete_command.clone(), cx);
                        }))
                        .child(
                            svg()
                                .size(px(13.))
                                .path("icons/fe/delete.svg")
                                .text_color(rgb(palette.text_dimmed)),
                        ),
                );
            }
            list = list.child(row);
        }

        div()
            .id(SharedString::from("command-suggestions-overlay"))
            .absolute()
            .occlude()
            .left(px(placement.x))
            .top(px(placement.y))
            .w(px(menu_w))
            .h(px(placement.height))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgba((palette.surface << 8) | 0xf2))
            .shadow_lg()
            .when_some(selected_preview, |this, command| {
                this.child(
                    div()
                        .absolute()
                        .bottom(px(placement.height + 6.))
                        .left_0()
                        .w_full()
                        .max_w(px(560.))
                        .p_2()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.surface_elevated))
                        .shadow_lg()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .text_size(px(11.))
                        .text_color(rgb(palette.text))
                        .whitespace_normal()
                        .child(command),
                )
            })
            .child(
                div()
                    .h(px(SUGGESTION_OVERLAY_HEADER_HEIGHT))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(10.))
                            .font_weight(FontWeight(600.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(
                                svg()
                                    .size(px(12.))
                                    .path("icons/commands.svg")
                                    .text_color(rgb(palette.text_dimmed)),
                            )
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(format!("{} {match_label}", state.items.len())),
                    ),
            )
            .child(list)
            .child(
                div()
                    .h(px(SUGGESTION_OVERLAY_FOOTER_HEIGHT))
                    .flex_none()
                    .px_2()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .items_center()
                    .text_size(px(10.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(footer),
            )
            .into_any_element()
    }

    pub(in crate::features) fn suggestion_overlay_position_for_session(
        &self,
        session_id: Option<&str>,
        cursor_row: usize,
        cursor_col: usize,
        menu_w: f32,
        menu_h: f32,
    ) -> Option<SuggestionOverlayPlacement> {
        let bounds = self.terminal_surface_bounds_for_session(session_id)?;
        let insets = self.terminal_content_insets_for_bounds(session_id, bounds);
        suggestion_overlay_position(
            SuggestionOverlayGeometry {
                bounds,
                cell_size: self.terminal_cell_size(),
                content_origin: (insets.left, insets.top),
                gutter: self.terminal_gutter_width_px_for_session(session_id),
                viewport_size: self.shell.viewport_size(),
            },
            SuggestionOverlayTarget {
                cursor: (cursor_row, cursor_col),
                menu_size: (menu_w, menu_h),
            },
        )
    }
}

fn command_suggestion_state_changed(
    current: Option<&CommandSuggestionState>,
    next: &CommandSuggestionState,
) -> bool {
    current != Some(next)
}

fn command_suggestion_item_for_selection(
    items: &[CommandSuggestionItem],
    selected_index: Option<usize>,
) -> Option<&CommandSuggestionItem> {
    items.get(selected_index?)
}

fn command_suggestion_step_selection(
    current: Option<usize>,
    len: usize,
    direction: i32,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if direction > 0 {
        return match current {
            None => Some(0),
            Some(index) if index + 1 < len => Some(index + 1),
            Some(_) => None,
        };
    }
    if direction < 0 {
        return match current {
            None => Some(len - 1),
            Some(0) => None,
            Some(index) => Some(index.min(len - 1).saturating_sub(1)),
        };
    }
    command_suggestion_clamp_selection(current, len)
}

fn command_suggestion_clamp_selection(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    current.map(|index| index.min(len - 1))
}

pub(in crate::features) fn suggestion_overlay_desired_height(
    item_count: usize,
    row_height: f32,
) -> f32 {
    (item_count as f32 * row_height + SUGGESTION_OVERLAY_CHROME_HEIGHT)
        .min(SUGGESTION_OVERLAY_MAX_HEIGHT)
}

pub(in crate::features) fn suggestion_overlay_position(
    geometry: SuggestionOverlayGeometry,
    target: SuggestionOverlayTarget,
) -> Option<SuggestionOverlayPlacement> {
    let SuggestionOverlayGeometry {
        bounds,
        cell_size,
        content_origin: (pad_left, pad_top),
        gutter,
        viewport_size,
    } = geometry;
    let SuggestionOverlayTarget {
        cursor: (cursor_row, cursor_col),
        menu_size: (menu_w, menu_h),
    } = target;
    let (cell_w, cell_h) = cell_size;
    let bounds_x = f32::from(bounds.origin.x);
    let bounds_y = f32::from(bounds.origin.y);
    let bounds_height = f32::from(bounds.size.height);
    let base_x = bounds_x + pad_left + gutter + cursor_col as f32 * cell_w;
    let cursor_top = bounds_y + pad_top + cursor_row as f32 * cell_h;
    let cursor_bottom = cursor_top + cell_h;
    let (viewport_w, viewport_h) = viewport_size;
    let surface_top = bounds_y.max(SUGGESTION_OVERLAY_VIEWPORT_MARGIN);
    let surface_bottom =
        (bounds_y + bounds_height).min(viewport_h - SUGGESTION_OVERLAY_VIEWPORT_MARGIN);
    if surface_bottom <= surface_top || menu_h <= 0.0 {
        return None;
    }

    let below_y = cursor_bottom + SUGGESTION_OVERLAY_CURSOR_GAP;
    let above_bottom = cursor_top - SUGGESTION_OVERLAY_CURSOR_GAP;
    let space_below = (surface_bottom - below_y).max(0.0);
    let space_above = (above_bottom - surface_top).max(0.0);
    let (y, height) = if menu_h <= space_below {
        (below_y, menu_h)
    } else if menu_h <= space_above {
        (above_bottom - menu_h, menu_h)
    } else if space_above > space_below {
        (above_bottom - space_above, space_above)
    } else {
        (below_y, space_below)
    };
    if height <= 0.0 {
        return None;
    }

    let max_x = (viewport_w - menu_w - SUGGESTION_OVERLAY_VIEWPORT_MARGIN)
        .max(SUGGESTION_OVERLAY_VIEWPORT_MARGIN);
    Some(SuggestionOverlayPlacement {
        x: base_x.clamp(SUGGESTION_OVERLAY_VIEWPORT_MARGIN, max_x),
        y,
        height,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::features) struct SuggestionOverlayPlacement {
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

#[derive(Clone, Copy)]
pub(in crate::features) struct SuggestionOverlayGeometry {
    pub bounds: Bounds<Pixels>,
    pub cell_size: (f32, f32),
    pub content_origin: (f32, f32),
    pub gutter: f32,
    pub viewport_size: (f32, f32),
}

#[derive(Clone, Copy)]
pub(in crate::features) struct SuggestionOverlayTarget {
    pub cursor: (usize, usize),
    pub menu_size: (f32, f32),
}

#[cfg(test)]
mod overlay_position_tests {
    use gpui::{Bounds, point, px, size};

    use super::{
        SuggestionOverlayGeometry, SuggestionOverlayTarget, suggestion_overlay_desired_height,
        suggestion_overlay_position,
    };

    #[test]
    fn suggestion_overlay_position_anchors_below_cursor() {
        let placement = suggestion_overlay_position(
            SuggestionOverlayGeometry {
                bounds: Bounds::new(point(px(10.0), px(20.0)), size(px(800.0), px(400.0))),
                cell_size: (8.0, 16.0),
                content_origin: (8.0, 0.0),
                gutter: 0.0,
                viewport_size: (1024.0, 768.0),
            },
            SuggestionOverlayTarget {
                cursor: (2, 4),
                menu_size: (300.0, 120.0),
            },
        )
        .expect("surface has room below the cursor");

        assert_eq!(placement.x, 50.0);
        assert_eq!(placement.y, 72.0);
        assert_eq!(placement.height, 120.0);
    }

    #[test]
    fn suggestion_overlay_position_clamps_and_flips_above_cursor() {
        let placement = suggestion_overlay_position(
            SuggestionOverlayGeometry {
                bounds: Bounds::new(point(px(700.0), px(100.0)), size(px(200.0), px(500.0))),
                cell_size: (8.0, 16.0),
                content_origin: (8.0, 0.0),
                gutter: 0.0,
                viewport_size: (900.0, 620.0),
            },
            SuggestionOverlayTarget {
                cursor: (24, 30),
                menu_size: (300.0, 140.0),
            },
        )
        .expect("surface has room above the cursor");

        assert_eq!(placement.x, 592.0);
        assert_eq!(placement.y, 340.0);
        assert_eq!(placement.height, 140.0);
        assert_eq!(placement.y + placement.height, 480.0);
    }

    #[test]
    fn suggestion_overlay_position_limits_height_without_covering_cursor() {
        let placement = suggestion_overlay_position(
            SuggestionOverlayGeometry {
                bounds: Bounds::new(point(px(20.0), px(100.0)), size(px(600.0), px(180.0))),
                cell_size: (8.0, 16.0),
                content_origin: (0.0, 0.0),
                gutter: 0.0,
                viewport_size: (1024.0, 768.0),
            },
            SuggestionOverlayTarget {
                cursor: (6, 4),
                menu_size: (300.0, 140.0),
            },
        )
        .expect("surface has constrained room above the cursor");

        assert_eq!(placement.y, 100.0);
        assert_eq!(placement.height, 92.0);
        assert_eq!(placement.y + placement.height, 192.0);
    }

    #[test]
    fn suggestion_overlay_position_uses_terminal_surface_bottom() {
        let placement = suggestion_overlay_position(
            SuggestionOverlayGeometry {
                bounds: Bounds::new(point(px(20.0), px(100.0)), size(px(600.0), px(200.0))),
                cell_size: (8.0, 16.0),
                content_origin: (0.0, 0.0),
                gutter: 0.0,
                viewport_size: (1024.0, 768.0),
            },
            SuggestionOverlayTarget {
                cursor: (8, 4),
                menu_size: (300.0, 120.0),
            },
        )
        .expect("surface has room above but not below the cursor");

        assert_eq!(placement.y, 104.0);
        assert_eq!(placement.height, 120.0);
        assert_eq!(placement.y + placement.height, 224.0);
    }

    #[test]
    fn suggestion_overlay_height_is_content_sized_and_capped() {
        assert_eq!(suggestion_overlay_desired_height(1, 28.0), 76.0);
        assert_eq!(suggestion_overlay_desired_height(10, 28.0), 320.0);
        assert_eq!(suggestion_overlay_desired_height(1, 36.0), 84.0);
        assert_eq!(suggestion_overlay_desired_height(8, 36.0), 320.0);
    }
}

fn terminal_line_prefix_for_cell_col(line: &str, cell_col: usize) -> String {
    let end = terminal_byte_index_for_cell_col(line, cell_col);
    line.get(..end).unwrap_or(line).to_string()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use zzclawterm_core::{TerminalInputState, apply_terminal_input_data};

    use crate::features::terminal::terminal_runtime::TERMINAL_INPUT_LATENCY_WINDOW;
    use crate::models::{CommandSuggestionItem, CommandSuggestionState};

    use super::{
        command_history_input_update, command_suggestion_clamp_selection,
        command_suggestion_input_can_defer_refresh, command_suggestion_input_candidate_chars,
        command_suggestion_input_obvious_pager_prefix, command_suggestion_item_for_selection,
        command_suggestion_refresh_input_delay, command_suggestion_state_changed,
        command_suggestion_step_selection, terminal_line_prefix_for_cell_col,
    };

    #[test]
    fn terminal_line_prefix_uses_terminal_cells_for_wide_chars() {
        assert_eq!(terminal_line_prefix_for_cell_col("界x", 0), "");
        assert_eq!(terminal_line_prefix_for_cell_col("界x", 1), "");
        assert_eq!(terminal_line_prefix_for_cell_col("界x", 2), "界");
        assert_eq!(terminal_line_prefix_for_cell_col("界x", 3), "界x");
    }

    #[test]
    fn terminal_line_prefix_keeps_combining_mark_with_base_char() {
        let text = "e\u{301}x";

        assert_eq!(terminal_line_prefix_for_cell_col(text, 0), "");
        assert_eq!(terminal_line_prefix_for_cell_col(text, 1), "e\u{301}");
        assert_eq!(terminal_line_prefix_for_cell_col(text, 2), "e\u{301}x");
    }

    #[test]
    fn command_suggestion_refresh_input_delay_waits_for_terminal_idle_window() {
        let now = Instant::now();
        assert_eq!(
            command_suggestion_refresh_input_delay(Some(now), now),
            Some(TERMINAL_INPUT_LATENCY_WINDOW)
        );
        assert_eq!(
            command_suggestion_refresh_input_delay(
                Some(now - TERMINAL_INPUT_LATENCY_WINDOW - Duration::from_millis(1)),
                now
            ),
            None
        );
        assert_eq!(command_suggestion_refresh_input_delay(None, now), None);
    }

    #[test]
    fn command_suggestion_input_defer_refresh_requires_cursor_at_end() {
        let mut state = TerminalInputState {
            value: "git status".to_string(),
            cursor: 3,
            ..TerminalInputState::default()
        };
        assert!(!command_suggestion_input_can_defer_refresh(&state));

        state.cursor = state.value.len();
        assert!(command_suggestion_input_can_defer_refresh(&state));

        state.desynced = true;
        assert!(!command_suggestion_input_can_defer_refresh(&state));
    }

    #[test]
    fn command_suggestion_input_detects_obvious_pager_prefix_without_sanitizing() {
        let state = TerminalInputState {
            value: "  /search".to_string(),
            cursor: 9,
            ..TerminalInputState::default()
        };
        assert!(command_suggestion_input_obvious_pager_prefix(&state));

        let state = TerminalInputState {
            value: "git status".to_string(),
            cursor: 10,
            ..TerminalInputState::default()
        };
        assert!(!command_suggestion_input_obvious_pager_prefix(&state));
        assert_eq!(command_suggestion_input_candidate_chars(&state), 10);
    }

    #[test]
    fn command_suggestion_state_change_skips_identical_overlay() {
        let state = CommandSuggestionState {
            session_id: "session".to_string(),
            draft: "git".to_string(),
            items: vec![CommandSuggestionItem {
                command: "git status".to_string(),
                display: "git status".to_string(),
                source: "history".to_string(),
                score: 42,
                indices: vec![0, 1, 2],
            }],
            selected_index: None,
            cursor_row: 3,
            cursor_col: 4,
        };
        let mut selected = state.clone();
        selected.selected_index = Some(0);

        assert!(!command_suggestion_state_changed(Some(&state), &state));
        assert!(command_suggestion_state_changed(None, &state));
        assert!(command_suggestion_state_changed(Some(&state), &selected));
    }

    #[test]
    fn command_suggestion_step_selection_enters_and_exits_with_down() {
        assert_eq!(command_suggestion_step_selection(None, 3, 1), Some(0));
        assert_eq!(command_suggestion_step_selection(Some(0), 3, 1), Some(1));
        assert_eq!(command_suggestion_step_selection(Some(2), 3, 1), None);
        assert_eq!(command_suggestion_step_selection(None, 0, 1), None);
    }

    #[test]
    fn command_suggestion_step_selection_enters_and_exits_with_up() {
        assert_eq!(command_suggestion_step_selection(None, 3, -1), Some(2));
        assert_eq!(command_suggestion_step_selection(Some(2), 3, -1), Some(1));
        assert_eq!(command_suggestion_step_selection(Some(0), 3, -1), None);
        assert_eq!(command_suggestion_step_selection(None, 0, -1), None);
    }

    #[test]
    fn command_suggestion_clamp_selection_preserves_none_and_clamps_overflow() {
        assert_eq!(command_suggestion_clamp_selection(None, 3), None);
        assert_eq!(command_suggestion_clamp_selection(Some(1), 3), Some(1));
        assert_eq!(command_suggestion_clamp_selection(Some(7), 3), Some(2));
        assert_eq!(command_suggestion_clamp_selection(Some(0), 0), None);
    }

    #[test]
    fn command_suggestion_accept_requires_selected_item() {
        let items = vec![CommandSuggestionItem {
            command: "git status".to_string(),
            display: "git status".to_string(),
            source: "history".to_string(),
            score: 42,
            indices: vec![0, 1, 2],
        }];

        assert!(command_suggestion_item_for_selection(&items, None).is_none());
        assert!(command_suggestion_item_for_selection(&items, Some(1)).is_none());
        assert_eq!(
            command_suggestion_item_for_selection(&items, Some(0))
                .map(|item| item.command.as_str()),
            Some("git status")
        );
    }

    #[test]
    fn command_history_input_update_captures_submission_before_enter_reset() {
        let mut state = apply_terminal_input_data(&TerminalInputState::new(), "git status");

        let submitted = command_history_input_update(&mut state, "\r");

        assert_eq!(submitted.as_deref(), Some("git status"));
        assert!(state.value.is_empty());
    }
}
