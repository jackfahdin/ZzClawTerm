use rust_i18n::t;

use futures::StreamExt as _;

use nyaterm_ui::NyaScrollable;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb, rgba,
    svg,
};
use nyaterm_core::{CredentialPromptKind, SavedCredential, TerminalInputState, truncate_preview};
use nyaterm_store::{StoreDomain, store_request};
use nyaterm_terminal::TerminalSnapshot;

use crate::features::NyaTermApp;
use crate::models::{
    CredentialAutofillMatchEvent, CredentialAutofillMatchOutcome, CredentialAutofillMatchRequest,
    CredentialAutofillMatchRequestKey, CredentialSuggestionState, PendingCredentialAutofill,
};

use super::command_suggestions::{
    SUGGESTION_OVERLAY_FOOTER_HEIGHT, SUGGESTION_OVERLAY_HEADER_HEIGHT,
    suggestion_overlay_desired_height,
};

const CREDENTIAL_AUTOFILL_INPUT_TAIL_LIMIT: usize = 4096;
const RECENT_PROMPT_TTL_MS: u64 = 30_000;
const PENDING_PASSWORD_TTL_MS: u64 = 60_000;
const CREDENTIAL_PROMPT_INPUT_TTL_MS: u64 = 120_000;

impl NyaTermApp {
    pub(in crate::features) fn now_unix_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
    }

    pub(in crate::features) fn dismiss_credential_suggestions(&mut self, cx: &mut Context<Self>) {
        let had_panel = self.terminal.assist.dismiss_credential_suggestions();
        if had_panel {
            cx.notify();
        }
    }

    pub(in crate::features) fn is_credential_prompt_input_mode(&self) -> bool {
        let now = Self::now_unix_ms();
        self.terminal.assist.credential_prompt_input_mode(now)
    }

    fn prune_recent_credential_prompts(&mut self, now: u64) {
        self.terminal
            .assist
            .credential_autofill_recent
            .retain(|_, ts| now.saturating_sub(*ts) <= RECENT_PROMPT_TTL_MS);
    }

    fn remember_credential_prompt(
        &mut self,
        kind: CredentialPromptKind,
        prompt_text: &str,
        now: u64,
    ) -> bool {
        self.prune_recent_credential_prompts(now);
        let key = format!("{kind:?}:{prompt_text}");
        if let Some(last) = self.terminal.assist.credential_autofill_recent.get(&key)
            && now.saturating_sub(*last) < RECENT_PROMPT_TTL_MS
        {
            return false;
        }
        self.terminal
            .assist
            .credential_autofill_recent
            .insert(key, now);
        true
    }

    fn show_credential_panel(
        &mut self,
        kind: CredentialPromptKind,
        matches: Vec<SavedCredential>,
        prompt_text: String,
        cx: &mut Context<Self>,
    ) {
        if matches.is_empty() {
            return;
        }
        let Some(session_id) = self.session.active_id_owned() else {
            return;
        };
        let (cursor_row, cursor_col) = self.active_terminal_cursor_cell_for_autofill();
        self.dismiss_command_suggestions(cx);
        self.terminal.assist.credential_suggestions = Some(CredentialSuggestionState {
            session_id,
            kind,
            matches,
            prompt_text,
            selected_index: 0,
            cursor_row,
            cursor_col,
        });
        cx.notify();
    }

    fn active_terminal_cursor_cell_for_autofill(&self) -> (usize, usize) {
        let offset = self.active_terminal_display_offset();
        let snapshot = self.terminal_snapshot_for_session(self.session.active_id(), offset);
        let row = if snapshot.cursor.row == usize::MAX {
            snapshot.row_count().saturating_sub(1)
        } else {
            snapshot.cursor.row
        };
        (row, snapshot.cursor.col)
    }

    pub(in crate::features) fn drain_pending_credential_autofill_detection(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        // Common idle path: no credentials, no detection, no match pipeline reply.
        if self
            .terminal
            .assist
            .credential_autofill_pending_request
            .is_none()
            && !self.terminal.assist.credential_autofill_detection_pending
            && self.security.credentials().is_empty()
            && self.terminal.assist.credential_autofill_pending.is_none()
        {
            return false;
        }
        // Replies arrive on `start_credential_autofill_match_drain`; what is
        // left here is snapshot-driven detection.
        let mut root_overlay_dirty = false;
        let detection_was_pending = self.terminal.assist.credential_autofill_detection_pending;
        let runtime_backlog = CredentialAutofillRuntimeBacklog {
            queued_output_bytes: self.shell.session_event_queued_output_bytes(),
            pending_session_events: self.session.pending_event_count(),
            pending_terminal_frame_events: self.terminal.view.pending_frame_events.len(),
            queued_terminal_frame_events: self.terminal.view.frame_pipeline.queued_event_count(),
            queued_terminal_frame_output_bytes: self
                .terminal
                .view
                .frame_pipeline
                .queued_output_bytes(),
        };
        let match_request_pending = self
            .terminal
            .assist
            .credential_autofill_pending_request
            .is_some();
        if credential_autofill_snapshot_detection_can_run(
            self.session.active_id(),
            !self.security.credentials().is_empty()
                || self.terminal.assist.credential_autofill_pending.is_some(),
            runtime_backlog,
            match_request_pending,
        ) {
            root_overlay_dirty |= self.sync_credential_autofill_from_active_snapshot(cx);
        }
        if !credential_autofill_detection_should_run_this_tick(
            detection_was_pending,
            credential_autofill_pending_detection_can_run(
                self.session.active_id(),
                self.terminal.assist.credential_autofill_detection_pending,
                runtime_backlog,
                match_request_pending,
            ),
        ) {
            return root_overlay_dirty;
        }
        self.terminal.assist.credential_autofill_detection_pending = false;
        let _ = self.detect_credential_prompt(cx);
        root_overlay_dirty
    }

    fn sync_credential_autofill_from_active_snapshot(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session_id) = self.session.active_id_owned() else {
            return false;
        };
        let Some(prompt_text) = self
            .terminal
            .view
            .views
            .get(&session_id)
            .and_then(|view| view.frame_snapshot.as_deref())
            .and_then(credential_autofill_prompt_text_from_snapshot)
        else {
            return self.sync_credential_autofill_prompt_text(&session_id, String::new(), cx);
        };
        self.sync_credential_autofill_prompt_text(&session_id, prompt_text, cx)
    }

    fn sync_credential_autofill_prompt_text(
        &mut self,
        session_id: &str,
        prompt_text: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.session.active_id() != Some(session_id) {
            return false;
        }
        if prompt_text.is_empty() {
            if !self.terminal.assist.credential_autofill_buffer.is_empty() {
                self.terminal.assist.credential_autofill_buffer.clear();
                self.terminal.assist.credential_prompt_input_until_ms = 0;
                return false;
            }
            return false;
        }
        if self.terminal.assist.credential_autofill_buffer == prompt_text
            && (self.terminal.assist.credential_autofill_detection_pending
                || self.terminal.assist.credential_autofill_pending.is_none())
        {
            return false;
        }
        let detected_prompt_kind = credential_autofill_detect_prompt_kind(&prompt_text);
        if detected_prompt_kind.is_none()
            && self.terminal.assist.credential_autofill_pending.is_none()
        {
            if !self.terminal.assist.credential_autofill_buffer.is_empty() {
                self.terminal.assist.credential_autofill_buffer.clear();
                self.terminal.assist.credential_prompt_input_until_ms = 0;
                return false;
            }
            return false;
        }

        if self.terminal.assist.credential_autofill_buffer != prompt_text {
            self.terminal.assist.credential_autofill_buffer = prompt_text;
        }
        let mut root_overlay_dirty = false;
        if detected_prompt_kind.is_some() {
            self.terminal.assist.credential_prompt_input_until_ms =
                Self::now_unix_ms().saturating_add(CREDENTIAL_PROMPT_INPUT_TTL_MS);
            // Suppress command suggestions while a credential prompt is live.
            if self.terminal.assist.command_suggestions.take().is_some() {
                root_overlay_dirty = true;
            }
            self.terminal.assist.command_input_tracker = TerminalInputState::new();
        }

        if self.terminal.assist.credential_suggestions.is_some()
            || self.terminal.assist.credential_autofill_sending
        {
            return root_overlay_dirty;
        }
        if !self.terminal.assist.credential_autofill_detection_pending {
            self.terminal.assist.credential_autofill_detection_pending = true;
        }
        let _ = cx;
        root_overlay_dirty
    }

    pub(in crate::features) fn detect_credential_prompt(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> bool {
        if self.session.active_id().is_none()
            || self.terminal.assist.credential_suggestions.is_some()
        {
            return false;
        }
        if self.security.credentials().is_empty() {
            return false;
        }

        let now = Self::now_unix_ms();
        let prompt_text = credential_autofill_prompt_text_from_visible(
            &self.terminal.assist.credential_autofill_buffer,
        );
        if prompt_text.is_empty() {
            return false;
        }
        let Some(prompt_kind) = credential_autofill_detect_prompt_kind(&prompt_text) else {
            return false;
        };
        let current_line = prompt_text.trim().to_string();
        let Some(active_session_id) = self.session.active_id_owned() else {
            return false;
        };
        let credentials = self.security.credentials().to_vec();

        if let Some(pending) = self.terminal.assist.credential_autofill_pending.clone()
            && pending.expires_at_ms <= now
        {
            self.terminal.assist.credential_autofill_pending = None;
        }

        if let Some(pending) = self.terminal.assist.credential_autofill_pending.clone()
            && pending.session_id != active_session_id
        {
            self.terminal.assist.credential_autofill_pending = None;
        }

        if self.terminal.assist.credential_autofill_pending.is_none()
            && !self.remember_credential_prompt(prompt_kind, &prompt_text, now)
        {
            return false;
        }

        self.terminal.assist.credential_autofill_next_request_id = self
            .terminal
            .assist
            .credential_autofill_next_request_id
            .saturating_add(1);
        let key = CredentialAutofillMatchRequestKey {
            request_id: self.terminal.assist.credential_autofill_next_request_id,
            session_id: active_session_id,
            prompt_text,
        };
        self.terminal.assist.credential_autofill_pending_request = Some(key.clone());
        self.terminal
            .assist
            .credential_autofill_match_pipeline
            .request(CredentialAutofillMatchRequest {
                key,
                current_line,
                prompt_kind,
                credentials,
                pending: self.terminal.assist.credential_autofill_pending.clone(),
            });
        true
    }

    /// Deliver credential-autofill match replies as they arrive.
    ///
    /// Started once at window open. The reply queue dedups per (session, prompt)
    /// and drops the oldest under pressure, so it stays a queue and only the wake
    /// signal is a channel; see `models::event_wake`.
    ///
    /// Only the *reply* half moves here. Prompt detection still runs on the
    /// runtime tick, because it scans the terminal frame snapshot and
    /// deliberately holds off while output backlog is high -- that belongs with
    /// the terminal frame work, driven off a frame being applied.
    pub(in crate::features) fn start_credential_autofill_match_drain(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(mut wake_rx) = self
            .terminal
            .assist
            .credential_autofill_match_pipeline
            .take_wake_receiver()
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            loop {
                // Arm before draining: a reply pushed between the drain and the
                // arm would otherwise go unsignalled and sit until something
                // unrelated woke us.
                let drained = this.update(cx, |this, cx| {
                    this.terminal
                        .assist
                        .credential_autofill_match_pipeline
                        .arm_event_wake();
                    let dirty = this.drain_credential_autofill_match_events(cx);
                    if dirty {
                        cx.notify();
                    }
                    dirty
                });
                match drained {
                    Err(_) => break,
                    // Applying one reply can leave more queued; keep going before
                    // sleeping, rather than waiting for the next signal.
                    Ok(true) => continue,
                    Ok(false) => {}
                }
                if wake_rx.next().await.is_none() {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_credential_autofill_match_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut dirty = false;
        while let Some(event) = self
            .terminal
            .assist
            .credential_autofill_match_pipeline
            .try_recv_event()
        {
            dirty |= self.apply_credential_autofill_match_event(event, cx);
        }
        dirty
    }

    fn apply_credential_autofill_match_event(
        &mut self,
        event: CredentialAutofillMatchEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .terminal
            .assist
            .credential_autofill_pending_request
            .as_ref()
            != Some(&event.key)
        {
            return false;
        }
        self.terminal.assist.credential_autofill_pending_request = None;
        if self.session.active_id() != Some(event.key.session_id.as_str()) {
            return false;
        }
        if credential_autofill_prompt_text_from_visible(
            &self.terminal.assist.credential_autofill_buffer,
        ) != event.key.prompt_text
        {
            return false;
        }

        match event.outcome {
            CredentialAutofillMatchOutcome::Suggest {
                kind,
                matches,
                clear_pending,
            } => {
                if clear_pending {
                    self.terminal.assist.credential_autofill_pending = None;
                }
                self.show_credential_panel(kind, matches, event.key.prompt_text, cx);
                true
            }
            CredentialAutofillMatchOutcome::AutoFill { credential, kind } => {
                self.terminal.assist.credential_autofill_pending = None;
                self.terminal.assist.credential_autofill_buffer.clear();
                self.terminal.assist.credential_autofill_recent.clear();
                self.send_credential_value(&credential, kind, &event.key.session_id, cx);
                true
            }
            CredentialAutofillMatchOutcome::NoMatch { clear_pending } => {
                if clear_pending {
                    self.terminal.assist.credential_autofill_pending = None;
                }
                false
            }
        }
    }

    fn send_credential_value(
        &mut self,
        credential: &SavedCredential,
        kind: CredentialPromptKind,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        if session_id.is_empty() {
            return;
        }
        if self.session.is_disconnected(session_id) {
            self.shell.set_status(
                "session disconnected - reconnect before filling credentials".to_string(),
            );
            cx.notify();
            return;
        }
        if self.session.active_id() != Some(session_id) {
            self.activate_session_id_with_surface_sync(session_id, cx);
        }
        match kind {
            CredentialPromptKind::Username => {
                let mut payload = credential.username.clone();
                payload.push('\r');
                self.send_terminal_input_without_suggestion_track(payload.into_bytes(), cx);
                self.shell
                    .set_status(format!("filled username from '{}'", credential.name));
            }
            CredentialPromptKind::Password => {
                let credential_id = credential.id.clone();
                let credential_name = credential.name.clone();
                let session_id = session_id.to_string();
                let submitted = self.submit_store_request(
                    0,
                    store_request(StoreDomain::Security, move |store| {
                        store.load_decrypted_credential_by_id(&credential_id)
                    }),
                    move |this, event, cx| {
                        if this.session.active_id() != Some(session_id.as_str()) {
                            this.shell.set_status(
                                "credential fill cancelled because the active session changed"
                                    .to_string(),
                            );
                            cx.notify();
                            return;
                        }
                        if this.session.is_disconnected(&session_id) {
                            this.shell.set_status(
                                "session disconnected - reconnect before filling credentials"
                                    .to_string(),
                            );
                            cx.notify();
                            return;
                        }
                        match event.outcome {
                            Ok(Some(entry)) => {
                                let Some(mut password) =
                                    entry.password.filter(|value| !value.is_empty())
                                else {
                                    this.shell.set_status(format!(
                                        "credential '{credential_name}' has no password"
                                    ));
                                    cx.notify();
                                    return;
                                };
                                password.expose_secret_mut().push('\r');
                                this.send_terminal_input_without_suggestion_track(
                                    password.into_secret().into_bytes(),
                                    cx,
                                );
                                this.shell.set_status(format!(
                                    "filled password from '{credential_name}'"
                                ));
                            }
                            Ok(None) => this.shell.set_status(format!(
                                "credential '{credential_name}' was not found"
                            )),
                            Err(error) => this.shell.set_status(format!(
                                "failed to load credential '{credential_name}': {error}"
                            )),
                        }
                        cx.notify();
                    },
                    cx,
                );
                if submitted {
                    self.shell
                        .set_status(format!("loading password from '{}'", credential.name));
                }
            }
        }
        cx.notify();
    }

    pub(in crate::features) fn apply_selected_credential_suggestion(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.terminal.assist.credential_suggestions.clone() else {
            return;
        };
        let Some(credential) = state.matches.get(state.selected_index).cloned() else {
            return;
        };
        self.select_credential_suggestion(credential, cx);
    }

    pub(in crate::features) fn select_credential_suggestion(
        &mut self,
        credential: SavedCredential,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.terminal.assist.credential_suggestions.clone() else {
            return;
        };
        if self.terminal.assist.credential_autofill_sending {
            return;
        }
        let was_username = state.kind == CredentialPromptKind::Username;
        self.terminal.assist.credential_autofill_sending = true;
        self.send_credential_value(&credential, state.kind, &state.session_id, cx);
        if was_username {
            self.terminal.assist.credential_autofill_pending = Some(PendingCredentialAutofill {
                session_id: state.session_id.clone(),
                credential_id: credential.id,
                expires_at_ms: Self::now_unix_ms().saturating_add(PENDING_PASSWORD_TTL_MS),
            });
        } else {
            self.terminal.assist.credential_autofill_pending = None;
        }
        self.terminal.assist.credential_autofill_sending = false;
        self.terminal.assist.credential_suggestions = None;
        self.terminal.assist.credential_autofill_recent.clear();

        if was_username {
            // Keep buffer so a password prompt that arrived during selection can still be detected.
            if !self.terminal.assist.credential_autofill_buffer.is_empty() {
                self.detect_credential_prompt(cx);
            }
        } else {
            self.terminal.assist.credential_autofill_buffer.clear();
            self.terminal.assist.credential_autofill_recent.clear();
        }
        cx.notify();
    }

    /// Handle credential panel keys. Returns true when the key was consumed.
    pub(in crate::features) fn handle_credential_suggestion_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.terminal.assist.credential_suggestions.as_ref() else {
            return false;
        };
        if state.matches.is_empty() {
            return false;
        }
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.alt || keystroke.modifiers.control {
            // Non-navigation keys dismiss the panel (Tauri: any other input dismisses).
            return false;
        }
        match keystroke.key.as_str() {
            "escape" => {
                self.dismiss_credential_suggestions(cx);
                true
            }
            "up" => {
                if let Some(state) = self.terminal.assist.credential_suggestions.as_mut() {
                    if state.selected_index == 0 {
                        state.selected_index = state.matches.len().saturating_sub(1);
                    } else {
                        state.selected_index -= 1;
                    }
                    cx.notify();
                }
                true
            }
            "down" => {
                if let Some(state) = self.terminal.assist.credential_suggestions.as_mut() {
                    state.selected_index = (state.selected_index + 1) % state.matches.len().max(1);
                    cx.notify();
                }
                true
            }
            "enter" => {
                self.apply_selected_credential_suggestion(cx);
                true
            }
            _ => {
                // Typing while the panel is open dismisses it (Tauri parity).
                self.dismiss_credential_suggestions(cx);
                false
            }
        }
    }

    pub(in crate::features) fn credential_suggestions_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let Some(state) = self.terminal.assist.credential_suggestions.as_ref() else {
            return div().into_any_element();
        };
        if state.matches.is_empty() {
            return div().into_any_element();
        }

        let menu_w = 340.0_f32;
        let menu_h = suggestion_overlay_desired_height(state.matches.len(), 36.0);
        let Some(placement) = self.suggestion_overlay_position_for_session(
            Some(&state.session_id),
            state.cursor_row,
            state.cursor_col,
            menu_w,
            menu_h,
        ) else {
            return div().into_any_element();
        };

        let title = t!(match state.kind {
            CredentialPromptKind::Password => "credentialAutofill.passwordTitle",
            CredentialPromptKind::Username => "credentialAutofill.usernameTitle",
        });
        let kind_icon = match state.kind {
            CredentialPromptKind::Password => "icons/auth.svg",
            CredentialPromptKind::Username => "icons/sessions.svg",
        };
        let footer = format!(
            "↑↓ {} · Enter {} · Esc {}",
            t!("credentialAutofill.select"),
            t!("credentialAutofill.fill"),
            t!("credentialAutofill.dismiss")
        );

        let mut list = div()
            .id(SharedString::from("credential-suggestions-list"))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar();

        for (index, credential) in state.matches.iter().enumerate() {
            let selected = index == state.selected_index;
            let credential_id = credential.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("credential-suggestion-{index}")))
                    .h(px(36.))
                    .flex_none()
                    .px_3()
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
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.terminal.assist.credential_suggestions.as_mut() {
                            state.selected_index = index;
                        }
                        if let Some(credential) = this
                            .terminal
                            .assist
                            .credential_suggestions
                            .as_ref()
                            .and_then(|state| {
                                state
                                    .matches
                                    .iter()
                                    .find(|entry| entry.id == credential_id)
                                    .cloned()
                            })
                        {
                            this.select_credential_suggestion(credential, cx);
                        }
                    }))
                    .child(svg().size(px(14.)).flex_none().path(kind_icon).text_color(
                        if selected {
                            rgb(palette.accent)
                        } else {
                            rgb(palette.text_dimmed)
                        },
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.text))
                                    .child(truncate_preview(&credential.name, 36)),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child(truncate_preview(&credential.username, 40)),
                            ),
                    ),
            );
        }

        div()
            .id(SharedString::from("credential-suggestions-overlay"))
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
            .overflow_hidden()
            .child(
                div()
                    .h(px(SUGGESTION_OVERLAY_HEADER_HEIGHT))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .items_center()
                    .gap_1()
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
                                    .path(kind_icon)
                                    .text_color(rgb(palette.text_dimmed)),
                            )
                            .child(title),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(format!("{}", state.matches.len())),
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
}

