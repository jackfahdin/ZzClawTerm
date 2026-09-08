use std::collections::HashSet;
use std::time::Duration;

use super::{
    CloudLocalStore, CloudSyncError, CloudSyncRemote, LocalCloudSyncOptions, RemoteSyncPointer,
    SYNC_SNAPSHOTS_DIR, current_time_ms, remote_path,
};

pub const SYNC_SNAPSHOT_KEEP_RECENT: usize = 5;
pub const SYNC_SNAPSHOT_GC_GRACE_PERIOD: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotGcEntry {
    path: String,
    revision_id: String,
    created_at_ms: u64,
    deletable: bool,
}

pub fn cleanup_sync_snapshots_with_remote(
    local_store: &dyn CloudLocalStore,
    options: &LocalCloudSyncOptions,
    remote: &dyn CloudSyncRemote,
    latest: Option<&RemoteSyncPointer>,
) -> Result<(), CloudSyncError> {
    let prefix = remote_path(&options.remote_root, SYNC_SNAPSHOTS_DIR);
    let mut snapshots = Vec::new();
    for path in remote.list_files(&prefix)? {
        let Some(revision_id) = snapshot_revision_from_path(&path) else {
            continue;
        };
        let entry = match remote.read_if_exists(&path) {
            Ok(Some(bytes)) => {
                match local_store
                    .decode_sync_snapshot(&bytes, options.master_password.expose_secret())
                {
                    Ok(snapshot) if snapshot.meta.revision_id == revision_id => SnapshotGcEntry {
                        path,
                        revision_id,
                        created_at_ms: snapshot.meta.created_at_ms,
                        deletable: !snapshot.meta.payload_hash.is_empty(),
                    },
                    _ => SnapshotGcEntry {
                        path,
                        revision_id,
                        created_at_ms: 0,
                        deletable: false,
                    },
                }
            }
            _ => SnapshotGcEntry {
                path,
                revision_id,
                created_at_ms: 0,
                deletable: false,
            },
        };
        snapshots.push(entry);
    }

    let mut first_error = None;
    for path in plan_snapshot_gc(
        snapshots,
        latest.map(|pointer| pointer.revision_id.as_str()),
        current_time_ms(),
        SYNC_SNAPSHOT_KEEP_RECENT,
        SYNC_SNAPSHOT_GC_GRACE_PERIOD,
    ) {
        if let Err(error) = remote.delete(&path)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn plan_snapshot_gc(
    mut snapshots: Vec<SnapshotGcEntry>,
    latest_revision: Option<&str>,
    now_ms: u64,
    keep_recent: usize,
    grace_period: Duration,
) -> Vec<String> {
    let mut protected = HashSet::new();
    if let Some(revision) = latest_revision {
        protected.insert(revision.to_string());
    }
    snapshots.sort_by_key(|snapshot| snapshot.created_at_ms);
    for snapshot in snapshots.iter().rev().take(keep_recent) {
        protected.insert(snapshot.revision_id.clone());
    }
    let grace_ms = u64::try_from(grace_period.as_millis()).unwrap_or(u64::MAX);
    snapshots
        .into_iter()
        .filter(|snapshot| snapshot.deletable)
        .filter(|snapshot| !protected.contains(&snapshot.revision_id))
        .filter(|snapshot| now_ms.saturating_sub(snapshot.created_at_ms) > grace_ms)
        .map(|snapshot| snapshot.path)
        .collect()
}

fn snapshot_revision_from_path(path: &str) -> Option<String> {
    let filename = path.rsplit('/').next()?;
    filename
        .strip_suffix(".redb.enc")
        .filter(|revision| !revision.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{SnapshotGcEntry, plan_snapshot_gc};
    use std::time::Duration;

    fn entry(revision: &str, created_at_ms: u64) -> SnapshotGcEntry {
        SnapshotGcEntry {
            path: format!("nyaterm/sync/snapshots/{revision}.redb.enc"),
            revision_id: revision.to_string(),
            created_at_ms,
            deletable: true,
        }
    }

    #[test]
    fn keeps_latest_and_five_most_recent_snapshots() {
        let delete = plan_snapshot_gc(
            (1..=8).map(|n| entry(&format!("r{n}"), n)).collect(),
            Some("r1"),
            100_000_000,
            5,
            Duration::ZERO,
        );
        assert!(!delete.iter().any(|path| path.ends_with("r1.redb.enc")));
        assert_eq!(delete.len(), 2);
    }

    #[test]
    fn keeps_recent_and_unreadable_snapshots() {
        let mut unreadable = entry("unreadable", 1);
        unreadable.deletable = false;
        let delete = plan_snapshot_gc(
            vec![entry("old", 1), entry("fresh", 99_000), unreadable],
            None,
            100_000,
            0,
            Duration::from_secs(2),
        );
        assert_eq!(delete, vec!["nyaterm/sync/snapshots/old.redb.enc"]);
    }
}
