use crate::http::cloud_sync::{
    NativeAliyunDriveRemote, NativeGoogleDriveRemote, NativeOneDriveRemote, NativeS3Remote,
    NativeSnippetHttpClient, NativeWebdavRemote,
};
use zzclawterm_core::cloud_sync::control::CloudSyncOperationControl;
use zzclawterm_core::cloud_sync::{self, CloudLocalStore, CloudSyncRemote};
use zzclawterm_core::{
    CloudSyncError, CloudSyncResult, CloudSyncSettings, CloudSyncState, GiteeSnippetHttpBackend,
    GithubGistHttpBackend, LocalCloudSyncOptions, RemoteSyncPointer, SnippetRemote,
};

macro_rules! with_provider_remote {
    ($settings:expr, $remote:ident, $body:block) => {{
        match $settings.provider.as_str() {
            "webdav" => {
                let $remote = NativeWebdavRemote::new(&$settings.webdav)?;
                $body
            }
            "s3" => {
                let $remote = NativeS3Remote::new(&$settings.s3)?;
                $body
            }
            "google_drive" => {
                let $remote = NativeGoogleDriveRemote::new(&$settings.google_drive)?;
                $body
            }
            "onedrive" => {
                let $remote = NativeOneDriveRemote::new(&$settings.onedrive)?;
                $body
            }
            "aliyun_drive" => {
                let $remote = NativeAliyunDriveRemote::new(&$settings.aliyun_drive)?;
                $body
            }
            "gitee_snippet" => {
                let backend = GiteeSnippetHttpBackend::new(
                    &$settings.gitee_snippet,
                    NativeSnippetHttpClient::new()?,
                )?;
                let $remote = SnippetRemote::new("gitee_snippet", backend);
                $body
            }
            "github_gist" => {
                let backend = GithubGistHttpBackend::new(
                    &$settings.github_gist,
                    NativeSnippetHttpClient::new()?,
                )?;
                let $remote = SnippetRemote::new("github_gist", backend);
                $body
            }
            provider => Err(CloudSyncError::Remote(format!(
                "native cloud provider '{provider}' is not wired yet"
            ))),
        }
    }};
}

fn with_remote<T>(
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    operation: impl FnOnce(&dyn CloudSyncRemote) -> Result<T, CloudSyncError>,
) -> Result<T, CloudSyncError> {
    if settings.provider == "local_directory" {
        let remote = zzclawterm_core::LocalDirectoryRemote::new(options.remote_dir.clone());
        return operation(&remote);
    }
    with_provider_remote!(settings, remote, { operation(&remote) })
}

pub(crate) fn check_provider_snapshot(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    state: &CloudSyncState,
    control: &CloudSyncOperationControl,
) -> Result<cloud_sync::CloudSyncCheck, CloudSyncError> {
    control.checkpoint()?;
    with_remote(settings, options, |remote| {
        cloud_sync::check_snapshot_with_remote_state(
            &control.local_store(local_store),
            options,
            &control.remote(remote),
            state,
            settings.auto_pull_remote_changes,
        )
    })
}

pub(crate) fn push_checked_provider_snapshot(
    store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    checked: cloud_sync::CloudSyncCheck,
    control: &CloudSyncOperationControl,
) -> Result<CloudSyncResult, CloudSyncError> {
    with_remote(settings, options, |remote| {
        checked.push(store, options, remote, control)
    })
}

pub(crate) fn pull_checked_provider_snapshot(
    store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    checked: cloud_sync::CloudSyncCheck,
    before_apply: &dyn Fn() -> Result<(), CloudSyncError>,
    control: &CloudSyncOperationControl,
) -> Result<CloudSyncResult, CloudSyncError> {
    with_remote(settings, options, |remote| {
        checked.pull(store, options, remote, before_apply, control)
    })
}

pub(in crate::features) fn test_provider_connection(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    control: &CloudSyncOperationControl,
) -> Result<(), CloudSyncError> {
    control.checkpoint()?;
    with_provider_remote!(settings, remote, {
        cloud_sync::load_sync_pointer_from_remote(
            &control.local_store(local_store),
            &control.remote(&remote),
            &settings.remote_root,
        )?;
        control.checkpoint()
    })
}

pub(crate) fn push_provider_snapshot(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    state: &CloudSyncState,
    force: bool,
    control: &CloudSyncOperationControl,
) -> Result<CloudSyncResult, CloudSyncError> {
    control.checkpoint()?;
    with_remote(settings, options, |remote| {
        cloud_sync::push_snapshot_with_remote_controlled(
            local_store,
            options,
            remote,
            state,
            force,
            control,
        )
    })
}

pub(crate) fn pull_provider_snapshot(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    state: &CloudSyncState,
    force: bool,
    control: &CloudSyncOperationControl,
) -> Result<CloudSyncResult, CloudSyncError> {
    pull_provider_snapshot_guarded(
        local_store,
        settings,
        options,
        state,
        force,
        &|| Ok(()),
        control,
    )
}

pub(crate) fn pull_provider_snapshot_guarded(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    state: &CloudSyncState,
    force: bool,
    before_apply: &dyn Fn() -> Result<(), CloudSyncError>,
    control: &CloudSyncOperationControl,
) -> Result<CloudSyncResult, CloudSyncError> {
    control.checkpoint()?;
    with_remote(settings, options, |remote| {
        cloud_sync::pull_snapshot_with_remote_controlled(
            local_store,
            options,
            remote,
            state,
            force,
            before_apply,
            control,
        )
    })
}

pub(in crate::features) fn recover_provider_snapshot(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    control: &CloudSyncOperationControl,
) -> Result<CloudSyncResult, CloudSyncError> {
    control.checkpoint()?;
    with_remote(settings, options, |remote| {
        cloud_sync::recover_current_snapshot_with_remote_controlled(
            local_store,
            options,
            remote,
            control,
        )
    })
}

pub(in crate::features) fn cleanup_provider_snapshots(
    local_store: &dyn CloudLocalStore,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    latest: Option<&RemoteSyncPointer>,
    control: &CloudSyncOperationControl,
) -> Result<(), CloudSyncError> {
    control.checkpoint()?;
    with_remote(settings, options, |remote| {
        cloud_sync::cleanup_sync_snapshots_with_remote_controlled(
            local_store,
            options,
            remote,
            latest,
            control,
        )
    })
}
