//! Grouped AI feature state.
//!
//! The AI panel spans several independent concerns: provider settings, the
//! chat composer and transcript, session history, model discovery, and the
//! agent loop. They were seventy `ai_*` fields on `NyaTermApp`, which made it
//! impossible to see which ones move together.

mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use std::time::{Duration, Instant};

use gpui::FocusHandle;
use nyaterm_core::{
    AgentCaptureProcessResult, AgentCommandExecutionMode, AgentOutputCaptureProcessor,
    AiCommandCard, AiMessage, AiMessageRole, AiMode, AiSession, AiSettings, truncate_preview, uuid,
};

use crate::features::{
    runtime_jobs::AiAgentLoopState, runtime_jobs::AiAgentStepStatus, runtime_jobs::AiAgentStepView,
    runtime_jobs::AiChatJobOutput, runtime_jobs::AiChatWorkerEvent,
    runtime_jobs::AiDiscoveryJobResult,
};
use crate::models::{
    AiActionEditorField, AiActionListKind, AiDetectedErrorState, AiInputField, AiMessageMenuState,
    AiPreparedRequest,
};

pub(in crate::features) struct AiFeatureState {
    settings: AiSettingsState,
    chat: AiChatState,
    history: AiHistoryState,
    discovery: AiDiscoveryState,
    agent: AiAgentState,
    panel: AiPanelState,
}

/// Focus handles the AI feature needs at construction time.
pub(in crate::features) struct AiFeatureFocus {
    pub chat: FocusHandle,
    pub action: FocusHandle,
    pub manual_model: FocusHandle,
    pub credential: FocusHandle,
}

pub(in crate::features) struct AiFeatureInit {
    pub settings: AiSettings,
    pub model_draft: String,
    pub base_url_draft: String,
    pub chat_session_id: String,
    pub session_count: usize,
    pub message_count: usize,
    pub audit_count: usize,
}

/// Provider settings, model catalog editing and credential drafts.
struct AiSettingsState {
    config: AiSettings,
    model_draft: String,
    base_url_draft: String,
    secret_draft: nyaterm_core::SecretString,
    model_collapsed_groups: HashSet<String>,
    model_query: String,
    manual_model_drafts: HashMap<String, String>,
    manual_model_focus: FocusHandle,
    manual_model_edit_group: Option<String>,
    /// Per-credential API-key drafts; empty means keep the stored secret.
    credential_secret_drafts: HashMap<String, String>,
    credential_focus: FocusHandle,
    action_edit: Option<(AiActionListKind, String, AiActionEditorField)>,
    action_focus: FocusHandle,
    persistence_generation: u64,
    persistence_in_flight: Option<u64>,
    persistence_pending: Option<AiSettings>,
    persistence_dirty: bool,
    pending_full_access: Option<AiFullAccessSetting>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::features) enum AiFullAccessSetting {
    ExternalAgent,
    Codex,
    ClaudeCode,
    McpHost,
}

pub(in crate::features) struct AiSettingsPersistenceCompletion {
    pub(in crate::features) apply_result: bool,
    pub(in crate::features) report_result: bool,
    pub(in crate::features) next: Option<(u64, AiSettings)>,
}

/// Composer, in-flight request and the visible transcript.
struct AiChatState {
    tx: UnboundedSender<AiChatWorkerEvent>,
    /// Taken once by `NyaTermApp::start_ai_chat_event_drain`, which owns
    /// delivery from then on. `None` afterwards, so a second start is a no-op.
    rx: Option<UnboundedReceiver<AiChatWorkerEvent>>,
    pending: bool,
    job_id: u64,
    cancel: Option<Arc<AtomicBool>>,
    session_id: String,
    prompt_draft: String,
    target_session_ids: Vec<String>,
    mention_open: bool,
    mention_query: String,
    mention_index: usize,
    prepared_request: Option<AiPreparedRequest>,
    response_preview: String,
    messages: Vec<Arc<AiMessage>>,
    streaming_assistant_id: Option<String>,
    message_menu: Option<AiMessageMenuState>,
    quoted_text: Option<String>,
    command_cards: Vec<AiCommandCard>,
    focus: FocusHandle,
    focus_pending: bool,
}

/// Stored sessions, the history browser and the counters shown beside it.
struct AiHistoryState {
    open: bool,
    query: String,
    job_id: u64,
    pending: bool,
    sessions: Vec<AiSession>,
    session_count: usize,
    message_count: usize,
    audit_count: usize,
    usage_count_job_id: u64,
    audit_write_lock: Arc<Mutex<()>>,
}

/// Model discovery job and the model picker it feeds.
struct AiDiscoveryState {
    tx: UnboundedSender<AiDiscoveryJobResult>,
    /// Taken once by `NyaTermApp::start_ai_discovery_event_drain`, which owns
    /// delivery from then on. `None` afterwards, so a second start is a no-op.
    rx: Option<UnboundedReceiver<AiDiscoveryJobResult>>,
    pending: bool,
    menu_open: bool,
    query: String,
    index: usize,
}

/// Agent loop: the running task, its steps and their disclosure state.
struct AiAgentState {
    task_prompt: Option<String>,
    step_index: u16,
    loop_state: Option<AiAgentLoopState>,
    /// True while the agent-loop observation clock task is alive.
    loop_clock_armed: bool,
    capture: AgentOutputCaptureProcessor,
    steps: Vec<AiAgentStepView>,
    thought_expanded: HashSet<u16>,
    output_expanded: HashSet<u16>,
}

pub(in crate::features) struct AiChatLaunch {
    pub(in crate::features) job_id: u64,
    pub(in crate::features) cancel: Arc<AtomicBool>,
    pub(in crate::features) tx: UnboundedSender<AiChatWorkerEvent>,
    pub(in crate::features) session_id: String,
}

pub(in crate::features) struct AiChatFinishEffect {
    pub(in crate::features) session_id: String,
    pub(in crate::features) succeeded: bool,
    pub(in crate::features) clear_prompt_input: bool,
    pub(in crate::features) refresh_usage_counts: bool,
    pub(in crate::features) auto_execute_first: bool,
}

pub(in crate::features) enum AiAgentBackgroundEffect {
    Ignored,
    MatchedStale,
    Continue(Box<AiAgentLoopState>, nyaterm_core::CommandObservation),
    Failed,
}

