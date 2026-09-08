use std::collections::HashMap;

use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};

use super::{
    ConnectionStore, LEGACY_TEXT_REMOTE_FILE_BACKEND_CACHE, SETTINGS_REMOTE_FILE_BACKEND_CACHE,
    SETTINGS_TABLE, StorageError, TEXT_DOCS_TABLE, current_time_ms, deserialize_json,
    write_json_in_txn,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RemoteFileBackendCache {
    #[serde(default)]
    pub entries: HashMap<String, RemoteFileBackendCacheEntry>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteFileBackendCacheEntry {
    pub last_working_backend: String,
    #[serde(default)]
    pub sftp_unavailable: bool,
    #[serde(default)]
    pub last_failure_reason: Option<String>,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl ConnectionStore {
    pub fn load_remote_file_backend_cache(&self) -> Result<RemoteFileBackendCache, StorageError> {
        let txn = self.db.begin_read()?;
        if let Ok(table) = txn.open_table(SETTINGS_TABLE)
            && let Some(raw) = table.get(SETTINGS_REMOTE_FILE_BACKEND_CACHE)?
        {
            return deserialize_json(raw.value());
        }
        if let Ok(table) = txn.open_table(TEXT_DOCS_TABLE)
            && let Some(raw) = table.get(LEGACY_TEXT_REMOTE_FILE_BACKEND_CACHE)?
        {
            return serde_json::from_str(raw.value()).map_err(StorageError::from);
        }
        Ok(RemoteFileBackendCache::default())
    }

    pub fn update_remote_file_backend_cache_entry(
        &self,
        key: &str,
        backend: &str,
        sftp_unavailable: bool,
        failure_reason: Option<String>,
    ) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        let mut cache = {
            let settings = txn.open_table(SETTINGS_TABLE)?;
            if let Some(raw) = settings.get(SETTINGS_REMOTE_FILE_BACKEND_CACHE)? {
                deserialize_json(raw.value())?
            } else {
                drop(settings);
                let legacy = txn.open_table(TEXT_DOCS_TABLE)?;
                match legacy.get(LEGACY_TEXT_REMOTE_FILE_BACKEND_CACHE)? {
                    Some(raw) => serde_json::from_str(raw.value())?,
                    None => RemoteFileBackendCache::default(),
                }
            }
        };
        let previous_extra = cache
            .entries
            .get(key)
            .map(|entry| entry.extra.clone())
            .unwrap_or_default();
        cache.entries.insert(
            key.to_string(),
            RemoteFileBackendCacheEntry {
                last_working_backend: backend.to_string(),
                sftp_unavailable,
                last_failure_reason: failure_reason,
                updated_at: current_time_ms() / 1_000,
                extra: previous_extra,
            },
        );
        write_json_in_txn(
            &txn,
            SETTINGS_TABLE,
            SETTINGS_REMOTE_FILE_BACKEND_CACHE,
            &cache,
        )?;
        txn.commit()?;
        Ok(())
    }
}
