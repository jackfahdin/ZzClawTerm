//! UI-independent capability authorization, scope and bounded output policy.

mod output_store;
mod policy;
mod scope;
mod sftp;

pub use output_store::{
    DEFAULT_OUTPUT_ENTRY_LIMIT, DEFAULT_OUTPUT_STORE_LIMIT, DEFAULT_OUTPUT_TTL,
    MAX_OUTPUT_CHUNK_BYTES, OutputChunk, OutputStore, OutputStoreConfig, OutputStoreError,
    ProtectedOutput,
};
pub use policy::{PolicyDecision, RiskAssessment, assess_command_risk, decide_policy};
pub use scope::{
    CapabilityScope, CapabilityScopeError, CapabilityScopeSnapshot, CapabilitySession,
};
pub use sftp::{SftpRiskOperation, assess_sftp_risk};

pub use nyaterm_mcp_protocol::CapabilityAccess;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityDefinition {
    pub id: &'static str,
    pub access: CapabilityAccess,
    pub requires_session: bool,
}
