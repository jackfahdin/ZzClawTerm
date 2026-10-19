//! Named thread construction and executor-owned background cleanup.

use std::io;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use gpui::{BackgroundExecutor, Task};

pub(crate) fn spawn_joinable<T: Send + 'static>(
    name: impl Into<String>,
    run: impl FnOnce() -> T + Send + 'static,
) -> io::Result<JoinHandle<T>> {
    thread::Builder::new().name(name.into()).spawn(run)
}

/// Tracks cleanup on the existing executor so dropping a worker never needs to
/// create another OS thread. The feature drains these tasks during shutdown.
#[derive(Clone)]
pub(crate) struct BackgroundCleanup {
    inner: Arc<BackgroundCleanupInner>,
}

struct BackgroundCleanupInner {
    executor: BackgroundExecutor,
    pending: Mutex<Vec<Task<()>>>,
}

impl BackgroundCleanup {
    pub(crate) fn new(executor: BackgroundExecutor) -> Self {
        Self {
            inner: Arc::new(BackgroundCleanupInner {
                executor,
                pending: Mutex::new(Vec::new()),
            }),
        }
    }

    pub(crate) fn submit(&self, cleanup: impl FnOnce() + Send + 'static) {
        let mut pending = self.inner.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.retain(|task| !task.is_ready());
        pending.push(self.inner.executor.spawn(async move { cleanup() }));
    }

    #[must_use = "await cleanup before releasing workspace resources"]
    pub(crate) fn drain(&self) -> impl Future<Output = ()> + Send + 'static {
        let pending =
            std::mem::take(&mut *self.inner.pending.lock().unwrap_or_else(|e| e.into_inner()));
        async move {
            for task in pending {
                task.await;
            }
        }
    }
}

impl Drop for BackgroundCleanupInner {
    fn drop(&mut self) {
        // Abnormal teardown must still complete cleanup without blocking GPUI.
        // Normal workspace shutdown awaits the tasks through `drain` instead.
        for task in self
            .pending
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            task.detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use gpui::TestAppContext;

    use super::BackgroundCleanup;

    #[gpui::test]
    fn shutdown_drains_cleanup_submitted_by_a_dropped_clone(cx: &mut TestAppContext) {
        let cleanup = BackgroundCleanup::new(cx.background_executor.clone());
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::clone(&finished);
        let other_owner = cleanup.clone();
        other_owner.submit(move || worker_finished.store(true, Ordering::Release));
        drop(other_owner);

        cx.foreground_executor().block_test(cleanup.drain());
        assert!(finished.load(Ordering::Acquire));
        cx.foreground_executor().block_test(cleanup.drain());
    }

    #[gpui::test]
    fn dropping_the_last_owner_keeps_pending_cleanup_running(cx: &mut TestAppContext) {
        let cleanup = BackgroundCleanup::new(cx.background_executor.clone());
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::clone(&finished);
        cleanup.submit(move || worker_finished.store(true, Ordering::Release));
        drop(cleanup);

        cx.run_until_parked();
        assert!(finished.load(Ordering::Acquire));
    }
}
