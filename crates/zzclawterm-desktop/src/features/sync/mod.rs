//! Cloud sync provider adapters and cloud sync runtime.

mod auto_sync;
mod cloud_sync_provider;
mod cloud_sync_runtime;
mod state;

pub(crate) use auto_sync::{AutoSyncResult, AutoSyncTrigger, run_auto_sync};
pub(in crate::features) use cloud_sync_provider::{
    cleanup_provider_snapshots, pull_provider_snapshot, push_provider_snapshot,
    recover_provider_snapshot, test_provider_connection,
};
pub(in crate::features) use state::{CloudSyncFeatureState, CloudSyncLiveState};
