use serde::{Deserialize, Serialize};

/// Tauri-compatible VNC trust record, separate from SSH known hosts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VncKnownHostRecord {
    pub host: String,
    pub port: u16,
    pub sha256_fingerprint: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(flatten)]
    pub extensions: std::collections::BTreeMap<String, serde_json::Value>,
}
