use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::Context;
use rust_i18n::t;
use zzclawterm_core::{
    AppRuntime, CloudRemoteCheckDecision, CloudSyncError, CloudSyncResult, CloudSyncSettings,
    CloudSyncState, LocalCloudSyncOptions,
};
use zzclawterm_store::{StoreBlockingClient, StoreDomain};

use crate::features::ZzClawTermApp;

use super::cloud_sync_provider::{
    check_provider_snapshot, cleanup_provider_snapshots, pull_provider_snapshot_guarded,
    push_provider_snapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoSyncTrigger {
    Startup,
    Periodic,
    Focus,
    Settings,
    Change,
    DeferredPull,
    Retry,
}

pub(crate) enum AutoSyncResult {
    Disabled,
    Skipped,
    Debouncing(Duration),
    Busy,
    RemoteAvailable { retry_when_safe: bool },
    UpToDate(CloudSyncState),
    Synced(Box<CloudSyncResult>),
}

pub(crate) fn run_auto_sync(
    store: &StoreBlockingClient,
    runtime: &AppRuntime,
    trigger: AutoSyncTrigger,
    change_elapsed: Duration,
    pull_safe: bool,
    before_apply: &dyn Fn() -> Result<(), CloudSyncError>,
) -> Result<AutoSyncResult, CloudSyncError> {
    let _operation = match crate::features::cloud_sync_operation_lock().try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(AutoSyncResult::Busy),
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
    };
    let (settings, state, password) = store
        .request_fn(StoreDomain::CloudSync, |database| {
            Ok((
                database.load_cloud_sync_settings()?,
                database.load_cloud_sync_state()?,
                database.load_cloud_sync_master_password()?,
            ))
        })
        .map_err(|error| CloudSyncError::LocalStore(error.category().to_string()))?;
    if !settings.enabled {
        return Ok(AutoSyncResult::Disabled);
    }
    if settings.provider == "github_gist"
        && matches!(
            trigger,
            AutoSyncTrigger::Startup
                | AutoSyncTrigger::Periodic
                | AutoSyncTrigger::Focus
                | AutoSyncTrigger::DeferredPull
        )
    {
        return Ok(AutoSyncResult::Skipped);
    }
    if trigger == AutoSyncTrigger::Startup && !settings.auto_check_on_startup {
        return Ok(AutoSyncResult::Skipped);
    }
    if trigger == AutoSyncTrigger::Change {
        if !settings.auto_push_on_change {
            return Ok(AutoSyncResult::Skipped);
        }
        let debounce = Duration::from_secs(settings.sync_debounce_seconds.clamp(1, 3_600));
        if change_elapsed < debounce {
            return Ok(AutoSyncResult::Debouncing(debounce - change_elapsed));
        }
    }
    let password = password.ok_or(zzclawterm_core::PortableSnapshotError::MissingMasterPassword)?;
    let options = LocalCloudSyncOptions {
        config_dir: runtime.config_dir().to_path_buf(),
        portable_key_path: runtime.portable_key_path().map(ToOwned::to_owned),
        remote_dir: runtime.config_dir().join("cloud-sync-local"),
        remote_root: settings.remote_root.clone(),
        device_id: state.device_id.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        master_password: password,
        enabled: true,
    };
    let (decision, pointer) = check_provider_snapshot(store, &settings, &options, &state)?;
    match decision {
        CloudRemoteCheckDecision::LocalChanged if settings.auto_push_on_change => {
            let result = push_provider_snapshot(store, &settings, &options, &state, false)?;
            cleanup_if_due(store, &settings, &options, &result);
            Ok(AutoSyncResult::Synced(Box::new(result)))
        }
        CloudRemoteCheckDecision::AutoPull if pull_safe => {
            let result = match pull_provider_snapshot_guarded(
                store,
                &settings,
                &options,
                &state,
                false,
                before_apply,
            ) {
                Err(CloudSyncError::AutoPullDeferred) => {
                    return Ok(AutoSyncResult::RemoteAvailable {
                        retry_when_safe: true,
                    });
                }
                result => result?,
            };
            cleanup_if_due(store, &settings, &options, &result);
            Ok(AutoSyncResult::Synced(Box::new(result)))
        }
        CloudRemoteCheckDecision::AutoPull => Ok(AutoSyncResult::RemoteAvailable {
            retry_when_safe: true,
        }),
        CloudRemoteCheckDecision::RemoteAvailable => Ok(AutoSyncResult::RemoteAvailable {
            retry_when_safe: false,
        }),
        CloudRemoteCheckDecision::Conflict => unreachable!("conflict returned by remote check"),
        CloudRemoteCheckDecision::UpToDate | CloudRemoteCheckDecision::LocalChanged => {
            let mut checked = state;
            checked.last_checked_at_ms = Some(now_ms());
            if decision == CloudRemoteCheckDecision::UpToDate
                && let Some(pointer) = pointer
            {
                let mut local =
                    zzclawterm_core::CloudLocalStore::build_sync_snapshot(store, &options)?;
                local.recalculate_hash()?;
                checked.last_synced_payload_hash = Some(local.meta.payload_hash);
                checked.last_applied_remote_revision = Some(pointer.revision_id);
            }
            let persisted = checked.clone();
            store
                .request_fn(StoreDomain::CloudSync, move |database| {
                    database.save_cloud_sync_state(&persisted)
                })
                .map_err(|error| CloudSyncError::LocalStore(error.category().to_string()))?;
            Ok(AutoSyncResult::UpToDate(checked))
        }
    }
}

