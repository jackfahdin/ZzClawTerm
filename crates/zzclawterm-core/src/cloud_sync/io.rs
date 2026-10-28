//! Optional transport capabilities. Validators are opaque, never content hashes.

#[derive(Clone, Default, PartialEq, Eq)]
pub struct CloudObjectMetadata {
    pub path: String,
    pub etag: Option<String>,
    pub size: Option<u64>,
}

pub enum CloudObjectRead {
    Missing,
    Modified {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
    NotModified,
}

#[derive(Debug, Clone, Copy)]
pub enum CloudSyncIoEvent {
    FullValidation,
    CacheHit,
    HeadsChecked,
}

/// Application body bytes, excluding HTTP headers and TLS overhead.
#[derive(Debug, Default, Clone, Copy)]
pub struct CloudSyncIoStats {
    pub requests: u64,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub not_modified: u64,
    pub cache_hits: u64,
    pub full_validations: u64,
}
