use crate::features::ZzClawTermApp;
use crate::features::transfers::file_operations::{
    FileOperationEndpoint, FileTransferIntent, FileTransferOperation,
};
use gpui::{Context, Window};
use rust_i18n::t;
use std::path::PathBuf;
use zzclawterm_files::file_browser::FileBrowserBackendKind;
#[cfg(test)]
use zzclawterm_files::file_browser::{
    FileBrowserService, file_browser_copy_destination_invalid, file_browser_join, file_browser_name,
    file_browser_path_is_root,
};
#[cfg(test)]
use zzclawterm_files::operations::{VerifiedCopyRequest, copy_verified};
use zzclawterm_files::sftp::{RemoteFilePath, SftpFileEntry, SftpFileType};
#[cfg(test)]
use zzclawterm_files::sftp::SftpTransferControl;
#[cfg(test)]
use zzclawterm_files::sftp_transfer_types::{
    SftpDuplicatePolicy, SftpDuplicateResolver, SftpPathTransferOptions,
};

#[cfg(test)]
fn invalid_copy_destination(
    kind: FileBrowserBackendKind,
    source: &str,
    target: &str,
    directory: bool,
) -> bool {
    zzclawterm_files::file_browser::file_browser_copy_destination_invalid(
        kind, source, target, directory,
    )
}

fn external_file_paths(cx: &gpui::App) -> Vec<PathBuf> {
    cx.read_from_clipboard()
        .and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                gpui::ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
                _ => None,
            })
        })
        .unwrap_or_default()
}

impl ZzClawTermApp {
    pub(in crate::features) fn can_paste_transfer_file_clipboard(&self, cx: &gpui::App) -> bool {
        self.transfer
            .file_clipboard()
            .is_some_and(|clipboard| clipboard.matches_system_clipboard(cx))
            || !external_file_paths(cx).is_empty()
    }

    pub(in crate::features) fn capture_transfer_file_clipboard(
        &mut self,
        cut: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.session.active_id_owned() else {
            return;
        };
        let endpoint =
            if self.session.active_file_browser_backend() == Some(FileBrowserBackendKind::Local) {
                FileOperationEndpoint::Local
            } else {
                FileOperationEndpoint::Session(id)
            };
        let entries = self.selected_transfer_entries();
        self.capture_file_clipboard(endpoint, entries, cut, cx);
    }