fn credential_autofill_snapshot_detection_can_run(
    active_session_id: Option<&str>,
    has_credentials_or_pending: bool,
    backlog: CredentialAutofillRuntimeBacklog,
    match_request_pending: bool,
) -> bool {
    active_session_id.is_some()
        && has_credentials_or_pending
        && backlog.is_empty()
        && !match_request_pending
}

fn credential_autofill_pending_detection_can_run(
    active_session_id: Option<&str>,
    detection_pending: bool,
    backlog: CredentialAutofillRuntimeBacklog,
    match_request_pending: bool,
) -> bool {
    active_session_id.is_some() && detection_pending && backlog.is_empty() && !match_request_pending
}

#[derive(Clone, Copy, Default)]
struct CredentialAutofillRuntimeBacklog {
    queued_output_bytes: usize,
    pending_session_events: usize,
    pending_terminal_frame_events: usize,
    queued_terminal_frame_events: usize,
    queued_terminal_frame_output_bytes: usize,
}

impl CredentialAutofillRuntimeBacklog {
    fn is_empty(self) -> bool {
        self.queued_output_bytes == 0
            && self.pending_session_events == 0
            && self.pending_terminal_frame_events == 0
            && self.queued_terminal_frame_events == 0
            && self.queued_terminal_frame_output_bytes == 0
    }
}

