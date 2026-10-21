use zzclawterm_transport::{
    FileBrowserService, RemoteFilePath, SftpFileEntry, SftpFileType,
    file_browser_copy_destination_invalid, file_browser_join, file_browser_name,
};

use super::pane::PaneEndpoint;

#[derive(Clone)]
pub(in crate::features) struct FileManagerCopyPlan {
    pub source: PaneEndpoint,
    pub source_service: FileBrowserService,
    pub entries: Vec<SftpFileEntry>,
    pub target: PaneEndpoint,
    pub target_service: FileBrowserService,
    pub target_directory: RemoteFilePath,
    pub cut: bool,
}

impl FileManagerCopyPlan {
    pub(in crate::features) fn destination_for(&self, entry: &SftpFileEntry) -> String {
        let name = file_browser_name(self.source_service.kind(), &entry.path);
        file_browser_join(
            self.target_service.kind(),
            &self.target_directory.display_path,
            &name,
        )
    }

    pub(in crate::features) fn rejects(&self, entry: &SftpFileEntry) -> bool {
        if self.source != self.target {
            return false;
        }
        file_browser_copy_destination_invalid(
            self.source_service.kind(),
            &entry.path,
            &self.destination_for(entry),
            entry.file_type == SftpFileType::Directory || entry.is_directory(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FileManagerCopyPlan;
    use crate::features::file_manager::pane::PaneEndpoint;
    use zzclawterm_transport::{FileBrowserService, RemoteFilePath, SftpFileEntry, SftpFileType};

    fn file(path: &str) -> SftpFileEntry {
        SftpFileEntry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.to_string(),
            file_type: SftpFileType::Directory,
            size: None,
            permissions: None,
            owner: String::new(),
            group: String::new(),
            modified_at: None,
            raw_path_token: None,
            symlink_target_is_directory: false,
        }
    }

    #[test]
    fn frozen_plan_keeps_the_captured_target_after_later_path_changes() {
        let plan = FileManagerCopyPlan {
            source: PaneEndpoint::Local,
            source_service: FileBrowserService::local(),
            entries: vec![file("/tmp/src")],
            target: PaneEndpoint::Local,
            target_service: FileBrowserService::local(),
            target_directory: RemoteFilePath::new("/tmp/dest"),
            cut: false,
        };
        let captured = plan.target_directory.display_path.clone();
        assert_eq!(captured, "/tmp/dest");
        assert!(!plan.cut);
        assert!(plan.rejects(&file("/tmp/dest")));
        let later_directory = RemoteFilePath::new("/tmp/elsewhere");
        assert_eq!(plan.target_directory.display_path, captured);
        assert_ne!(
            plan.target_directory.display_path,
            later_directory.display_path
        );
    }
}
