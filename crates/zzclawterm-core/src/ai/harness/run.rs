use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::capabilities::CapabilityScope;

use super::registry::{AgentTool, AgentToolCall, AgentToolRegistry};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentPlanTask {
    pub id: String,
    pub description: String,
    pub status: AgentTaskStatus,
    #[serde(default)]
    pub verification: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentPlan {
    pub tasks: Vec<AgentPlanTask>,
}

impl AgentPlan {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        if self.tasks.len() > 100 {
            return Err("plan exceeds 100 tasks".into());
        }
        for task in &self.tasks {
            if task.id.trim().is_empty()
                || task.id.len() > 128
                || !ids.insert(&task.id)
                || task.description.trim().is_empty()
            {
                return Err("plan requires unique ids and nonempty descriptions".into());
            }
        }
        if self
            .tasks
            .iter()
            .filter(|task| task.status == AgentTaskStatus::InProgress)
            .count()
            > 1
        {
            return Err("only one plan task may be in progress".into());
        }
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.tasks
            .iter()
            .all(|task| task.status == AgentTaskStatus::Completed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentQuestion {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

impl AgentQuestion {
    pub fn validate_all(questions: &[Self]) -> Result<(), String> {
        let mut ids = HashSet::new();
        if !(1..=3).contains(&questions.len()) {
            return Err("ask one to three questions".into());
        }
        for question in questions {
            if question.id.trim().is_empty()
                || question.id.len() > 128
                || !ids.insert(&question.id)
                || question.question.trim().is_empty()
            {
                return Err("questions require unique ids and nonempty text".into());
            }
            if let Some(options) = &question.options {
                let mut seen = HashSet::new();
                if !(2..=6).contains(&options.len())
                    || options
                        .iter()
                        .any(|option| option.trim().is_empty() || !seen.insert(option))
                {
                    return Err("provide two to six distinct options or omit options".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentVerificationStatus {
    Verified,
    Unverified,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentVerification {
    pub status: AgentVerificationStatus,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunStatus {
    Planning,
    Running,
    NeedsApproval,
    WaitingForUser,
    Completed,
    Unverified,
    Blocked,
    Failed,
    Cancelled,
}

impl AgentRunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Unverified | Self::Blocked | Self::Failed | Self::Cancelled
        )
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AgentToolResult {
    pub call_id: String,
    pub value: Value,
    pub is_error: bool,
}

impl std::fmt::Debug for AgentToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentToolResult")
            .field("call_id", &self.call_id)
            .field("is_error", &self.is_error)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequestContext {
    pub run_id: String,
    pub calls: Vec<(AgentToolCall, AgentToolResult)>,
    pub remaining_steps: u16,
}

pub struct AgentRun {
    pub id: String,
    pub objective: String,
    pub scope: CapabilityScope,
    pub status: AgentRunStatus,
    pub plan: AgentPlan,
    pub final_verification: Option<AgentVerification>,
    pub max_steps: u16,
    pub used_steps: u16,
    pub pending_call: Option<AgentToolCall>,
    pub questions: Vec<AgentQuestion>,
    pub records: Vec<(AgentToolCall, AgentToolResult)>,
    seen_calls: HashSet<String>,
}

impl AgentRun {
    pub fn new(objective: String, scope: CapabilityScope, max_steps: u16) -> Self {
        Self {
            id: format!("run-{}", uuid::Uuid::new_v4()),
            objective,
            scope,
            status: AgentRunStatus::Planning,
            plan: AgentPlan::default(),
            final_verification: None,
            max_steps,
            used_steps: 0,
            pending_call: None,
            questions: Vec::new(),
            records: Vec::new(),
            seen_calls: HashSet::new(),
        }
    }

    pub fn begin_call(&mut self, call: AgentToolCall) -> Result<(), String> {
        if self.status.is_terminal() || self.pending_call.is_some() {
            return Err("run is not ready for a tool call".into());
        }
        if self.seen_calls.contains(&call.id) {
            return Err("duplicate agent call".into());
        }
        AgentToolRegistry::validate(&call)?;
        if call.tool != AgentTool::FinalAnswer && self.used_steps >= self.max_steps {
            return Err("Agent reached its step limit; provide a final summary".into());
        }
        if call.tool != AgentTool::FinalAnswer {
            self.used_steps = self.used_steps.saturating_add(1);
        }
        self.seen_calls.insert(call.id.clone());
        self.pending_call = Some(call);
        self.status = AgentRunStatus::Running;
        Ok(())
    }

    pub fn complete_call(&mut self, result: AgentToolResult) -> bool {
        if self.status.is_terminal()
            || self
                .pending_call
                .as_ref()
                .is_none_or(|call| call.id != result.call_id)
        {
            return false;
        }
        let call = self.pending_call.take().expect("matching pending call");
        self.records.push((call, result));
        self.questions.clear();
        self.status = AgentRunStatus::Planning;
        true
    }

    /// Return an over-quota decision to the model without enabling another operation.
    pub fn reject_over_limit(&mut self, call: AgentToolCall) -> bool {
        if self.status.is_terminal()
            || self.pending_call.is_some()
            || self.used_steps < self.max_steps
            || call.tool == AgentTool::FinalAnswer
            || self.seen_calls.contains(&call.id)
            || AgentToolRegistry::validate(&call).is_err()
        {
            return false;
        }
        self.seen_calls.insert(call.id.clone());
        self.records.push((call.clone(), AgentToolResult {
            call_id: call.id,
            value: serde_json::json!({"code":"step_limit","message":"No operations remain; use final_answer with the remaining work and verification limitations."}),
            is_error: true,
        }));
        self.status = AgentRunStatus::Planning;
        true
    }

    pub fn update_plan(&mut self, plan: AgentPlan) -> Result<(), String> {
        plan.validate()?;
        // Removing completed or unfinished tasks would hide remaining work.
        if self
            .plan
            .tasks
            .iter()
            .any(|task| !plan.tasks.iter().any(|next| next.id == task.id))
        {
            return Err("existing plan task ids must be retained".into());
        }
        self.plan = plan;
        Ok(())
    }

    pub fn wait_for_user(&mut self, questions: Vec<AgentQuestion>) -> Result<(), String> {
        AgentQuestion::validate_all(&questions)?;
        if self
            .pending_call
            .as_ref()
            .is_none_or(|call| call.tool != AgentTool::RequestUserInput)
        {
            return Err("no pending question call".into());
        }
        self.questions = questions;
        self.status = AgentRunStatus::WaitingForUser;
        Ok(())
    }

    pub fn finish(&mut self, verification: AgentVerification) -> Result<(), String> {
        if self
            .pending_call
            .as_ref()
            .is_none_or(|call| call.tool != AgentTool::FinalAnswer)
        {
            return Err("no pending final answer".into());
        }
        if verification.status == AgentVerificationStatus::Verified && !self.plan.is_complete() {
            return Err("unfinished plan cannot be marked verified".into());
        }
        let call_id = self
            .pending_call
            .as_ref()
            .expect("pending final")
            .id
            .clone();
        self.complete_call(AgentToolResult {
            call_id,
            value: serde_json::json!({"finished":true}),
            is_error: false,
        });
        self.status = match verification.status {
            AgentVerificationStatus::Verified => AgentRunStatus::Completed,
            AgentVerificationStatus::Unverified => AgentRunStatus::Unverified,
            AgentVerificationStatus::Blocked => AgentRunStatus::Blocked,
        };
        self.final_verification = Some(verification);
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.status = AgentRunStatus::Cancelled;
        self.pending_call = None;
        self.questions.clear();
    }
    pub fn request_context(&self) -> AgentRequestContext {
        AgentRequestContext {
            run_id: self.id.clone(),
            calls: self.records.clone(),
            remaining_steps: self.max_steps.saturating_sub(self.used_steps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentRun, AgentRunStatus, AgentToolResult, AgentVerification, AgentVerificationStatus,
    };
    use crate::ai::AiToolCall;
    use crate::ai::harness::AgentToolRegistry;
    use crate::capabilities::CapabilityScope;
    use serde_json::json;

    fn call(name: &str, args: serde_json::Value, id: &str) -> crate::ai::harness::AgentToolCall {
        AgentToolRegistry::parse(
            &[AiToolCall {
                thought_signature: None,
                id: Some(id.into()),
                name: name.into(),
                arguments: args,
            }],
            "",
        )
        .unwrap()
    }

    #[test]
    fn questions_pause_and_resume_once_in_the_same_run() {
        let mut run = AgentRun::new(
            "diagnose".into(),
            CapabilityScope::explicit(["s".into()], Some("s".into())),
            2,
        );
        let question = call(
            "request_user_input",
            json!({"questions":[{"id":"host","question":"Which host?"}]}),
            "q",
        );
        run.begin_call(question.clone()).unwrap();
        run.wait_for_user(serde_json::from_value(question.arguments["questions"].clone()).unwrap())
            .unwrap();
        assert_eq!(run.status, AgentRunStatus::WaitingForUser);
        let answer = AgentToolResult {
            call_id: "q".into(),
            value: json!({"host":"staging"}),
            is_error: false,
        };
        assert!(run.complete_call(answer.clone()));
        assert!(!run.complete_call(answer));
        assert_eq!(run.used_steps, 1);
        assert_eq!(run.request_context().calls.len(), 1);
        assert!(run.begin_call(question).is_err());
    }

    #[test]
    fn step_limit_allows_a_final_summary_but_not_another_operation() {
        let mut run = AgentRun::new("task".into(), CapabilityScope::AllSessions, 0);
        assert!(
            run.begin_call(call("get_environment", json!({}), "read"))
                .is_err()
        );
        run.begin_call(call(
            "final_answer",
            json!({"answer":"limit reached"}),
            "final",
        ))
        .unwrap();
        run.finish(AgentVerification {
            status: AgentVerificationStatus::Unverified,
            summary: "limit reached".into(),
        })
        .unwrap();
        assert_eq!(run.status, AgentRunStatus::Unverified);
    }

    #[test]
    fn cancellation_discards_late_tool_results() {
        let mut run = AgentRun::new("task".into(), CapabilityScope::AllSessions, 2);
        run.begin_call(call("get_environment", json!({}), "read"))
            .unwrap();
        run.cancel();
        assert!(!run.complete_call(AgentToolResult {
            call_id: "read".into(),
            value: json!({}),
            is_error: false
        }));
        assert_eq!(run.status, AgentRunStatus::Cancelled);
    }

    #[test]
    fn verified_completion_requires_finished_plan_and_plan_edits_retain_task_ids() {
        let mut run = AgentRun::new("task".into(), CapabilityScope::AllSessions, 4);
        let plan: super::AgentPlan = serde_json::from_value(
            json!({"tasks":[{"id":"check","description":"Check service","status":"in_progress"}]}),
        )
        .unwrap();
        run.update_plan(plan).unwrap();
        assert!(run.update_plan(super::AgentPlan::default()).is_err());
        run.begin_call(call(
            "final_answer",
            json!({"answer":"done","verification":{"status":"verified","summary":"verified"}}),
            "final",
        ))
        .unwrap();
        assert!(
            run.finish(AgentVerification {
                status: AgentVerificationStatus::Verified,
                summary: "verified".into()
            })
            .is_err()
        );
        assert_eq!(run.plan.tasks.len(), 1);
        run.finish(AgentVerification {
            status: AgentVerificationStatus::Blocked,
            summary: "service unavailable".into(),
        })
        .unwrap();
        assert_eq!(run.status, AgentRunStatus::Blocked);
    }

    #[test]
    fn plans_and_questions_reject_ambiguous_ids_and_concurrent_current_tasks() {
        let plan: super::AgentPlan = serde_json::from_value(json!({"tasks":[
            {"id":"a","description":"first","status":"in_progress"},
            {"id":"b","description":"second","status":"in_progress"}
        ]}))
        .unwrap();
        assert!(plan.validate().is_err());
        assert!(AgentToolRegistry::parse(&[],r#"{"action":"request_user_input","arguments":{"questions":[{"id":"a","question":"first"},{"id":"a","question":"second"}]}}"#).is_err());
    }

    #[test]
    fn query_read_question_execute_verify_summary_flow_keeps_one_run_and_distinct_records() {
        let mut run = AgentRun::new(
            "inspect and repair service".into(),
            CapabilityScope::explicit(["s".into()], Some("s".into())),
            7,
        );
        let id = run.id.clone();
        let plan: super::AgentPlan = serde_json::from_value(json!({"tasks":[{"id":"repair","description":"Inspect, repair and verify","status":"in_progress"}]})).unwrap();
        run.begin_call(call(
            "update_plan",
            serde_json::to_value(&plan).unwrap(),
            "plan",
        ))
        .unwrap();
        run.update_plan(plan.clone()).unwrap();
        assert!(run.complete_call(AgentToolResult {
            call_id: "plan".into(),
            value: json!({"updated":true}),
            is_error: false
        }));
        for (name, args, call_id, result) in [
            (
                "get_environment",
                json!({}),
                "query",
                json!({"sessions":[{"id":"s"}]}),
            ),
            (
                "sftp_read_text",
                json!({"sessionId":"s","path":"/etc/service.conf"}),
                "read",
                json!({"content":"enabled=false"}),
            ),
        ] {
            run.begin_call(call(name, args, call_id)).unwrap();
            assert!(run.complete_call(AgentToolResult {
                call_id: call_id.into(),
                value: result,
                is_error: false
            }));
        }
        let question = call(
            "request_user_input",
            json!({"questions":[{"id":"repair","question":"Enable the service?","options":["yes","no"]}]}),
            "ask",
        );
        run.begin_call(question.clone()).unwrap();
        run.wait_for_user(serde_json::from_value(question.arguments["questions"].clone()).unwrap())
            .unwrap();
        assert!(run.complete_call(AgentToolResult {
            call_id: "ask".into(),
            value: json!({"answers":{"repair":"yes"}}),
            is_error: false
        }));
        for (call_id, command) in [("execute", "service start"), ("verify", "service status")] {
            run.begin_call(call(
                "terminal_execute",
                json!({"sessionId":"s","command":command}),
                call_id,
            ))
            .unwrap();
            assert!(run.complete_call(AgentToolResult{call_id:call_id.into(),value:json!({"output":"running","exitCode":0,"timedOut":false,"sourceTruncated":false}),is_error:false}));
        }
        let mut completed = plan;
        completed.tasks[0].status = super::AgentTaskStatus::Completed;
        completed.tasks[0].verification = Some("Status check returned running, exit 0".into());
        run.begin_call(call(
            "update_plan",
            serde_json::to_value(&completed).unwrap(),
            "verified-plan",
        ))
        .unwrap();
        run.update_plan(completed).unwrap();
        assert!(run.complete_call(AgentToolResult {
            call_id: "verified-plan".into(),
            value: json!({"updated":true}),
            is_error: false
        }));
        run.begin_call(call("final_answer",json!({"answer":"Service running","verification":{"status":"verified","summary":"status returned running, exit 0"}}),"summary")).unwrap();
        run.finish(AgentVerification {
            status: AgentVerificationStatus::Verified,
            summary: "status returned running, exit 0".into(),
        })
        .unwrap();
        assert_eq!(run.id, id);
        assert_eq!(run.records.len(), 8);
        assert_eq!(run.used_steps, 7);
        assert_eq!(run.status, AgentRunStatus::Completed);
    }
}
