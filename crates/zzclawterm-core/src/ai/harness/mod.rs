//! Native agent control state and adapters around the shared capability contract.

mod registry;
mod run;
pub mod transcript;

pub use registry::{AgentTool, AgentToolCall, AgentToolRegistry};
pub use run::{
    AgentPlan, AgentPlanTask, AgentQuestion, AgentRequestContext, AgentRun, AgentRunStatus,
    AgentTaskStatus, AgentToolResult, AgentVerification, AgentVerificationStatus,
};