pub(in crate::features) enum AiAgentObservationPoll {
    Waiting,
    Target(AiAgentLoopState),
    TimedOut(AiAgentLoopState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum AiSettingsMutation {
    Ignored,
    Notify,
    Persist,
}

/// Panel chrome: status line, focus routing and the detected-error banner.
struct AiPanelState {
    execution_menu_open: bool,
    status: String,
    focused_field: AiInputField,
    detected_error: Option<AiDetectedErrorState>,
    error_notice_at: HashMap<String, Instant>,
    panel_refresh_requested: bool,
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

impl AiFeatureState {
    pub(in crate::features) fn new(init: AiFeatureInit, focus: AiFeatureFocus) -> Self {
        let AiFeatureInit {
            settings,
            model_draft,
            base_url_draft,
            chat_session_id,
            session_count,
            message_count,
            audit_count,
        } = init;
        let (chat_tx, chat_rx) = unbounded();
        let (discovery_tx, discovery_rx) = unbounded();
        Self {
            settings: AiSettingsState {
                config: settings,
                model_draft,
                base_url_draft,
                secret_draft: nyaterm_core::SecretString::default(),
                model_collapsed_groups: HashSet::new(),
                model_query: String::new(),
                manual_model_drafts: HashMap::new(),
                manual_model_focus: focus.manual_model,
                manual_model_edit_group: None,
                credential_secret_drafts: HashMap::new(),
                credential_focus: focus.credential,
                action_edit: None,
                action_focus: focus.action,
                persistence_generation: 0,
                persistence_in_flight: None,
                persistence_pending: None,
                persistence_dirty: false,
                pending_full_access: None,
            },
            chat: AiChatState {
                tx: chat_tx,
                rx: Some(chat_rx),
                pending: false,
                job_id: 0,
                cancel: None,
                session_id: chat_session_id,
                prompt_draft: String::new(),
                target_session_ids: Vec::new(),
                mention_open: false,
                mention_query: String::new(),
                mention_index: 0,
                prepared_request: None,
                response_preview: "Ask mode ready".to_string(),
                messages: Vec::new(),
                streaming_assistant_id: None,
                message_menu: None,
                quoted_text: None,
                command_cards: Vec::new(),
                focus: focus.chat,
                focus_pending: false,
            },
            history: AiHistoryState {
                open: false,
                query: String::new(),
                job_id: 0,
                pending: false,
                sessions: Vec::new(),
                session_count,
                message_count,
                audit_count,
                usage_count_job_id: 0,
                audit_write_lock: Arc::new(Mutex::new(())),
            },
            discovery: AiDiscoveryState {
                tx: discovery_tx,
                rx: Some(discovery_rx),
                pending: false,
                menu_open: false,
                query: String::new(),
                index: 0,
            },
            agent: AiAgentState {
                task_prompt: None,
                step_index: 0,
                loop_state: None,
                loop_clock_armed: false,
                capture: AgentOutputCaptureProcessor::new(),
                steps: Vec::new(),
                thought_expanded: HashSet::new(),
                output_expanded: HashSet::new(),
            },
            panel: AiPanelState {
                execution_menu_open: false,
                status: "AI settings ready".to_string(),
                focused_field: AiInputField::Model,
                detected_error: None,
                error_notice_at: HashMap::new(),
                panel_refresh_requested: false,
            },
        }
    }

    pub(in crate::features) fn chat_or_agent_is_running(&self) -> bool {
        self.chat.pending || self.agent.loop_state.is_some()
    }

    pub(in crate::features) fn chat_is_pending(&self) -> bool {
        self.chat.pending
    }

    pub(in crate::features) fn chat_focus(&self) -> &FocusHandle {
        &self.chat.focus
    }

    pub(in crate::features) fn chat_focus_is_pending(&self) -> bool {
        self.chat.focus_pending
    }

    pub(in crate::features) fn take_chat_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.chat.focus_pending)
    }

    pub(in crate::features) fn chat_session_id(&self) -> &str {
        &self.chat.session_id
    }

    pub(in crate::features) fn chat_prompt_draft(&self) -> &str {
        &self.chat.prompt_draft
    }

    pub(in crate::features) fn chat_request_prompt(&self) -> Option<String> {
        let prompt = self.chat.prompt_draft.trim();
        if prompt.is_empty() {
            return None;
        }
        Some(
            self.chat
                .quoted_text
                .as_deref()
                .map(str::trim)
                .filter(|quoted| !quoted.is_empty())
                .map(|quoted| format!("> {quoted}\n\n{prompt}"))
                .unwrap_or_else(|| prompt.to_string()),
        )
    }

    pub(in crate::features) fn reject_chat_start(
        &mut self,
        message: impl Into<String>,
        update_panel: bool,
    ) {
        self.chat.response_preview = message.into();
        if update_panel {
            self.panel.status = self.chat.response_preview.clone();
        }
    }

    pub(in crate::features) fn chat_prepared_request_cloned(&self) -> Option<AiPreparedRequest> {
        self.chat.prepared_request.clone()
    }

    pub(in crate::features) fn chat_target_session_ids(&self) -> &[String] {
        &self.chat.target_session_ids
    }

    pub(in crate::features) fn chat_mention_query(&self) -> &str {
        &self.chat.mention_query
    }

    pub(in crate::features) fn chat_targets_session(&self, session_id: &str) -> bool {
        self.chat
            .target_session_ids
            .iter()
            .any(|target_id| target_id == session_id)
    }

    pub(in crate::features) fn chat_mention_is_open(&self) -> bool {
        self.chat.mention_open
    }

    pub(in crate::features) fn chat_mention_index(&self) -> usize {
        self.chat.mention_index
    }

    pub(in crate::features) fn clamp_chat_mention_index(&mut self, len: usize) -> usize {
        if len == 0 {
            self.chat.mention_index = 0;
        } else {
            self.chat.mention_index = self.chat.mention_index.min(len - 1);
        }
        self.chat.mention_index
    }

    pub(in crate::features) fn set_chat_mention_index(&mut self, index: usize) {
        self.chat.mention_index = index;
    }

    pub(in crate::features) fn close_chat_mention(&mut self) {
        self.chat.close_mention();
    }

    pub(in crate::features) fn hide_chat_mention(&mut self) {
        self.chat.mention_open = false;
        self.chat.mention_query.clear();
    }

    pub(in crate::features) fn move_chat_mention_index(
        &mut self,
        candidate_count: usize,
        delta: isize,
    ) {
        if candidate_count == 0 {
            return;
        }
        self.chat.mention_index = if delta < 0 {
            (self.chat.mention_index + candidate_count - 1) % candidate_count
        } else {
            (self.chat.mention_index + 1) % candidate_count
        };
    }

    pub(in crate::features) fn set_chat_prompt_draft(&mut self, text: String) {
        self.chat.prompt_draft = text;
        self.chat.sync_mention_from_prompt();
    }

    pub(in crate::features) fn blur_chat_prompt(&mut self) {
        self.hide_chat_mention();
        self.chat.response_preview = "AI prompt blurred".to_string();
    }

    pub(in crate::features) fn remove_chat_target_session(&mut self, session_id: &str) {
        self.chat
            .target_session_ids
            .retain(|target_id| target_id != session_id);
        self.panel.status = if self.chat.target_session_ids.is_empty() {
            "AI target sessions cleared".to_string()
        } else {
            "AI target session removed".to_string()
        };
    }

    pub(in crate::features) fn select_chat_mention(
        &mut self,
        session_id: String,
        display_name: String,
    ) {
        if self
            .chat
            .target_session_ids
            .iter()
            .any(|target_id| target_id == &session_id)
        {
            self.chat
                .target_session_ids
                .retain(|target_id| target_id != &session_id);
        } else {
            self.chat.target_session_ids.push(session_id);
        }
        if let Some(at_index) = self.chat.prompt_draft.rfind('@') {
            let suffix = &self.chat.prompt_draft[at_index + 1..];
            if !suffix.chars().any(char::is_whitespace) {
                self.chat.prompt_draft.truncate(at_index);
            }
        }
        self.chat.close_mention();
        self.panel.status = format!("AI target session selected: {display_name}");
    }

    pub(in crate::features) fn begin_chat_job(&mut self) -> AiChatLaunch {
        self.chat.job_id = self.chat.job_id.wrapping_add(1).max(1);
        let cancel = Arc::new(AtomicBool::new(false));
        self.chat.cancel = Some(cancel.clone());
        AiChatLaunch {
            job_id: self.chat.job_id,
            cancel,
            tx: self.chat.tx.clone(),
            session_id: self.chat.session_id.clone(),
        }
    }

    pub(in crate::features) fn begin_chat_request(
        &mut self,
        request_prompt: String,
        mode: AiMode,
        source_label: Option<&str>,
    ) -> AiChatLaunch {
        let launch = self.begin_chat_job();
        if mode == AiMode::Agent {
            self.agent.task_prompt = Some(request_prompt.clone());
            self.agent.step_index = 0;
            self.agent.steps.clear();
            self.agent.thought_expanded.clear();
            self.agent.output_expanded.clear();
            self.upsert_agent_step(
                0,
                AiAgentStepStatus::Planning,
                "Planning",
                truncate_preview(&request_prompt, 120),
            );
        } else {
            self.agent.task_prompt = None;
            self.agent.step_index = 0;
            self.agent.loop_state = None;
            self.agent.steps.clear();
            self.agent.thought_expanded.clear();
            self.agent.output_expanded.clear();
        }
        self.chat.pending = true;
        self.chat.response_preview = if mode == AiMode::Agent {
            "Running AI Agent step...".to_string()
        } else {
            "Running AI request...".to_string()
        };
        self.chat.command_cards.clear();
        let now = nyaterm_core::now_rfc3339();
        let assistant_id = format!("assistant-{}", uuid());
        self.chat.messages.push(Arc::new(AiMessage {
            id: format!("user-{}", uuid()),
            session_id: self.chat.session_id.clone(),
            role: AiMessageRole::User,
            content: request_prompt,
            created_at: now.clone(),
            reasoning_content: None,
            command_cards: Vec::new(),
        }));
        self.chat.messages.push(Arc::new(AiMessage {
            id: assistant_id.clone(),
            session_id: self.chat.session_id.clone(),
            role: AiMessageRole::Assistant,
            content: String::new(),
            created_at: now,
            reasoning_content: None,
            command_cards: Vec::new(),
        }));
        self.chat.prompt_draft.clear();
        self.chat.quoted_text = None;
        self.chat.message_menu = None;
        self.chat.close_mention();
        self.chat.streaming_assistant_id = Some(assistant_id);
        self.panel.status = if mode == AiMode::Agent {
            "AI Agent step started".to_string()
        } else if let Some(source_label) = source_label {
            format!("AI file action started: {source_label}")
        } else {
            "AI Ask request started".to_string()
        };
        self.chat.prepared_request = None;
        launch
    }

    pub(in crate::features) fn cancel_chat_and_agent(&mut self) {
        if let Some(cancel) = self.chat.cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.chat.job_id = self.chat.job_id.wrapping_add(1).max(1);
        self.chat.pending = false;
        self.chat.cancel = None;
        let cancelled_step = self
            .agent
            .loop_state
            .as_ref()
            .map(|state| state.step_index)
            .or_else(|| self.agent.steps.last().map(|step| step.step_index));
        if let Some(state) = self.agent.loop_state.take()
            && let Some(marker_id) = state.marker_id.as_deref()
        {
            self.agent.capture.cancel(marker_id);
        }
        self.agent.capture = AgentOutputCaptureProcessor::new();
        self.agent.task_prompt = None;
        self.chat.command_cards.clear();
        self.chat.response_preview = "AI request cancelled".to_string();
        if let Some(assistant_id) = self.chat.streaming_assistant_id.take()
            && let Some(message) = self
                .chat
                .messages
                .iter_mut()
                .rev()
                .find(|message| message.id == assistant_id)
            && message.content.trim().is_empty()
        {
            let message = Arc::make_mut(message);
            message.content = "AI request cancelled".to_string();
        }
        self.panel.status = "AI request cancelled".to_string();
        if let Some(step_index) = cancelled_step {
            self.upsert_agent_step(
                step_index,
                AiAgentStepStatus::Cancelled,
                "Cancelled",
                "AI Agent request was cancelled",
            );
        }
    }

    pub(in crate::features) fn take_chat_event_receiver(
        &mut self,
    ) -> Option<UnboundedReceiver<AiChatWorkerEvent>> {
        self.chat.rx.take()
    }

    /// Whether a chat request is outstanding, so a worker event is still wanted.
    ///
    /// A cancelled or already-settled request leaves nothing to apply; the
    /// per-event `job_id` checks would reject it anyway, but dropping it here
    /// keeps that intent explicit.
    pub(in crate::features) fn chat_event_is_wanted(&self) -> bool {
        self.chat.pending
    }

    pub(in crate::features) fn apply_chat_delta(
        &mut self,
        job_id: u64,
        text_delta: &str,
        reasoning_delta: Option<&str>,
    ) -> bool {
        if job_id != self.chat.job_id {
            return false;
        }
        if self.chat.response_preview == "Running AI request..." {
            self.chat.response_preview.clear();
        }
        self.chat.response_preview.push_str(text_delta);
        self.chat.response_preview = truncate_preview(&self.chat.response_preview, 320);
        if let Some(assistant_id) = self.chat.streaming_assistant_id.as_deref()
            && let Some(message) = self
                .chat
                .messages
                .iter_mut()
                .rev()
                .find(|message| message.id == assistant_id)
        {
            let message = Arc::make_mut(message);
            message.content.push_str(text_delta);
            if let Some(delta) = reasoning_delta.filter(|delta| !delta.trim().is_empty()) {
                let existing = message.reasoning_content.take().unwrap_or_default();
                message.reasoning_content = Some(format!("{existing}{delta}"));
            }
        }
        self.panel.status = if reasoning_delta.is_some_and(|delta| !delta.trim().is_empty()) {
            "AI stream receiving; reasoning captured".to_string()
        } else {
            "AI stream receiving".to_string()
        };
        true
    }

    pub(in crate::features) fn apply_agent_tool_delta(
        &mut self,
        job_id: u64,
        tool_name: Option<&str>,
        arguments_delta_len: usize,
    ) -> bool {
        if job_id != self.chat.job_id {
            return false;
        }
        let tool_label = tool_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("tool");
        self.panel.status = if arguments_delta_len == 0 {
            format!("AI Agent selected {tool_label}")
        } else {
            format!("AI Agent streaming {tool_label} arguments (+{arguments_delta_len} chars)")
        };
        self.upsert_agent_step(
            self.last_agent_step_index(),
            AiAgentStepStatus::Tool,
            format!("Tool {tool_label}"),
            if arguments_delta_len == 0 {
                "Provider selected an Agent tool".to_string()
            } else {
                format!("Streaming arguments (+{arguments_delta_len} chars)")
            },
        );
        true
    }

    pub(in crate::features) fn finish_agent_background(
        &mut self,
        job_id: u64,
        state: AiAgentLoopState,
        result: Result<nyaterm_core::CommandObservation, String>,
        observation_summary: impl FnOnce(&nyaterm_core::CommandObservation) -> String,
    ) -> AiAgentBackgroundEffect {
        if job_id != self.chat.job_id {
            return AiAgentBackgroundEffect::Ignored;
        }
        self.chat.cancel = None;
        let Some(active_state) = self.agent.loop_state.take() else {
            return AiAgentBackgroundEffect::MatchedStale;
        };
        if active_state.background_job_id != Some(job_id) {
            self.agent.loop_state = Some(active_state);
            return AiAgentBackgroundEffect::MatchedStale;
        }
        match result {
            Ok(observation) => {
                self.panel.status = match observation.exit_code {
                    Some(code) => format!("AI Agent background command exited with {code}"),
                    None => "AI Agent background command completed".to_string(),
                };
                let detail = observation_summary(&observation);
                self.upsert_agent_step(
                    state.step_index,
                    AiAgentStepStatus::Completed,
                    "Observed",
                    detail,
                );
                AiAgentBackgroundEffect::Continue(Box::new(state), observation)
            }
            Err(error) => {
                self.panel.status = format!("AI Agent background command failed: {error}");
                self.chat.response_preview = self.panel.status.clone();
                self.upsert_agent_step(
                    state.step_index,
                    AiAgentStepStatus::Failed,
                    "Failed",
                    truncate_preview(&error, 140),
                );
                AiAgentBackgroundEffect::Failed
            }
        }
    }

    pub(in crate::features) fn finish_chat_job(
        &mut self,
        job_id: u64,
        session_id: String,
        result: Result<AiChatJobOutput, String>,
    ) -> Option<AiChatFinishEffect> {
        if job_id != self.chat.job_id {
            return None;
        }
        self.chat.pending = false;
        self.chat.cancel = None;
        match result {
            Ok(output) => {
                let command_count = output.command_cards.len();
                self.chat.response_preview = if output.text.trim().is_empty() {
                    "AI returned an empty response".to_string()
                } else {
                    truncate_preview(&output.text, 320)
                };
                let mode_label = if output.mode == AiMode::Agent {
                    "AI Agent"
                } else {
                    "AI Ask"
                };
                let mut status =
                    format!("{mode_label} completed; {command_count} command card(s) parsed");
                if output.reasoning.is_some() {
                    status.push_str("; reasoning captured");
                }
                if let Some(note) = output.approval_note.as_deref() {
                    status.push_str("; ");
                    status.push_str(note);
                }
                if output.mode == AiMode::Agent && command_count > 0 && !output.auto_execute_first {
                    status.push_str("; awaiting command approval");
                }
                self.panel.status = status;
                if output.mode == AiMode::Agent {
                    let (step_status, step_title) = if command_count == 0 {
                        (AiAgentStepStatus::Completed, "Final Answer")
                    } else if output.auto_execute_first {
                        (AiAgentStepStatus::Running, "Auto Execute")
                    } else {
                        (AiAgentStepStatus::NeedsApproval, "Needs Approval")
                    };
                    self.upsert_agent_step(
                        self.last_agent_step_index(),
                        step_status,
                        step_title,
                        truncate_preview(&output.text, 140),
                    );
                }
                self.chat.command_cards = output.command_cards.clone();
                if let Some(assistant_id) = self.chat.streaming_assistant_id.take()
                    && let Some(message) = self
                        .chat
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|message| message.id == assistant_id)
                {
                    let message = Arc::make_mut(message);
                    if !output.text.trim().is_empty() {
                        message.content = output.text.clone();
                    } else if message.content.trim().is_empty() {
                        message.content = "AI returned an empty response".to_string();
                    }
                    message.reasoning_content = output.reasoning;
                    message.command_cards = output.command_cards;
                }
                self.chat.prompt_draft.clear();
                if output.mode == AiMode::Agent && command_count == 0 {
                    self.agent.loop_state = None;
                    self.agent.task_prompt = None;
                }
                Some(AiChatFinishEffect {
                    session_id,
                    succeeded: true,
                    clear_prompt_input: true,
                    refresh_usage_counts: true,
                    auto_execute_first: output.auto_execute_first
                        && !self.chat.command_cards.is_empty(),
                })
            }
            Err(error) => {
                self.chat.response_preview = format!("AI request failed: {error}");
                self.chat.command_cards.clear();
                self.panel.status = self.chat.response_preview.clone();
                if let Some(assistant_id) = self.chat.streaming_assistant_id.take()
                    && let Some(message) = self
                        .chat
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|message| message.id == assistant_id)
                {
                    let message = Arc::make_mut(message);
                    message.content = format!("AI request failed: {error}");
                }
                if self.agent.task_prompt.is_some() {
                    self.upsert_agent_step(
                        self.last_agent_step_index(),
                        AiAgentStepStatus::Failed,
                        "Failed",
                        truncate_preview(&error, 140),
                    );
                }
                Some(AiChatFinishEffect {
                    session_id,
                    succeeded: false,
                    clear_prompt_input: false,
                    refresh_usage_counts: false,
                    auto_execute_first: false,
                })
            }
        }
    }

    pub(in crate::features) fn chat_prepared_request(&self) -> Option<&AiPreparedRequest> {
        self.chat.prepared_request.as_ref()
    }

    pub(in crate::features) fn chat_response_preview(&self) -> &str {
        &self.chat.response_preview
    }

    pub(in crate::features) fn set_chat_response_preview(&mut self, preview: impl Into<String>) {
        self.chat.response_preview = preview.into();
    }

    pub(in crate::features) fn chat_messages(&self) -> &[Arc<AiMessage>] {
        &self.chat.messages
    }

    pub(in crate::features) fn chat_snapshot_messages(&self) -> Arc<[Arc<AiMessage>]> {
        let streaming_id = self.chat_streaming_assistant_id();
        self.chat_messages()
            .iter()
            .map(|message| {
                if streaming_id == Some(message.id.as_str()) {
                    Arc::new((**message).clone())
                } else {
                    Arc::clone(message)
                }
            })
            .collect::<Vec<_>>()
            .into()
    }

    pub(in crate::features) fn chat_streaming_assistant_id(&self) -> Option<&str> {
        self.chat.streaming_assistant_id.as_deref()
    }

    pub(in crate::features) fn chat_command_cards(&self) -> &[AiCommandCard] {
        &self.chat.command_cards
    }

    pub(in crate::features) fn command_card(&self, index: usize) -> Option<AiCommandCard> {
        self.chat.command_cards.get(index).cloned()
    }

    pub(in crate::features) fn find_command_card(&self, card_id: &str) -> Option<AiCommandCard> {
        self.chat
            .command_cards
            .iter()
            .find(|card| card.id == card_id)
            .cloned()
            .or_else(|| {
                self.chat
                    .messages
                    .iter()
                    .flat_map(|message| message.command_cards.iter())
                    .find(|card| card.id == card_id)
                    .cloned()
            })
    }

    pub(in crate::features) fn chat_message_menu(&self) -> Option<&AiMessageMenuState> {
        self.chat.message_menu.as_ref()
    }

    pub(in crate::features) fn chat_quote(&self) -> Option<&str> {
        self.chat.quoted_text.as_deref()
    }

    pub(in crate::features) fn close_message_menu(&mut self) {
        self.chat.close_message_menu();
    }

    pub(in crate::features) fn open_message_menu(&mut self, menu: AiMessageMenuState) {
        self.chat.message_menu = Some(menu);
        self.history.open = false;
        self.panel.execution_menu_open = false;
        self.discovery.menu_open = false;
    }

    pub(in crate::features) fn quote_message(&mut self, text: String) -> bool {
        let value = text.trim().to_string();
        let quoted = !value.is_empty();
        if quoted {
            self.chat.quoted_text = Some(value);
            self.panel.status = "AI message quoted".to_string();
        } else {
            self.panel.status = "AI message is empty".to_string();
        }
        self.chat.message_menu = None;
        quoted
    }

    pub(in crate::features) fn finish_copy_message(&mut self, copied: bool) {
        self.panel.status = if copied {
            "AI message copied".to_string()
        } else {
            "AI message is empty".to_string()
        };
        self.chat.message_menu = None;
    }

    pub(in crate::features) fn prepare_external_request(
        &mut self,
        request: AiPreparedRequest,
        response_preview: impl Into<String>,
        status: impl Into<String>,
        focus: bool,
    ) {
        self.chat.prepared_request = Some(request);
        self.chat.response_preview = response_preview.into();
        self.panel.status = status.into();
        self.chat.focus_pending = focus;
        self.close_transient_menus();
    }

    pub(in crate::features) fn prepare_detected_error_request(
        &mut self,
        request: AiPreparedRequest,
        session_id: String,
    ) {
        self.chat.prepared_request = Some(request);
        if !self.chat.target_session_ids.contains(&session_id) {
            self.chat.target_session_ids.push(session_id);
        }
        self.close_transient_menus();
    }

    pub(in crate::features) fn history_is_open(&self) -> bool {
        self.history.open
    }

    pub(in crate::features) fn history_query(&self) -> &str {
        &self.history.query
    }

    pub(in crate::features) fn history_sessions(&self) -> &[AiSession] {
        &self.history.sessions
    }

    pub(in crate::features) fn history_is_pending(&self) -> bool {
        self.history.pending
    }

    pub(in crate::features) fn request_history_clear_confirm(&mut self) -> bool {
        if self.history.sessions.is_empty() {
            return false;
        }
        self.chat.message_menu = None;
        self.discovery.menu_open = false;
        self.panel.execution_menu_open = false;
        true
    }

    pub(in crate::features) fn confirm_history_clear(&mut self) -> bool {
        self.history.open = false;
        true
    }

    pub(in crate::features) fn close_history(&mut self) {
        self.history.open = false;
        self.history.query.clear();
    }

    pub(in crate::features) fn clear_history_query(&mut self) {
        self.history.query.clear();
    }

    pub(in crate::features) fn toggle_history(&mut self) -> bool {
        self.panel.execution_menu_open = false;
        self.history.open = !self.history.open;
        if self.history.open {
            self.chat.message_menu = None;
            self.discovery.menu_open = false;
        } else {
            self.history.query.clear();
        }
        self.history.open
    }

    pub(in crate::features) fn history_actions_are_disabled(&self) -> bool {
        self.history.sessions.is_empty() || self.history.pending || self.chat_or_agent_is_running()
    }

    pub(in crate::features) fn history_audit_write_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.history.audit_write_lock)
    }

    pub(in crate::features) fn begin_history_operation(
        &mut self,
        status: impl Into<String>,
    ) -> Option<u64> {
        if self.history.pending {
            self.panel.status = "AI history operation already in progress".to_string();
            return None;
        }
        self.history.job_id = self.history.job_id.wrapping_add(1).max(1);
        self.history.pending = true;
        self.panel.status = status.into();
        Some(self.history.job_id)
    }

    pub(in crate::features) fn finish_history_session_list(
        &mut self,
        job_id: u64,
        result: Result<Vec<AiSession>, String>,
    ) -> bool {
        if self.history.job_id != job_id {
            return false;
        }
        self.history.pending = false;
        match result {
            Ok(sessions) => {
                self.history.sessions = sessions;
                self.panel.status = "AI history loaded".to_string();
            }
            Err(error) => {
                self.history.sessions.clear();
                self.panel.status = format!("failed to load AI history: {error}");
            }
        }
        true
    }

    pub(in crate::features) fn finish_history_message_load(
        &mut self,
        job_id: u64,
        source_session_id: &str,
        target_session_id: String,
        result: Result<Vec<AiMessage>, String>,
        loaded_status: String,
    ) -> bool {
        if self.history.job_id != job_id {
            return false;
        }
        self.history.pending = false;
        if self.chat.session_id != source_session_id {
            self.panel.status = "AI session load cancelled".to_string();
            return true;
        }
        match result {
            Ok(messages) => {
                self.chat.session_id = target_session_id;
                self.chat.messages = messages.into_iter().map(Arc::new).collect();
                self.chat.streaming_assistant_id = None;
                self.history.open = false;
                self.chat.message_menu = None;
                self.chat.quoted_text = None;
                self.chat.command_cards.clear();
                if let Some(last) = self
                    .chat
                    .messages
                    .iter()
                    .rev()
                    .find(|message| matches!(message.role, AiMessageRole::Assistant))
                {
                    self.chat.response_preview = truncate_preview(&last.content, 320);
                    self.chat.command_cards = last.command_cards.clone();
                } else {
                    self.chat.response_preview.clear();
                }
                self.panel.status = loaded_status;
            }
            Err(error) => {
                self.panel.status = format!("failed to load AI session: {error}");
            }
        }
        true
    }

    pub(in crate::features) fn finish_history_session_delete(
        &mut self,
        job_id: u64,
        session_id: &str,
        result: Result<(), String>,
    ) -> Option<bool> {
        if self.history.job_id != job_id {
            return None;
        }
        self.history.pending = false;
        match result {
            Ok(()) => {
                if self.chat.session_id == session_id {
                    self.chat.messages.clear();
                    self.chat.command_cards.clear();
                    self.chat.streaming_assistant_id = None;
                    self.chat.message_menu = None;
                    self.chat.quoted_text = None;
                    self.chat.session_id = format!("ai-session-{}", uuid());
                    self.chat.response_preview = "Ask mode ready".to_string();
                }
                self.history
                    .sessions
                    .retain(|session| session.id != session_id);
                self.panel.status = "AI session deleted".to_string();
                Some(true)
            }
            Err(error) => {
                self.panel.status = format!("failed to delete AI session: {error}");
                Some(false)
            }
        }
    }

    pub(in crate::features) fn finish_history_clear(
        &mut self,
        job_id: u64,
        source_session_id: &str,
        result: Result<(), String>,
    ) -> Option<bool> {
        if self.history.job_id != job_id {
            return None;
        }
        self.history.pending = false;
        match result {
            Ok(()) => {
                self.history.sessions.clear();
                self.history.query.clear();
                if self.chat.session_id == source_session_id {
                    self.chat.messages.clear();
                    self.chat.command_cards.clear();
                    self.chat.streaming_assistant_id = None;
                    self.chat.message_menu = None;
                    self.chat.quoted_text = None;
                    self.clear_detected_error();
                    self.chat.session_id = format!("ai-session-{}", uuid());
                    self.chat.response_preview =
                        if self.settings.config.default_mode == AiMode::Agent {
                            "Agent mode ready".to_string()
                        } else {
                            "Ask mode ready".to_string()
                        };
                }
                self.panel.status = "AI history cleared".to_string();
                Some(true)
            }
            Err(error) => {
                self.panel.status = format!("failed to clear AI history: {error}");
                Some(false)
            }
        }
    }

    pub(in crate::features) fn set_history_query(&mut self, query: String) {
        self.history.query = query;
    }

    pub(in crate::features) fn begin_history_usage_count_job(&mut self) -> u64 {
        self.history.usage_count_job_id = self.history.usage_count_job_id.wrapping_add(1).max(1);
        self.history.usage_count_job_id
    }

    pub(in crate::features) fn finish_history_usage_counts(
        &mut self,
        job_id: u64,
        result: Result<(usize, usize, usize), String>,
    ) -> bool {
        if self.history.usage_count_job_id != job_id {
            return false;
        }
        let Ok((sessions, messages, audits)) = result else {
            return false;
        };
        self.history.session_count = sessions;
        self.history.message_count = messages;
        self.history.audit_count = audits;
        true
    }

    pub(in crate::features) fn discovery_is_pending(&self) -> bool {
        self.discovery.pending
    }

    pub(in crate::features) fn discovery_menu_is_open(&self) -> bool {
        self.discovery.menu_open
    }

    pub(in crate::features) fn discovery_query(&self) -> &str {
        &self.discovery.query
    }

    pub(in crate::features) fn discovery_index(&self) -> usize {
        self.discovery.index
    }

    pub(in crate::features) fn clamp_discovery_index(&mut self, len: usize) -> usize {
        if len == 0 {
            self.discovery.index = 0;
        } else {
            self.discovery.index = self.discovery.index.min(len - 1);
        }
        self.discovery.index
    }

    pub(in crate::features) fn set_discovery_index(&mut self, index: usize) {
        self.discovery.index = index;
    }

    pub(in crate::features) fn toggle_discovery_menu(&mut self, selected_index: usize) -> bool {
        self.discovery.menu_open = !self.discovery.menu_open;
        if self.discovery.menu_open {
            self.discovery.index = selected_index;
            self.history.open = false;
            self.panel.execution_menu_open = false;
            self.chat.message_menu = None;
        } else {
            self.discovery.query.clear();
            self.discovery.index = 0;
        }
        self.discovery.menu_open
    }

    pub(in crate::features) fn close_discovery_menu(&mut self) {
        self.discovery.menu_open = false;
        self.discovery.query.clear();
        self.discovery.index = 0;
    }

    pub(in crate::features) fn begin_discovery_job(
        &mut self,
    ) -> Option<UnboundedSender<AiDiscoveryJobResult>> {
        if self.discovery.pending {
            self.panel.status = "AI model discovery already running".to_string();
            return None;
        }
        self.discovery.pending = true;
        self.panel.status = "Discovering AI models...".to_string();
        Some(self.discovery.tx.clone())
    }

    pub(in crate::features) fn take_discovery_event_receiver(
        &mut self,
    ) -> Option<UnboundedReceiver<AiDiscoveryJobResult>> {
        self.discovery.rx.take()
    }

    /// A discovery reply settles the job it belongs to.
    pub(in crate::features) fn note_discovery_event_delivered(&mut self) {
        self.discovery.pending = false;
    }

    pub(in crate::features) fn set_discovery_query(&mut self, query: String) {
        self.discovery.query = query;
        self.discovery.index = 0;
    }

    /// Returns whether the text field must also be cleared.
    pub(in crate::features) fn escape_discovery_search(&mut self, selected_index: usize) -> bool {
        if self.discovery.query.is_empty() {
            self.discovery.menu_open = false;
            false
        } else {
            self.discovery.query.clear();
            self.discovery.index = selected_index;
            true
        }
    }

    pub(in crate::features) fn move_discovery_index(&mut self, choice_count: usize, delta: isize) {
        if choice_count == 0 {
            return;
        }
        self.discovery.index = if delta < 0 {
            (self.discovery.index + choice_count - 1) % choice_count
        } else {
            (self.discovery.index + 1) % choice_count
        };
    }

    pub(in crate::features) fn agent_steps(&self) -> &[AiAgentStepView] {
        &self.agent.steps
    }

    pub(in crate::features) fn upsert_agent_step(
        &mut self,
        step_index: u16,
        status: AiAgentStepStatus,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let title = title.into();
        let detail = detail.into();
        let lower_title = title.to_ascii_lowercase();
        let looks_like_command = matches!(
            status,
            AiAgentStepStatus::Running | AiAgentStepStatus::Tool | AiAgentStepStatus::NeedsApproval
        ) || lower_title.contains("background")
            || lower_title.contains("auto execute")
            || lower_title.contains("needs approval")
            || lower_title.contains("shell")
            || lower_title.contains("running");
        let looks_like_observation = lower_title.contains("observ")
            || lower_title == "done"
            || lower_title == "completed"
            || lower_title == "failed"
            || matches!(
                status,
                AiAgentStepStatus::Completed | AiAgentStepStatus::Failed
            );
        let looks_like_thought = lower_title.contains("plan")
            || lower_title.contains("think")
            || lower_title.contains("final answer")
            || matches!(status, AiAgentStepStatus::Planning);

        if let Some(step) = self
            .agent
            .steps
            .iter_mut()
            .find(|step| step.step_index == step_index)
        {
            step.status = status;
            step.title = title;
            if !detail.trim().is_empty() {
                step.detail = detail.clone();
            }
            if looks_like_command && !detail.trim().is_empty() {
                step.command = Some(detail.clone());
            }
            if looks_like_observation && !detail.trim().is_empty() {
                step.observation = Some(detail.clone());
            }
            if looks_like_thought && !detail.trim().is_empty() {
                step.thought = Some(detail);
            }
        } else {
            self.agent.steps.push(AiAgentStepView {
                step_index,
                status,
                title,
                detail: detail.clone(),
                thought: (looks_like_thought && !detail.trim().is_empty()).then(|| detail.clone()),
                command: (looks_like_command && !detail.trim().is_empty()).then(|| detail.clone()),
                observation: (looks_like_observation && !detail.trim().is_empty())
                    .then_some(detail),
            });
        }
        let overflow = self.agent.steps.len().saturating_sub(16);
        if overflow > 0 {
            let removed: Vec<u16> = self
                .agent
                .steps
                .iter()
                .take(overflow)
                .map(|step| step.step_index)
                .collect();
            self.agent.steps.drain(..overflow);
            for index in removed {
                self.agent.thought_expanded.remove(&index);
                self.agent.output_expanded.remove(&index);
            }
        }
    }

    pub(in crate::features) fn toggle_agent_thought_expanded(&mut self, step_index: u16) {
        if !self.agent.thought_expanded.remove(&step_index) {
            self.agent.thought_expanded.insert(step_index);
        }
    }

    pub(in crate::features) fn toggle_agent_output_expanded(&mut self, step_index: u16) {
        if !self.agent.output_expanded.remove(&step_index) {
            self.agent.output_expanded.insert(step_index);
        }
    }

    pub(in crate::features) fn agent_task_prompt_or_preview(&self) -> String {
        self.agent
            .task_prompt
            .clone()
            .unwrap_or_else(|| self.chat.response_preview.clone())
    }

    pub(in crate::features) fn begin_agent_step(
        &mut self,
        max_steps: u16,
    ) -> Result<(String, u16), String> {
        let step_index = self.agent.step_index;
        if step_index.saturating_add(1) >= max_steps {
            self.agent.loop_state = None;
            return Err(format!(
                "AI Agent reached max step limit ({max_steps}); review terminal output"
            ));
        }
        self.agent.step_index = self.agent.step_index.saturating_add(1);
        Ok((self.agent_task_prompt_or_preview(), step_index))
    }

    pub(in crate::features) fn register_agent_capture(&mut self, marker_id: String) {
        self.agent.capture.register(marker_id);
    }

    pub(in crate::features) fn set_agent_loop(&mut self, state: AiAgentLoopState) {
        self.agent.loop_state = Some(state);
    }

    pub(in crate::features) fn stop_agent_for_closed_target(&mut self) -> Option<u16> {
        let state = self.agent.loop_state.take()?;
        self.panel.status = "AI Agent loop stopped because the target session closed".to_string();
        self.upsert_agent_step(
            state.step_index,
            AiAgentStepStatus::Failed,
            "Stopped",
            "Target session closed",
        );
        Some(state.step_index)
    }

    pub(in crate::features) fn poll_agent_observation(
        &mut self,
        now: Instant,
        current_len: usize,
        quiet: Duration,
    ) -> AiAgentObservationPoll {
        if self.chat.pending {
            return AiAgentObservationPoll::Waiting;
        }
        let Some(state) = self.agent.loop_state.as_mut() else {
            return AiAgentObservationPoll::Waiting;
        };
        if state.background_job_id.is_some() {
            return AiAgentObservationPoll::Waiting;
        }
        if current_len != state.last_seen_len {
            state.last_seen_len = current_len;
            state.stable_since = now;
            return AiAgentObservationPoll::Waiting;
        }
        if now < state.min_wait_until {
            return AiAgentObservationPoll::Waiting;
        }
        let has_observed_output = current_len > state.output_start_len;
        let output_is_quiet = now.duration_since(state.stable_since) >= quiet;
        let timed_out = now >= state.timeout_at;
        if timed_out && state.marker_id.is_some() {
            let state = self.agent.loop_state.take().expect("agent loop is present");
            if let Some(marker_id) = state.marker_id.as_deref() {
                self.agent.capture.cancel(marker_id);
            }
            self.panel.status = format!("AI Agent command capture timed out: {}", state.command);
            return AiAgentObservationPoll::TimedOut(state);
        }
        if !timed_out && (!has_observed_output || !output_is_quiet) {
            return AiAgentObservationPoll::Waiting;
        }
        if state.marker_id.is_some() {
            return AiAgentObservationPoll::Waiting;
        }
        AiAgentObservationPoll::Target(self.agent.loop_state.take().expect("agent loop is present"))
    }

    pub(in crate::features) fn take_agent_loop_for_marker(
        &mut self,
        marker_id: &str,
    ) -> Option<AiAgentLoopState> {
        if !self
            .agent
            .loop_state
            .as_ref()
            .is_some_and(|state| state.marker_id.as_deref() == Some(marker_id))
        {
            return None;
        }
        self.agent.loop_state.take()
    }

    pub(in crate::features) fn take_agent_loop_for_session(
        &mut self,
        session_id: &str,
    ) -> Option<AiAgentLoopState> {
        if !self
            .agent
            .loop_state
            .as_ref()
            .is_some_and(|state| state.terminal_session_id == session_id)
        {
            return None;
        }
        let state = self.agent.loop_state.take()?;
        if let Some(marker_id) = state.marker_id.as_deref() {
            self.agent.capture.cancel(marker_id);
        }
        Some(state)
    }

    pub(in crate::features) fn begin_agent_continuation(
        &mut self,
        state: &AiAgentLoopState,
    ) -> Option<AiChatLaunch> {
        if self.chat.pending {
            self.agent.loop_state = Some(state.clone());
            return None;
        }
        let mut launch = self.begin_chat_job();
        launch.session_id = state.ai_session_id.clone();
        self.chat.pending = true;
        self.chat.response_preview = format!(
            "Running AI Agent continuation step {}/{}...",
            state.step_index + 2,
            state.max_steps
        );
        self.chat.command_cards.clear();
        self.panel.status = self.chat.response_preview.clone();
        self.upsert_agent_step(
            state.step_index.saturating_add(1),
            AiAgentStepStatus::Planning,
            "Planning",
            "Continuing from the latest command observation",
        );
        Some(launch)
    }

    pub(in crate::features) fn agent_thought_is_expanded(&self, step_index: u16) -> bool {
        self.agent.thought_expanded.contains(&step_index)
    }

    pub(in crate::features) fn agent_output_is_expanded(&self, step_index: u16) -> bool {
        self.agent.output_expanded.contains(&step_index)
    }

    pub(in crate::features) fn request_agent_auto_confirm(&mut self) {
        self.close_transient_menus();
    }

    pub(in crate::features) fn confirm_agent_auto_execution(&mut self) -> bool {
        self.settings.config.agent_command_execution_mode = AgentCommandExecutionMode::Auto;
        self.panel.status = "Agent execution mode: auto".to_string();
        true
    }

    pub(in crate::features) fn last_agent_step_index(&self) -> u16 {
        self.agent
            .steps
            .last()
            .map(|step| step.step_index)
            .unwrap_or(0)
    }

    pub(in crate::features) fn agent_loop_snapshot(&self) -> Option<AiAgentLoopState> {
        self.agent.loop_state.clone()
    }

    pub(in crate::features) fn agent_loop_clock_is_armed(&self) -> bool {
        self.agent.loop_clock_armed
    }

    pub(in crate::features) fn set_agent_loop_clock_armed(&mut self, armed: bool) {
        self.agent.loop_clock_armed = armed;
    }

    pub(in crate::features) fn request_panel_refresh(&mut self) -> bool {
        if self.panel.panel_refresh_requested {
            return false;
        }
        self.panel.panel_refresh_requested = true;
        true
    }

    pub(in crate::features) fn take_panel_refresh_request(&mut self) -> bool {
        std::mem::take(&mut self.panel.panel_refresh_requested)
    }

    pub(in crate::features) fn clear_panel_refresh_request(&mut self) {
        self.panel.panel_refresh_requested = false;
    }

    pub(in crate::features) fn process_agent_output(
        &mut self,
        text: &str,
    ) -> AgentCaptureProcessResult {
        self.agent.capture.process(text)
    }

    pub(in crate::features) fn reset_agent_runtime(&mut self) {
        self.agent.loop_state = None;
        self.agent.capture = AgentOutputCaptureProcessor::new();
    }

    pub(in crate::features) fn agent_capture_is_active_for(&self, session_id: &str) -> bool {
        self.agent.capture.has_active()
            && self
                .agent
                .loop_state
                .as_ref()
                .is_some_and(|state| state.terminal_session_id == session_id)
    }

    pub(in crate::features) fn panel_status(&self) -> &str {
        &self.panel.status
    }

    pub(in crate::features) fn set_panel_status(&mut self, status: impl Into<String>) {
        self.panel.status = status.into();
    }

    pub(in crate::features) fn apply_settings_input(&mut self, field: AiInputField, text: String) {
        self.panel.focused_field = field;
        match field {
            AiInputField::Model => self.settings.model_draft = text,
            AiInputField::BaseUrl => self.settings.base_url_draft = text,
            AiInputField::ApiKey => self.settings.secret_draft = text.into(),
            AiInputField::RequestUserAgent => self.settings.config.request_user_agent = text,
            AiInputField::CodexExecutable => {
                self.settings.config.codex.executable_path = non_empty(text)
            }
            AiInputField::CodexDefaultModel => {
                self.settings.config.codex.default_model = non_empty(text)
            }
            AiInputField::CodexConfigDirectory => {
                self.settings.config.codex.config_directory = non_empty(text)
            }
            AiInputField::ClaudeExecutable => {
                self.settings.config.claude_code.executable_path = non_empty(text)
            }
            AiInputField::ClaudeDefaultModel => {
                self.settings.config.claude_code.default_model = non_empty(text)
            }
            AiInputField::ClaudeConfigDirectory => {
                self.settings.config.claude_code.config_directory = non_empty(text)
            }
        }
        self.panel.status = "AI settings edited".to_string();
    }

    pub(in crate::features) fn panel_execution_menu_is_open(&self) -> bool {
        self.panel.execution_menu_open
    }

    pub(in crate::features) fn toggle_execution_menu(&mut self) -> bool {
        self.history.open = false;
        self.history.query.clear();
        self.panel.execution_menu_open = !self.panel.execution_menu_open;
        if self.panel.execution_menu_open {
            self.chat.message_menu = None;
            self.discovery.menu_open = false;
        }
        self.panel.execution_menu_open
    }

    pub(in crate::features) fn close_execution_menu(&mut self) {
        self.panel.execution_menu_open = false;
    }

    pub(in crate::features) fn panel_detected_error(&self) -> Option<&AiDetectedErrorState> {
        self.panel.detected_error.as_ref()
    }

    pub(in crate::features) fn dismiss_detected_error(&mut self) {
        self.panel.dismiss_detected_error();
    }

    pub(in crate::features) fn clear_detected_error(&mut self) {
        self.panel.detected_error = None;
    }

    pub(in crate::features) fn note_detected_error(
        &mut self,
        session_id: String,
        output: String,
        now: Instant,
    ) -> bool {
        if self
            .panel
            .error_notice_at
            .get(&session_id)
            .is_some_and(|last| now.duration_since(*last) < std::time::Duration::from_secs(30))
        {
            return false;
        }
        self.panel.error_notice_at.insert(session_id.clone(), now);
        self.panel.detected_error = Some(AiDetectedErrorState { session_id, output });
        self.panel.status = "terminal error detected".to_string();
        true
    }

    pub(in crate::features) fn close_transient_menus(&mut self) {
        self.history.open = false;
        self.discovery.menu_open = false;
        self.panel.execution_menu_open = false;
        self.chat.message_menu = None;
    }

    pub(in crate::features) fn transient_menus_are_open(&self) -> bool {
        self.history.open
            || self.discovery.menu_open
            || self.panel.execution_menu_open
            || self.chat.message_menu.is_some()
    }
}

