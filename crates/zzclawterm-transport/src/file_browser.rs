use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;

use crate::{
    FileTransferEndpoint, LocalFileService, RemoteBinaryFile, RemoteFilePath, RemoteFileService,
    RemoteTextDocument, RemoteTextRevision, RemoteTextWriteResult, SftpAttributeUpdate,
    SftpFileEntry, SftpFileProperties, SftpRemoteTextFile,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileBrowserBackendKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileBrowserCapabilities {
    pub ordinary_transfer: bool,
    pub create_symlink: bool,
    pub edit_attributes: bool,
    pub direct_external_open: bool,
}

impl FileBrowserCapabilities {
    pub const LOCAL: Self = Self {
        ordinary_transfer: false,
        create_symlink: false,
        edit_attributes: false,
        direct_external_open: true,
    };
    pub const REMOTE: Self = Self {
        ordinary_transfer: true,
        create_symlink: true,
        edit_attributes: true,
        direct_external_open: false,
    };
}

#[derive(Clone)]
pub enum FileBrowserService {
    Local(LocalFileService),
    Remote(Arc<RemoteFileService>),
}

impl std::fmt::Debug for FileBrowserService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("FileBrowserService")
            .field(&self.kind())
            .finish()
    }
}

impl FileBrowserService {
    pub fn local() -> Self {
        Self::Local(LocalFileService)
    }

    pub fn remote(service: RemoteFileService) -> Self {
        Self::Remote(Arc::new(service))
    }

    pub fn kind(&self) -> FileBrowserBackendKind {
        match self {
            Self::Local(_) => FileBrowserBackendKind::Local,
            Self::Remote(_) => FileBrowserBackendKind::Remote,
        }
    }

    pub fn capabilities(&self) -> FileBrowserCapabilities {
        match self {
            Self::Local(_) => FileBrowserCapabilities::LOCAL,
            Self::Remote(_) => FileBrowserCapabilities::REMOTE,
        }
    }

    pub fn home_dir(&self) -> anyhow::Result<String> {
        match self {
            Self::Local(service) => service.home_dir(),
            Self::Remote(service) => service.home_dir(),
        }
    }

    pub fn list_dir(&self, path: impl AsRef<str>) -> anyhow::Result<Vec<SftpFileEntry>> {
        match self {
            Self::Local(service) => service.list_dir(resolve_local_path(path.as_ref())),
            Self::Remote(service) => service.list_dir(path),
        }
    }

    pub fn list_dir_path(&self, path: &RemoteFilePath) -> anyhow::Result<Vec<SftpFileEntry>> {
        match self {
            Self::Local(service) => service.list_dir(resolve_local_path(&path.display_path)),
            Self::Remote(service) => service.list_dir_path(path),
        }
    }

    pub fn create_dir_path(&self, path: impl AsRef<str>, mode: Option<u32>) -> anyhow::Result<()> {
        match self {
            Self::Local(service) => service.create_dir(resolve_local_path(path.as_ref()), mode),
            Self::Remote(service) => service.create_dir_path(path, mode),
        }
    }

    pub fn create_file_path(&self, path: impl AsRef<str>, mode: Option<u32>) -> anyhow::Result<()> {
        match self {
            Self::Local(service) => service.create_file(resolve_local_path(path.as_ref()), mode),
            Self::Remote(service) => service.create_file_path(path, mode),
        }
    }

