use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_OUTPUT_ENTRY_LIMIT: usize = 4 * 1024 * 1024;
pub const DEFAULT_OUTPUT_STORE_LIMIT: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT_CHUNK_BYTES: usize = 64 * 1024;
pub const DEFAULT_OUTPUT_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputStoreConfig {
    pub entry_limit: usize,
    pub total_limit: usize,
    pub ttl: Duration,
    pub max_chunk_bytes: usize,
}

impl Default for OutputStoreConfig {
    fn default() -> Self {
        Self {
            entry_limit: DEFAULT_OUTPUT_ENTRY_LIMIT,
            total_limit: DEFAULT_OUTPUT_STORE_LIMIT,
            ttl: DEFAULT_OUTPUT_TTL,
            max_chunk_bytes: MAX_OUTPUT_CHUNK_BYTES,
        }
    }
}

struct Entry {
    bytes: Vec<u8>,
    total_bytes: usize,
    created: Instant,
}

pub struct OutputStore {
    entries: HashMap<String, Entry>,
    lru: VecDeque<String>,
    bytes: usize,
    config: OutputStoreConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedOutput {
    pub preview: String,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
    pub total_bytes: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
    pub data: String,
    pub offset: usize,
    pub next_offset: usize,
    pub total_bytes: usize,
    pub eof: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OutputStoreError {
    #[error("output is unavailable or has expired")]
    Unavailable,
    #[error("invalid output offset")]
    InvalidOffset,
}

impl Default for OutputStore {
    fn default() -> Self {
        Self::new(OutputStoreConfig::default())
    }
}

impl OutputStore {
    pub fn new(config: OutputStoreConfig) -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            bytes: 0,
            config: OutputStoreConfig {
                entry_limit: config.entry_limit.max(1),
                total_limit: config.total_limit.max(1),
                ttl: config.ttl,
                max_chunk_bytes: config.max_chunk_bytes.max(1),
            },
        }
    }

    pub fn protect(&mut self, text: String, inline_limit: usize) -> ProtectedOutput {
        self.cleanup();
        let total_bytes = text.len();
        if total_bytes <= inline_limit {
            return ProtectedOutput {
                preview: text,
                truncated: false,
                output_id: None,
                total_bytes,
                source_truncated: false,
            };
        }
        let preview_end = boundary_at_or_before(&text, inline_limit);
        let preview = text[..preview_end].to_string();
        let keep_end = boundary_at_or_before(&text, self.config.entry_limit.min(text.len()));
        let output_id = format!("out_{}", uuid::Uuid::new_v4());
        self.insert(
            output_id.clone(),
            text.as_bytes()[..keep_end].to_vec(),
            total_bytes,
        );
        ProtectedOutput {
            preview,
            truncated: true,
            output_id: Some(output_id),
            total_bytes,
            source_truncated: keep_end < total_bytes,
        }
    }

    pub fn read(
        &mut self,
        output_id: &str,
        offset: usize,
        max_bytes: usize,
    ) -> Result<OutputChunk, OutputStoreError> {
        self.cleanup();
        let entry = self
            .entries
            .get(output_id)
            .ok_or(OutputStoreError::Unavailable)?;
        if offset > entry.bytes.len() {
            return Err(OutputStoreError::InvalidOffset);
        }
        let text =
            std::str::from_utf8(&entry.bytes).map_err(|_| OutputStoreError::InvalidOffset)?;
        if !text.is_char_boundary(offset) {
            return Err(OutputStoreError::InvalidOffset);
        }
        let requested = max_bytes.clamp(1, self.config.max_chunk_bytes);
        let end = boundary_at_or_before(text, offset.saturating_add(requested).min(text.len()));
        let chunk = OutputChunk {
            data: text[offset..end].to_string(),
            offset,
            next_offset: end,
            total_bytes: entry.total_bytes,
            eof: end == entry.bytes.len(),
        };
        self.lru.retain(|id| id != output_id);
        self.lru.push_back(output_id.to_string());
        Ok(chunk)
    }

