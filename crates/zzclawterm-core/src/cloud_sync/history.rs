use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::{CloudSyncError, current_time_ms, uuid_v4};

pub const CLOUD_SYNC_HISTORY_DOMAIN: &str = "cloud_sync.history";
pub const CLOUD_SYNC_HISTORY_EVENT: &str = "entry";
pub const CLOUD_SYNC_HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSyncHistoryEntry {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub timestamp_ms: u64,
    pub kind: String,
    pub status: String,
    pub trigger: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    pub message: String,
}

impl CloudSyncHistoryEntry {
    pub fn sync(
        status: impl Into<String>,
        trigger: impl Into<String>,
        provider: Option<String>,
        revision: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid_v4(),
            timestamp_ms: current_time_ms(),
            kind: "sync".to_string(),
            status: status.into(),
            trigger: trigger.into(),
            provider,
            revision,
            duration_ms: None,
            message: message.into(),
        }
    }
}

pub fn append_cloud_sync_history(
    log_dir: impl AsRef<Path>,
    entry: &CloudSyncHistoryEntry,
) -> Result<(), CloudSyncError> {
    let log_dir = log_dir.as_ref();
    std::fs::create_dir_all(log_dir).map_err(|source| CloudSyncError::CreateDir {
        path: log_dir.to_path_buf(),
        source,
    })?;
    let path = log_dir.join(cloud_sync_history_log_file_name());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|source| CloudSyncError::WriteFile {
            path: path.clone(),
            source,
        })?;
    let line = serde_json::to_string(&cloud_sync_history_log_value(entry))?;
    writeln!(file, "{line}").map_err(|source| CloudSyncError::WriteFile { path, source })
}

pub fn read_cloud_sync_history(
    log_dir: impl AsRef<Path>,
    retention_days: u32,
    limit: usize,
) -> Result<Vec<CloudSyncHistoryEntry>, CloudSyncError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let log_dir = log_dir.as_ref();
    let mut entries = Vec::new();
    for path in collect_cloud_sync_history_log_files(log_dir, retention_days)? {
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        for line in content.lines().rev() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(entry) = parse_cloud_sync_history_entry(&value) {
                entries.push(entry);
                if entries.len() >= limit {
                    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
                    return Ok(entries);
                }
            }
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    Ok(entries)
}

fn cloud_sync_history_log_file_name() -> String {
    format!(
        "{}-cloud-sync.{}",
        crate::diagnostics::LOG_FILE_PREFIX,
        crate::diagnostics::LOG_FILE_SUFFIX
    )
}

fn cloud_sync_history_log_value(entry: &CloudSyncHistoryEntry) -> serde_json::Value {
    serde_json::json!({
        "level": history_log_level(&entry.status),
        "domain": CLOUD_SYNC_HISTORY_DOMAIN,
        "event": CLOUD_SYNC_HISTORY_EVENT,
        "message": entry.message,
        "ids": {
            "history_id": entry.id,
        },
        "data": {
            "id": entry.id,
            "timestamp_ms": entry.timestamp_ms,
            "kind": entry.kind,
            "status": entry.status,
            "trigger": entry.trigger,
            "provider": entry.provider,
            "revision": entry.revision,
            "duration_ms": entry.duration_ms,
        },
        "error": null,
        "client_timestamp": null,
    })
}

fn history_log_level(status: &str) -> &'static str {
    match status {
        "failed" => "error",
        "conflict" => "warn",
        _ => "info",
    }
}

fn collect_cloud_sync_history_log_files(
    log_dir: &Path,
    retention_days: u32,
) -> Result<Vec<PathBuf>, CloudSyncError> {
    let min_modified = SystemTime::now()
        .checked_sub(Duration::from_secs(
            u64::from(retention_days.max(1)) * 24 * 60 * 60,
        ))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let entries = match std::fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(CloudSyncError::ReadFile {
                path: log_dir.to_path_buf(),
                source,
            });
        }
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| CloudSyncError::ReadFile {
            path: log_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !is_cloud_sync_history_log_file(&path) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if modified >= min_modified {
            files.push((path, modified));
        }
    }
    files.sort_by(|(left_path, left_modified), (right_path, right_modified)| {
        right_modified
            .cmp(left_modified)
            .then_with(|| right_path.cmp(left_path))
    });
    Ok(files.into_iter().map(|(path, _)| path).collect())
}

fn is_cloud_sync_history_log_file(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                value.starts_with(crate::diagnostics::LOG_FILE_PREFIX)
                    && value.ends_with(crate::diagnostics::LOG_FILE_SUFFIX)
            })
}

fn parse_cloud_sync_history_entry(value: &serde_json::Value) -> Option<CloudSyncHistoryEntry> {
    let root = value.as_object()?;
    if root.get("domain")?.as_str()? != CLOUD_SYNC_HISTORY_DOMAIN {
        return None;
    }
    if root.get("event")?.as_str()? != CLOUD_SYNC_HISTORY_EVENT {
        return None;
    }
    let data = root.get("data")?.as_object()?;
    Some(CloudSyncHistoryEntry {
        id: data.get("id")?.as_str()?.to_string(),
        timestamp_ms: data.get("timestamp_ms")?.as_u64()?,
        kind: data.get("kind")?.as_str()?.to_string(),
        status: data.get("status")?.as_str()?.to_string(),
        trigger: data.get("trigger")?.as_str()?.to_string(),
        provider: data
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        revision: data
            .get("revision")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        duration_ms: data.get("duration_ms").and_then(serde_json::Value::as_u64),
        message: root.get("message")?.as_str()?.to_string(),
    })
}
