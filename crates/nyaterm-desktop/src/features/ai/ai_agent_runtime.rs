use std::time::{Duration, Instant};

use gpui::Context;
use nyaterm_core::{
    AgentCapturedOutput, AiAction, AiChatRequest, AiCommandCard, AiExecutionProfile, AiMode,
    AppendAiAuditRequest, CommandObservation, build_agent_capture_command,
    build_observation_message, truncate_preview, uuid,
};
use nyaterm_store::StoreDomain;
use nyaterm_transport::{SessionKind, SshProcessService, run_local_command};

use crate::features::{
    NyaTermApp, runtime_jobs::AiAgentBackgroundTarget, runtime_jobs::AiAgentLoopState,
    runtime_jobs::AiAgentStepStatus, runtime_jobs::AiChatJobResult,
    runtime_jobs::AiChatWorkerEvent, runtime_jobs::await_blocking_job,
};
use crate::models::SessionLaunchConfig;

use super::ai_jobs::{
    ai_job_cancelled, observation_summary, remote_command_observation, run_ai_ask_job,
};
use super::state::AiAgentObservationPoll;
use super::{
    AGENT_DEFAULT_STEP_TIMEOUT, AGENT_OBSERVATION_MIN_WAIT, AGENT_OBSERVATION_POLL_INTERVAL,
    AGENT_OBSERVATION_QUIET,
};

impl NyaTermApp {
    pub(in crate::features) fn upsert_ai_agent_step(
        &mut self,
        step_index: u16,
        status: AiAgentStepStatus,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.ai.upsert_agent_step(step_index, status, title, detail);
    }

