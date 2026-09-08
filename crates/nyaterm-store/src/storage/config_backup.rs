use std::path::{Path, PathBuf};

use super::StorageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigBackupInfo {
    pub database_path: PathBuf,
    pub backup_path: PathBuf,
    pub bytes: u64,
    pub safety_backup_path: Option<PathBuf>,
}

pub(super) fn ensure_parent_dir(path: &Path) -> Result<(), StorageError> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| StorageError::CreateDir {
        path: parent.to_path_buf(),
        source,
    })
}

pub(super) fn write_portable_snapshot_file(
    database_path: PathBuf,
    output_path: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<ConfigBackupInfo, StorageError> {
    let backup_path = output_path.as_ref().to_path_buf();
    ensure_parent_dir(&backup_path)?;
    std::fs::write(&backup_path, bytes).map_err(|source| StorageError::ConfigBackupCopy {
        from: database_path.clone(),
        to: backup_path.clone(),
        source,
    })?;

    Ok(ConfigBackupInfo {
        database_path,
        backup_path,
        bytes: bytes.len().try_into().unwrap_or(u64::MAX),
        safety_backup_path: None,
    })
}

pub(super) fn validate_config_backup_source(path: &Path) -> Result<(), StorageError> {
    if !path.exists() {
        return Err(StorageError::ConfigBackupMissing {
            path: path.to_path_buf(),
        });
    }
    if !path.is_file() {
        return Err(StorageError::ConfigBackupNotFile {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

pub(super) fn ensure_not_same_existing_file(left: &Path, right: &Path) -> Result<(), StorageError> {
    if !left.exists() || !right.exists() {
        return Ok(());
    }
    let left = left.canonicalize().ok();
    let right = right.canonicalize().ok();
    if let (Some(left), Some(right)) = (left, right)
        && left == right
    {
        return Err(StorageError::ConfigBackupSamePath { path: left });
    }
    Ok(())
}

pub(super) fn copy_config_database(from: &Path, to: &Path) -> Result<u64, StorageError> {
    validate_config_backup_source(from)?;
    ensure_parent_dir(to)?;
    std::fs::copy(from, to).map_err(|source| StorageError::ConfigBackupCopy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    })
}
