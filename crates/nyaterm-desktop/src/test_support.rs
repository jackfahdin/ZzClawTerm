use std::path::{Path, PathBuf};

use nyaterm_store::{FlushBarrier, StoreBlockingClient, StoreConfig, StoreRuntime};

/// Removes a test's isolated configuration tree after its owners have shut down.
///
/// Bind this guard before the GPUI test context. Rust drops locals in reverse
/// order, which closes database and log handles before cleanup runs on Windows.
pub(crate) struct TestConfigDir {
    path: PathBuf,
}

impl TestConfigDir {
    pub(crate) fn new(prefix: &str) -> Self {
        Self::from_path(std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            nyaterm_core::uuid()
        )))
    }

    /// Takes cleanup ownership of an existing test configuration root.
    ///
    /// Restrict adopted paths to a direct child of the system temporary
    /// directory so a malformed test runtime can never turn this guard into a
    /// broad recursive deletion.
    pub(crate) fn from_path(path: PathBuf) -> Self {
        let temp_dir = absolute_path(&std::env::temp_dir());
        let path = absolute_path(&path);
        assert_eq!(
            path.parent(),
            Some(temp_dir.as_path()),
            "test configuration directory must be a direct child of {}",
            temp_dir.display()
        );
        assert!(
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nyaterm-")),
            "test configuration directory must use the nyaterm- prefix: {}",
            path.display()
        );
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestConfigDir {
    fn drop(&mut self) {
        const RETRY_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(10);
        let deadline = std::time::Instant::now() + RETRY_WINDOW;
        loop {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) if std::time::Instant::now() < deadline => {
                    // redb is owned by a storage worker. Its last sender may
                    // have dropped, but Windows can still reject deletion
                    // until that worker observes disconnect and closes the DB.
                    std::thread::sleep(RETRY_DELAY);
                }
                Err(error) => {
                    eprintln!(
                        "failed to remove test configuration directory {}: {error}",
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

/// Opens an isolated test store and waits until its redb worker is ready.
///
/// Waiting on the barrier is part of the cleanup contract: without it, a very
/// short test can remove its untouched directory before the worker starts, and
/// the late worker then recreates `config/nyaterm.redb` after cleanup.
pub(crate) fn blocking_test_store(root: &Path) -> StoreBlockingClient {
    let runtime = StoreRuntime::spawn(StoreConfig {
        config_dir: root.join("config"),
        portable_key_path: None,
    })
    .expect("spawn test store");
    let store = runtime.blocking_client();
    store
        .request(0, FlushBarrier)
        .expect("receive test store barrier")
        .outcome
        .expect("initialize test store");
    store
}

#[test]
fn test_config_dir_removes_its_tree_on_drop() {
    let path = {
        let dir = TestConfigDir::new("nyaterm-test-cleanup");
        std::fs::create_dir_all(dir.path().join("config")).expect("create test tree");
        dir.path().to_path_buf()
    };

    assert!(!path.exists(), "temporary test tree was not removed");
}

#[test]
fn synchronized_test_store_cannot_recreate_its_tree_after_cleanup() {
    let path = {
        let dir = TestConfigDir::new("nyaterm-test-store-cleanup");
        let path = dir.path().to_path_buf();
        let store = blocking_test_store(dir.path());
        assert!(path.join("config/nyaterm.redb").is_file());
        drop(store);
        path
    };

    // This delay would expose the old race: an unsynchronized worker could
    // start after the guard returned and recreate the database behind it.
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(
        !path.exists(),
        "the storage worker recreated its temporary tree after cleanup"
    );
}