    pub(in crate::features) fn toggle_ai_agent_thought_expanded(
        &mut self,
        step_index: u16,
        cx: &mut Context<Self>,
    ) {
        self.ai.toggle_agent_thought_expanded(step_index);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn toggle_ai_agent_output_expanded(
        &mut self,
        step_index: u16,
        cx: &mut Context<Self>,
    ) {
        self.ai.toggle_agent_output_expanded(step_index);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn record_ai_command_card_audit(
        &mut self,
        card: &AiCommandCard,
        execute: bool,
        inserted_to_terminal: bool,
        cx: &mut Context<Self>,
    ) {
        let store = self.store_blocking_client();
        let write_lock = self.ai.history_audit_write_lock();
        let request = AppendAiAuditRequest {
            connection_id: card.target_terminal_session_id.clone().or_else(|| {
                card.target
                    .as_ref()
                    .map(|target| target.terminal_session_id.clone())
            }),
            action: if execute {
                "ai.command_card_run".to_string()
            } else {
                "ai.command_card_insert".to_string()
            },
            user_input: Some(self.ai.chat_response_preview().to_string()),
            generated_command: Some(card.command.clone()),
            risk_level: card.risk_level.clone(),
            inserted_to_terminal,
            executed: execute,
            blocked: false,
            ..Default::default()
        };
        let task = self.blocking_jobs.submit_task("ai-audit-save", move |_| {
            let _guard = write_lock
                .lock()
                .map_err(|_| "AI audit write lock poisoned".to_string())?;
            store
                .request_fn(StoreDomain::Ai, move |database| {
                    database.append_ai_audit(request)
                })
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            if let Err(error) = await_blocking_job(task).await.and_then(|result| result) {
                let _ = this.update(cx, |this, cx| {
                    this.settings
                        .update_store_status(format!("AI audit save failed: {error}"), false);
                    this.request_settings_panel_refresh(cx);
                    this.defer_ai_panel_snapshot_flush(cx);
                });
            } else {
                let _ = this.update(cx, |this, cx| {
                    this.refresh_ai_usage_counts(cx);
                });
            }
        })
        .detach();
    }

    pub(in crate::features) fn begin_ai_agent_observation(
        &mut self,
        command: &str,
        terminal_session_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<Option<String>, String> {
        let before = self.ai_header_presentation();
        let terminal_session_id = terminal_session_id.to_string();
        if self.session.session_info(&terminal_session_id).is_none()
            || self.session.is_disconnected(&terminal_session_id)
        {
            return Err("AI Agent target terminal is unavailable".to_string());
        }
        let max_steps = self.ai.settings_max_agent_steps();
        let (task_prompt, step_index) = match self.ai.begin_agent_step(max_steps) {
            Ok(step) => step,
            Err(error) => {
                self.ai.set_panel_status(error);
                return Ok(None);
            }
        };
        let now = Instant::now();
        let timeout = self
            .ai
            .settings_config()
            .agent_step_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(AGENT_DEFAULT_STEP_TIMEOUT);
        let profile = self.ai_execution_profile_for_session(&terminal_session_id);
        if profile == AiExecutionProfile::Disabled {
            return Err("AI Agent command execution is disabled for this session".to_string());
        }
        let marker_id = format!("agent-{}", uuid());
        let (marker_id, wrapped_command) =
            match build_agent_capture_command(profile, &marker_id, command.trim()) {
                Some(wrapped) => {
                    self.ai.register_agent_capture(marker_id.clone());
                    (Some(marker_id), Some(wrapped))
                }
                None => (None, None),
            };
        let output_start_len = self
            .terminal_buffer_text_for_session(&terminal_session_id)
            .len();
        let available_target_ids = self.ai_effective_target_session_ids();
        let available_targets = self.ai_terminal_targets_for_sessions(&available_target_ids);
        self.ai.set_agent_loop(AiAgentLoopState {
            ai_session_id: self.ai.chat_session_id().to_string(),
            terminal_session_id: terminal_session_id.clone(),
            available_targets,
            default_target_session_id: Some(terminal_session_id),
            task_prompt,
            command: command.trim().to_string(),
            marker_id,
            background_job_id: None,
            step_index,
            max_steps,
            output_start_len,
            started_at: now,
            min_wait_until: now + AGENT_OBSERVATION_MIN_WAIT,
            timeout_at: now + timeout,
            last_seen_len: output_start_len,
            stable_since: now,
        });
        self.ensure_ai_agent_loop_clock(cx);
        self.sync_session_event_bridge_policy();
        self.ai.set_panel_status(format!(
            "AI Agent observing command output for step {}/{}",
            step_index + 1,
            max_steps
        ));
        self.upsert_ai_agent_step(
            step_index,
            AiAgentStepStatus::Running,
            "Running",
            truncate_preview(command.trim(), 140),
        );
        self.notify_root_if_ai_header_changed(before, cx);
        Ok(wrapped_command)
    }

    pub(in crate::features) fn begin_ai_agent_background_execution(
        &mut self,
        command: &str,
        terminal_session_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let before = self.ai_header_presentation();
        let terminal_session_id = terminal_session_id.to_string();
        let session = self
            .session
            .session_info(&terminal_session_id)
            .filter(|_| !self.session.is_disconnected(&terminal_session_id))
            .ok_or_else(|| "Active terminal session was not found".to_string())?;
        let (target, target_label) = match session.kind {
            SessionKind::Ssh => {
                let config = self
                    .session
                    .metadata(&terminal_session_id)
                    .and_then(|metadata| match &metadata.launch_config {
                        SessionLaunchConfig::Ssh(config) => Some(config.as_ref().clone()),
                        _ => None,
                    })
                    .or_else(|| {
                        (self.session.active_id() == Some(terminal_session_id.as_str()))
                            .then(|| self.session.active_ssh_config_owned())
                            .flatten()
                    })
                    .ok_or_else(|| "Target SSH session is missing its exec config".to_string())?;
                (AiAgentBackgroundTarget::Ssh(Box::new(config)), "SSH")
            }
            SessionKind::LocalPty => (
                AiAgentBackgroundTarget::Local {
                    working_dir: session.working_dir.clone(),
                },
                "local",
            ),
            SessionKind::Telnet
            | SessionKind::RawTcp
            | SessionKind::Serial
            | SessionKind::Rdp
            | SessionKind::Vnc => {
                return Err(format!(
                    "AI Agent background execution is not supported for {:?} sessions",
                    session.kind
                ));
            }
        };
        let max_steps = self.ai.settings_max_agent_steps();
        let (task_prompt, step_index) = self.ai.begin_agent_step(max_steps)?;
        let now = Instant::now();
        let timeout = self
            .ai
            .settings_config()
            .agent_step_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(AGENT_DEFAULT_STEP_TIMEOUT);
        let launch = self.ai.begin_chat_job();
        let job_id = launch.job_id;
        let cancel = launch.cancel;
        let output_start_len = self
            .terminal_buffer_text_for_session(&terminal_session_id)
            .len();
        let available_target_ids = self.ai_effective_target_session_ids();
        let available_targets = self.ai_terminal_targets_for_sessions(&available_target_ids);
        let state = AiAgentLoopState {
            ai_session_id: self.ai.chat_session_id().to_string(),
            terminal_session_id: terminal_session_id.clone(),
            available_targets,
            default_target_session_id: Some(terminal_session_id),
            task_prompt,
            command: command.trim().to_string(),
            marker_id: None,
            background_job_id: Some(job_id),
            step_index,
            max_steps,
            output_start_len,
            started_at: now,
            min_wait_until: now,
            timeout_at: now + timeout,
            last_seen_len: output_start_len,
            stable_since: now,
        };
        self.ai.set_agent_loop(state.clone());
        self.ensure_ai_agent_loop_clock(cx);
        self.ai.set_panel_status(format!(
            "AI Agent running {target_label} background command for step {}/{}",
            step_index + 1,
            max_steps
        ));
        self.upsert_ai_agent_step(
            step_index,
            AiAgentStepStatus::Running,
            format!("{target_label} background"),
            truncate_preview(command.trim(), 140),
        );
        let tx = launch.tx;
        let command = state.command.clone();
        let rejected_tx = tx.clone();
        let rejected_state = state.clone();
        if let Err(error) = self.blocking_jobs.submit_detached(
            "ai-agent-background-command",
            move |scheduler_cancel| {
                let started = Instant::now();
                let result = if scheduler_cancel.is_cancelled() || ai_job_cancelled(&cancel) {
                    Err("AI Agent background command cancelled".to_string())
                } else {
                    match target {
                        AiAgentBackgroundTarget::Ssh(config) => SshProcessService::new(*config)
                            .run_command(&command, timeout)
                            .map(|output| remote_command_observation(output, started))
                            .map_err(|error| error.to_string()),
                        AiAgentBackgroundTarget::Local { working_dir } => {
                            run_local_command(&command, working_dir, timeout)
                                .map(|output| remote_command_observation(output, started))
                                .map_err(|error| error.to_string())
                        }
                    }
                };
                if !scheduler_cancel.is_cancelled() && !ai_job_cancelled(&cancel) {
                    let _ = tx.unbounded_send(AiChatWorkerEvent::AgentBackgroundFinished {
                        job_id,
                        state,
                        result,
                    });
                }
            },
        ) {
            let _ = rejected_tx.unbounded_send(AiChatWorkerEvent::AgentBackgroundFinished {
                job_id,
                state: rejected_state,
                result: Err(error.to_string()),
            });
        }
        self.defer_ai_panel_snapshot_flush(cx);
        self.notify_root_if_ai_header_changed(before, cx);
        Ok(())
    }

    /// Watch for the terminal to fall quiet while an agent loop is running.
    ///
    /// This one stays a poll on purpose, and the honest reason is that it is watching
    /// for the *absence* of output: `poll_agent_observation` waits for the output
    /// length to hold still for `AGENT_OBSERVATION_QUIET`, and nothing emits an event
    /// when output stops. What changes is the scope -- it polls only while an agent
    /// loop exists, at its own cadence, instead of riding a global tick that also had
    /// to name `ai.has_background_work()` in its quiet gate to stay responsive.
    ///
    /// Idempotent, and retires itself when the loop ends.
    pub(in crate::features) fn ensure_ai_agent_loop_clock(&mut self, cx: &mut Context<Self>) {
        if self.ai.agent_loop_clock_is_armed() || self.ai.agent_loop_snapshot().is_none() {
            return;
        }
        self.ai.set_agent_loop_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(AGENT_OBSERVATION_POLL_INTERVAL)
                    .await;
                let Ok(keep_running) = this.update(cx, |this, cx| {
                    let before = this.ai_header_presentation();
                    if this.drive_ai_agent_loop(cx) {
                        this.defer_ai_panel_snapshot_flush(cx);
                    }
                    let running = this.ai.agent_loop_snapshot().is_some();
                    if !running {
                        this.ai.set_agent_loop_clock_armed(false);
                    }
                    this.notify_root_if_ai_header_changed(before, cx);
                    running
                }) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::features) fn drive_ai_agent_loop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(snapshot) = self.ai.agent_loop_snapshot() else {
            return false;
        };
        if self.ai.chat_is_pending() || snapshot.background_job_id.is_some() {
            return false;
        }
        let terminal_session_id = snapshot.terminal_session_id.clone();
        if self.session.session_info(&terminal_session_id).is_none()
            || self.session.is_disconnected(&terminal_session_id)
        {
            self.ai.stop_agent_for_closed_target();
            let _ = cx;
            return true;
        }
        let now = Instant::now();
        let current_len = self
            .terminal
            .session_output_len_or_default(&terminal_session_id);
        match self
            .ai
            .poll_agent_observation(now, current_len, AGENT_OBSERVATION_QUIET)
        {
            AiAgentObservationPoll::Waiting => false,
            AiAgentObservationPoll::TimedOut(state) => {
                self.sync_session_event_bridge_policy();
                let duration_ms = now
                    .duration_since(state.started_at)
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                let observation = CommandObservation {
                    output:
                        "(command timed out; capture markers were not detected in terminal output)"
                            .to_string(),
                    exit_code: None,
                    duration_ms,
                };
                self.upsert_ai_agent_step(
                    state.step_index,
                    AiAgentStepStatus::Failed,
                    "Timed out",
                    observation_summary(&observation),
                );
                self.start_ai_agent_continuation(state, observation, cx);
                true
            }
            AiAgentObservationPoll::Target(state) => {
                let terminal_output =
                    self.terminal_buffer_text_for_session(&state.terminal_session_id);
                let output = terminal_output
                    .get(state.output_start_len..)
                    .unwrap_or_default()
                    .to_string();
                let duration_ms = now
                    .duration_since(state.started_at)
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                let observation = CommandObservation {
                    output,
                    exit_code: None,
                    duration_ms,
                };
                self.upsert_ai_agent_step(
                    state.step_index,
                    AiAgentStepStatus::Completed,
                    "Observed",
                    observation_summary(&observation),
                );
                self.start_ai_agent_continuation(state, observation, cx);
                true
            }
        }
    }

    pub(in crate::features) fn handle_ai_agent_captured_output(
        &mut self,
        captured: AgentCapturedOutput,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.ai.take_agent_loop_for_marker(&captured.marker_id) else {
            return;
        };
        let observation = CommandObservation {
            output: captured.output,
            exit_code: captured.exit_code,
            duration_ms: captured.duration_ms,
        };
        self.ai.set_panel_status(match observation.exit_code {
            Some(code) => format!("AI Agent captured command output with exit code {code}"),
            None => "AI Agent captured command output".to_string(),
        });
        self.upsert_ai_agent_step(
            state.step_index,
            AiAgentStepStatus::Completed,
            "Observed",
            observation_summary(&observation),
        );
        self.start_ai_agent_continuation(state, observation, cx);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn note_ai_agent_output_discontinuity(
        &mut self,
        session_id: &str,
        dropped_bytes: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.ai.take_agent_loop_for_session(session_id) else {
            return false;
        };
        if state.marker_id.is_some() {
            self.sync_session_event_bridge_policy();
        }
        let duration_ms = state
            .started_at
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let observation = CommandObservation {
            output: format!(
                "(terminal output dropped {dropped_bytes} byte(s); command output is incomplete)"
            ),
            exit_code: None,
            duration_ms,
        };
        self.ai.set_panel_status(
            "AI Agent command observation stopped because terminal output was dropped".to_string(),
        );
        self.upsert_ai_agent_step(
            state.step_index,
            AiAgentStepStatus::Failed,
            "Output dropped",
            observation_summary(&observation),
        );
        self.start_ai_agent_continuation(state, observation, cx);
        self.defer_ai_panel_snapshot_flush(cx);
        true
    }

    fn ai_execution_profile_for_session(&self, session_id: &str) -> AiExecutionProfile {
        let configured = if self.session.active_id() == Some(session_id) {
            self.session.active_ai_execution_profile()
        } else {
            self.session
                .metadata(session_id)
                .map(|metadata| metadata.ai_execution_profile)
                .unwrap_or(AiExecutionProfile::SendOnly)
        };
        if configured != AiExecutionProfile::Auto {
            return configured;
        }
        self.session
            .session_info(session_id)
            .filter(|_| !self.session.is_disconnected(session_id))
            .map(|session| match session.kind {
                SessionKind::LocalPty
                | SessionKind::Ssh
                | SessionKind::Telnet
                | SessionKind::RawTcp => AiExecutionProfile::Posix,
                SessionKind::Serial | SessionKind::Rdp | SessionKind::Vnc => {
                    AiExecutionProfile::SendOnly
                }
            })
            .unwrap_or(AiExecutionProfile::SendOnly)
    }

    pub(in crate::features) fn start_ai_agent_continuation(
        &mut self,
        state: AiAgentLoopState,
        observation: CommandObservation,
        cx: &mut Context<Self>,
    ) {
        let Some(launch) = self.ai.begin_agent_continuation(&state) else {
            return;
        };
        let observation_message = build_observation_message(
            &observation,
            &state.command,
            &self.settings.summary().language,
        );
        let settings = self.ai.settings_config_cloned();
        let terminal_session_id = state.terminal_session_id.clone();
        let context = self.ai_terminal_context_for_session(Some(&terminal_session_id));
        let targets = state.available_targets.clone();
        let target_contexts = targets
            .iter()
            .cloned()
            .map(|target| nyaterm_core::AiTargetContext {
                context: self.ai_terminal_context_for_session(Some(&target.terminal_session_id)),
                target: Some(target),
            })
            .collect();
        let request = AiChatRequest {
            stream_id: None,
            session_id: Some(state.ai_session_id.clone()),
            connection_id: Some(terminal_session_id.clone()),
            terminal_session_id: Some(terminal_session_id.clone()),
            owner_scope: nyaterm_core::AiSessionScope {
                r#type: nyaterm_core::AiSessionScopeType::Terminal,
                target_id: Some(terminal_session_id.clone()),
                connection_ids: Vec::new(),
                label: self.session.display_name(&terminal_session_id),
            },
            targets,
            target_contexts,
            mode: AiMode::Agent,
            agent_kind: settings.default_agent_kind.clone(),
            permission_mode: settings.external_agent_permission_mode.clone(),
            model_id: settings.default_model_id.clone(),
            model_name: None,
            default_target_session_id: state.default_target_session_id.clone(),
            existing_external_session_id: None,
            attachments: Vec::new(),
            action: AiAction::GenerateCommand,
            user_input: format!(
                "Continue the same Agent task.\n\nOriginal task:\n{}\n\n{}",
                state.task_prompt, observation_message
            ),
            context,
            options: Default::default(),
        };
        let store = self.store_blocking_client();
        let tx = launch.tx;
        let session_id = launch.session_id;
        let job_id = launch.job_id;
        let cancel = launch.cancel;
        let rejected_tx = tx.clone();
        let rejected_session_id = session_id.clone();
        if let Err(error) =
            self.blocking_jobs
                .submit_detached("ai-agent-continuation", move |scheduler_cancel| {
                    let result = if scheduler_cancel.is_cancelled() {
                        Err("AI Agent continuation cancelled".to_string())
                    } else {
                        run_ai_ask_job(
                            store,
                            settings,
                            request,
                            None,
                            Some(tx.clone()),
                            cancel,
                            job_id,
                        )
                    };
                    let _ = tx.unbounded_send(AiChatWorkerEvent::Finished(AiChatJobResult {
                        job_id,
                        session_id,
                        result,
                    }));
                })
        {
            let _ = rejected_tx.unbounded_send(AiChatWorkerEvent::Finished(AiChatJobResult {
                job_id,
                session_id: rejected_session_id,
                result: Err(error.to_string()),
            }));
        }
        self.defer_ai_panel_snapshot_flush(cx);
    }
}
