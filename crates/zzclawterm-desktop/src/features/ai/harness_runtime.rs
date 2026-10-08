use gpui::Context;
use rust_i18n::t;
use serde_json::{Value, json};
use zzclawterm_core::ai::harness::{
    AgentPlan, AgentTool, AgentToolCall, AgentToolResult, AgentVerification,
};
use zzclawterm_core::ai::{AiCommandCard, CommandObservation};
use zzclawterm_core::capabilities::CapabilityScopeSnapshot;
use zzclawterm_mcp_protocol::{RpcError, TerminalExecuteArgs, TerminalExecuteResult};

use crate::features::ZzClawTermApp;
use crate::features::capability_runtime::{CapabilityCaller, CapabilityRequest};
use crate::features::runtime_jobs::{AiChatJobResult, AiChatWorkerEvent};

use super::ai_jobs::{AiJobRunOptions, run_ai_ask_job};
use super::presentation::AiAgentStepKind;

impl ZzClawTermApp {
    pub(in crate::features) fn cancel_native_run_for_closed_session(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        let scope = format!("terminal:{session_id}");
        if self.ai.native_owner_in_scope(&scope).is_none() {
            return;
        }
        let visible = self.ai.active_scope_key().to_string();
        self.ai.switch_scope(&scope);
        self.cancel_ai_chat(cx);
        self.ai.switch_scope(&visible);
    }

    fn persist_native_chat_projection(&mut self, cx: &mut Context<Self>) {
        if !self.ai.settings_config().record_history {
            return;
        }
        let Some(message) = self
            .ai
            .chat_messages()
            .last()
            .map(|message| (**message).clone())
        else {
            return;
        };
        let store = self.store_blocking_client();
        let lock = self.ai.history_audit_write_lock();
        let task = self
            .blocking_jobs
            .submit_task("native-agent-history", move |_| {
                let _guard = lock
                    .lock()
                    .map_err(|_| "AI history lock poisoned".to_string())?;
                store
                    .request_fn(zzclawterm_store::StoreDomain::Ai, move |database| {
                        database.append_ai_message(message)
                    })
                    .map_err(|_| "Cannot save Agent history".to_string())
            });
        cx.spawn(async move |_this, _cx| {
            let _ = crate::features::runtime_jobs::await_blocking_result(task).await;
        })
        .detach();
    }