impl AiChatState {
    fn close_message_menu(&mut self) {
        self.message_menu = None;
    }

    /// Tracks the trailing `@mention` the composer is currently completing.
    ///
    /// Only a trailing run with no whitespace counts, so the picker closes as
    /// soon as the user types past the mention. The rules are unchanged.
    fn sync_mention_from_prompt(&mut self) {
        let Some(at_index) = self.prompt_draft.rfind('@') else {
            self.close_mention();
            return;
        };
        let query = &self.prompt_draft[at_index + 1..];
        if query.chars().any(char::is_whitespace) {
            self.close_mention();
            return;
        }
        if self.mention_query != query {
            self.mention_query = query.to_string();
            self.mention_index = 0;
        }
        self.mention_open = true;
    }

    fn close_mention(&mut self) {
        self.mention_open = false;
        self.mention_query.clear();
        self.mention_index = 0;
    }
}

impl AiPanelState {
    pub(in crate::features) fn dismiss_detected_error(&mut self) {
        self.detected_error = None;
        self.status = "terminal error notice dismissed".to_string();
    }
}

/// Transitions that span more than one AI concern.
impl AiFeatureState {
    pub(in crate::features) fn clear_quote(&mut self) {
        self.chat.quoted_text = None;
        self.panel.status = "AI quote cleared".to_string();
    }

