use super::{
    ConnectionStore, MemoryRemote, OptionsTestLocalStore, local_connection, options,
    unique_temp_dir,
};
use crate::SessionsConfig;
use crate::cloud_sync::control::{CloudSyncCancellation, CloudSyncOperationControl};
use crate::cloud_sync::{
    CloudSyncError, CloudSyncOutcome, CloudSyncRemote, CloudSyncState, LocalCloudSyncOptions,
    SYNC_LATEST_FILE, pull_snapshot_with_remote_controlled, push_snapshot_with_remote_controlled,
    recover_current_snapshot_with_remote_controlled, remote_path,
};

#[derive(Clone, Copy)]
enum CancelAt {
    SnapshotRead,
    SnapshotWrite,
    PointerWrite,
}

struct CancellingRemote<'a> {
    remote: &'a MemoryRemote,
    control: &'a CloudSyncOperationControl,
    at: CancelAt,
}

impl CloudSyncRemote for CancellingRemote<'_> {
    fn provider(&self) -> &'static str {
        "memory"
    }
    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        self.remote.create_dir(path)
    }
    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let value = self.remote.read_if_exists(path)?;
        if matches!(self.at, CancelAt::SnapshotRead) && path.contains("sync/snapshots/") {
            assert_eq!(
                self.control.request_cancel(),
                CloudSyncCancellation::Stopping
            );
        }
        Ok(value)
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        self.remote.write(path, bytes)?;
        if matches!(self.at, CancelAt::SnapshotWrite) && path.contains("sync/snapshots/") {
            assert_eq!(
                self.control.request_cancel(),
                CloudSyncCancellation::Stopping
            );
        }
        if matches!(self.at, CancelAt::PointerWrite) && path.ends_with(SYNC_LATEST_FILE) {
            assert_eq!(
                self.control.request_cancel(),
                CloudSyncCancellation::FinishingCommit
            );
        }
        Ok(())
    }
    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        self.remote.delete(path)
    }
    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        self.remote.list_files(path)
    }
}

fn seeded_options(label: &str) -> LocalCloudSyncOptions {
    let config = unique_temp_dir(label);
    let options = options(&config, &config.join("remote"), label);
    ConnectionStore::open(&config)
        .unwrap()
        .replace_sessions(&SessionsConfig {
            connections: vec![local_connection(label, label, "bash")],
            ..SessionsConfig::default()
        })
        .unwrap();
    options
}

#[test]
fn cancelling_a_download_or_before_apply_preserves_local_data() {
    let source = seeded_options("cancel-source");
    let target = seeded_options("cancel-target");
    let remote = MemoryRemote::default();
    super::push_snapshot_with_remote(&source, &remote, &CloudSyncState::default(), false).unwrap();
    for during_download in [true, false] {
        let control = CloudSyncOperationControl::default();
        let cancelling = CancellingRemote {
            remote: &remote,
            control: &control,
            at: CancelAt::SnapshotRead,
        };
        let result = pull_snapshot_with_remote_controlled(
            &OptionsTestLocalStore(&target),
            &target,
            if during_download {
                &cancelling
            } else {
                &remote
            },
            &CloudSyncState::default(),
            true,
            &|| {
                control.request_cancel();
                Ok(())
            },
            &control,
        );
        assert!(matches!(result, Err(CloudSyncError::Cancelled)));
        let store = ConnectionStore::open(&target.config_dir).unwrap();
        assert_eq!(
            store.load_sessions().unwrap().connections[0].name,
            "cancel-target"
        );
        assert!(
            store
                .load_cloud_sync_state()
                .unwrap()
                .last_applied_remote_revision
                .is_none()
        );
    }
}

#[test]
fn cancelled_staging_never_publishes_but_cancel_during_publication_preserves_success() {
    for at in [CancelAt::SnapshotWrite, CancelAt::PointerWrite] {
        let source = seeded_options("cancel-upload");
        let remote = MemoryRemote::default();
        let control = CloudSyncOperationControl::default();
        let cancelling = CancellingRemote {
            remote: &remote,
            control: &control,
            at,
        };
        let result = push_snapshot_with_remote_controlled(
            &OptionsTestLocalStore(&source),
            &source,
            &cancelling,
            &CloudSyncState::default(),
            false,
            &control,
        );
        let pointer = remote
            .read_if_exists(&remote_path(&source.remote_root, SYNC_LATEST_FILE))
            .unwrap();
        let store = ConnectionStore::open(&source.config_dir).unwrap();
        match at {
            CancelAt::SnapshotWrite => {
                assert!(matches!(result, Err(CloudSyncError::Cancelled)));
                assert!(pointer.is_none());
                assert!(
                    store
                        .load_cloud_sync_state()
                        .unwrap()
                        .last_applied_remote_revision
                        .is_none()
                );
            }
            CancelAt::PointerWrite => {
                let result = result.unwrap();
                assert_eq!(result.outcome, CloudSyncOutcome::Uploaded);
                assert!(pointer.is_some());
                assert_eq!(
                    store
                        .load_cloud_sync_state()
                        .unwrap()
                        .last_applied_remote_revision,
                    result.state.last_applied_remote_revision
                );
            }
            CancelAt::SnapshotRead => unreachable!(),
        }
    }
}

#[test]
fn recovery_finishes_local_apply_after_publishing_remote_pointer() {
    let source = seeded_options("recover-source");
    let target = seeded_options("recover-target");
    let remote = MemoryRemote::default();
    super::push_snapshot_with_remote(&source, &remote, &CloudSyncState::default(), false).unwrap();
    let control = CloudSyncOperationControl::default();
    let cancelling = CancellingRemote {
        remote: &remote,
        control: &control,
        at: CancelAt::PointerWrite,
    };
    let result = recover_current_snapshot_with_remote_controlled(
        &OptionsTestLocalStore(&target),
        &target,
        &cancelling,
        &control,
    )
    .unwrap();
    assert_eq!(result.outcome, CloudSyncOutcome::Recovered);
    assert_eq!(
        ConnectionStore::open(&target.config_dir)
            .unwrap()
            .load_sessions()
            .unwrap()
            .connections[0]
            .name,
        "recover-source"
    );
}
