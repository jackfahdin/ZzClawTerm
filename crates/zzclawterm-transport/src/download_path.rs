//! Download target name validation and local path boundary checks.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

#[cfg(windows)]
mod windows;

/// Unlike Path::exists, treat dangling links as occupied and propagate probe errors.
pub(crate) fn target_exists(target: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(target) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Reject existing symbolic links between the trusted download root and the target.
///
/// The user-selected directory is the trust root; existing components below it must
/// not be symbolic links. Callers should recheck after creating missing directories
/// rather than trust `create_dir_all` to safely resolve remote-controlled names.
/// File-writing entry points separately validate regular-file targets.
pub(crate) fn ensure_no_symlink(root: &Path, target: &Path) -> anyhow::Result<()> {
    let root = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    let relative = if root == Path::new(".") {
        target.strip_prefix("./").unwrap_or(target)
    } else {
        target
            .strip_prefix(root)
            .map_err(|_| anyhow::anyhow!("download target escapes its selected directory"))?
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("download target contains an unsafe path component")
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                anyhow::ensure!(
                    !metadata.file_type().is_symlink(),
                    "download target contains a symbolic link"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    anyhow::ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        "download target contains a Windows reparse point"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Return an existing regular file's permissions and validate the download boundary.
fn existing_file_permissions(
    root: &Path,
    target: &Path,
) -> anyhow::Result<Option<std::fs::Permissions>> {
    ensure_no_symlink(root, target)?;
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            anyhow::ensure!(metadata.is_file(), "download target is not a regular file");
            Ok(Some(metadata.permissions()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Return existing target permissions and probe write access for the current process.
pub(crate) fn writable_file_permissions(
    root: &Path,
    target: &Path,
) -> anyhow::Result<Option<std::fs::Permissions>> {
    let permissions = existing_file_permissions(root, target)?;
    if permissions.is_some() {
        // Probe write access without truncating the target. The owner write bit alone
        // does not prove that the current user is allowed to modify the file.
        let probe = OpenOptions::new().write(true).open(target)?;
        drop(probe);
    }
    Ok(permissions)
}

fn sanitize_existing_permissions(
    target: &fs::File,
    permissions: std::fs::Permissions,
) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        // Downloaded content must not inherit setuid/setgid/sticky bits; retain only rwx bits.
        if permissions.mode() & 0o7000 != 0 {
            let mut safe_permissions = permissions;
            safe_permissions.set_mode(safe_permissions.mode() & 0o777);
            target.set_permissions(safe_permissions)?;
        }
    }
    #[cfg(not(unix))]
    let _ = (target, permissions);
    Ok(())
}

/// Return the trusted download root containing the target.
///
/// `Path::parent` returns an empty path for a bare filename. Normalize it to the current
/// directory so the temporary file's `./` prefix is not rejected as a `CurDir` component
/// by the path boundary check.
pub(crate) fn root_for_target(target: &Path) -> &Path {
    target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Clean up the temporary path on drop; take ownership only after successful creation
/// to avoid deleting a file owned by someone else.
pub(crate) struct DownloadTemporary {
    path: PathBuf,
    replace_existing: bool,
}

struct NewTargetGuard {
    path: PathBuf,
    file: Option<fs::File>,
    committed: bool,
}

impl NewTargetGuard {
    fn create(target: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        Ok(Self {
            path: target.to_path_buf(),
            file: Some(file),
            committed: false,
        })
    }

    fn file_mut(&mut self) -> &mut fs::File {
        self.file
            .as_mut()
            .expect("new download target file must remain open until commit")
    }

    fn commit(mut self) -> io::Result<()> {
        self.file_mut().sync_all()?;
        self.file.take();
        self.committed = true;
        Ok(())
    }
}

impl Drop for NewTargetGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

fn populate_new_target<R: Read>(source: &mut R, target: &Path) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let mut destination = NewTargetGuard::create(target)?;
    io::copy(source, destination.file_mut())
        .context("download commit failed while populating new target")?;
    destination
        .commit()
        .context("failed to sync committed download")
}

impl DownloadTemporary {
    /// Copy the resume prefix during preparation, before entering the download loop.
    /// Any preparation failure leaves the original file intact.
    pub(crate) async fn prepare_async(
        root: &Path,
        target: &Path,
        resume_offset: u64,
    ) -> anyhow::Result<(Self, tokio::fs::File)> {
        let root = root.to_path_buf();
        let target = target.to_path_buf();
        let (file, temporary) = tokio::task::spawn_blocking(move || {
            use std::io::Read as _;
            let (temporary, mut file) = Self::create(&root, &target)?;
            if resume_offset > 0 {
                ensure_no_symlink(&root, &target)?;
                let source = fs::File::open(&target)?;
                let copied = io::copy(&mut source.take(resume_offset), &mut file)?;
                anyhow::ensure!(copied == resume_offset, "local resume source changed");
                file.flush()?;
            }
            // Close the handle before dropping the path guard if the result is never received.
            Ok::<_, anyhow::Error>((file, temporary))
        })
        .await??;
        Ok((temporary, tokio::fs::File::from_std(file)))
    }

    pub(crate) fn create(root: &Path, target: &Path) -> anyhow::Result<(Self, fs::File)> {
        let replace_existing = existing_file_permissions(root, target)?.is_some();
        let path = root_for_target(target)
            .join(format!(".zzclawterm-download-{}", zzclawterm_core::uuid()));
        ensure_no_symlink(root, &path)?;
        let temporary = Self {
            path,
            replace_existing,
        };
        #[cfg(not(windows))]
        let file = {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            options.open(&temporary.path)?
        };
        #[cfg(windows)]
        let file = windows::create_private_file(&temporary.path)?;
        Ok((temporary, file))
    }

    /// Write existing targets through their original inode to preserve local permissions and
    /// ownership. New targets are created normally in the destination directory and populated
    /// from the temporary file without replacing a name that appeared during the transfer. The
    /// root and its ancestors must be user-controlled because path checks do not protect against
    /// concurrent directory replacement.
    fn commit(self, root: &Path, target: &Path) -> anyhow::Result<()> {
        let permissions = writable_file_permissions(root, target)?;
        anyhow::ensure!(
            self.replace_existing || permissions.is_none(),
            "download target appeared during transfer"
        );
        ensure_no_symlink(root, &self.path)?;
        if let Some(permissions) = permissions {
            let mut source = fs::File::open(&self.path)?;
            let mut destination = OpenOptions::new().write(true).open(target)?;
            anyhow::ensure!(
                destination.metadata()?.is_file(),
                "download target is not a regular file"
            );
            // Clear special permission bits before writing, using the same file handle throughout.
            sanitize_existing_permissions(&destination, permissions)?;
            destination.set_len(0)?;
            use anyhow::Context as _;
            io::copy(&mut source, &mut destination)
                .context("download commit failed; target may contain partial data")?;
            destination
                .sync_all()
                .context("failed to sync committed download")?;
        } else {
            let mut source = fs::File::open(&self.path)?;
            populate_new_target(&mut source, target)?;
        }
        Ok(())
    }

    /// Move the commit and guard into the blocking pool so dropping the outer future
    /// does not interrupt the file writeback.
    pub(crate) async fn commit_async(self, root: &Path, target: &Path) -> anyhow::Result<()> {
        let root = root.to_path_buf();
        let target = target.to_path_buf();
        tokio::task::spawn_blocking(move || self.commit(&root, &target)).await?
    }
}

impl Drop for DownloadTemporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Stage contents in a temporary file in the destination directory, then commit while preserving
/// existing inodes and the operating system's normal creation permissions for new targets.
pub(crate) fn staged_write_file(root: &Path, target: &Path, contents: &[u8]) -> anyhow::Result<()> {
    writable_file_permissions(root, target)?;
    let (temporary, mut file) = DownloadTemporary::create(root, target)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    temporary.commit(root, target)
}

/// Extract and validate the final component of a remote POSIX path.
pub fn file_name(remote_path: &str) -> anyhow::Result<&str> {
    let name = remote_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    validate_name(name)?;
    Ok(name)
}

/// Ignore non-downloadable directory entries before interpreting their local names.
/// Explicit downloads use separate source validation and still reject these types.
pub(crate) fn validate_directory_entry(
    name: &str,
    file_type: crate::SftpFileType,
) -> anyhow::Result<bool> {
    if !matches!(
        file_type,
        crate::SftpFileType::File | crate::SftpFileType::Directory
    ) || matches!(name, "." | "..")
    {
        return Ok(false);
    }
    validate_name(name)?;
    Ok(true)
}

/// Validate that a name is a single normal local filesystem component.
///
/// Remote names are server-controlled and must not introduce path separators, parent
/// components, or control characters into a local `PathBuf`. On Windows, also reject
/// reserved device names and trailing spaces or dots to prevent platform-specific
/// path interpretation.
pub fn validate_name(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\', '\0'])
            && !name.chars().any(char::is_control),
        "unsafe remote download name"
    );

    #[cfg(windows)]
    {
        anyhow::ensure!(
            !name.ends_with(' ')
                && !name.ends_with('.')
                && !name.contains(['<', '>', ':', '"', '|', '?', '*']),
            "unsupported Windows remote download name"
        );
        let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ["COM", "LPT"].iter().any(|prefix| {
                stem.strip_prefix(prefix).is_some_and(|suffix| {
                    matches!(
                        suffix,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                })
            });
        anyhow::ensure!(!reserved, "reserved Windows remote download name");
    }

    Ok(())
}

/// Generate a local name comparison key for batch downloads.
pub fn target_key(name: &str) -> String {
    #[cfg(any(windows, target_os = "macos"))]
    {
        use unicode_normalization::UnicodeNormalization as _;

        name.nfc().collect::<String>().to_lowercase()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn ignored_directory_entries_do_not_block_regular_downloads() -> anyhow::Result<()> {
        use crate::SftpFileType;
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-filtered-download-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir(&root)?;
        for (name, kind) in [
            ("bad\\link", SftpFileType::Symlink),
            ("bad\\special", SftpFileType::Other),
            ("CON", SftpFileType::Symlink),
            ("regular", SftpFileType::File),
        ] {
            if super::validate_directory_entry(name, kind)? {
                super::staged_write_file(&root, &root.join(name), b"contents")?;
            }
        }
        assert_eq!(std::fs::read(root.join("regular"))?, b"contents");
        assert_eq!(std::fs::read_dir(&root)?.count(), 1);
        assert!(super::validate_directory_entry("bad\\file", SftpFileType::File).is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn commit_does_not_overwrite_a_target_created_during_download() -> anyhow::Result<()> {
        use std::io::Write as _;
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-commit-conflict-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir(&root)?;
        let target = root.join("download");
        let (temporary, mut file) = super::DownloadTemporary::create(&root, &target)?;
        let temporary_path = temporary.path.clone();
        file.write_all(b"download")?;
        drop(file);
        std::fs::write(&target, b"other writer")?;
        assert!(temporary.commit(&root, &target).is_err());
        assert_eq!(std::fs::read(&target)?, b"other writer");
        assert!(!temporary_path.exists());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn new_downloads_use_normal_creation_permissions() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;

        let root =
            std::env::temp_dir().join(format!("zzclawterm-final-mode-{}", zzclawterm_core::uuid()));
        std::fs::create_dir(&root)?;
        let reference = root.join("reference");
        std::fs::write(&reference, b"normal")?;
        let expected = std::fs::metadata(&reference)?.permissions().mode() & 0o777;
        let target = root.join("download");
        super::staged_write_file(&root, &target, b"downloaded")?;
        assert_eq!(
            std::fs::metadata(&target)?.permissions().mode() & 0o777,
            expected
        );

        let target = root.join("async-download");
        let (temporary, mut file) =
            super::DownloadTemporary::prepare_async(&root, &target, 0).await?;
        use tokio::io::AsyncWriteExt as _;
        file.write_all(b"downloaded").await?;
        file.flush().await?;
        drop(file);
        temporary.commit_async(&root, &target).await?;
        assert_eq!(
            std::fs::metadata(&target)?.permissions().mode() & 0o777,
            expected
        );
        assert_eq!(std::fs::read(&target)?, b"downloaded");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    struct FailingReader {
        first: Option<&'static [u8]>,
    }

    impl std::io::Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if let Some(bytes) = self.first.take() {
                let length = bytes.len().min(buffer.len());
                buffer[..length].copy_from_slice(&bytes[..length]);
                return Ok(length);
            }
            Err(std::io::Error::other("injected read failure"))
        }
    }

    #[test]
    fn failed_new_target_commit_removes_partial_final_file() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-commit-cleanup-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir(&root)?;
        let target = root.join("download");
        let mut source = FailingReader {
            first: Some(b"partial"),
        };
        assert!(super::populate_new_target(&mut source, &target).is_err());
        assert!(!target.exists());
        assert_eq!(std::fs::read_dir(&root)?.count(), 0);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn failed_resume_preparation_preserves_source_and_cleans_temporary() -> anyhow::Result<()>
    {
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-resume-prepare-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir(&root)?;
        let target = root.join("partial");
        std::fs::write(&target, b"prefix")?;
        assert!(
            super::DownloadTemporary::prepare_async(&root, &target, 100)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&target)?, b"prefix");
        assert_eq!(std::fs::read_dir(&root)?.count(), 1);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn temporary_prefix_is_private_and_dropped_without_committing() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-download-prefix-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir(&root)?;
        let target = root.join("partial");
        std::fs::write(&target, b"prefix")?;
        let (temporary, file) = super::DownloadTemporary::prepare_async(&root, &target, 6).await?;
        let path = temporary.path.clone();
        assert_eq!(path.parent(), Some(root.as_path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o077, 0);
        }
        assert_eq!(std::fs::read(&path)?, b"prefix");
        drop(file);
        drop(temporary);
        assert!(!path.exists());
        assert_eq!(std::fs::read(&target)?, b"prefix");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    use super::{
        existing_file_permissions, file_name, root_for_target, staged_write_file, target_key,
        validate_name, writable_file_permissions,
    };

    #[test]
    fn rejects_path_components_and_preserves_safe_whitespace() {
        for name in ["", ".", "..", "../file", "..\\file", "/file", "a\0b"] {
            assert!(validate_name(name).is_err(), "{name:?}");
        }
        assert_eq!(file_name("/srv/report name").unwrap(), "report name");
        // Windows rejects trailing spaces; other platforms must preserve them without trimming.
        #[cfg(not(windows))]
        {
            assert_eq!(file_name("/srv/report ").unwrap(), "report ");
            assert_eq!(file_name("/srv/.. ").unwrap(), ".. ");
        }
        #[cfg(windows)]
        {
            assert!(file_name("/srv/report ").is_err());
            assert!(file_name("/srv/.. ").is_err());
        }
    }

    #[test]
    fn target_key_matches_the_common_case_insensitive_platform_rule() {
        #[cfg(any(windows, target_os = "macos"))]
        assert_eq!(target_key("Readme"), target_key("README"));
        #[cfg(not(any(windows, target_os = "macos")))]
        assert_ne!(target_key("Readme"), target_key("README"));
    }

    #[test]
    fn bare_relative_targets_use_the_current_directory_as_root() {
        let target = Path::new("result.txt");
        let root = root_for_target(target);
        assert_eq!(root, Path::new("."));
        assert!(super::ensure_no_symlink(root, Path::new("./result.txt")).is_ok());
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn target_key_matches_platform_unicode_equivalence() {
        assert_eq!(target_key("e\u{301}.txt"), target_key("é.txt"));
    }

    #[test]
    fn staged_write_replaces_regular_file_and_rejects_directory() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-atomic-download-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        let target = root.join("result.txt");
        staged_write_file(&root, &target, b"first")?;
        assert_eq!(std::fs::read(&target)?, b"first");
        staged_write_file(&root, &target, b"second")?;
        assert_eq!(std::fs::read(&target)?, b"second");

        let directory = root.join("directory");
        std::fs::create_dir(&directory)?;
        assert!(existing_file_permissions(&root, &directory).is_err());
        assert!(writable_file_permissions(&root, &directory).is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn async_commit_preserves_an_existing_target() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "zzclawterm-async-commit-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        let target = root.join("result.txt");
        std::fs::write(&target, b"old")?;
        let (temporary, mut file) = super::DownloadTemporary::create(&root, &target)?;
        let temporary_path = temporary.path.clone();
        use std::io::Write as _;
        file.write_all(b"new")?;
        drop(file);
        #[cfg(unix)]
        let inode_before = {
            use std::os::unix::fs::MetadataExt as _;
            std::fs::metadata(&target)?.ino()
        };

        temporary.commit_async(&root, &target).await?;
        assert_eq!(std::fs::read(&target)?, b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            assert_eq!(std::fs::metadata(&target)?.ino(), inode_before);
        }
        assert!(!temporary_path.exists());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn staged_write_preserves_existing_permissions() -> anyhow::Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let root = std::env::temp_dir().join(format!(
            "zzclawterm-atomic-mode-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        let target = root.join("result.txt");
        std::fs::write(&target, b"old")?;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))?;
        let inode_before = std::fs::metadata(&target)?.ino();

        staged_write_file(&root, &target, b"new")?;
        assert_eq!(std::fs::metadata(&target)?.ino(), inode_before);
        assert_eq!(
            std::fs::metadata(&target)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read(&target)?, b"new");

        let readonly = root.join("readonly.txt");
        std::fs::write(&readonly, b"locked")?;
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o400))?;
        assert!(staged_write_file(&root, &readonly, b"blocked").is_err());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn staged_write_drops_privileged_permission_bits() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "zzclawterm-atomic-safe-mode-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        let target = root.join("result.txt");
        std::fs::write(&target, b"old")?;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o6755))?;

        staged_write_file(&root, &target, b"new")?;
        assert_eq!(
            std::fs::metadata(&target)?.permissions().mode() & 0o7777,
            0o755
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_and_dangling_links_below_download_root() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "zzclawterm-download-path-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        let outside = root.join("outside");
        std::fs::create_dir(&outside)?;
        assert!(super::ensure_no_symlink(&root, &root.join("../outside/file")).is_err());
        let link = root.join("link");
        symlink(&outside, &link)?;
        assert!(super::ensure_no_symlink(&root, &link.join("file")).is_err());

        let dangling = root.join("dangling");
        symlink(root.join("missing"), &dangling)?;
        assert!(super::ensure_no_symlink(&root, &dangling).is_err());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn staged_write_never_follows_a_target_symlink() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "zzclawterm-atomic-link-{}",
            zzclawterm_core::uuid()
        ));
        std::fs::create_dir_all(&root)?;
        let outside = root.join("outside.txt");
        std::fs::write(&outside, b"outside")?;
        let target = root.join("target.txt");
        symlink(&outside, &target)?;

        assert!(staged_write_file(&root, &target, b"blocked").is_err());
        assert_eq!(std::fs::read(&outside)?, b"outside");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_device_and_trimmed_names() {
        for name in ["CON", "NUL.txt", "name.", "name ", "C:target"] {
            assert!(validate_name(name).is_err(), "{name:?}");
        }
    }
}
