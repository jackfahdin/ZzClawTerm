//! Owned runtime for watching files opened in an external editor.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use futures::channel::mpsc::UnboundedSender;

use crate::models::{TransferJobEvent, TransferJobResult};

const WATCH_INTERVAL: Duration = Duration::from_millis(1000);
const UPLOAD_SETTLE: Duration = Duration::from_millis(450);
const STARTUP_SUPPRESSION: Duration = Duration::from_secs(2);

pub(super) struct ExternalEditorWatcher {
    stop_tx: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl ExternalEditorWatcher {
    pub(super) fn spawn(
        job_id: String,
        remote_path: String,
        raw_path_token: Option<String>,
        local_path: PathBuf,
        transfer_tx: UnboundedSender<TransferJobResult>,
    ) -> std::io::Result<Self> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("nyaterm-external-editor-watch".to_string())
            .spawn(move || {
                watch_external_editor_file(
                    job_id,
                    remote_path,
                    raw_path_token,
                    local_path,
                    transfer_tx,
                    stop_rx,
                    WatchTimings::default(),
                );
            })?;
        Ok(Self {
            stop_tx: Some(stop_tx),
            worker: Some(worker),
        })
    }

    pub(super) fn stop_and_join(&mut self) {
        self.stop_tx.take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("external editor watcher panicked during shutdown");
        }
    }
}

impl Drop for ExternalEditorWatcher {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[derive(Clone, Copy)]
struct WatchTimings {
    interval: Duration,
    settle: Duration,
    startup_suppression: Duration,
}

impl Default for WatchTimings {
    fn default() -> Self {
        Self {
            interval: WATCH_INTERVAL,
            settle: UPLOAD_SETTLE,
            startup_suppression: STARTUP_SUPPRESSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalFileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

impl LocalFileFingerprint {
    fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn is_content_change_from(&self, previous: &Self, within_startup_window: bool) -> bool {
        if self.len != previous.len {
            return true;
        }
        self.modified != previous.modified && !within_startup_window
    }
}

#[allow(clippy::too_many_arguments)]
fn watch_external_editor_file(
    job_id: String,
    remote_path: String,
    raw_path_token: Option<String>,
    local_path: PathBuf,
    transfer_tx: UnboundedSender<TransferJobResult>,
    stop_rx: mpsc::Receiver<()>,
    timings: WatchTimings,
) {
    let watch_started = Instant::now();
    let mut baseline = match LocalFileFingerprint::from_path(&local_path) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            let _ = transfer_tx.unbounded_send(TransferJobResult {
                id: job_id,
                event: TransferJobEvent::Finished(Err(format!(
                    "external editor watch failed for {}: {error}",
                    local_path.display()
                ))),
            });
            return;
        }
    };

    loop {
        if wait_for_stop(&stop_rx, timings.interval) {
            break;
        }
        let current = match LocalFileFingerprint::from_path(&local_path) {
            Ok(fingerprint) => fingerprint,
            Err(_) => break,
        };
        if !current.is_content_change_from(
            &baseline,
            watch_started.elapsed() <= timings.startup_suppression,
        ) {
            if current != baseline {
                baseline = current;
            }
            continue;
        }

        if wait_for_stop(&stop_rx, timings.settle) {
            break;
        }
        baseline = LocalFileFingerprint::from_path(&local_path).unwrap_or(current);
        if transfer_tx
            .unbounded_send(TransferJobResult {
                id: job_id.clone(),
                event: TransferJobEvent::ExternalModified {
                    remote_path: remote_path.clone(),
                    raw_path_token: raw_path_token.clone(),
                    local_path: local_path.clone(),
                },
            })
            .is_err()
        {
            break;
        }
        if let Ok(after_upload) = LocalFileFingerprint::from_path(&local_path) {
            baseline = after_upload;
        }
    }
}

fn wait_for_stop(stop_rx: &mpsc::Receiver<()>, timeout: Duration) -> bool {
    match stop_rx.recv_timeout(timeout) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
        Err(mpsc::RecvTimeoutError::Timeout) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use futures::channel::mpsc::unbounded;

    use super::{WatchTimings, watch_external_editor_file};

    #[test]
    fn watcher_stop_wakes_interval_wait_without_polling_delay() {
        let path = std::env::temp_dir().join(format!(
            "nyaterm-external-watch-{}-{}",
            std::process::id(),
            nyaterm_core::uuid()
        ));
        fs::write(&path, b"initial").expect("write fixture");
        let (transfer_tx, _transfer_rx) = unbounded();
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker_path = path.clone();
        let worker = std::thread::Builder::new()
            .name("nyaterm-external-watch-test".to_string())
            .spawn(move || {
                watch_external_editor_file(
                    "job".to_string(),
                    "/remote/file".to_string(),
                    None,
                    worker_path,
                    transfer_tx,
                    stop_rx,
                    WatchTimings {
                        interval: Duration::from_secs(30),
                        settle: Duration::ZERO,
                        startup_suppression: Duration::ZERO,
                    },
                );
            })
            .expect("spawn watcher");

        let started = Instant::now();
        drop(stop_tx);
        worker.join().expect("join watcher");
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = fs::remove_file(path);
    }
}
