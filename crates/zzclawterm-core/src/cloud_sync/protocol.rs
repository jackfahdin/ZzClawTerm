use crate::{CloudLocalStore, PortableSnapshotError, RawPortableSnapshot};

use super::{
    CloudSyncError, CloudSyncRemote, LocalCloudSyncOptions, REMOTE_SYNC_POINTER_SCHEMA_VERSION,
    RemoteSyncPointer, SYNC_CURRENT_FILE, legacy_sync_snapshot_file, load_sync_pointer_from_remote,
    remote_path, write_sync_pointer,
};

#[derive(Debug)]
pub(super) enum RemoteSnapshotResolution {
    Current(RawPortableSnapshot),
    LegacyMigrated(RawPortableSnapshot),
    Inconsistent {
        pointer: RemoteSyncPointer,
        recovery_candidate: RawPortableSnapshot,
    },
}

pub(super) fn pointer_from_snapshot(snapshot: &RawPortableSnapshot) -> RemoteSyncPointer {
    RemoteSyncPointer {
        schema_version: REMOTE_SYNC_POINTER_SCHEMA_VERSION,
        revision_id: snapshot.meta.revision_id.clone(),
        created_at_ms: snapshot.meta.created_at_ms,
        payload_hash: snapshot.meta.payload_hash.clone(),
        device_id: snapshot.meta.device_id.clone(),
        app_version: snapshot.meta.app_version.clone(),
    }
}

pub(super) fn upload_sync_snapshot(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    options: &LocalCloudSyncOptions,
    snapshot: &RawPortableSnapshot,
) -> Result<(), CloudSyncError> {
    let bytes =
        local_store.encode_sync_snapshot(snapshot, options.master_password.expose_secret())?;
    remote.write(
        &remote_path(
            &options.remote_root,
            &legacy_sync_snapshot_file(&snapshot.meta.revision_id),
        ),
        &bytes,
    )
}

pub(super) fn read_snapshot_for_pointer(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    options: &LocalCloudSyncOptions,
    pointer: &RemoteSyncPointer,
) -> Result<RawPortableSnapshot, CloudSyncError> {
    let path = remote_path(
        &options.remote_root,
        &legacy_sync_snapshot_file(&pointer.revision_id),
    );
    let Some(bytes) = remote.read_if_exists(&path)? else {
        return Err(CloudSyncError::SnapshotMissing {
            revision: pointer.revision_id.clone(),
        });
    };
    let snapshot = decode_remote_snapshot(
        local_store,
        &bytes,
        options.master_password.expose_secret(),
        &pointer.revision_id,
    )?;
    validate_snapshot_against_pointer(pointer, &snapshot)?;
    Ok(snapshot)
}

pub(super) fn write_current_sync_snapshot_compat(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    options: &LocalCloudSyncOptions,
    snapshot: &RawPortableSnapshot,
) -> Result<(), CloudSyncError> {
    let bytes =
        local_store.encode_sync_snapshot(snapshot, options.master_password.expose_secret())?;
    remote.write(
        &remote_path(&options.remote_root, SYNC_CURRENT_FILE),
        &bytes,
    )
}

fn read_current_sync_snapshot_compat(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    options: &LocalCloudSyncOptions,
) -> Result<Option<RawPortableSnapshot>, CloudSyncError> {
    let Some(bytes) =
        remote.read_if_exists(&remote_path(&options.remote_root, SYNC_CURRENT_FILE))?
    else {
        return Ok(None);
    };
    decode_remote_snapshot(
        local_store,
        &bytes,
        options.master_password.expose_secret(),
        "current",
    )
    .map(Some)
}

pub(super) fn ensure_remote_head_unchanged(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    remote_root: &str,
    expected: Option<&RemoteSyncPointer>,
) -> Result<(), CloudSyncError> {
    let actual = load_sync_pointer_from_remote(local_store, remote, remote_root)?;
    let expected_revision = expected.map(|pointer| pointer.revision_id.clone());
    let actual_revision = actual.as_ref().map(|pointer| pointer.revision_id.clone());
    if expected_revision != actual_revision {
        return Err(CloudSyncError::ConcurrentUpdate {
            expected_revision,
            actual_revision,
        });
    }
    Ok(())
}

pub(super) fn resolve_remote_snapshot(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    options: &LocalCloudSyncOptions,
    pointer: &RemoteSyncPointer,
) -> Result<RemoteSnapshotResolution, CloudSyncError> {
    match read_snapshot_for_pointer(local_store, remote, options, pointer) {
        Ok(snapshot) => return Ok(RemoteSnapshotResolution::Current(snapshot)),
        Err(CloudSyncError::SnapshotMissing { .. }) => {}
        Err(error) => return Err(error),
    }

    let Some(current) = read_current_sync_snapshot_compat(local_store, remote, options)? else {
        return Err(CloudSyncError::SnapshotMissing {
            revision: pointer.revision_id.clone(),
        });
    };
    if validate_snapshot_against_pointer(pointer, &current).is_ok() {
        upload_sync_snapshot(local_store, remote, options, &current)?;
        read_snapshot_for_pointer(local_store, remote, options, pointer)?;
        return Ok(RemoteSnapshotResolution::LegacyMigrated(current));
    }

    Ok(RemoteSnapshotResolution::Inconsistent {
        pointer: pointer.clone(),
        recovery_candidate: current,
    })
}

pub(super) fn recover_current_remote_snapshot(
    local_store: &dyn CloudLocalStore,
    remote: &dyn CloudSyncRemote,
    options: &LocalCloudSyncOptions,
) -> Result<RawPortableSnapshot, CloudSyncError> {
    let Some(snapshot) = read_current_sync_snapshot_compat(local_store, remote, options)? else {
        return Err(CloudSyncError::SnapshotMissing {
            revision: "current".to_string(),
        });
    };
    let pointer = pointer_from_snapshot(&snapshot);
    upload_sync_snapshot(local_store, remote, options, &snapshot)?;
    read_snapshot_for_pointer(local_store, remote, options, &pointer)?;
    write_sync_pointer(local_store, remote, &options.remote_root, &pointer)?;
    Ok(snapshot)
}

pub(super) fn validate_snapshot_against_pointer(
    pointer: &RemoteSyncPointer,
    snapshot: &RawPortableSnapshot,
) -> Result<(), CloudSyncError> {
    if snapshot.meta.revision_id != pointer.revision_id {
        return Err(CloudSyncError::RevisionMismatch {
            pointer_revision: pointer.revision_id.clone(),
            snapshot_revision: snapshot.meta.revision_id.clone(),
        });
    }
    if snapshot.meta.payload_hash != pointer.payload_hash {
        return Err(CloudSyncError::HashMismatch {
            expected: pointer.payload_hash.clone(),
            actual: snapshot.meta.payload_hash.clone(),
        });
    }
    Ok(())
}

fn decode_remote_snapshot(
    local_store: &dyn CloudLocalStore,
    bytes: &[u8],
    master_password: &str,
    revision: &str,
) -> Result<RawPortableSnapshot, CloudSyncError> {
    local_store
        .decode_sync_snapshot(bytes, master_password)
        .map_err(|error| match error {
            CloudSyncError::PortableSnapshot(
                error @ (PortableSnapshotError::MissingMasterPassword
                | PortableSnapshotError::Decrypt { .. }),
            ) => CloudSyncError::PortableSnapshot(error),
            _ => CloudSyncError::CorruptedSnapshot {
                revision: revision.to_string(),
            },
        })
}
