use std::cell::Cell;
use std::cell::RefCell;

use super::{
    ConnectionStore, MemoryRemote, OptionsTestLocalStore, local_connection, options,
    test_local_data, unique_temp_dir,
};
use crate::SessionsConfig;
use crate::cloud_sync::{
    CloudLocalStore, CloudRemoteCheckDecision, CloudSyncError, CloudSyncOutcome, CloudSyncRemote,
    CloudSyncState, check_snapshot_with_remote_state, pull_snapshot_with_remote_guarded,
    push_snapshot_with_remote,
};

struct EditingRemote<'a> {
    inner: &'a MemoryRemote,
    edit: RefCell<Option<Box<dyn FnOnce() + 'a>>>,
}

struct CountingRemote<'a> {
    inner: &'a MemoryRemote,
    snapshot_reads: Cell<usize>,
}

impl CloudSyncRemote for CountingRemote<'_> {
    fn provider(&self) -> &'static str {
        "memory"
    }
    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        self.inner.create_dir(path)
    }
    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        if path.contains("sync/snapshots/") {
            self.snapshot_reads.set(self.snapshot_reads.get() + 1);
        }
        self.inner.read_if_exists(path)
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        self.inner.write(path, bytes)
    }
    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        self.inner.delete(path)
    }
    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        self.inner.list_files(path)
    }
}

#[test]
fn automatic_push_reuses_checked_snapshot_and_upload_ciphertext() {
    let dir = unique_temp_dir("sync-count-reads");
    let options = options(&dir, &dir, "device");
    let store = OptionsTestLocalStore(&options);
    let memory = MemoryRemote::default();
    let first =
        push_snapshot_with_remote(&store, &options, &memory, &CloudSyncState::default(), false)
            .unwrap();
    let remote = CountingRemote {
        inner: &memory,
        snapshot_reads: Cell::new(0),
    };
    let local = ConnectionStore::open(&options.config_dir).unwrap();
    local
        .replace_sessions(&SessionsConfig {
            connections: vec![local_connection("checked", "Checked edit", "bash")],
            ..SessionsConfig::default()
        })
        .unwrap();
    let checked =
        check_snapshot_with_remote_state(&store, &options, &remote, &first.state, true).unwrap();
    local
        .replace_sessions(&SessionsConfig {
            connections: vec![local_connection("later", "Later edit", "bash")],
            ..SessionsConfig::default()
        })
        .unwrap();
    let result = checked
        .push(
            &store,
            &options,
            &remote,
            &crate::cloud_sync::control::CloudSyncOperationControl::default(),
        )
        .unwrap();
    assert_eq!(
        remote.snapshot_reads.get(),
        2,
        "one existing snapshot plus one upload readback"
    );
    let revision = result.pointer.unwrap().revision_id;
    assert_eq!(
        memory
            .read_if_exists(&crate::cloud_sync::remote_path(
                &options.remote_root,
                &crate::cloud_sync::legacy_sync_snapshot_file(&revision)
            ))
            .unwrap(),
        memory
            .read_if_exists(&crate::cloud_sync::remote_path(
                &options.remote_root,
                crate::cloud_sync::SYNC_CURRENT_FILE
            ))
            .unwrap()
    );
    assert_eq!(
        check_snapshot_with_remote_state(&store, &options, &remote, &result.state, true)
            .unwrap()
            .decision,
        CloudRemoteCheckDecision::LocalChanged
    );
}

#[test]
fn checked_pull_still_rejects_local_edits_saved_after_the_check() {
    let source_dir = unique_temp_dir("checked-source");
    let target_dir = unique_temp_dir("checked-target");
    let source = options(&source_dir, &source_dir, "source");
    let target = options(&target_dir, &target_dir, "target");
    let remote = MemoryRemote::default();
    let first = push_snapshot_with_remote(
        &OptionsTestLocalStore(&source),
        &source,
        &remote,
        &CloudSyncState::default(),
        false,
    )
    .unwrap();
    let target_store = OptionsTestLocalStore(&target);
    let state = check_snapshot_with_remote_state(
        &target_store,
        &target,
        &remote,
        &CloudSyncState::default(),
        true,
    )
    .unwrap()
    .state;
    ConnectionStore::open(&source.config_dir)
        .unwrap()
        .replace_sessions(&SessionsConfig {
            connections: vec![local_connection("remote", "Remote edit", "bash")],
            ..SessionsConfig::default()
        })
        .unwrap();
    push_snapshot_with_remote(
        &OptionsTestLocalStore(&source),
        &source,
        &remote,
        &first.state,
        false,
    )
    .unwrap();
    let checked =
        check_snapshot_with_remote_state(&target_store, &target, &remote, &state, true).unwrap();
    assert_eq!(checked.decision, CloudRemoteCheckDecision::AutoPull);
    ConnectionStore::open(&target.config_dir)
        .unwrap()
        .replace_sessions(&SessionsConfig {
            connections: vec![local_connection("local", "Local edit", "bash")],
            ..SessionsConfig::default()
        })
        .unwrap();
    assert!(matches!(
        checked.pull(
            &target_store,
            &target,
            &remote,
            &|| Ok(()),
            &crate::cloud_sync::control::CloudSyncOperationControl::default()
        ),
        Err(CloudSyncError::LocalChangedDuringSync)
    ));
}