fn credential_autofill_detection_should_run_this_tick(
    detection_was_pending: bool,
    can_run: bool,
) -> bool {
    detection_was_pending && can_run
}

fn credential_autofill_prompt_text_from_snapshot(snapshot: &TerminalSnapshot) -> Option<String> {
    let line = credential_autofill_prompt_line_from_snapshot(snapshot)?;
    Some(credential_autofill_prompt_text_from_visible(line))
}

fn credential_autofill_prompt_line_from_snapshot(snapshot: &TerminalSnapshot) -> Option<&str> {
    if snapshot.cursor.row != usize::MAX {
        return snapshot.line(snapshot.cursor.row);
    }
    snapshot
        .rows()
        .iter()
        .rev()
        .find(|row| !row.text.trim().is_empty())
        .map(|row| row.text.as_str())
}

#[cfg(test)]
fn credential_autofill_prompt_line_from_viewport(
    lines: &[String],
    cursor_row: usize,
) -> Option<&str> {
    if lines.is_empty() {
        return None;
    }
    if cursor_row != usize::MAX {
        return lines.get(cursor_row).map(String::as_str);
    }
    lines
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(String::as_str)
}

fn credential_autofill_visible_tail(text: &str) -> &str {
    if text.len() <= CREDENTIAL_AUTOFILL_INPUT_TAIL_LIMIT {
        return text;
    }
    let mut start = text.len() - CREDENTIAL_AUTOFILL_INPUT_TAIL_LIMIT;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

fn credential_autofill_prompt_text_from_visible(output: &str) -> String {
    if output
        .chars()
        .last()
        .is_some_and(|ch| ch == '\r' || ch == '\n')
    {
        return String::new();
    }

    let tail = credential_autofill_visible_tail(output);
    let prompt_start = tail.rfind(['\r', '\n']).map(|index| index + 1).unwrap_or(0);
    let prompt = tail[prompt_start..].trim();
    let prompt_len = prompt.chars().count();
    if prompt_len > 500 {
        prompt.chars().skip(prompt_len - 500).collect::<String>()
    } else {
        prompt.to_string()
    }
}

fn credential_autofill_detect_prompt_kind(prompt: &str) -> Option<CredentialPromptKind> {
    let trimmed = prompt.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .last()
            .is_some_and(|ch| ch == ':' || ch == '：')
    {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("passphrase")
        || lower.contains("passcode")
        || lower.contains("pin")
        || lower.contains("otp")
        || lower.contains("verification code")
        || lower.contains("authentication code")
        || lower.contains("auth code")
        || lower.contains("2fa")
        || lower.contains("mfa")
        || trimmed.contains("密码")
        || trimmed.contains("口令")
        || trimmed.contains("验证码")
        || trimmed.contains("动态码")
        || trimmed.contains("动态口令")
    {
        return Some(CredentialPromptKind::Password);
    }
    if lower.contains("username")
        || lower.contains("user name")
        || lower.contains("login as")
        || lower.contains("login")
        || lower.contains("account")
        || lower.contains("user")
        || trimmed.contains("用户名")
        || trimmed.contains("用户")
        || trimmed.contains("账号")
        || trimmed.contains("账户")
        || trimmed.contains("登录名")
    {
        return Some(CredentialPromptKind::Username);
    }
    None
}

#[cfg(test)]
mod tests {
    use nyaterm_core::CredentialPromptKind;

    use super::{
        CREDENTIAL_AUTOFILL_INPUT_TAIL_LIMIT, CredentialAutofillRuntimeBacklog,
        credential_autofill_detect_prompt_kind, credential_autofill_detection_should_run_this_tick,
        credential_autofill_pending_detection_can_run,
        credential_autofill_prompt_line_from_viewport,
        credential_autofill_prompt_text_from_visible,
        credential_autofill_snapshot_detection_can_run, credential_autofill_visible_tail,
    };

    fn backlog(
        queued_output_bytes: usize,
        pending_session_events: usize,
        pending_terminal_frame_events: usize,
        queued_terminal_frame_events: usize,
        queued_terminal_frame_output_bytes: usize,
    ) -> CredentialAutofillRuntimeBacklog {
        CredentialAutofillRuntimeBacklog {
            queued_output_bytes,
            pending_session_events,
            pending_terminal_frame_events,
            queued_terminal_frame_events,
            queued_terminal_frame_output_bytes,
        }
    }

    #[test]
    fn credential_autofill_snapshot_detection_requires_active_session_and_credentials() {
        assert!(credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            None,
            true,
            backlog(0, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            false,
            backlog(0, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 0, 0),
            true
        ));
    }

    #[test]
    fn credential_autofill_snapshot_detection_waits_for_all_backlogs() {
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(1, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(0, 1, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 1, 0, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 1, 0),
            false
        ));
        assert!(!credential_autofill_snapshot_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 0, 1),
            false
        ));
    }

    #[test]
    fn credential_autofill_pending_detection_runs_only_when_idle() {
        assert!(credential_autofill_pending_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_pending_detection_can_run(
            None,
            true,
            backlog(0, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_pending_detection_can_run(
            Some("active"),
            false,
            backlog(0, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_pending_detection_can_run(
            Some("active"),
            true,
            backlog(1, 0, 0, 0, 0),
            false
        ));
        assert!(!credential_autofill_pending_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 1, 0),
            false
        ));
        assert!(!credential_autofill_pending_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 0, 1),
            false
        ));
        assert!(!credential_autofill_pending_detection_can_run(
            Some("active"),
            true,
            backlog(0, 0, 0, 0, 0),
            true
        ));
    }

    #[test]
    fn credential_autofill_detection_waits_for_next_tick_after_snapshot_sync() {
        assert!(!credential_autofill_detection_should_run_this_tick(
            false, true
        ));
        assert!(credential_autofill_detection_should_run_this_tick(
            true, true
        ));
        assert!(!credential_autofill_detection_should_run_this_tick(
            true, false
        ));
    }

    #[test]
    fn credential_autofill_prompt_line_uses_cursor_row() {
        let lines = vec![
            "Last login".to_string(),
            "Password:".to_string(),
            "ignored:".to_string(),
        ];

        assert_eq!(
            credential_autofill_prompt_line_from_viewport(&lines, 1),
            Some("Password:")
        );
    }

    #[test]
    fn credential_autofill_prompt_line_falls_back_to_last_nonempty_line() {
        let lines = vec![
            "Last login".to_string(),
            "Password:".to_string(),
            "".to_string(),
        ];

        assert_eq!(
            credential_autofill_prompt_line_from_viewport(&lines, usize::MAX),
            Some("Password:")
        );
    }

    #[test]
    fn credential_autofill_visible_tail_caps_input_on_boundary() {
        let text = format!("{}密码：", "测".repeat(3000));
        let tail = credential_autofill_visible_tail(&text);

        assert!(tail.len() <= CREDENTIAL_AUTOFILL_INPUT_TAIL_LIMIT);
        assert!(tail.is_char_boundary(0));
        assert!(tail.ends_with("密码："));
    }

    #[test]
    fn credential_autofill_prompt_text_reads_visible_last_line() {
        assert_eq!(
            credential_autofill_prompt_text_from_visible("hello\nPassword: "),
            "Password:"
        );
        assert_eq!(
            credential_autofill_prompt_text_from_visible("Password:\n"),
            ""
        );

        let long = format!("{}Password: ", "x".repeat(700));
        let prompt = credential_autofill_prompt_text_from_visible(&long);
        assert_eq!(prompt.chars().count(), 500);
        assert!(prompt.ends_with("Password:"));
    }

    #[test]
    fn credential_autofill_detect_prompt_kind_without_regex() {
        assert_eq!(
            credential_autofill_detect_prompt_kind("Password:"),
            Some(CredentialPromptKind::Password)
        );
        assert_eq!(
            credential_autofill_detect_prompt_kind("login as:"),
            Some(CredentialPromptKind::Username)
        );
        assert_eq!(
            credential_autofill_detect_prompt_kind("密码："),
            Some(CredentialPromptKind::Password)
        );
        assert_eq!(
            credential_autofill_detect_prompt_kind("Password accepted"),
            None
        );
    }
}