    pub fn create_symlink_path(
        &self,
        link_path: impl AsRef<str>,
        target_path: impl AsRef<str>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Local(_) => bail!("creating symbolic links is unavailable for local browsing"),
            Self::Remote(service) => service.create_symlink_path(link_path, target_path),
        }
    }

    pub fn delete_remote_path(&self, path: &RemoteFilePath) -> anyhow::Result<()> {
        match self {
            Self::Local(service) => service.delete(resolve_local_path(&path.display_path)),
            Self::Remote(service) => service.delete_remote_path(path),
        }
    }

    pub fn rename_remote_paths(
        &self,
        old_path: &RemoteFilePath,
        new_path: &RemoteFilePath,
    ) -> anyhow::Result<()> {
        match self {
            Self::Local(service) => service.rename(
                resolve_local_path(&old_path.display_path),
                resolve_local_path(&new_path.display_path),
            ),
            Self::Remote(service) => service.rename_remote_paths(old_path, new_path),
        }
    }

    pub fn remote_file_properties(
        &self,
        path: &RemoteFilePath,
    ) -> anyhow::Result<SftpFileProperties> {
        match self {
            Self::Local(service) => service.properties(resolve_local_path(&path.display_path)),
            Self::Remote(service) => service.remote_file_properties(path),
        }
    }

    pub fn update_remote_path_attributes(
        &self,
        path: &RemoteFilePath,
        update: SftpAttributeUpdate,
    ) -> anyhow::Result<()> {
        match self {
            Self::Local(_) => bail!("editing attributes is unavailable for local browsing"),
            Self::Remote(service) => service.update_remote_path_attributes(path, update),
        }
    }

    pub fn read_text_file_path(
        &self,
        path: &RemoteFilePath,
        max_bytes: u64,
    ) -> anyhow::Result<SftpRemoteTextFile> {
        match self {
            Self::Local(service) => {
                service.read_text_file(resolve_local_path(&path.display_path), max_bytes)
            }
            Self::Remote(service) => service.read_text_file_path(path, max_bytes),
        }
    }

    pub fn read_text_document_path(
        &self,
        path: &RemoteFilePath,
        max_bytes: u64,
    ) -> anyhow::Result<RemoteTextDocument> {
        match self {
            Self::Local(service) => {
                service.read_text_document(resolve_local_path(&path.display_path), max_bytes)
            }
            Self::Remote(service) => service.read_text_document_path(path, max_bytes),
        }
    }

    pub fn write_text_document_path(
        &self,
        path: &RemoteFilePath,
        content: impl AsRef<str>,
        expected_revision: Option<&RemoteTextRevision>,
        force: bool,
    ) -> anyhow::Result<RemoteTextWriteResult> {
        match self {
            Self::Local(service) => service.write_text_document(
                resolve_local_path(&path.display_path),
                content,
                expected_revision,
                force,
            ),
            Self::Remote(service) => {
                service.write_text_document_path(path, content, expected_revision, force)
            }
        }
    }

    pub fn read_file_bytes_path(
        &self,
        path: &RemoteFilePath,
        max_bytes: u64,
    ) -> anyhow::Result<RemoteBinaryFile> {
        match self {
            Self::Local(service) => {
                service.read_file_bytes(resolve_local_path(&path.display_path), max_bytes)
            }
            Self::Remote(service) => service.read_file_bytes_path(path, max_bytes),
        }
    }

    pub fn transfer_endpoint(&self, path: &RemoteFilePath) -> FileTransferEndpoint {
        match self {
            Self::Local(_) => FileTransferEndpoint::Local(resolve_local_path(&path.display_path)),
            Self::Remote(service) => FileTransferEndpoint::Remote {
                service: service.clone(),
                path: path.clone(),
            },
        }
    }

    pub fn local_path(&self, path: &RemoteFilePath) -> Option<PathBuf> {
        matches!(self, Self::Local(_)).then(|| resolve_local_path(&path.display_path))
    }

    pub fn remote_service(&self) -> Option<Arc<RemoteFileService>> {
        match self {
            Self::Local(_) => None,
            Self::Remote(service) => Some(service.clone()),
        }
    }
}

fn resolve_local_path(path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|current| current.join(&path))
            .unwrap_or(path)
    }
}

pub fn file_browser_name(kind: FileBrowserBackendKind, path: &str) -> String {
    match kind {
        FileBrowserBackendKind::Local => Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string()),
        FileBrowserBackendKind::Remote => path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .to_string(),
    }
}

pub fn file_browser_parent(kind: FileBrowserBackendKind, path: &str) -> String {
    match kind {
        FileBrowserBackendKind::Local => Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .filter(|parent| !parent.is_empty())
            .unwrap_or_else(|| path.to_string()),
        FileBrowserBackendKind::Remote => {
            let path = path.trim_end_matches('/');
            match path.rfind('/') {
                Some(0) => "/".to_string(),
                Some(index) => path[..index].to_string(),
                None => ".".to_string(),
            }
        }
    }
}

pub fn file_browser_join(kind: FileBrowserBackendKind, parent: &str, child: &str) -> String {
    match kind {
        FileBrowserBackendKind::Local => {
            Path::new(parent).join(child).to_string_lossy().into_owned()
        }
        FileBrowserBackendKind::Remote => {
            let absolute = parent.starts_with('/');
            let parent = parent.trim_end_matches('/');
            match parent {
                "" if absolute => format!("/{child}"),
                "" | "." => child.to_string(),
                parent => format!("{parent}/{child}"),
            }
        }
    }
}

pub fn valid_file_browser_child_name(kind: FileBrowserBackendKind, name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && (kind == FileBrowserBackendKind::Remote || !cfg!(windows) || !name.contains('\\'))
}

#[cfg(test)]
mod tests {
    use super::{
        FileBrowserBackendKind, file_browser_join, file_browser_name, file_browser_parent,
        valid_file_browser_child_name,
    };

    #[test]
    fn remote_paths_keep_posix_semantics() {
        assert_eq!(
            file_browser_join(FileBrowserBackendKind::Remote, "/", "tmp"),
            "/tmp"
        );
        assert_eq!(
            file_browser_parent(FileBrowserBackendKind::Remote, "/tmp"),
            "/"
        );
        assert_eq!(
            file_browser_name(FileBrowserBackendKind::Remote, "/tmp/a"),
            "a"
        );
        assert!(valid_file_browser_child_name(
            FileBrowserBackendKind::Remote,
            "a\\b"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn local_paths_keep_windows_roots_and_separators() {
        assert_eq!(
            file_browser_parent(FileBrowserBackendKind::Local, r"C:\tmp"),
            r"C:\"
        );
        assert_eq!(
            file_browser_join(FileBrowserBackendKind::Local, r"C:\", "tmp"),
            r"C:\tmp"
        );
        assert_eq!(
            file_browser_name(FileBrowserBackendKind::Local, r"C:\tmp\a"),
            "a"
        );
        assert!(!valid_file_browser_child_name(
            FileBrowserBackendKind::Local,
            "a\\b"
        ));
    }
}