    pub(in crate::features) fn capture_file_clipboard(
        &mut self,
        source: FileOperationEndpoint,
        entries: Vec<SftpFileEntry>,
        cut: bool,
        cx: &mut Context<Self>,
    ) {
        if entries.is_empty() {
            self.report_transfer_entry_error(
                "file-clipboard",
                source.kind(),
                &zzclawterm_files::file_browser::FileTransferEntryError::NoSelection.into(),
                cx,
            );
            return;
        }
        if entries.iter().any(|entry| {
            zzclawterm_files::file_browser::file_browser_path_is_root(source.kind(), &entry.path)
                || matches!(entry.name.as_str(), "." | "..")
        }) {
            self.report_transfer_entry_error(
                "file-clipboard",
                source.kind(),
                &zzclawterm_files::file_browser::FileTransferEntryError::InvalidDestination.into(),
                cx,
            );
            return;
        }
        let entries = if source.kind() == FileBrowserBackendKind::Local {
            entries
                .iter()
                .filter(|entry| {
                    !entries.iter().any(|parent| {
                        parent.path != entry.path
                            && parent.is_directory()
                            && zzclawterm_files::file_browser::file_browser_copy_destination_invalid(
                                FileBrowserBackendKind::Local,
                                &parent.path,
                                &entry.path,
                                true,
                            )
                    })
                })
                .cloned()
                .collect()
        } else {
            entries
        };
        // Local listings carry a local identity token, not a base64 SFTP wire path.
        let entries = if source.kind() == FileBrowserBackendKind::Local {
            entries
        } else {
            match zzclawterm_files::drag_export::tree::prune_nested_roots(entries) {
                Ok(entries) => entries,
                Err(error) => {
                    self.report_browser_transfer_error("file-clipboard", &error, cx);
                    return;
                }
            }
        };
        if entries.iter().any(|entry| {
            !matches!(
                entry.file_type,
                SftpFileType::File | SftpFileType::Directory
            )
        }) {
            self.report_browser_transfer_error(
                "file-clipboard",
                &zzclawterm_files::file_browser::FileTransferEntryError::UnsupportedFileType.into(),
                cx,
            );
            return;
        }
        if cfg!(target_os = "windows") && cx.clipboard_revision().is_some() && !cut {
            if let Err(error) = self.publish_file_clipboard(&source, &entries, cx) {
                self.report_browser_transfer_error("file-clipboard", &error, cx);
                return;
            }
        } else {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_metadata(
                String::new(),
                zzclawterm_core::uuid(),
            ));
        }
        let revision = cx.clipboard_revision();
        let item = if revision.is_none() {
            cx.read_from_clipboard()
        } else {
            None
        };
        self.transfer
            .set_file_clipboard(source, entries, cut, item, revision);
        self.notify_operation_at(
            cx.active_window(),
            "file-clipboard",
            zzclawterm_ui::notification::ZzClawNotificationKind::Success,
            t!("common.operationSucceeded").to_string(),
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn paste_transfer_file_clipboard(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.session.active_id_owned() else {
            return;
        };
        let target =
            if self.session.active_file_browser_backend() == Some(FileBrowserBackendKind::Local) {
                FileOperationEndpoint::Local
            } else {
                FileOperationEndpoint::Session(id)
            };
        self.paste_file_clipboard(
            target,
            RemoteFilePath::new(self.transfer_browser_operation_target_directory()),
            cx,
        );
    }

    pub(in crate::features) fn paste_file_clipboard(
        &mut self,
        target: FileOperationEndpoint,
        directory: RemoteFilePath,
        cx: &mut Context<Self>,
    ) {
        let internal = self
            .transfer
            .file_clipboard()
            .cloned()
            .filter(|clipboard| clipboard.matches_system_clipboard(cx));
        let Some(clipboard) = internal else {
            let paths = external_file_paths(cx);
            if !paths.is_empty() {
                self.upload_external_paths_to_endpoint(target, directory, paths, cx);
            } else {
                self.notify_operation_at(
                    cx.active_window(),
                    "file-clipboard",
                    zzclawterm_ui::notification::ZzClawNotificationKind::Warning,
                    t!("fileExplorer.clipboardEmpty").to_string(),
                    cx,
                );
            }
            return;
        };
        let entries = clipboard
            .entries
            .into_iter()
            .filter(|entry| {
                !clipboard.cut
                    || !self
                        .transfer
                        .cut_path_in_flight(clipboard.generation, &entry.identity_key())
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            self.notify_operation_at(
                cx.active_window(),
                "file-clipboard",
                zzclawterm_ui::notification::ZzClawNotificationKind::Warning,
                t!("fileExplorer.clipboardMoving").to_string(),
                cx,
            );
            return;
        }
        self.submit_file_transfer(
            "file-clipboard".into(),
            FileTransferIntent {
                source: clipboard.source,
                entries,
                target,
                target_directory: directory,
                operation: if clipboard.cut {
                    FileTransferOperation::Move
                } else {
                    FileTransferOperation::Copy
                },
            },
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TransferFileClipboard, invalid_copy_destination, local_clipboard_entry,
        prefer_external_paths,
    };
    use std::path::PathBuf;
    use zzclawterm_files::file_browser::FileBrowserBackendKind;
    use zzclawterm_files::sftp::SftpFileType;

    #[test]
    fn clipboard_copy_reports_partial_progress_before_the_finished_output() {
        use std::sync::{Arc, Mutex};
        use zzclawterm_files::file_browser::FileBrowserService;
        use zzclawterm_files::operations::{
            FileOperationResult, FileOperationSummary, VerifiedCopyRequest, copy_verified,
        };
        use zzclawterm_files::sftp::{RemoteFilePath, SftpTransferControl};
        use zzclawterm_files::sftp_transfer_types::SftpPathTransferOptions;

        let root = zzclawterm_core::test_support::TestTempDir::new("zzclawterm-clipboard-progress");
        std::fs::create_dir_all(root.path()).unwrap();
        let source = root.join("source");
        let target = root.join("target");
        let payload = vec![7_u8; 512 * 1024];
        std::fs::write(&source, &payload).unwrap();
        let values = Arc::new(Mutex::new(Vec::new()));
        let record = values.clone();
        let output = copy_verified(
            VerifiedCopyRequest {
                source_service: FileBrowserService::local(),
                target_service: FileBrowserService::local(),
                source: RemoteFilePath::new(source.to_string_lossy()),
                target_path: RemoteFilePath::new(target.to_string_lossy()),
                cut: false,
                control: SftpTransferControl::new(),
                options: SftpPathTransferOptions::default(),
            },
            move |p| record.lock().unwrap().push(p),
        )
        .unwrap();
        assert!(
            matches!(output, FileOperationResult::Completed(FileOperationSummary { bytes, .. }) if bytes == payload.len() as u64)
        );
        let values = values.lock().unwrap();
        assert!(
            values
                .iter()
                .any(|p| p.bytes_transferred > 0 && p.bytes_transferred < payload.len() as u64)
        );
        assert_eq!(
            values.last().unwrap().bytes_transferred,
            payload.len() as u64
        );
        assert_eq!(std::fs::read(target).unwrap(), payload);
    }

    #[test]
    fn local_cut_deletes_source_only_after_a_successful_copy() {
        use zzclawterm_files::file_browser::FileBrowserService;
        use zzclawterm_files::operations::{VerifiedCopyRequest, copy_verified};
        use zzclawterm_files::sftp::RemoteFilePath;
        use zzclawterm_files::sftp::SftpTransferControl;
        use zzclawterm_files::sftp_transfer_types::SftpPathTransferOptions;
        let root = zzclawterm_core::test_support::TestTempDir::new("zzclawterm-clipboard-cut");
        std::fs::create_dir_all(root.path()).unwrap();
        // macOS temporary paths can contain symlink ancestors such as /var.
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let source = canonical_root.join("source.txt");
        let target = canonical_root.join("target.txt");
        std::fs::write(&source, b"payload").unwrap();
        copy_verified(
            VerifiedCopyRequest {
                source_service: FileBrowserService::local(),
                target_service: FileBrowserService::local(),
                source: RemoteFilePath::new(source.to_string_lossy()),
                target_path: RemoteFilePath::new(target.to_string_lossy()),
                cut: true,
                control: SftpTransferControl::new(),
                options: SftpPathTransferOptions::default(),
            },
            |_| {},
        )
        .unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(target).unwrap(), b"payload");
    }

    #[test]
    fn cancelled_cut_keeps_the_source() {
        use zzclawterm_files::file_browser::FileBrowserService;
        use zzclawterm_files::operations::{VerifiedCopyRequest, copy_verified};
        use zzclawterm_files::sftp::RemoteFilePath;
        use zzclawterm_files::sftp::SftpTransferControl;
        use zzclawterm_files::sftp_transfer_types::SftpPathTransferOptions;
        let root =
            zzclawterm_core::test_support::TestTempDir::new("zzclawterm-clipboard-cut-cancel");
        std::fs::create_dir_all(root.path()).unwrap();
        // Exercise cancellation without failing the symlink-ancestor check first.
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let source = canonical_root.join("source.txt");
        let target = canonical_root.join("target.txt");
        std::fs::write(&source, b"keep-me").unwrap();
        let control = SftpTransferControl::new();
        control.cancel();
        let result = copy_verified(
            VerifiedCopyRequest {
                source_service: FileBrowserService::local(),
                target_service: FileBrowserService::local(),
                source: RemoteFilePath::new(source.to_string_lossy()),
                target_path: RemoteFilePath::new(target.to_string_lossy()),
                cut: true,
                control,
                options: SftpPathTransferOptions::default(),
            },
            |_| {},
        );
        assert!(result.is_err());
        assert!(source.exists());
        assert_eq!(std::fs::read(source).unwrap(), b"keep-me");
    }

    #[test]
    fn remote_copy_rejects_self_descendant_and_root_after_normalization() {
        for (source, target, directory) in [
            ("/src", "/src/./", false),
            ("/src", "/src/child/file", true),
            ("/", "/other", true),
        ] {
            assert!(invalid_copy_destination(
                FileBrowserBackendKind::Remote,
                source,
                target,
                directory
            ));
        }
        assert!(!invalid_copy_destination(
            FileBrowserBackendKind::Remote,
            "/src",
            "/other/file",
            true
        ));
    }
}
