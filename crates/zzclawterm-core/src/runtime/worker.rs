//! Fallible named worker creation with deterministic test-only fault injection.
use std::io;
use std::thread::JoinHandle;

#[cfg(any(test, feature = "test-support"))]
thread_local! { static FAIL_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) }; }

#[cfg(any(test, feature = "test-support"))]
pub struct SpawnFailureGuard;

#[cfg(any(test, feature = "test-support"))]
pub fn fail_after(successful_spawns: usize) -> SpawnFailureGuard {
    FAIL_AFTER.set(Some(successful_spawns));
    SpawnFailureGuard
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for SpawnFailureGuard {
    fn drop(&mut self) {
        FAIL_AFTER.set(None);
    }
}

pub fn spawn<T: Send + 'static>(
    name: String,
    run: impl FnOnce() -> T + Send + 'static,
) -> io::Result<JoinHandle<T>> {
    #[cfg(any(test, feature = "test-support"))]
    if FAIL_AFTER.with(|slot| match slot.get() {
        Some(0) => {
            slot.set(None);
            true
        }
        Some(remaining) => {
            slot.set(Some(remaining - 1));
            false
        }
        None => false,
    }) {
        return Err(io::Error::other("injected worker creation failure"));
    }
    std::thread::Builder::new().name(name).spawn(run)
}