fn cleanup_if_due(
    store: &StoreBlockingClient,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    result: &CloudSyncResult,
) {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
    if result
        .state
        .last_gc_attempt_at_ms
        .is_some_and(|last| now_ms().saturating_sub(last) < DAY_MS)
    {
        return;
    }
    let _ = cleanup_provider_snapshots(store, settings, options, result.pointer.as_ref());
    let _ = store.request_fn(StoreDomain::CloudSync, |database| {
        let mut latest = database.load_cloud_sync_state()?;
        latest.last_gc_attempt_at_ms = Some(now_ms());
        database.save_cloud_sync_state(&latest)
    });
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

impl ZzClawTermApp {
    pub(crate) fn auto_cloud_sync_pull_blocked(&self) -> bool {
        self.session.session_order_len() > 0
            || self.session.start_has_pending()
            || self.remote_desktop.has_active_sessions()
            || self.settings_draft_dirty()
    }

    pub(crate) fn apply_auto_cloud_sync_result(
        &mut self,
        result: &Result<AutoSyncResult, CloudSyncError>,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync.job_running() {
            return;
        }
        match result {
            Ok(AutoSyncResult::RemoteAvailable { .. }) => {
                self.cloud_sync
                    .set_status(t!("settings.syncRemoteAvailable"));
            }
            Ok(AutoSyncResult::UpToDate(state)) => {
                self.cloud_sync
                    .complete_job(state.clone(), t!("settings.syncUpToDate").to_string());
            }
            Ok(AutoSyncResult::Synced(sync)) => {
                let message = match sync.outcome {
                    zzclawterm_core::CloudSyncOutcome::Downloaded => t!("settings.syncPullSuccess"),
                    zzclawterm_core::CloudSyncOutcome::Uploaded => t!("settings.syncPushSuccess"),
                    _ => t!("settings.syncUpToDate"),
                }
                .to_string();
                self.cloud_sync
                    .complete_job(sync.state.clone(), message.clone());
                self.shell.set_status(message);
                if sync.outcome == zzclawterm_core::CloudSyncOutcome::Downloaded {
                    self.refresh_store_from_runtime_and_sync_theme(cx);
                }
            }
            Err(error) => {
                let message = match error {
                    CloudSyncError::Conflict(_) => t!("settings.syncConflictTitle").to_string(),
                    _ => t!("settings.syncAutoFailed", detail = error).to_string(),
                };
                self.cloud_sync.fail_job(
                    error,
                    message.clone(),
                    self.cloud_sync.settings().provider.clone(),
                    true,
                );
                self.shell.set_status(message);
            }
            _ => return,
        }
        self.request_settings_panel_refresh(cx);
        cx.notify();
    }
}
