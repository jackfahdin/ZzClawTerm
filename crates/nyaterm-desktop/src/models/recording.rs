use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use nyaterm_transport::{
    RecordingContext, RecordingManager, RecordingProfile, RecordingStatus, RecordingStatusState,
    TerminalHistorySearchRequest, TerminalHistorySearchResponse,
};

const RECORDING_WRITE_QUEUE_BYTE_LIMIT: u64 = 4 * 1024 * 1024;

pub(crate) struct RecordingWritePipeline {
    command_tx: mpsc::Sender<RecordingWriteCommand>,
    worker: Option<thread::JoinHandle<()>>,
    queued_bytes: Arc<AtomicU64>,
    dropped: Arc<Mutex<DroppedPayloads>>,
    /// Taken once by `NyaTermApp::start_recording_event_drain`, which owns
    /// delivery from then on. `None` afterwards, so a second start is a no-op.
    event_rx: Option<UnboundedReceiver<RecordingWriteEvent>>,
}

#[derive(Clone)]
pub(crate) struct RecordingWriteHandle {
    command_tx: mpsc::Sender<RecordingWriteCommand>,
    queued_bytes: Arc<AtomicU64>,
    dropped: Arc<Mutex<DroppedPayloads>>,
}

#[derive(Default)]
struct DroppedPayloads {
    sessions: HashMap<String, DroppedPayloadState>,
}

#[derive(Default)]
struct DroppedPayloadState {
    bytes: u64,
    command_pending: bool,
}

impl RecordingWritePipeline {
    pub(crate) fn spawn(memory_limit_bytes: usize) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        // History-search replies and status events are small and applied as posted. Payload
        // memory is bounded separately so control commands never block GPUI or reorder.
        let (event_tx, event_rx) = unbounded();
        let queued_bytes = Arc::new(AtomicU64::new(0));
        let dropped = Arc::new(Mutex::new(DroppedPayloads::default()));
        let worker_event_tx = event_tx.clone();
        let worker_queued_bytes = Arc::clone(&queued_bytes);
        let worker_dropped = Arc::clone(&dropped);
        let worker = thread::Builder::new()
            .name("nyaterm-recording-writer".to_string())
            .spawn(move || {
                run_recording_writer(
                    memory_limit_bytes,
                    command_rx,
                    worker_event_tx,
                    worker_queued_bytes,
                    worker_dropped,
                )
            })
            .expect("failed to spawn recording writer");
        Self {
            command_tx,
            worker: Some(worker),
            queued_bytes,
            dropped,
            event_rx: Some(event_rx),
        }
    }

    pub(crate) fn write_output(&self, session_id: impl Into<String>, text: impl Into<String>) {
        self.writer().write_output(session_id, text);
    }

    pub(crate) fn write_input(&self, session_id: impl Into<String>, data: impl Into<Vec<u8>>) {
        self.writer().write_input(session_id, data);
    }

    pub(crate) fn write_raw_input(&self, session_id: impl Into<String>, data: impl Into<Vec<u8>>) {
        self.writer().write_raw_input(session_id, data);
    }

    pub(crate) fn cleanup_session(&self, session_id: impl Into<String>) {
        let _ = self.command_tx.send(RecordingWriteCommand::CleanupSession {
            session_id: session_id.into(),
        });
    }

    pub(crate) fn request_history_search(&self, key: RecordingHistorySearchKey) {
        if key.query.trim().is_empty() {
            return;
        }
        let _ = self
            .command_tx
            .send(RecordingWriteCommand::HistorySearch { key });
    }

    pub(crate) fn set_memory_limit(&self, memory_limit_bytes: usize) {
        let _ = self
            .command_tx
            .send(RecordingWriteCommand::SetMemoryLimit { memory_limit_bytes });
    }

    pub(crate) fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<RecordingWriteEvent>> {
        self.event_rx.take()
    }

    pub(crate) fn writer(&self) -> RecordingWriteHandle {
        RecordingWriteHandle {
            command_tx: self.command_tx.clone(),
            queued_bytes: Arc::clone(&self.queued_bytes),
            dropped: Arc::clone(&self.dropped),
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if self.worker.is_none() {
            return;
        }
        let _ = self.command_tx.send(RecordingWriteCommand::Shutdown);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("recording writer panicked during shutdown");
        }
    }
}