impl CloudSyncRemote for EditingRemote<'_> {
    fn provider(&self) -> &'static str {
        "memory"
    }
    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        self.inner.create_dir(path)
    }
    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let bytes = self.inner.read_if_exists(path)?;
        if path.contains("sync/snapshots/")
            && let Some(edit) = self.edit.borrow_mut().take()
        {
            edit();
        }
        Ok(bytes)
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        self.inner.write(path, bytes)
    }
    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        self.inner.delete(path)
    }
    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        self.inner.list_files(path)
    }
}

#[test]
fn vnc_and_unknown_entity_only_changes_are_uploaded_even_with_legacy_baselines() {
    for entity in ["vnc_known_hosts", "future_entity"] {
        let dir = unique_temp_dir("sync-complete-hash");
        let options = options(&dir, &dir.join("remote"), "device");
        let store = OptionsTestLocalStore(&options);
        let remote = MemoryRemote::default();
        let first =
            push_snapshot_with_remote(&store, &options, &remote, &CloudSyncState::default(), false)
                .unwrap();
        test_local_data()
            .lock()
            .unwrap()
            .get_mut(&options.config_dir)
            .unwrap()
            .entities
            .insert(entity.into(), "[]".into());
        let mut old_state = first.state.clone();
        old_state.last_synced_content_hash = None;
        for state in [&first.state, &old_state] {
            assert_eq!(
                check_snapshot_with_remote_state(&store, &options, &remote, state, true)
                    .unwrap()
                    .decision,
                CloudRemoteCheckDecision::LocalChanged
            );
        }
        let pushed =
            push_snapshot_with_remote(&store, &options, &remote, &old_state, false).unwrap();
        assert_eq!(pushed.outcome, CloudSyncOutcome::Uploaded);
        assert_eq!(
            first.state.last_synced_payload_hash,
            pushed.state.last_synced_payload_hash
        );
        assert_ne!(
            first.state.last_synced_content_hash,
            pushed.state.last_synced_content_hash
        );
    }
}

#[test]
fn check_baseline_never_absorbs_an_edit_saved_during_download() {
    let dir = unique_temp_dir("sync-check-edit");
    let options = options(&dir, &dir.join("remote"), "device");
    let store = OptionsTestLocalStore(&options);
    let remote = MemoryRemote::default();
    let first =
        push_snapshot_with_remote(&store, &options, &remote, &CloudSyncState::default(), false)
            .unwrap();
    let editing = EditingRemote {
        inner: &remote,
        edit: RefCell::new(Some(Box::new(|| {
            ConnectionStore::open(&options.config_dir)
                .unwrap()
                .replace_sessions(&SessionsConfig {
                    connections: vec![local_connection("new", "Saved during download", "bash")],
                    ..SessionsConfig::default()
                })
                .unwrap();
        }))),
    };
    let checked =
        check_snapshot_with_remote_state(&store, &options, &editing, &first.state, true).unwrap();
    assert_eq!(checked.decision, CloudRemoteCheckDecision::UpToDate);
    assert_eq!(
        checked.state.last_synced_content_hash,
        first.state.last_synced_content_hash
    );
    store.persist_cloud_sync_state(&checked.state).unwrap();
    let next =
        check_snapshot_with_remote_state(&store, &options, &remote, &checked.state, true).unwrap();
    assert_eq!(next.decision, CloudRemoteCheckDecision::LocalChanged);
}

#[test]
fn pull_refuses_to_overwrite_a_save_between_download_and_apply_even_when_forced() {
    let source_dir = unique_temp_dir("sync-pull-source");
    let target_dir = unique_temp_dir("sync-pull-target");
    let source = options(&source_dir, &source_dir.join("remote"), "source");
    let target = options(&target_dir, &target_dir.join("remote"), "target");
    ConnectionStore::open(&source.config_dir)
        .unwrap()
        .replace_sessions(&SessionsConfig {
            connections: vec![local_connection("remote", "Remote", "bash")],
            ..SessionsConfig::default()
        })
        .unwrap();
    let remote = MemoryRemote::default();
    push_snapshot_with_remote(
        &OptionsTestLocalStore(&source),
        &source,
        &remote,
        &CloudSyncState::default(),
        false,
    )
    .unwrap();
    let result = pull_snapshot_with_remote_guarded(
        &OptionsTestLocalStore(&target),
        &target,
        &remote,
        &CloudSyncState::default(),
        true,
        &|| {
            ConnectionStore::open(&target.config_dir)
                .unwrap()
                .replace_sessions(&SessionsConfig {
                    connections: vec![local_connection("local", "Concurrent save", "bash")],
                    ..SessionsConfig::default()
                })
                .unwrap();
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(CloudSyncError::LocalChangedDuringSync)
    ));
    let target = ConnectionStore::open(&target.config_dir).unwrap();
    assert_eq!(
        target.load_sessions().unwrap().connections[0].name,
        "Concurrent save"
    );
    assert!(
        target
            .load_cloud_sync_state()
            .unwrap()
            .last_applied_remote_revision
            .is_none()
    );
}

#[test]
fn missing_remote_is_repopulated_even_when_local_baseline_is_unchanged() {
    let dir = unique_temp_dir("sync-remote-deleted");
    let options = options(&dir, &dir.join("remote"), "device");
    let store = OptionsTestLocalStore(&options);
    let first = push_snapshot_with_remote(
        &store,
        &options,
        &MemoryRemote::default(),
        &CloudSyncState::default(),
        false,
    )
    .unwrap();
    let result = push_snapshot_with_remote(
        &store,
        &options,
        &MemoryRemote::default(),
        &first.state,
        false,
    )
    .unwrap();
    assert_eq!(result.outcome, CloudSyncOutcome::Uploaded);
}
