//! Local and remote target selection for SFTP transfers.

use std::path::{Path, PathBuf};

use russh_sftp::client::SftpSession;

use super::{
    SftpDuplicateDecision, SftpDuplicatePolicy, SftpDuplicateRequest, SftpDuplicateResolver,
    SftpPathCodec, SftpTransferDirection,
};

pub(super) fn resolve_remote_upload_target(
    local_path: &Path,
    remote_path: &str,
) -> anyhow::Result<String> {
    if remote_path == "." || remote_path.ends_with('/') {
        Ok(remote_join(remote_path, &local_file_name(local_path)?))
    } else {
        Ok(remote_path.to_string())
    }
}

fn local_file_name(path: &Path) -> anyhow::Result<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("local path has no file name: {}", path.display()))
}

pub(super) fn resolve_local_download_target(
    remote_path: &str,
    local_path: &Path,
    is_directory: bool,
    duplicate_policy: SftpDuplicatePolicy,
    duplicate_resolver: Option<&dyn SftpDuplicateResolver>,
) -> anyhow::Result<Option<PathBuf>> {
    if !local_path.exists() {
        return Ok(Some(local_path.to_path_buf()));
    }

    match resolve_duplicate_decision(
        SftpTransferDirection::Download,
        remote_path,
        &local_path.display().to_string(),
        is_directory,
        duplicate_policy,
        duplicate_resolver,
    )? {
        SftpDuplicateDecision::Overwrite => Ok(Some(local_path.to_path_buf())),
        SftpDuplicateDecision::Skip => Ok(None),
        SftpDuplicateDecision::Rename => resolve_renamed_local_target(local_path).map(Some),
    }
}

pub(super) async fn resolve_remote_write_target(
    sftp: &SftpSession,
    codec: &SftpPathCodec,
    local_path: &str,
    remote_path: &str,
    is_directory: bool,
    duplicate_policy: SftpDuplicatePolicy,
    duplicate_resolver: Option<&dyn SftpDuplicateResolver>,
) -> anyhow::Result<Option<String>> {
    if !sftp
        .try_exists_bytes(codec.encode_path(remote_path)?)
        .await?
    {
        return Ok(Some(remote_path.to_string()));
    }

    match resolve_duplicate_decision(
        SftpTransferDirection::Upload,
        local_path,
        remote_path,
        is_directory,
        duplicate_policy,
        duplicate_resolver,
    )? {
        SftpDuplicateDecision::Overwrite => Ok(Some(remote_path.to_string())),
        SftpDuplicateDecision::Skip => Ok(None),
        SftpDuplicateDecision::Rename => resolve_renamed_remote_target(sftp, codec, remote_path)
            .await
            .map(Some),
    }
}

pub(super) fn resolve_duplicate_decision(
    direction: SftpTransferDirection,
    source_path: &str,
    target_path: &str,
    is_directory: bool,
    duplicate_policy: SftpDuplicatePolicy,
    duplicate_resolver: Option<&dyn SftpDuplicateResolver>,
) -> anyhow::Result<SftpDuplicateDecision> {
    match duplicate_policy {
        SftpDuplicatePolicy::Overwrite => Ok(SftpDuplicateDecision::Overwrite),
        SftpDuplicatePolicy::Skip => Ok(SftpDuplicateDecision::Skip),
        SftpDuplicatePolicy::Rename => Ok(SftpDuplicateDecision::Rename),
        SftpDuplicatePolicy::Ask => {
            let resolver = duplicate_resolver.ok_or_else(|| {
                anyhow::anyhow!("SFTP duplicate policy is ask but no resolver is available")
            })?;
            resolver
                .resolve_duplicate(&SftpDuplicateRequest {
                    direction,
                    source_path: source_path.to_string(),
                    target_path: target_path.to_string(),
                    is_directory,
                })
                .map_err(anyhow::Error::msg)
        }
    }
}

fn resolve_renamed_local_target(local_path: &Path) -> anyhow::Result<PathBuf> {
    let stem = local_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| local_file_name(local_path).unwrap_or_else(|_| "download".to_string()));
    let extension = local_path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let parent = local_path.parent().unwrap_or_else(|| Path::new("."));
    for index in 1..=999 {
        let candidate = parent.join(format!("{stem}({index}){extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "unable to find a non-conflicting local path for {}",
        local_path.display()
    )
}

async fn resolve_renamed_remote_target(
    sftp: &SftpSession,
    codec: &SftpPathCodec,
    remote_path: &str,
) -> anyhow::Result<String> {
    for index in 1..=999 {
        let candidate = remote_conflict_candidate(remote_path, index);
        if !sftp
            .try_exists_bytes(codec.encode_path(&candidate)?)
            .await?
        {
            return Ok(candidate);
        }
    }
    anyhow::bail!("unable to find a non-conflicting remote path for {remote_path}")
}

pub(super) fn remote_conflict_candidate(remote_path: &str, index: usize) -> String {
    let (parent, name) = remote_split_parent_name(remote_path);
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), format!(".{extension}")),
        _ => (name, String::new()),
    };
    remote_join(&parent, &format!("{stem}({index}){extension}"))
}

fn remote_split_parent_name(remote_path: &str) -> (String, String) {
    let trimmed = remote_path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", name)) => ("/".to_string(), name.to_string()),
        Some((parent, name)) => (parent.to_string(), name.to_string()),
        None => (".".to_string(), trimmed.to_string()),
    }
}

pub(super) fn remote_join(base: &str, child: &str) -> String {
    if base.is_empty() || base == "." {
        child.to_string()
    } else if base == "/" {
        format!("/{child}")
    } else if base.ends_with('/') {
        format!("{base}{child}")
    } else {
        format!("{base}/{child}")
    }
}