impl Drop for RecordingWritePipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl RecordingWriteHandle {
    pub(crate) fn write_output(&self, session_id: impl Into<String>, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        let session_id = session_id.into();
        self.enqueue_payload(
            &session_id,
            text.len(),
            RecordingWriteCommand::WriteOutput {
                session_id: session_id.clone(),
                text,
            },
        );
    }

    pub(crate) fn write_input(&self, session_id: impl Into<String>, data: impl Into<Vec<u8>>) {
        let data = data.into();
        if data.is_empty() {
            return;
        }
        let session_id = session_id.into();
        self.enqueue_payload(
            &session_id,
            data.len(),
            RecordingWriteCommand::WriteInput {
                session_id: session_id.clone(),
                data,
            },
        );
    }

    pub(crate) fn write_raw_input(&self, session_id: impl Into<String>, data: impl Into<Vec<u8>>) {
        let data = data.into();
        if data.is_empty() {
            return;
        }
        let session_id = session_id.into();
        self.enqueue_payload(
            &session_id,
            data.len(),
            RecordingWriteCommand::WriteRawInput {
                session_id: session_id.clone(),
                data,
            },
        );
    }

    fn enqueue_payload(
        &self,
        session_id: &str,
        payload_bytes: usize,
        command: RecordingWriteCommand,
    ) {
        if !try_reserve_queued_bytes(&self.queued_bytes, payload_bytes) {
            self.report_dropped_payload(session_id, payload_bytes);
            return;
        }

        if self.command_tx.send(command).is_err() {
            release_queued_bytes(&self.queued_bytes, payload_bytes);
        }
    }

    fn report_dropped_payload(&self, session_id: &str, payload_bytes: usize) {
        let should_enqueue = {
            let mut dropped = self
                .dropped
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let state = dropped.sessions.entry(session_id.to_string()).or_default();
            state.bytes = state.bytes.saturating_add(payload_bytes as u64);
            if state.command_pending {
                false
            } else {
                state.command_pending = true;
                true
            }
        };
        if should_enqueue {
            let _ = self.command_tx.send(RecordingWriteCommand::ReportDropped {
                session_id: session_id.to_string(),
            });
        }
    }

    pub(crate) fn start(
        &self,
        session_id: String,
        context: RecordingContext,
        profile: RecordingProfile,
        explicit_path: Option<std::path::PathBuf>,
        memory_limit_bytes: usize,
    ) -> Result<String, String> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(RecordingWriteCommand::Start {
                session_id,
                context: Box::new(context),
                profile,
                explicit_path,
                memory_limit_bytes,
                reply_tx,
            })
            .map_err(|_| "recording writer stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "recording writer stopped before start completed".to_string())?
    }

    pub(crate) fn stop(&self, session_id: String) -> Result<String, String> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(RecordingWriteCommand::Stop {
                session_id,
                reply_tx,
            })
            .map_err(|_| "recording writer stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "recording writer stopped before stop completed".to_string())?
    }

    pub(crate) fn save_transcript(
        &self,
        session_id: String,
        path: String,
        include_io_labels: bool,
        include_timestamps: bool,
        memory_limit_bytes: usize,
    ) -> Result<String, String> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(RecordingWriteCommand::SaveTranscript {
                session_id,
                path,
                include_io_labels,
                include_timestamps,
                memory_limit_bytes,
                reply_tx,
            })
            .map_err(|_| "recording writer stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "recording writer stopped before transcript save completed".to_string())?
    }