    pub fn cleanup(&mut self) {
        let ttl = self.config.ttl;
        let expired = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.created.elapsed() >= ttl)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            self.remove(&id);
        }
        self.lru.retain(|id| self.entries.contains_key(id));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn insert(&mut self, id: String, bytes: Vec<u8>, total_bytes: usize) {
        while self.bytes.saturating_add(bytes.len()) > self.config.total_limit {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            self.remove(&oldest);
        }
        // A deliberately tiny total limit can be smaller than one configured
        // entry. Keep only the suffix allowed by both limits rather than
        // violating the connection-wide capacity invariant.
        let keep = bytes.len().min(self.config.total_limit);
        let keep = boundary_at_or_before_bytes(&bytes, keep);
        let bytes = bytes[..keep].to_vec();
        self.bytes += bytes.len();
        self.lru.push_back(id.clone());
        self.entries.insert(
            id,
            Entry {
                bytes,
                total_bytes,
                created: Instant::now(),
            },
        );
    }

    fn remove(&mut self, id: &str) {
        if let Some(entry) = self.entries.remove(id) {
            self.bytes = self.bytes.saturating_sub(entry.bytes.len());
        }
        self.lru.retain(|candidate| candidate != id);
    }
}

fn boundary_at_or_before(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn boundary_at_or_before_bytes(bytes: &[u8], index: usize) -> usize {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    boundary_at_or_before(text, index)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{OutputStore, OutputStoreConfig, OutputStoreError};

    fn config(entry_limit: usize, total_limit: usize) -> OutputStoreConfig {
        OutputStoreConfig {
            entry_limit,
            total_limit,
            ttl: Duration::from_secs(60),
            max_chunk_bytes: 16,
        }
    }

    #[test]
    fn protects_and_pages_utf8_without_splitting_characters() {
        let mut store = OutputStore::new(config(128, 256));
        let protected = store.protect("猫".repeat(100), 10);
        assert!(protected.truncated);
        assert!(protected.source_truncated);
        assert!(protected.preview.is_char_boundary(protected.preview.len()));

        let id = protected.output_id.unwrap();
        let first = store.read(&id, 0, 13).unwrap();
        assert!(!first.data.is_empty());
        assert!(first.next_offset <= 13);
        let second = store.read(&id, first.next_offset, 64).unwrap();
        assert_eq!(second.offset, first.next_offset);
        assert!(store.read(&id, 1, 10).is_err());
    }

    #[test]
    fn inline_boundary_and_store_ids_are_connection_local() {
        let mut first = OutputStore::new(config(128, 256));
        assert!(first.protect("x".repeat(16), 16).output_id.is_none());
        let id = first.protect("x".repeat(17), 16).output_id.unwrap();
        let mut second = OutputStore::new(config(128, 256));
        assert_eq!(second.read(&id, 0, 1), Err(OutputStoreError::Unavailable));
        assert_eq!(first.read(&id, 0, 1).unwrap().data, "x");
    }

    #[test]
    fn evicts_lru_and_expires_entries() {
        let mut store = OutputStore::new(config(8, 16));
        let first = store.protect("a".repeat(9), 1).output_id.unwrap();
        let second = store.protect("b".repeat(9), 1).output_id.unwrap();
        store.read(&first, 0, 1).unwrap();
        let third = store.protect("c".repeat(9), 1).output_id.unwrap();
        assert!(store.read(&second, 0, 1).is_err());
        assert!(store.read(&first, 0, 1).is_ok());
        assert!(store.read(&third, 0, 1).is_ok());

        let mut expiring = OutputStore::new(OutputStoreConfig {
            ttl: Duration::ZERO,
            ..config(8, 16)
        });
        let id = expiring.protect("expired".repeat(2), 1).output_id.unwrap();
        assert_eq!(expiring.read(&id, 0, 1), Err(OutputStoreError::Unavailable));
        assert!(expiring.is_empty());
    }
}
