//! Shared test-only filesystem helpers.
//!
//! This module is compiled for `zzclawterm-core` tests and can be enabled by
//! other workspace crates through the `test-support` feature.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Owns a direct child of the system temporary directory and removes it when
/// the last guard is dropped.
///
/// Tests in this workspace commonly keep database workers, subprocesses, or
/// other file handles alive briefly after their body finishes. Windows can
/// reject immediate recursive deletion in that window, so cleanup retries for
/// a bounded period instead of silently leaking the tree.
#[derive(Clone, Debug)]
pub struct TestTempDir {
    inner: Arc<TestTempDirInner>,
}

#[derive(Debug)]
struct TestTempDirInner {
    path: PathBuf,
}

impl TestTempDir {
    /// Creates an unmaterialized unique test root under the system temp dir.
    pub fn new(prefix: &str) -> Self {
        assert!(
            prefix.starts_with("zzclawterm-"),
            "test temporary directory prefix must start with zzclawterm-: {prefix}"
        );
        Self::from_path(std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        )))
    }

    /// Takes cleanup ownership of an existing or future test root.
    ///
    /// Restrict cleanup to direct `zzclawterm-*` children of the system temp dir
    /// so a malformed fixture can never turn the guard into a broad recursive
    /// deletion.
    pub fn from_path(path: PathBuf) -> Self {
        let temp_dir = absolute_path(&std::env::temp_dir());
        let path = absolute_path(&path);
        assert_eq!(
            path.parent(),
            Some(temp_dir.as_path()),
            "test temporary directory must be a direct child of {}",
            temp_dir.display()
        );
        assert!(
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("zzclawterm-")),
            "test temporary directory must use the zzclawterm- prefix: {}",
            path.display()
        );
        Self {
            inner: Arc::new(TestTempDirInner { path }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }
}

impl AsRef<Path> for TestTempDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for TestTempDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl Drop for TestTempDirInner {
    fn drop(&mut self) {
        const RETRY_WINDOW: Duration = Duration::from_secs(5);
        const RETRY_DELAY: Duration = Duration::from_millis(10);

        let deadline = Instant::now() + RETRY_WINDOW;
        loop {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) if Instant::now() < deadline => std::thread::sleep(RETRY_DELAY),
                Err(error) => {
                    eprintln!(
                        "failed to remove test temporary directory {}: {error}",
                        self.path.display()
                    );
                    return;
                }
            }
        }
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .expect("resolve current test directory")
            .join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::TestTempDir;

    #[test]
    fn removes_tree_when_last_guard_drops() {
        let path = {
            let dir = TestTempDir::new("zzclawterm-core-test-cleanup");
            let clone = dir.clone();
            std::fs::create_dir_all(dir.join("nested")).expect("create test tree");
            let path = dir.path().to_path_buf();
            drop(dir);
            assert!(path.exists(), "clone must retain cleanup ownership");
            drop(clone);
            path
        };

        assert!(!path.exists(), "temporary test tree was not removed");
    }

    #[test]
    fn removes_tree_during_panic_unwind() {
        let dir = TestTempDir::new("zzclawterm-core-test-unwind");
        let path = dir.path().to_path_buf();
        let result = std::panic::catch_unwind(move || {
            std::fs::create_dir_all(dir.join("nested")).expect("create test tree");
            panic!("exercise cleanup during unwind");
        });

        assert!(result.is_err());
        assert!(!path.exists(), "temporary test tree survived panic cleanup");
    }
}