    /// Resets every per-conversation concern and mints a new session id.
    ///
    /// Provider settings are deliberately untouched; the response preview is
    /// seeded from the configured default mode exactly as before.
    pub(in crate::features) fn start_new_chat(&mut self) {
        self.chat.prompt_draft.clear();
        self.chat.target_session_ids.clear();
        self.chat.message_menu = None;
        self.chat.quoted_text = None;
        self.chat.close_mention();
        self.chat.response_preview = if self.settings.config.default_mode == AiMode::Agent {
            "Agent mode ready".to_string()
        } else {
            "Ask mode ready".to_string()
        };
        self.chat.command_cards.clear();
        self.chat.messages.clear();
        self.chat.streaming_assistant_id = None;
        self.chat.prepared_request = None;
        self.chat.session_id = format!("ai-session-{}", uuid());

        self.agent.task_prompt = None;
        self.agent.step_index = 0;
        self.agent.loop_state = None;
        self.agent.capture = AgentOutputCaptureProcessor::new();
        self.agent.steps.clear();
        self.agent.thought_expanded.clear();
        self.agent.output_expanded.clear();

        self.history.open = false;
        self.history.query.clear();

        self.discovery.menu_open = false;
        self.discovery.query.clear();
        self.discovery.index = 0;

        self.panel.detected_error = None;
        self.panel.execution_menu_open = false;
        self.panel.status = "new AI chat".to_string();
    }
}

#[cfg(test)]
mod tests;
