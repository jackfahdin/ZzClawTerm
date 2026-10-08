use super::{AutoSyncCoordinator, auto_sync_backoff, permanent_auto_sync_error};
use crate::features::{AutoSyncResult, AutoSyncTrigger};
use std::time::Duration;
use std::time::Instant;
use zzclawterm_core::{CloudSyncError, CloudSyncState};

#[test]
fn completion_observes_changes_before_their_notifications_arrive() {
    let mut sync = AutoSyncCoordinator::new(4);
    let job = sync
        .next_job(sync.next_due, 4, || true)
        .expect("startup sync");
    sync.complete(
        job.trigger,
        &Ok(AutoSyncResult::UpToDate(CloudSyncState::default())),
        Some(5),
    );
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
    assert!(sync.change_since.is_some());
    let due = sync.next_due;
    sync.record_sync_mutation(5);
    assert_eq!(sync.next_due, due, "late notification keeps the debounce");
}

#[test]
fn dispatch_captures_generation_and_keeps_changes_made_during_the_job() {
    let mut sync = AutoSyncCoordinator::new(4);
    sync.record_sync_mutation(5);
    let now = sync.next_due;
    let job = sync.next_job(now, 5, || true).expect("change is due");
    assert_eq!(job.trigger, AutoSyncTrigger::Change);
    assert_eq!(job.change_elapsed, Duration::from_secs(1));
    assert_eq!(sync.in_flight_generation, 5);
    assert!(sync.next_job(now, 5, || true).is_none());

    sync.record_sync_mutation(6);
    sync.complete(
        job.trigger,
        &Ok(AutoSyncResult::UpToDate(CloudSyncState::default())),
        None,
    );
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
    assert!(sync.change_since.is_some());
    let next = sync
        .next_job(sync.next_due, 6, || true)
        .expect("new change remains scheduled");
    assert_eq!(next.trigger, AutoSyncTrigger::Change);
    assert_eq!(sync.in_flight_generation, 6);
}

#[test]
fn deferred_pull_waits_for_safe_windows_and_does_not_replace_a_local_change() {
    let mut sync = AutoSyncCoordinator::new(0);
    sync.pending = None;
    sync.remote_available = true;
    let now = Instant::now();
    assert!(sync.next_job(now, 0, || false).is_none());
    assert_eq!(sync.pending, None);
    let job = sync.next_job(now, 0, || true).expect("safe pull");
    assert_eq!(job.trigger, AutoSyncTrigger::DeferredPull);

    sync.running = false;
    sync.record_sync_mutation(1);
    let job = sync
        .next_job(sync.next_due, 1, || true)
        .expect("local change takes precedence");
    assert_eq!(job.trigger, AutoSyncTrigger::Change);
}

#[test]
fn periodic_checks_respect_debounce_and_pause() {
    let mut sync = AutoSyncCoordinator::new(0);
    sync.record_sync_mutation(1);
    let due = sync.next_due;
    sync.next_periodic = Instant::now();
    assert!(
        sync.next_job(due - Duration::from_millis(1), 1, || panic!(
            "idle tick reads no windows"
        ))
        .is_none()
    );
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
    assert_eq!(sync.next_due, due);
    sync.paused = true;
    assert!(
        sync.next_job(due, 1, || panic!("paused tick reads no windows"))
            .is_none()
    );
    assert!(!sync.running());
    sync.settings_changed();
    let job = sync
        .next_job(due, 1, || true)
        .expect("resumed local change");
    assert_eq!(job.trigger, AutoSyncTrigger::Change);
}

#[test]
fn auto_sync_focus_is_throttled_and_settings_resume_paused_checks() {
    let mut sync = AutoSyncCoordinator::new(0);
    sync.pending = None;
    sync.focused();
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Focus));
    sync.pending = None;
    sync.focused();
    assert_eq!(sync.pending, None);
    sync.paused = true;
    sync.failures = 3;
    sync.settings_changed();
    assert!(!sync.paused);
    assert_eq!(sync.failures, 0);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Settings));
}

#[test]
fn auto_sync_backoff_and_auth_pause_policy() {
    assert_eq!(auto_sync_backoff(0), Duration::from_secs(60));
    assert_eq!(auto_sync_backoff(1), Duration::from_secs(300));
    assert_eq!(auto_sync_backoff(2), Duration::from_secs(900));
    assert_eq!(auto_sync_backoff(3), Duration::from_secs(3_600));
    assert_eq!(auto_sync_backoff(99), Duration::from_secs(3_600));
    assert!(permanent_auto_sync_error(
        &zzclawterm_core::CloudSyncError::Remote("HTTP 401 Unauthorized".into())
    ));
    assert!(!permanent_auto_sync_error(
        &zzclawterm_core::CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "network timeout"
        ))
    ));
}

#[test]
fn completing_an_older_sync_preserves_changes_received_while_running() {
    let mut sync = AutoSyncCoordinator::new(4);
    sync.running = true;
    sync.pending = None;
    sync.record_sync_mutation(5);
    let due = sync.next_due;
    let changed = sync.change_since;

    sync.complete(
        AutoSyncTrigger::Change,
        &Ok(AutoSyncResult::UpToDate(CloudSyncState::default())),
        None,
    );

    assert!(!sync.running());
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
    assert_eq!(sync.change_since, changed);
    assert_eq!(sync.next_due, due);

    sync.in_flight_generation = 5;
    sync.running = true;
    sync.complete(
        AutoSyncTrigger::Change,
        &Ok(AutoSyncResult::UpToDate(CloudSyncState::default())),
        None,
    );
    assert_eq!(sync.pending, None);
    assert_eq!(sync.change_since, None);
}

#[test]
fn duplicate_or_stale_mutations_do_not_extend_the_debounce() {
    let mut sync = AutoSyncCoordinator::new(4);
    sync.record_sync_mutation(5);
    let due = sync.next_due;
    let changed = sync.change_since;
    sync.record_sync_mutation(5);
    sync.record_sync_mutation(3);
    assert_eq!(sync.mutation_generation, 5);
    assert_eq!(sync.next_due, due);
    assert_eq!(sync.change_since, changed);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
}

#[test]
fn authentication_failure_pauses_sync_until_settings_change() {
    let mut sync = AutoSyncCoordinator::new(0);
    sync.running = true;
    sync.complete(
        AutoSyncTrigger::Startup,
        &Err(CloudSyncError::Remote("HTTP 401 Unauthorized".into())),
        None,
    );
    assert!(!sync.running());
    assert!(sync.paused);
    assert_eq!(sync.failures, 1);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Retry));

    sync.settings_changed();
    assert!(!sync.paused);
    assert_eq!(sync.failures, 0);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Settings));
}
