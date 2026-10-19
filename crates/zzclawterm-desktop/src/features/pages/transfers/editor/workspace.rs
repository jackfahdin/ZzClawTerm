use std::fs::{self, DirBuilder};
use std::path::{Component, Path, PathBuf};

use super::helpers::sanitize_local_open_segment;

const WORKSPACE_PREFIX: &str = "zzclawterm-external-";

pub(super) fn external_edit_path(file_name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("{WORKSPACE_PREFIX}{}", uuid::Uuid::new_v4()))
        .join(sanitize_local_open_segment(file_name))
}

pub(super) struct ExternalEditWorkspace {
    // Deny rename/delete until the editor is launched on Windows, where
    // even a directory owned by the current user can otherwise become a junction.
    #[cfg(windows)]
    _directory: fs::File,
}

pub(super) fn prepare_external_edit_workspace(
    local_path: &Path,
) -> anyhow::Result<ExternalEditWorkspace> {
    prepare_workspace_in(&std::env::temp_dir(), local_path)
}

fn prepare_workspace_in(
    temp_root: &Path,
    local_path: &Path,
) -> anyhow::Result<ExternalEditWorkspace> {
    let relative = local_path.strip_prefix(temp_root)?;
    let components = relative.components().collect::<Vec<_>>();
    anyhow::ensure!(
        matches!(
            components.as_slice(),
            [Component::Normal(_), Component::Normal(_)]
        ),
        "External edit path escapes its private workspace"
    );
    let workspace = local_path.parent().expect("validated workspace path");
    let workspace_name = workspace.file_name().and_then(|name| name.to_str());
    anyhow::ensure!(
        workspace_name
            .and_then(|name| name.strip_prefix(WORKSPACE_PREFIX))
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()),
        "Invalid external edit workspace"
    );

    // Create one unpredictable directory directly beneath the trusted system
    // temp root. Never reuse an existing path or traverse pre-existing app/session
    // directories, which could be symlinks or Windows reparse points.
    let builder = DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder.create(workspace)?;
    let result = finish_workspace(workspace);
    if result.is_err() {
        let _ = fs::remove_dir(workspace);
    }
    result
}

fn finish_workspace(workspace: &Path) -> anyhow::Result<ExternalEditWorkspace> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };

        let directory = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(workspace)?;
        let metadata = directory.metadata()?;
        anyhow::ensure!(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "External edit workspace is not a regular directory"
        );
        crate::platform::windows_acl::set_current_user_only(workspace, true)?;
        Ok(ExternalEditWorkspace {
            _directory: directory,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = workspace;
        Ok(ExternalEditWorkspace {})
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{WORKSPACE_PREFIX, external_edit_path, prepare_workspace_in};

    #[test]
    fn external_edits_get_distinct_workspaces_and_safe_file_names() {
        let first = external_edit_path("../example.txt");
        let second = external_edit_path("../example.txt");
        assert_ne!(first.parent(), second.parent());
        assert_eq!(first.file_name().unwrap(), "_example.txt");
        assert!(first.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn workspace_exists_and_is_private_before_downloading() -> anyhow::Result<()> {
        let root = zzclawterm_core::test_support::TestTempDir::new("zzclawterm-external-edit");
        fs::create_dir(root.path())?;
        let directory = root.join(format!("{WORKSPACE_PREFIX}{}", uuid::Uuid::new_v4()));
        let target = directory.join("example.txt");
        let _workspace = prepare_workspace_in(&root, &target)?;
        assert!(directory.is_dir());
        assert!(!target.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&directory)?.permissions().mode() & 0o777,
                0o700
            );
        }
        #[cfg(windows)]
        crate::platform::windows_acl::assert_current_user_only(&directory, true);

        // Both the shell downloader and external editors stage files in this parent.
        let staged = directory.join(".download.tmp");
        fs::write(&staged, b"downloaded")?;
        fs::rename(&staged, &target)?;
        assert_eq!(fs::read(&target)?, b"downloaded");
        assert!(prepare_workspace_in(&root, &target).is_err());
        assert_eq!(fs::read(&target)?, b"downloaded");
        Ok(())
    }

    #[test]
    fn workspace_rejects_paths_outside_its_private_directory() -> anyhow::Result<()> {
        let root = zzclawterm_core::test_support::TestTempDir::new("zzclawterm-external-boundary");
        fs::create_dir(root.path())?;
        let directory = root.join(format!("{WORKSPACE_PREFIX}{}", uuid::Uuid::new_v4()));
        for target in [
            root.join("file.txt"),
            root.join("other/file.txt"),
            directory.join("../file.txt"),
            directory.join("nested/file.txt"),
            root.path().with_extension("outside").join("file.txt"),
        ] {
            assert!(prepare_workspace_in(&root, &target).is_err(), "{target:?}");
        }
        assert!(!directory.exists());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn workspace_rejects_junctions_and_blocks_directory_replacement() -> anyhow::Result<()> {
        use std::process::Command;

        let root = zzclawterm_core::test_support::TestTempDir::new("zzclawterm-external-junction");
        fs::create_dir(root.path())?;
        let outside = root.join("outside");
        fs::create_dir(&outside)?;
        let junction = root.join(format!("{WORKSPACE_PREFIX}{}", uuid::Uuid::new_v4()));
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()?;
        assert!(output.status.success(), "{output:?}");
        assert!(prepare_workspace_in(&root, &junction.join("file.txt")).is_err());
        assert!(!outside.join("file.txt").exists());
        fs::remove_dir(&junction)?;

        let directory = root.join(format!("{WORKSPACE_PREFIX}{}", uuid::Uuid::new_v4()));
        let workspace = prepare_workspace_in(&root, &directory.join("file.txt"))?;
        assert!(fs::rename(&directory, root.join("renamed")).is_err());
        assert!(fs::remove_dir(&directory).is_err());
        drop(workspace);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn workspace_rejects_symlink_directories() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let root = zzclawterm_core::test_support::TestTempDir::new("zzclawterm-external-symlink");
        fs::create_dir(root.path())?;
        let outside = root.join("outside");
        fs::create_dir(&outside)?;
        let link = root.join(format!("{WORKSPACE_PREFIX}{}", uuid::Uuid::new_v4()));
        symlink(&outside, &link)?;
        assert!(prepare_workspace_in(&root, &link.join("file.txt")).is_err());
        assert!(!outside.join("file.txt").exists());
        Ok(())
    }
}
