use crate::http::cloud_sync::{
    NativeAliyunDriveRemote, NativeGoogleDriveRemote, NativeOneDriveRemote, NativeS3Remote,
    NativeSnippetHttpClient, NativeWebdavRemote,
};
use nyaterm_core::{
    CloudSyncError, CloudSyncResult, CloudSyncSettings, CloudSyncState, GiteeSnippetHttpBackend,
    GithubGistHttpBackend, LocalCloudSyncOptions, RemoteSyncPointer, SnippetRemote,
    cleanup_sync_snapshots_with_remote, pull_local_snapshot, pull_snapshot_with_remote,
    push_local_snapshot, push_snapshot_with_remote, recover_current_snapshot_with_remote,
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

pub(in crate::features) fn test_provider_connection(
    local_store: &nyaterm_store::StoreBlockingClient,
    settings: &CloudSyncSettings,
) -> Result<(), CloudSyncError> {
    let remote_root = settings.remote_root.as_str();
    match settings.provider.as_str() {
        "webdav" => {
            let remote = NativeWebdavRemote::new(&settings.webdav)?;
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        "s3" => {
            let remote = NativeS3Remote::new(&settings.s3)?;
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        "google_drive" => {
            let remote = NativeGoogleDriveRemote::new(&settings.google_drive)?;
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        "onedrive" => {
            let remote = NativeOneDriveRemote::new(&settings.onedrive)?;
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        "aliyun_drive" => {
            let remote = NativeAliyunDriveRemote::new(&settings.aliyun_drive)?;
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        "gitee_snippet" => {
            let backend = GiteeSnippetHttpBackend::new(
                &settings.gitee_snippet,
                NativeSnippetHttpClient::new()?,
            )?;
            let remote = SnippetRemote::new("gitee_snippet", backend);
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        "github_gist" => {
            let backend =
                GithubGistHttpBackend::new(&settings.github_gist, NativeSnippetHttpClient::new()?)?;
            let remote = SnippetRemote::new("github_gist", backend);
            nyaterm_core::load_sync_pointer_from_remote(local_store, &remote, remote_root)?;
        }
        provider => {
            return Err(CloudSyncError::Remote(format!(
                "native cloud provider '{provider}' is not wired yet"
            )));
        }
    }
    Ok(())
}

pub(in crate::features) fn push_provider_snapshot(
    local_store: &nyaterm_store::StoreBlockingClient,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    state: &CloudSyncState,
    force: bool,
) -> Result<CloudSyncResult, CloudSyncError> {
    match settings.provider.as_str() {
        "webdav" => {
            let remote = NativeWebdavRemote::new(&settings.webdav)?;
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "s3" => {
            let remote = NativeS3Remote::new(&settings.s3)?;
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "google_drive" => {
            let remote = NativeGoogleDriveRemote::new(&settings.google_drive)?;
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "onedrive" => {
            let remote = NativeOneDriveRemote::new(&settings.onedrive)?;
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "aliyun_drive" => {
            let remote = NativeAliyunDriveRemote::new(&settings.aliyun_drive)?;
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "gitee_snippet" => {
            let backend = GiteeSnippetHttpBackend::new(
                &settings.gitee_snippet,
                NativeSnippetHttpClient::new()?,
            )?;
            let remote = SnippetRemote::new("gitee_snippet", backend);
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "github_gist" => {
            let backend =
                GithubGistHttpBackend::new(&settings.github_gist, NativeSnippetHttpClient::new()?)?;
            let remote = SnippetRemote::new("github_gist", backend);
            push_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "local_directory" => push_local_snapshot(local_store, options, state, force),
        provider => Err(CloudSyncError::Remote(format!(
            "native cloud provider '{provider}' is not wired yet"
        ))),
    }
}

pub(in crate::features) fn pull_provider_snapshot(
    local_store: &nyaterm_store::StoreBlockingClient,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    state: &CloudSyncState,
    force: bool,
) -> Result<CloudSyncResult, CloudSyncError> {
    match settings.provider.as_str() {
        "webdav" => {
            let remote = NativeWebdavRemote::new(&settings.webdav)?;
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "s3" => {
            let remote = NativeS3Remote::new(&settings.s3)?;
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "google_drive" => {
            let remote = NativeGoogleDriveRemote::new(&settings.google_drive)?;
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "onedrive" => {
            let remote = NativeOneDriveRemote::new(&settings.onedrive)?;
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "aliyun_drive" => {
            let remote = NativeAliyunDriveRemote::new(&settings.aliyun_drive)?;
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "gitee_snippet" => {
            let backend = GiteeSnippetHttpBackend::new(
                &settings.gitee_snippet,
                NativeSnippetHttpClient::new()?,
            )?;
            let remote = SnippetRemote::new("gitee_snippet", backend);
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "github_gist" => {
            let backend =
                GithubGistHttpBackend::new(&settings.github_gist, NativeSnippetHttpClient::new()?)?;
            let remote = SnippetRemote::new("github_gist", backend);
            pull_snapshot_with_remote(local_store, options, &remote, state, force)
        }
        "local_directory" => pull_local_snapshot(local_store, options, state, force),
        provider => Err(CloudSyncError::Remote(format!(
            "native cloud provider '{provider}' is not wired yet"
        ))),
    }
}

pub(in crate::features) fn recover_provider_snapshot(
    local_store: &nyaterm_store::StoreBlockingClient,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
) -> Result<CloudSyncResult, CloudSyncError> {
    with_provider_remote!(settings, remote, {
        recover_current_snapshot_with_remote(local_store, options, &remote)
    })
}

pub(in crate::features) fn cleanup_provider_snapshots(
    local_store: &nyaterm_store::StoreBlockingClient,
    settings: &CloudSyncSettings,
    options: &LocalCloudSyncOptions,
    latest: Option<&RemoteSyncPointer>,
) -> Result<(), CloudSyncError> {
    with_provider_remote!(settings, remote, {
        cleanup_sync_snapshots_with_remote(local_store, options, &remote, latest)
    })
}