    #[cfg(test)]
    pub(crate) fn flush(&self) {
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        if self
            .command_tx
            .send(RecordingWriteCommand::Flush { ack_tx })
            .is_ok()
        {
            let _ = ack_rx.recv();
        }
    }

    #[cfg(test)]
    fn block_writer(&self) -> mpsc::SyncSender<()> {
        let (started_tx, started_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(RecordingWriteCommand::Block {
                started_tx,
                release_rx,
            })
            .expect("recording writer should accept a test barrier");
        started_rx
            .recv()
            .expect("recording writer should enter the test barrier");
        release_tx
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RecordingWriteEvent {
    HistorySearch(RecordingHistorySearchEvent),
    Status(RecordingStatus),
    StatusRemoved { session_id: String },
}

#[derive(Clone, Debug)]
pub(crate) struct RecordingHistorySearchEvent {
    pub(crate) key: RecordingHistorySearchKey,
    pub(crate) result: Result<TerminalHistorySearchResponse, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordingHistorySearchKey {
    pub(crate) session_id: String,
    pub(crate) query: String,
    pub(crate) case_sensitive: bool,
    pub(crate) regex: bool,
    pub(crate) whole_word: bool,
    pub(crate) limit: Option<usize>,
    pub(crate) context_before: Option<usize>,
    pub(crate) context_after: Option<usize>,
    pub(crate) max_lines: Option<usize>,
}

impl RecordingHistorySearchKey {
    pub(crate) fn request(&self) -> TerminalHistorySearchRequest {
        TerminalHistorySearchRequest {
            session_id: self.session_id.clone(),
            query: self.query.clone(),
            case_sensitive: self.case_sensitive,
            regex: self.regex,
            whole_word: self.whole_word,
            limit: self.limit,
            context_before: self.context_before,
            context_after: self.context_after,
            max_lines: self.max_lines,
        }
    }
}

#[derive(Debug)]
enum RecordingWriteCommand {
    Shutdown,
    Start {
        session_id: String,
        context: Box<RecordingContext>,
        profile: RecordingProfile,
        explicit_path: Option<std::path::PathBuf>,
        memory_limit_bytes: usize,
        reply_tx: mpsc::SyncSender<Result<String, String>>,
    },
    Stop {
        session_id: String,
        reply_tx: mpsc::SyncSender<Result<String, String>>,
    },
    SetMemoryLimit {
        memory_limit_bytes: usize,
    },
    ReportDropped {
        session_id: String,
    },
    SaveTranscript {
        session_id: String,
        path: String,
        include_io_labels: bool,
        include_timestamps: bool,
        memory_limit_bytes: usize,
        reply_tx: mpsc::SyncSender<Result<String, String>>,
    },
    WriteOutput {
        session_id: String,
        text: String,
    },
    WriteInput {
        session_id: String,
        data: Vec<u8>,
    },
    WriteRawInput {
        session_id: String,
        data: Vec<u8>,
    },
    CleanupSession {
        session_id: String,
    },
    HistorySearch {
        key: RecordingHistorySearchKey,
    },
    #[cfg(test)]
    Flush {
        ack_tx: mpsc::SyncSender<()>,
    },
    #[cfg(test)]
    Block {
        started_tx: mpsc::SyncSender<()>,
        release_rx: mpsc::Receiver<()>,
    },
}

fn try_reserve_queued_bytes(queued_bytes: &AtomicU64, payload_bytes: usize) -> bool {
    let payload_bytes = payload_bytes as u64;
    let mut current = queued_bytes.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(payload_bytes) else {
            return false;
        };
        if next > RECORDING_WRITE_QUEUE_BYTE_LIMIT {
            return false;
        }
        match queued_bytes.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
}

fn release_queued_bytes(queued_bytes: &AtomicU64, payload_bytes: usize) {
    let previous = queued_bytes.fetch_sub(payload_bytes as u64, Ordering::AcqRel);
    debug_assert!(previous >= payload_bytes as u64);
}

fn run_recording_writer(
    memory_limit_bytes: usize,
    command_rx: mpsc::Receiver<RecordingWriteCommand>,
    event_tx: UnboundedSender<RecordingWriteEvent>,
    queued_bytes: Arc<AtomicU64>,
    dropped: Arc<Mutex<DroppedPayloads>>,
) {
    let recording_manager = RecordingManager::new();
    recording_manager.set_memory_limit(memory_limit_bytes);
    while let Ok(command) = command_rx.recv() {
        match command {
            RecordingWriteCommand::Shutdown => break,
            RecordingWriteCommand::Start {
                session_id,
                context,
                profile,
                explicit_path,
                memory_limit_bytes,
                reply_tx,
            } => {
                recording_manager.set_memory_limit(memory_limit_bytes);
                let result = recording_manager
                    .start_with_profile(&session_id, *context, profile, explicit_path)
                    .map_err(|error| error.to_string());
                send_recording_status_or_removed(
                    &recording_manager,
                    &event_tx,
                    &queued_bytes,
                    &session_id,
                    true,
                );
                let _ = reply_tx.send(result);
            }
            RecordingWriteCommand::Stop {
                session_id,
                reply_tx,
            } => {
                let result = recording_manager
                    .stop(&session_id)
                    .map_err(|error| error.to_string());
                send_recording_status_or_removed(
                    &recording_manager,
                    &event_tx,
                    &queued_bytes,
                    &session_id,
                    true,
                );
                let _ = reply_tx.send(result);
            }
            RecordingWriteCommand::SetMemoryLimit { memory_limit_bytes } => {
                recording_manager.set_memory_limit(memory_limit_bytes);
            }
            RecordingWriteCommand::ReportDropped { session_id } => {
                let dropped_bytes = take_dropped_payloads(&dropped, &session_id);
                if dropped_bytes > 0 {
                    recording_manager.report_dropped(
                        &session_id,
                        usize::try_from(dropped_bytes).unwrap_or(usize::MAX),
                    );
                    send_recording_status(
                        &recording_manager,
                        &event_tx,
                        &queued_bytes,
                        &session_id,
                        true,
                    );
                }
                if !recording_manager.is_recording(&session_id) {
                    remove_dropped_payloads(&dropped, &session_id);
                }
            }
            RecordingWriteCommand::SaveTranscript {
                session_id,
                path,
                include_io_labels,
                include_timestamps,
                memory_limit_bytes,
                reply_tx,
            } => {
                recording_manager.set_memory_limit(memory_limit_bytes);
                let result = recording_manager
                    .save_transcript(&session_id, &path, include_io_labels, include_timestamps)
                    .map_err(|error| error.to_string());
                let _ = reply_tx.send(result);
            }
            RecordingWriteCommand::WriteOutput { session_id, text } => {
                let payload_bytes = text.len();
                recording_manager.write_output(&session_id, &text);
                release_queued_bytes(&queued_bytes, payload_bytes);
                send_recording_status(
                    &recording_manager,
                    &event_tx,
                    &queued_bytes,
                    &session_id,
                    false,
                );
            }
            RecordingWriteCommand::WriteInput { session_id, data } => {
                let payload_bytes = data.len();
                recording_manager.write_input(&session_id, &data);
                release_queued_bytes(&queued_bytes, payload_bytes);
                send_recording_status(
                    &recording_manager,
                    &event_tx,
                    &queued_bytes,
                    &session_id,
                    false,
                );
            }
            RecordingWriteCommand::WriteRawInput { session_id, data } => {
                let payload_bytes = data.len();
                recording_manager.write_raw_input(&session_id, &data);
                release_queued_bytes(&queued_bytes, payload_bytes);
                send_recording_status(
                    &recording_manager,
                    &event_tx,
                    &queued_bytes,
                    &session_id,
                    false,
                );
            }
            RecordingWriteCommand::CleanupSession { session_id } => {
                recording_manager.cleanup_session(&session_id);
                remove_dropped_payloads(&dropped, &session_id);
                let _ = event_tx.unbounded_send(RecordingWriteEvent::StatusRemoved { session_id });
            }
            RecordingWriteCommand::HistorySearch { key } => {
                let result = recording_manager
                    .search_history(key.request())
                    .map_err(|error| error.to_string());
                let _ = event_tx.unbounded_send(RecordingWriteEvent::HistorySearch(
                    RecordingHistorySearchEvent { key, result },
                ));
            }
            #[cfg(test)]
            RecordingWriteCommand::Flush { ack_tx } => {
                let _ = ack_tx.send(());
            }
            #[cfg(test)]
            RecordingWriteCommand::Block {
                started_tx,
                release_rx,
            } => {
                let _ = started_tx.send(());
                let _ = release_rx.recv();
            }
        }
    }
}

fn take_dropped_payloads(dropped: &Mutex<DroppedPayloads>, session_id: &str) -> u64 {
    let mut dropped = dropped
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(state) = dropped.sessions.get_mut(session_id) else {
        return 0;
    };
    state.command_pending = false;
    std::mem::take(&mut state.bytes)
}

fn remove_dropped_payloads(dropped: &Mutex<DroppedPayloads>, session_id: &str) {
    dropped
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .sessions
        .remove(session_id);
}

fn send_recording_status_or_removed(
    manager: &RecordingManager,
    event_tx: &UnboundedSender<RecordingWriteEvent>,
    queued_bytes: &AtomicU64,
    session_id: &str,
    include_healthy: bool,
) {
    if manager.status(session_id).is_some() {
        send_recording_status(manager, event_tx, queued_bytes, session_id, include_healthy);
    } else {
        let _ = event_tx.unbounded_send(RecordingWriteEvent::StatusRemoved {
            session_id: session_id.to_string(),
        });
    }
}

fn send_recording_status(
    manager: &RecordingManager,
    event_tx: &UnboundedSender<RecordingWriteEvent>,
    queued_bytes: &AtomicU64,
    session_id: &str,
    include_healthy: bool,
) {
    let Some(mut status) = manager.status(session_id) else {
        return;
    };
    status.queued_bytes = queued_bytes.load(Ordering::Acquire);
    if include_healthy || status.state != RecordingStatusState::Recording {
        let _ = event_tx.unbounded_send(RecordingWriteEvent::Status(status));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use nyaterm_transport::{
        ExistingFileBehavior, RecordingContext, RecordingMode, RecordingProfile,
        RecordingRotationPolicy, RecordingStatusState,
    };
    use time::OffsetDateTime;

    use super::{
        RECORDING_WRITE_QUEUE_BYTE_LIMIT, RecordingHistorySearchKey, RecordingWriteEvent,
        RecordingWritePipeline,
    };

    #[test]
    fn recording_pipeline_preserves_write_order_before_flush() {
        let mut pipeline = RecordingWritePipeline::spawn(1024 * 1024);
        let mut event_rx = pipeline
            .take_event_receiver()
            .expect("recording events should be available");
        let session_id = "session-a";
        pipeline.write_input(session_id, b"echo hello\r".to_vec());
        pipeline.write_output(session_id, "hello\n");
        pipeline.request_history_search(RecordingHistorySearchKey {
            session_id: session_id.to_string(),
            query: "hello".to_string(),
            case_sensitive: false,
            regex: false,
            whole_word: false,
            limit: Some(10),
            context_before: Some(0),
            context_after: Some(0),
            max_lines: None,
        });
        pipeline.writer().flush();

        let results = loop {
            let event = event_rx.try_recv().expect("search result should be queued");
            if let RecordingWriteEvent::HistorySearch(event) = event {
                break event.result.expect("search should succeed");
            }
        };
        assert_eq!(results.total, 2);
        assert_eq!(results.results[0].source, "input");
        assert_eq!(results.results[1].source, "output");
    }

    #[test]
    fn recording_handle_orders_start_writes_and_stop() {
        let mut pipeline = RecordingWritePipeline::spawn(1024 * 1024);
        let mut event_rx = pipeline
            .take_event_receiver()
            .expect("recording events should be available");
        let writer = pipeline.writer();
        let session_id = "session-handle-order";
        let path = PathBuf::from(unique_recording_path("handle-order"));

        writer
            .start(
                session_id.to_string(),
                recording_context(session_id),
                recording_profile(&path),
                Some(path.clone()),
                1024 * 1024,
            )
            .expect("recording should start through the writer");
        writer.write_input(session_id, b"echo ordered\r".to_vec());
        writer.write_output(session_id, "echo ordered\r\nordered\n");
        writer
            .stop(session_id.to_string())
            .expect("recording should stop after queued writes");

        let text = fs::read_to_string(&path).expect("recording file should exist");
        assert!(text.contains("[INPUT] echo ordered"));
        assert!(text.contains("[OUTPUT] ordered"));
        let mut removed = false;
        while let Ok(event) = event_rx.try_recv() {
            removed |= matches!(
                event,
                RecordingWriteEvent::StatusRemoved { ref session_id }
                    if session_id == "session-handle-order"
            );
        }
        assert!(removed);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn failed_stop_still_removes_the_finished_recording_status() {
        let mut pipeline = RecordingWritePipeline::spawn(1024 * 1024);
        let mut event_rx = pipeline
            .take_event_receiver()
            .expect("recording events should be available");
        let writer = pipeline.writer();
        let session_id = "session-failed-stop";
        let initial_path = PathBuf::from(unique_recording_path("failed-stop-initial"));
        let blocked_base_path = PathBuf::from(unique_recording_path("failed-stop-base"));
        fs::write(&blocked_base_path, b"not a directory").unwrap();
        let mut profile = recording_profile(&initial_path);
        profile.base_path = blocked_base_path.clone();
        profile.path_template = "rotated.log".to_string();
        profile.rotation = RecordingRotationPolicy::Size { max_bytes: 1 };

        writer
            .start(
                session_id.to_string(),
                recording_context(session_id),
                profile,
                Some(initial_path.clone()),
                1024 * 1024,
            )
            .expect("recording should start");
        writer.write_output(session_id, "rotation must fail\n");
        writer.flush();
        assert!(writer.stop(session_id.to_string()).is_err());

        let mut saw_failed = false;
        let mut saw_removed_after_failed = false;
        while let Ok(event) = event_rx.try_recv() {
            match event {
                RecordingWriteEvent::Status(status)
                    if status.session_id == session_id
                        && status.state == RecordingStatusState::Failed =>
                {
                    saw_failed = true;
                }
                RecordingWriteEvent::StatusRemoved {
                    session_id: removed,
                } if removed == session_id => {
                    saw_removed_after_failed = saw_failed;
                }
                _ => {}
            }
        }
        assert!(saw_failed);
        assert!(saw_removed_after_failed);

        let _ = fs::remove_file(initial_path);
        let _ = fs::remove_file(blocked_base_path);
    }

    #[test]
    fn recording_queue_overflow_emits_degraded_status() {
        let mut pipeline = RecordingWritePipeline::spawn(1024 * 1024);
        let mut event_rx = pipeline
            .take_event_receiver()
            .expect("the event receiver should be available");
        let writer = pipeline.writer();
        let session_id = "session-overflow";
        let path = PathBuf::from(unique_recording_path("queue-overflow"));

        writer
            .start(
                session_id.to_string(),
                recording_context(session_id),
                recording_profile(&path),
                Some(path.clone()),
                1024 * 1024,
            )
            .expect("recording should start");
        let release_writer = writer.block_writer();
        let oversized = RECORDING_WRITE_QUEUE_BYTE_LIMIT as usize + 1;
        for _ in 0..3 {
            writer.write_output(session_id, "x".repeat(oversized));
        }
        {
            let dropped = writer
                .dropped
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let state = dropped
                .sessions
                .get(session_id)
                .expect("overflow should create one pending report");
            assert!(state.command_pending);
            assert_eq!(state.bytes, (oversized as u64) * 3);
        }
        release_writer.send(()).expect("release recording writer");
        writer.flush();

        let mut degraded_events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            if let RecordingWriteEvent::Status(status) = event
                && status.state == RecordingStatusState::Degraded
            {
                degraded_events.push(status);
            }
        }
        assert_eq!(degraded_events.len(), 1);
        let degraded_event = degraded_events.pop().unwrap();
        assert_eq!(
            degraded_event.dropped_bytes,
            (RECORDING_WRITE_QUEUE_BYTE_LIMIT + 1) * 3
        );
        assert_eq!(degraded_event.queued_bytes, 0);
        assert!(degraded_event.last_error.is_some());

        assert!(writer.stop(session_id.to_string()).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_pipeline_cleanup_runs_after_queued_writes_and_removes_status() {
        let mut pipeline = RecordingWritePipeline::spawn(1024 * 1024);
        let mut event_rx = pipeline
            .take_event_receiver()
            .expect("recording events should be available");
        let writer = pipeline.writer();
        let session_id = "session-b";
        let path = PathBuf::from(unique_recording_path("pipeline-cleanup"));
        writer
            .start(
                session_id.to_string(),
                recording_context(session_id),
                recording_profile(&path),
                Some(path.clone()),
                1024 * 1024,
            )
            .expect("recording should start");

        pipeline.write_output(session_id, "before cleanup\n");
        pipeline.cleanup_session(session_id);
        writer.flush();

        let text = fs::read_to_string(&path).expect("recording file should exist");
        assert!(text.contains("before cleanup"));
        let mut removed = false;
        while let Ok(event) = event_rx.try_recv() {
            removed |= matches!(
                event,
                RecordingWriteEvent::StatusRemoved { session_id: removed }
                    if removed == session_id
            );
        }
        assert!(removed);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_pipeline_search_runs_after_queued_writes() {
        let mut pipeline = RecordingWritePipeline::spawn(1024 * 1024);
        let session_id = "session-c";
        let key = RecordingHistorySearchKey {
            session_id: session_id.to_string(),
            query: "queued".to_string(),
            case_sensitive: false,
            regex: false,
            whole_word: false,
            limit: Some(8),
            context_before: Some(0),
            context_after: Some(0),
            max_lines: Some(30_000),
        };

        pipeline.write_output(session_id, "queued history line\n");
        pipeline.request_history_search(key.clone());
        pipeline.writer().flush();

        let event = pipeline
            .take_event_receiver()
            .expect("the pipeline holds its receiver until the drain starts")
            .try_recv()
            .expect("search event should be queued");
        let RecordingWriteEvent::HistorySearch(event) = event else {
            panic!("expected history search event");
        };
        assert_eq!(event.key, key);
        assert_eq!(event.result.expect("search should succeed").total, 1);
    }

    fn recording_context(session_id: &str) -> RecordingContext {
        RecordingContext {
            session_id: session_id.to_string(),
            session_name: session_id.to_string(),
            connection_id: None,
            connection_name: None,
            group_path: None,
            protocol: "local".to_string(),
            host: None,
            port: None,
            username: None,
            started_at: OffsetDateTime::now_utc(),
        }
    }

    fn recording_profile(path: &Path) -> RecordingProfile {
        RecordingProfile {
            mode: RecordingMode::Transcript,
            base_path: path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
            path_template: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("recording.log")
                .to_string(),
            include_timestamps: false,
            include_io_labels: true,
            include_session_metadata: false,
            rotation: RecordingRotationPolicy::Session,
            existing_file_behavior: ExistingFileBehavior::Overwrite,
            include_binary_transfer_payloads: false,
        }
    }

    fn unique_recording_path(name: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("nyaterm-{name}-{nanos}.log"))
            .to_string_lossy()
            .to_string()
    }
}