    pub(in crate::features) fn dispatch_native_agent_call(
        &mut self,
        call: AgentToolCall,
        cx: &mut Context<Self>,
    ) {
        let (run_id, scope, cancellation) = match self.ai.begin_native_call(call.clone()) {
            Ok(context) => context,
            Err(error) => {
                if self.ai.reject_native_over_limit(call.clone()) {
                    self.start_native_agent_continuation(cx);
                    return;
                }
                self.ai.cancel_chat_and_agent();
                self.ai.set_native_message(error);
                if let Some(owner) = self.ai.native_owner() {
                    self.clear_capability_owner(&owner, cx);
                }
                return;
            }
        };
        let index = self.ai.last_agent_step_index();
        self.upsert_ai_agent_step(
            index,
            crate::features::runtime_jobs::AiAgentStepStatus::Running,
            AiAgentStepKind::ToolProgress,
            call.tool.name(),
            &call.thought,
        );
        self.ai.link_native_step();
        self.ai.set_native_message(if call.thought.is_empty() {
            call.tool.name().into()
        } else {
            call.thought.clone()
        });
        match call.tool {
            AgentTool::UpdatePlan => {
                self.ai
                    .set_native_message(format!("{}\n{}", t!("ai.harness.plan"), call.thought));
                let result = serde_json::from_value::<AgentPlan>(call.arguments.clone())
                    .map_err(|_| "Invalid plan".to_string())
                    .and_then(|plan| self.ai.update_native_plan(plan));
                self.complete_native_control(
                    &run_id,
                    &call.id,
                    result.map(|()| json!({"updated":true})),
                    cx,
                );
            }
            AgentTool::RequestUserInput => {
                let questions = serde_json::from_value(call.arguments["questions"].clone())
                    .expect("validated questions");
                if let Err(error) = self.ai.wait_native_user(questions) {
                    self.complete_native_control(&run_id, &call.id, Err(error), cx);
                } else {
                    let view = self.ai.native_run_view().expect("waiting run");
                    self.ai.set_native_message(format!(
                        "{}\n{}",
                        t!("ai.harness.waitingForUser"),
                        view.questions
                            .iter()
                            .map(|question| question.question.as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                    self.persist_native_chat_projection(cx);
                    self.upsert_ai_agent_step(
                        index,
                        crate::features::runtime_jobs::AiAgentStepStatus::Tool,
                        AiAgentStepKind::ToolProgress,
                        "Waiting for user",
                        &call.thought,
                    );
                }
            }
            AgentTool::FinalAnswer => {
                let verification: AgentVerification =
                    serde_json::from_value(call.arguments["verification"].clone())
                        .expect("validated verification");
                let label = match verification.status {
                    zzclawterm_core::ai::harness::AgentVerificationStatus::Verified => {
                        t!("ai.harness.verified")
                    }
                    zzclawterm_core::ai::harness::AgentVerificationStatus::Unverified => {
                        t!("ai.harness.unverified")
                    }
                    zzclawterm_core::ai::harness::AgentVerificationStatus::Blocked => {
                        t!("ai.harness.blocked")
                    }
                };
                let answer = format!(
                    "{}\n\n[{}] {}",
                    call.arguments["answer"].as_str().expect("validated answer"),
                    label,
                    verification.summary
                );
                match self.ai.finish_native_run(verification, answer.clone()) {
                    Ok(()) => {
                        self.record_native_control_audit("final_answer", true, cx);
                        self.upsert_ai_agent_step(
                            index,
                            crate::features::runtime_jobs::AiAgentStepStatus::Completed,
                            AiAgentStepKind::FinalAnswer,
                            "Final Answer",
                            answer,
                        );
                        self.clear_capability_owner(&format!("native:{run_id}"), cx);
                        self.persist_native_chat_projection(cx);
                    }
                    Err(error) => self.complete_native_control(&run_id, &call.id, Err(error), cx),
                }
            }
            _ => {
                let (reply, result) = tokio::sync::oneshot::channel();
                let ai_session_id = self.ai.chat_session_id().to_string();
                let tool_kind = call.tool;
                let call_id = call.id.clone();
                let callback_run = run_id.clone();
                let callback_session = ai_session_id.clone();
                cx.spawn(async move |this, cx| {
                    let result = result.await.unwrap_or_else(|_| {
                        Err(RpcError {
                            code: "host_unavailable".into(),
                            message: "Capability request was closed.".into(),
                        })
                    });
                    let _ = this.update(cx, |this, cx| {
                        let Some(scope) = this.ai.scope_for_ai_session(&callback_session) else {
                            return;
                        };
                        let visible = this.ai.active_scope_key().to_string();
                        this.ai.switch_scope(&scope);
                        let (value, is_error) = match result {
                            Ok(value) => (value, false),
                            Err(error) => {
                                (json!({"code":error.code,"message":error.message}), true)
                            }
                        };
                        let uncertain_execution = tool_kind == AgentTool::TerminalExecute
                            && (value["exitCode"].is_null()
                                || value["timedOut"] == true
                                || value["sourceTruncated"] == true);
                        if this.ai.complete_native_call(
                            &callback_run,
                            AgentToolResult {
                                call_id,
                                value: value.clone(),
                                is_error,
                            },
                        ) {
                            this.upsert_ai_agent_step(
                                this.ai.last_agent_step_index(),
                                if is_error || uncertain_execution {
                                    crate::features::runtime_jobs::AiAgentStepStatus::Failed
                                } else {
                                    crate::features::runtime_jobs::AiAgentStepStatus::Completed
                                },
                                if tool_kind == AgentTool::TerminalExecute {
                                    AiAgentStepKind::Command
                                } else {
                                    AiAgentStepKind::ToolProgress
                                },
                                "Tool result",
                                zzclawterm_core::truncate_preview(&value.to_string(), 500),
                            );
                            this.ai.record_native_tool_output(&value);
                            this.persist_native_chat_projection(cx);
                            this.start_native_agent_continuation(cx);
                        }
                        this.ai.switch_scope(&visible);
                        this.defer_ai_panel_snapshot_flush(cx);
                    });
                })
                .detach();
                self.dispatch_capability_request(
                    CapabilityRequest {
                        connection_id: format!("native:{run_id}"),
                        request_id: format!("native:{run_id}:{}", call.id),
                        generation: String::new(),
                        client: "ZzClawTerm Native Agent".into(),
                        permission_mode: zzclawterm_core::AiPermissionMode::Confirm,
                        scope,
                        tool: call.tool.name().into(),
                        arguments: call.arguments,
                        cancellation,
                        approved: false,
                        approval_decision: None,
                        caller: CapabilityCaller::Native {
                            run_id,
                            ai_session_id,
                            model_risk: call.model_risk,
                        },
                        reply,
                    },
                    cx,
                );
            }
        }
        self.defer_ai_panel_snapshot_flush(cx);
    }

    fn complete_native_control(
        &mut self,
        run_id: &str,
        call_id: &str,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let (value, is_error) = match result {
            Ok(value) => (value, false),
            Err(error) => (json!({"code":"invalid_argument","message":error}), true),
        };
        if let Some(call) = self.ai.native_pending_call() {
            self.record_native_control_audit(call.tool.name(), !is_error, cx);
        }
        if self.ai.complete_native_call(
            run_id,
            AgentToolResult {
                call_id: call_id.into(),
                value,
                is_error,
            },
        ) {
            self.upsert_ai_agent_step(
                self.ai.last_agent_step_index(),
                if is_error {
                    crate::features::runtime_jobs::AiAgentStepStatus::Failed
                } else {
                    crate::features::runtime_jobs::AiAgentStepStatus::Completed
                },
                AiAgentStepKind::ToolProgress,
                "Tool result",
                if is_error {
                    "Control tool rejected"
                } else {
                    "Control tool completed"
                },
            );
            self.persist_native_chat_projection(cx);
            self.start_native_agent_continuation(cx);
        }
    }

    pub(in crate::features) fn start_native_agent_continuation(&mut self, cx: &mut Context<Self>) {
        let Some((launch, request)) = self.ai.begin_native_continuation() else {
            return;
        };
        let agent_history = self.ai.native_initial_history();
        let store = self.store_blocking_client();
        let settings = self.ai.settings_config_cloned();
        let job_id = launch.job_id;
        let session_id = launch.session_id;
        let tx = launch.tx;
        let reject_tx = tx.clone();
        let reject_session = session_id.clone();
        if let Err(error) = self.blocking_jobs.submit_detached(
            "native-agent-continuation",
            move |scheduler_cancel| {
                let result = if scheduler_cancel.is_cancelled() {
                    Err("Agent cancelled".into())
                } else {
                    run_ai_ask_job(
                        store,
                        settings,
                        request,
                        AiJobRunOptions {
                            mcp_credential: None,
                            stream_tx: Some(tx.clone()),
                            cancel: launch.cancel,
                            job_id,
                            agent_history,
                            persist_user_message: false,
                        },
                    )
                };
                let _ = tx.unbounded_send(AiChatWorkerEvent::Finished(AiChatJobResult {
                    job_id,
                    session_id,
                    result,
                }));
            },
        ) {
            let _ = reject_tx.unbounded_send(AiChatWorkerEvent::Finished(AiChatJobResult {
                job_id,
                session_id: reject_session,
                result: Err(error.to_string()),
            }));
        }
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn submit_native_agent_answers(
        &mut self,
        run_id: &str,
        call_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(answers) = self.ai.native_answers(run_id, call_id) else {
            return;
        };
        self.record_native_control_audit("request_user_input", true, cx);
        self.forget_text_inputs(&format!("ai.agent-answer.{run_id}."));
        if self.ai.complete_native_call(
            run_id,
            AgentToolResult {
                call_id: call_id.into(),
                value: answers,
                is_error: false,
            },
        ) {
            self.upsert_ai_agent_step(
                self.ai.last_agent_step_index(),
                crate::features::runtime_jobs::AiAgentStepStatus::Completed,
                AiAgentStepKind::ToolProgress,
                t!("ai.harness.submitAnswers"),
                String::new(),
            );
            self.start_native_agent_continuation(cx);
            self.defer_ai_panel_snapshot_flush(cx);
        }
    }

    pub(in crate::features) fn submit_native_answer_input(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.ai.native_run_view() else {
            return;
        };
        let Some(call_id) = view.call_id else {
            return;
        };
        if view.questions.iter().enumerate().any(|(index, _)| {
            id == super::panel::harness::answer_input_id(&view.run_id, &call_id, index)
        }) {
            self.submit_native_agent_answers(&view.run_id, &call_id, cx);
        }
    }

    pub(in crate::features) fn apply_native_answer_input(
        &mut self,
        id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.ai.native_run_view() else {
            return;
        };
        let Some(call_id) = view.call_id else {
            return;
        };
        for (index, question) in view.questions.iter().enumerate() {
            if id == super::panel::harness::answer_input_id(&view.run_id, &call_id, index) {
                self.apply_native_agent_answer(&view.run_id, &call_id, &question.id, text, cx);
                return;
            }
        }
    }

    pub(in crate::features) fn apply_native_agent_answer(
        &mut self,
        run_id: &str,
        call_id: &str,
        question_id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let accepted = self
            .ai
            .set_native_answer(run_id, call_id, question_id, text);
        if accepted {
            self.defer_ai_panel_snapshot_flush(cx);
        }
        accepted
    }

    pub(in crate::features) fn dispatch_native_terminal_execute(
        &mut self,
        request: CapabilityRequest,
        scope: &CapabilityScopeSnapshot,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_id) = request.arguments.get("sessionId").and_then(Value::as_str)
            && self.ai.scope_for_agent_target(session_id).is_some()
        {
            let _ = request.reply.send(Err(RpcError {
                code: "target_busy".into(),
                message: "Another Agent command is observing this target.".into(),
            }));
            return;
        }
        let args: TerminalExecuteArgs =
            serde_json::from_value(request.arguments).expect("validated terminal arguments");
        let session_id = scope
            .resolve_session(args.session_id.as_deref())
            .expect("validated scope");
        let call = self.ai.native_pending_call().expect("live native call");
        let local_risk = zzclawterm_core::capabilities::assess_command_risk(&args.command).level;
        let risk = call
            .model_risk
            .unwrap_or(local_risk.clone())
            .max(local_risk);
        let card = AiCommandCard {
            id: call.id,
            title: "Agent Command".into(),
            command: args.command,
            explanation: call.thought,
            risk_level: Some(risk),
            risk_reason: None,
            expected_effect: "Run the next Agent tool".into(),
            rollback: None,
            category: Some("AI Agent".into()),
            references: Vec::new(),
            target_terminal_session_id: Some(session_id.clone()),
            target: self
                .ai_terminal_targets_for_sessions(std::slice::from_ref(&session_id))
                .into_iter()
                .next(),
        };
        self.ai.present_native_command(card.clone());
        self.ai.register_native_terminal_reply(request.reply);
        let result = if self.ai.settings_config().agent_background_execution_enabled {
            self.begin_ai_agent_background_execution(&card, &session_id, cx)
        } else {
            self.begin_ai_agent_observation(&card, &session_id, cx)
                .map(|wrapped| {
                    let input = wrapped.unwrap_or_else(|| format!("{}\r", card.command));
                    self.send_terminal_input_to_session(session_id, input.into_bytes(), cx);
                })
        };
        if result.is_err() {
            self.ai.complete_native_terminal(Err(RpcError {
                code: "execution_failed".into(),
                message: "Cannot start terminal execution.".into(),
            }));
        }
    }

    pub(in crate::features) fn complete_native_terminal_observation(
        &mut self,
        observation: &CommandObservation,
        timed_out: bool,
        source_truncated: bool,
    ) -> bool {
        let result = serde_json::to_value(TerminalExecuteResult {
            output: observation.output.clone(),
            exit_code: observation.exit_code,
            duration_ms: observation.duration_ms,
            timed_out,
            source_truncated,
        })
        .expect("terminal result");
        self.ai.complete_native_terminal(Ok(result))
    }
}
