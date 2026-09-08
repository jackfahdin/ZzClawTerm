use std::path::{Component, Path, PathBuf};

use super::{CloudSyncError, CloudSyncRemote, SYNC_SNAPSHOTS_DIR};

#[derive(Debug, Clone)]
pub struct LocalDirectoryRemote {
    root_dir: PathBuf,
}

impl LocalDirectoryRemote {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    fn path_for(&self, path: &str) -> Result<PathBuf, CloudSyncError> {
        let mut local = self.root_dir.clone();
        for component in Path::new(path).components() {
            match component {
                Component::Normal(part) => local.push(part),
                Component::CurDir => {}
                _ => {
                    return Err(CloudSyncError::InvalidRemotePath {
                        path: path.to_string(),
                    });
                }
            }
        }
        Ok(local)
    }
}

impl CloudSyncRemote for LocalDirectoryRemote {
    fn provider(&self) -> &'static str {
        "local_directory"
    }

    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        let path = self.path_for(path)?;
        std::fs::create_dir_all(&path).map_err(|source| CloudSyncError::CreateDir {
            path: path.clone(),
            source,
        })
    }

    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let path = self.path_for(path)?;
        if !path.exists() {
            return Ok(None);
        }
        std::fs::read(&path)
            .map(Some)
            .map_err(|source| CloudSyncError::ReadFile { path, source })
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        let path = self.path_for(path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CloudSyncError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(&path, bytes).map_err(|source| CloudSyncError::WriteFile { path, source })
    }

    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        let path = self.path_for(path)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(CloudSyncError::WriteFile { path, source }),
        }
    }

    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        let directory = self.path_for(path)?;
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let prefix = path.trim().trim_matches('/');
        std::fs::read_dir(&directory)
            .map_err(|source| CloudSyncError::ReadFile {
                path: directory.clone(),
                source,
            })?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|kind| kind.is_file())
                    .map(|_| entry.file_name().to_string_lossy().into_owned())
            })
            .map(|name| Ok(remote_path(prefix, &name)))
            .collect()
    }
}

pub fn remote_path(base_root: &str, child: &str) -> String {
    let root = base_root.trim().trim_matches('/');
    let child = child.trim().trim_start_matches('/');
    if root.is_empty() {
        child.to_string()
    } else if child.is_empty() {
        root.to_string()
    } else {
        format!("{root}/{child}")
    }
}

pub fn drive_remote_segments(base_root: &str, child: &str) -> Vec<String> {
    remote_path(base_root, child)
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn google_drive_query_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

pub fn legacy_sync_snapshot_file(revision: &str) -> String {
    format!("{SYNC_SNAPSHOTS_DIR}{revision}.redb.enc")
}
