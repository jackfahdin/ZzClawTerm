use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedReceiver;
use nyaterm_transport::{
    SessionDrainStats, SessionEvent, SessionManager, TrzszDetector, ZmodemDetector,
};

use super::event_wake::{ANY_INTEREST, EventWake};
use super::{TerminalFrameOutputSubmission, TerminalFramePipeline};

const SESSION_EVENT_BRIDGE_DRAIN_BATCH: usize = 512;
const SESSION_EVENT_BRIDGE_OUTPUT_BUDGET: usize = 128 * 1024;
const SESSION_EVENT_BRIDGE_IDLE_SLEEP: Duration = Duration::from_millis(4);
/// Upper bound on one park in the source drain. The queue wakes this thread as
/// soon as a PTY read lands, so the timeout only bounds how long a fully idle
/// bridge waits before re-checking its stop flag.
const SESSION_EVENT_BRIDGE_WAIT_TIMEOUT: Duration = Duration::from_millis(50);
const SESSION_EVENT_BRIDGE_BUSY_SLEEP: Duration = Duration::from_millis(1);
const SESSION_EVENT_BRIDGE_UI_OUTPUT_LIMIT: usize = 1024 * 1024;
const SESSION_EVENT_BRIDGE_UI_OUTPUT_LOW_WATERMARK: usize =
    SESSION_EVENT_BRIDGE_UI_OUTPUT_LIMIT / 2;
const SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE: usize = 2 * 1024 * 1024;
const SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_LOW_WATERMARK: usize =
    SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE / 2;
const SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_EVENTS: usize = 4;
const SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_WINDOW: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, Default)]
pub(crate) struct SessionEventBridgeStats {
    pub(crate) direct_output_events: u64,
    pub(crate) direct_output_bytes: u64,
    pub(crate) direct_backpressure_events: u64,
    pub(crate) direct_backpressure_bytes: u64,
    pub(crate) drained_ui_events: usize,
    pub(crate) drained_ui_output_bytes: usize,
    pub(crate) ui_queued_events: usize,
    pub(crate) ui_queued_output_bytes: usize,
    pub(crate) source_queued_events: usize,
    pub(crate) source_queued_output_bytes: usize,
    pub(crate) dropped_output_bytes: usize,
}

#[derive(Debug, Default)]
pub(crate) struct SessionEventBridgeDrain {
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) stats: SessionEventBridgeStats,
}

pub(crate) struct SessionEventBridge {
    state: Arc<SessionEventBridgeState>,
    worker: Option<thread::JoinHandle<()>>,
}

struct SessionEventBridgeState {
    control: Mutex<SessionEventBridgeControl>,
    ui_queue: SessionEventBridgeQueue,
    /// Handed to `NyaTermApp::start_runtime_data_plane_drain` once, at window open.
    ui_queue_wake_rx: Mutex<Option<UnboundedReceiver<()>>>,
    source_queued_events: AtomicUsize,
    source_queued_output_bytes: AtomicUsize,
    direct_output_events: AtomicU64,
    direct_output_bytes: AtomicU64,
    direct_backpressure_events: AtomicU64,
    direct_backpressure_bytes: AtomicU64,
    stop: AtomicBool,
}

#[derive(Clone)]
struct SessionEventBridgeControl {
    ui_routed_sessions: HashSet<String>,
    encoding: String,
    scrollback_limit: usize,
    source_queued_events: usize,
    source_queued_output_bytes: usize,
}

#[derive(Clone)]
struct SessionEventBridgeControlSnapshot {
    ui_routed_sessions: HashSet<String>,
    encoding: String,
    scrollback_limit: usize,
}

#[derive(Clone, Copy, Debug)]
struct SessionEventBridgeSidebandProbe {
    events_remaining: usize,
    expires_at: Instant,
}

#[derive(Clone)]
struct SessionEventBridgeQueue {
    inner: Arc<Mutex<SessionEventBridgeQueueInner>>,
    /// Signalled after every push. `None` only in the queue's own unit tests.
    wake: Option<EventWake>,
}

#[derive(Default)]
struct SessionEventBridgeQueueInner {
    events: VecDeque<SessionEvent>,
    queued_output_bytes: usize,
}

impl SessionEventBridge {
    pub(crate) fn spawn(
        session_manager: Arc<SessionManager>,
        frame_pipeline: TerminalFramePipeline,
        encoding: String,
        scrollback_limit: usize,
    ) -> Self {
        let (ui_queue, ui_queue_wake_rx) = SessionEventBridgeQueue::new_with_wake();
        let state = Arc::new(SessionEventBridgeState {
            control: Mutex::new(SessionEventBridgeControl {
                ui_routed_sessions: HashSet::new(),
                encoding,
                scrollback_limit,
                source_queued_events: 0,
                source_queued_output_bytes: 0,
            }),
            ui_queue,
            ui_queue_wake_rx: Mutex::new(Some(ui_queue_wake_rx)),
            source_queued_events: AtomicUsize::new(0),
            source_queued_output_bytes: AtomicUsize::new(0),
            direct_output_events: AtomicU64::new(0),
            direct_output_bytes: AtomicU64::new(0),
            direct_backpressure_events: AtomicU64::new(0),
            direct_backpressure_bytes: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        });
        let worker_state = state.clone();
        let worker = thread::Builder::new()
            .name("nyaterm-session-event-bridge".to_string())
            .spawn(move || run_session_event_bridge(session_manager, frame_pipeline, worker_state))
            .expect("failed to spawn session event bridge");
        Self {
            state,
            worker: Some(worker),
        }
    }

    /// Taken once, by the drain task that consumes this queue.
    pub(crate) fn take_ui_queue_wake_receiver(&self) -> Option<UnboundedReceiver<()>> {
        self.state.ui_queue_wake_rx.lock().ok()?.take()
    }

    /// Declare interest in the next UI-queue push. Call before draining; see
    /// [`crate::models::event_wake`] for why the other order loses wakes.
    pub(crate) fn arm_ui_queue_wake(&self) {
        self.state.ui_queue.arm_wake();
    }

    /// Enqueue as the bridge worker thread would, so a test can exercise the wake
    /// and the drain without a live PTY.
    #[cfg(test)]
    pub(crate) fn push_ui_event_for_test(&self, event: SessionEvent) {
        self.state.ui_queue.push(event);
    }

    pub(crate) fn configure(&self, encoding: String, scrollback_limit: usize) {
        let Ok(mut control) = self.state.control.lock() else {
            return;
        };
        if control.encoding == encoding && control.scrollback_limit == scrollback_limit {
            return;
        }
        control.encoding = encoding;
        control.scrollback_limit = scrollback_limit;
    }

    pub(crate) fn route_session_to_ui(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        if let Ok(mut control) = self.state.control.lock() {
            if control.ui_routed_sessions.contains(session_id) {
                return;
            }
            control.ui_routed_sessions.insert(session_id.to_string());
        }
    }

    pub(crate) fn resume_session_direct_output(&self, session_id: &str) {
        if let Ok(mut control) = self.state.control.lock() {
            control.ui_routed_sessions.remove(session_id);
        }
    }

    pub(crate) fn clear_session(&self, session_id: &str) {
        self.resume_session_direct_output(session_id);
    }

    pub(crate) fn drain_events_with_output_budget(
        &self,
        max_events: usize,
        max_output_bytes: usize,
    ) -> SessionEventBridgeDrain {
        let mut drain = self
            .state
            .ui_queue
            .drain_with_output_budget(max_events, max_output_bytes);
        drain.stats.direct_output_events =
            self.state.direct_output_events.swap(0, Ordering::Relaxed);
        drain.stats.direct_output_bytes = self.state.direct_output_bytes.swap(0, Ordering::Relaxed);
        drain.stats.direct_backpressure_events = self
            .state
            .direct_backpressure_events
            .swap(0, Ordering::Relaxed);
        drain.stats.direct_backpressure_bytes = self
            .state
            .direct_backpressure_bytes
            .swap(0, Ordering::Relaxed);
        drain.stats.source_queued_events = self.state.source_queued_events.load(Ordering::Relaxed);
        drain.stats.source_queued_output_bytes = self
            .state
            .source_queued_output_bytes
            .load(Ordering::Relaxed);
        drain
    }

    pub(crate) fn queued_event_count(&self) -> usize {
        self.state.ui_queue.len()
    }

    pub(crate) fn queued_output_bytes(&self) -> usize {
        self.state.ui_queue.queued_output_bytes()
    }

    pub(crate) fn source_queued_event_count(&self) -> usize {
        self.state.source_queued_events.load(Ordering::Relaxed)
    }

    pub(crate) fn source_queued_output_bytes(&self) -> usize {
        self.state
            .source_queued_output_bytes
            .load(Ordering::Relaxed)
    }

    pub(crate) fn has_pending_ui_work(&self) -> bool {
        self.queued_event_count() > 0
            || self.source_queued_event_count() > 0
            || self.source_queued_output_bytes() > 0
    }

    /// Harvest direct/source counters without locking the UI queue.
    /// Used on idle ticks when there is no UI-routed work to drain.
    pub(crate) fn harvest_direct_stats(&self) -> SessionEventBridgeStats {
        SessionEventBridgeStats {
            direct_output_events: self.state.direct_output_events.swap(0, Ordering::Relaxed),
            direct_output_bytes: self.state.direct_output_bytes.swap(0, Ordering::Relaxed),
            direct_backpressure_events: self
                .state
                .direct_backpressure_events
                .swap(0, Ordering::Relaxed),
            direct_backpressure_bytes: self
                .state
                .direct_backpressure_bytes
                .swap(0, Ordering::Relaxed),
            drained_ui_events: 0,
            drained_ui_output_bytes: 0,
            ui_queued_events: self.queued_event_count(),
            ui_queued_output_bytes: self.queued_output_bytes(),
            source_queued_events: self.source_queued_event_count(),
            source_queued_output_bytes: self.source_queued_output_bytes(),
            dropped_output_bytes: 0,
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.state.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("session event bridge panicked during shutdown");
        }
    }
}

impl Drop for SessionEventBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl SessionEventBridgeState {
    fn control_snapshot(&self) -> Option<SessionEventBridgeControlSnapshot> {
        let control = self.control.lock().ok()?;
        Some(SessionEventBridgeControlSnapshot {
            ui_routed_sessions: control.ui_routed_sessions.clone(),
            encoding: control.encoding.clone(),
            scrollback_limit: control.scrollback_limit,
        })
    }

    fn update_source_stats(&self, stats: &SessionDrainStats) {
        self.source_queued_events
            .store(stats.queued_events, Ordering::Relaxed);
        self.source_queued_output_bytes
            .store(stats.queued_output_bytes, Ordering::Relaxed);
        if let Ok(mut control) = self.control.lock() {
            control.source_queued_events = stats.queued_events;
            control.source_queued_output_bytes = stats.queued_output_bytes;
        }
    }

    fn route_session_to_ui(&self, session_id: &str) {
        if let Ok(mut control) = self.control.lock() {
            control.ui_routed_sessions.insert(session_id.to_string());
        }
    }
}

impl SessionEventBridgeQueue {
    #[cfg(test)]
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionEventBridgeQueueInner::default())),
            wake: None,
        }
    }

    fn new_with_wake() -> (Self, UnboundedReceiver<()>) {
        let (wake, wake_rx) = EventWake::new();
        (
            Self {
                inner: Arc::new(Mutex::new(SessionEventBridgeQueueInner::default())),
                wake: Some(wake),
            },
            wake_rx,
        )
    }

    fn arm_wake(&self) {
        if let Some(wake) = &self.wake {
            wake.arm(ANY_INTEREST);
        }
    }

    #[cfg(test)]
    fn wake_count(&self) -> u64 {
        self.wake.as_ref().map(EventWake::signal_count).unwrap_or(0)
    }

    /// The single place the bridge's UI queue signals its consumer.
    ///
    /// The wake lives here rather than at the six `ui_queue.push` sites in
    /// `run_session_event_bridge`, so no producer path can be added later that
    /// enqueues without waking. `ANY_INTEREST` because the consumer treats every
    /// entry the same; the interest gate is what turns a flood into one wake per
    /// drain cycle instead of one per event.
    fn push(&self, event: SessionEvent) {
        {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            inner.push(event);
        }
        // Signalled outside the lock: nothing here needs it, and the consumer may
        // start draining the moment it is woken.
        if let Some(wake) = &self.wake {
            wake.signal(ANY_INTEREST);
        }
    }

    fn drain_with_output_budget(
        &self,
        max_events: usize,
        max_output_bytes: usize,
    ) -> SessionEventBridgeDrain {
        let Ok(mut inner) = self.inner.lock() else {
            return SessionEventBridgeDrain::default();
        };
        inner.drain(max_events, max_output_bytes)
    }

    fn len(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.events.len())
            .unwrap_or(0)
    }

    fn queued_output_bytes(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.queued_output_bytes)
            .unwrap_or(0)
    }
}

impl SessionEventBridgeQueueInner {
    fn push(&mut self, event: SessionEvent) {
        match &event {
            SessionEvent::Output { data, .. } if data.is_empty() => return,
            SessionEvent::Output { data, .. } => {
                self.queued_output_bytes = self.queued_output_bytes.saturating_add(data.len());
            }
            SessionEvent::OutputDropped { .. }
            | SessionEvent::CwdChanged { .. }
            | SessionEvent::CommandAccepted { .. }
            | SessionEvent::Exited { .. }
            | SessionEvent::Error { .. } => {}
        }
        self.events.push_back(event);
    }

    fn drain(&mut self, max_events: usize, max_output_bytes: usize) -> SessionEventBridgeDrain {
        let mut events = Vec::new();
        let mut stats = SessionEventBridgeStats::default();
        let mut drained_events = 0usize;
        let mut drained_output_bytes = 0usize;
        for _ in 0..max_events {
            if drained_output_bytes >= max_output_bytes
                && drained_events > 0
                && matches!(self.events.front(), Some(SessionEvent::Output { .. }))
            {
                break;
            }
            let remaining_output_budget = max_output_bytes.saturating_sub(drained_output_bytes);
            if remaining_output_budget == 0
                && drained_events > 0
                && matches!(self.events.front(), Some(SessionEvent::Output { .. }))
            {
                break;
            }
            if let Some(SessionEvent::Output { session_id, data }) = self.events.front_mut() {
                let take = data.len().min(remaining_output_budget);
                if data.len() > take {
                    let remaining = data.split_off(take);
                    let chunk = std::mem::replace(data, remaining);
                    let session_id = session_id.clone();
                    self.queued_output_bytes = self.queued_output_bytes.saturating_sub(chunk.len());
                    drained_events = drained_events.saturating_add(1);
                    drained_output_bytes = drained_output_bytes.saturating_add(chunk.len());
                    events.push(SessionEvent::Output {
                        session_id,
                        data: chunk,
                    });
                    continue;
                }
            }
            let Some(event) = self.events.pop_front() else {
                break;
            };
            drained_events = drained_events.saturating_add(1);
            match &event {
                SessionEvent::Output { data, .. } => {
                    drained_output_bytes = drained_output_bytes.saturating_add(data.len());
                    self.queued_output_bytes = self.queued_output_bytes.saturating_sub(data.len());
                }
                SessionEvent::OutputDropped { bytes, .. } => {
                    stats.dropped_output_bytes = stats.dropped_output_bytes.saturating_add(*bytes);
                }
                SessionEvent::CwdChanged { .. }
                | SessionEvent::CommandAccepted { .. }
                | SessionEvent::Exited { .. }
                | SessionEvent::Error { .. } => {}
            }
            events.push(event);
        }
        stats.drained_ui_events = drained_events;
        stats.drained_ui_output_bytes = drained_output_bytes;
        stats.ui_queued_events = self.events.len();
        stats.ui_queued_output_bytes = self.queued_output_bytes;
        SessionEventBridgeDrain { events, stats }
    }
}

fn run_session_event_bridge(
    session_manager: Arc<SessionManager>,
    frame_pipeline: TerminalFramePipeline,
    state: Arc<SessionEventBridgeState>,
) {
    let mut sideband_probe_sessions: HashMap<String, SessionEventBridgeSidebandProbe> =
        HashMap::new();
    let mut source_drain_backpressured = false;
    while !state.stop.load(Ordering::Relaxed) {
        let Some(control) = state.control_snapshot() else {
            thread::sleep(SESSION_EVENT_BRIDGE_IDLE_SLEEP);
            continue;
        };
        source_drain_backpressured = bridge_should_pause_source_drain(
            frame_pipeline.queued_output_bytes(),
            state.ui_queue.queued_output_bytes(),
            source_drain_backpressured,
        );
        if source_drain_backpressured {
            thread::sleep(SESSION_EVENT_BRIDGE_BUSY_SLEEP);
            continue;
        }
        // Park on the queue rather than polling it: a PTY read wakes this
        // thread directly, so the first hop of the echo path no longer spends
        // an arbitrary slice of the poll interval waiting to notice.
        let Ok(drain) = session_manager.drain_events_blocking_with_output_budget(
            SESSION_EVENT_BRIDGE_DRAIN_BATCH,
            SESSION_EVENT_BRIDGE_OUTPUT_BUDGET,
            SESSION_EVENT_BRIDGE_WAIT_TIMEOUT,
        ) else {
            thread::sleep(SESSION_EVENT_BRIDGE_IDLE_SLEEP);
            continue;
        };
        state.update_source_stats(&drain.stats);
        if drain.events.is_empty() {
            // The park above already absorbed the idle wait.
            continue;
        }
        let mut pending_direct_outputs = Vec::new();
        for event in drain.events {
            match event {
                SessionEvent::Output { session_id, data } => {
                    let now = Instant::now();
                    let sideband_probe_active = bridge_sideband_probe_active(
                        &mut sideband_probe_sessions,
                        &session_id,
                        now,
                    );
                    let sideband_probe_detected = bridge_output_may_contain_sideband_trigger(&data);
                    if sideband_probe_detected {
                        bridge_arm_sideband_probe(&mut sideband_probe_sessions, &session_id, now);
                    }
                    let needs_ui_probe = sideband_probe_active || sideband_probe_detected;
                    let frame_queued_output_bytes = frame_pipeline.queued_output_bytes();
                    if bridge_output_can_go_direct(
                        &control,
                        frame_queued_output_bytes,
                        &session_id,
                        needs_ui_probe,
                    ) {
                        state.direct_output_events.fetch_add(1, Ordering::Relaxed);
                        state
                            .direct_output_bytes
                            .fetch_add(data.len() as u64, Ordering::Relaxed);
                        pending_direct_outputs.push(TerminalFrameOutputSubmission {
                            session_id,
                            data,
                            encoding: control.encoding.clone(),
                            scrollback_limit: control.scrollback_limit,
                        });
                    } else if bridge_output_is_backpressured(
                        frame_queued_output_bytes,
                        &control,
                        &session_id,
                        needs_ui_probe,
                    ) {
                        state
                            .direct_backpressure_events
                            .fetch_add(1, Ordering::Relaxed);
                        state
                            .direct_backpressure_bytes
                            .fetch_add(data.len() as u64, Ordering::Relaxed);
                        pending_direct_outputs.push(TerminalFrameOutputSubmission {
                            session_id,
                            data,
                            encoding: control.encoding.clone(),
                            scrollback_limit: control.scrollback_limit,
                        });
                    } else {
                        flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
                        // Ambiguous side-band prefixes such as "*" or ":" are common in normal
                        // shell output. Probe this chunk on the UI side, but let the detector
                        // state decide whether future chunks need sticky UI routing.
                        let routed_session_id = session_id.clone();
                        state
                            .ui_queue
                            .push(SessionEvent::Output { session_id, data });
                        bridge_consume_sideband_probe(
                            &mut sideband_probe_sessions,
                            &routed_session_id,
                        );
                    }
                }
                SessionEvent::OutputDropped { session_id, bytes } => {
                    flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
                    sideband_probe_sessions.remove(&session_id);
                    state.route_session_to_ui(&session_id);
                    state
                        .ui_queue
                        .push(SessionEvent::OutputDropped { session_id, bytes });
                }
                SessionEvent::CwdChanged { session_id, cwd } => {
                    flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
                    state
                        .ui_queue
                        .push(SessionEvent::CwdChanged { session_id, cwd });
                }
                SessionEvent::CommandAccepted {
                    session_id,
                    command,
                } => {
                    flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
                    state.ui_queue.push(SessionEvent::CommandAccepted {
                        session_id,
                        command,
                    });
                }
                SessionEvent::Exited { session_id, reason } => {
                    flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
                    sideband_probe_sessions.remove(&session_id);
                    state
                        .ui_queue
                        .push(SessionEvent::Exited { session_id, reason });
                }
                SessionEvent::Error {
                    session_id,
                    message,
                } => {
                    flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
                    state.ui_queue.push(SessionEvent::Error {
                        session_id,
                        message,
                    });
                }
            }
        }
        flush_bridge_direct_outputs(&frame_pipeline, &mut pending_direct_outputs);
    }
}

fn bridge_should_pause_source_drain(
    frame_pipeline_queued_output_bytes: usize,
    ui_queued_output_bytes: usize,
    currently_backpressured: bool,
) -> bool {
    if currently_backpressured {
        frame_pipeline_queued_output_bytes > SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_LOW_WATERMARK
            || ui_queued_output_bytes > SESSION_EVENT_BRIDGE_UI_OUTPUT_LOW_WATERMARK
    } else {
        frame_pipeline_queued_output_bytes >= SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE
            || ui_queued_output_bytes >= SESSION_EVENT_BRIDGE_UI_OUTPUT_LIMIT
    }
}

fn flush_bridge_direct_outputs(
    frame_pipeline: &TerminalFramePipeline,
    pending_outputs: &mut Vec<TerminalFrameOutputSubmission>,
) {
    if !pending_outputs.is_empty() {
        frame_pipeline.submit_outputs(std::mem::take(pending_outputs));
    }
}

fn bridge_output_can_go_direct(
    control: &SessionEventBridgeControlSnapshot,
    frame_pipeline_queued_output_bytes: usize,
    session_id: &str,
    needs_ui_probe: bool,
) -> bool {
    !control.ui_routed_sessions.contains(session_id)
        && frame_pipeline_queued_output_bytes < SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE
        && !needs_ui_probe
}

fn bridge_output_is_backpressured(
    frame_pipeline_queued_output_bytes: usize,
    control: &SessionEventBridgeControlSnapshot,
    session_id: &str,
    needs_ui_probe: bool,
) -> bool {
    !control.ui_routed_sessions.contains(session_id)
        && frame_pipeline_queued_output_bytes >= SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE
        && !needs_ui_probe
}

fn bridge_output_may_contain_sideband_trigger(data: &[u8]) -> bool {
    ZmodemDetector::output_may_contain_trigger(data)
        || TrzszDetector::output_may_contain_trigger(data)
}

fn bridge_arm_sideband_probe(
    probes: &mut HashMap<String, SessionEventBridgeSidebandProbe>,
    session_id: &str,
    now: Instant,
) {
    probes.insert(
        session_id.to_string(),
        SessionEventBridgeSidebandProbe {
            events_remaining: SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_EVENTS,
            expires_at: now + SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_WINDOW,
        },
    );
}

fn bridge_sideband_probe_active(
    probes: &mut HashMap<String, SessionEventBridgeSidebandProbe>,
    session_id: &str,
    now: Instant,
) -> bool {
    let Some(probe) = probes.get(session_id).copied() else {
        return false;
    };
    if probe.events_remaining == 0 || now >= probe.expires_at {
        probes.remove(session_id);
        return false;
    }
    true
}

fn bridge_consume_sideband_probe(
    probes: &mut HashMap<String, SessionEventBridgeSidebandProbe>,
    session_id: &str,
) {
    let Some(probe) = probes.get_mut(session_id) else {
        return;
    };
    probe.events_remaining = probe.events_remaining.saturating_sub(1);
    if probe.events_remaining == 0 {
        probes.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::time::{Duration, Instant};

    use super::{
        SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE,
        SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_LOW_WATERMARK,
        SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_EVENTS, SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_WINDOW,
        SESSION_EVENT_BRIDGE_UI_OUTPUT_LIMIT, SESSION_EVENT_BRIDGE_UI_OUTPUT_LOW_WATERMARK,
        SessionEventBridgeControlSnapshot, SessionEventBridgeQueue, bridge_arm_sideband_probe,
        bridge_consume_sideband_probe, bridge_output_can_go_direct, bridge_output_is_backpressured,
        bridge_output_may_contain_sideband_trigger, bridge_should_pause_source_drain,
        bridge_sideband_probe_active,
    };
    use nyaterm_transport::SessionEvent;

    #[test]
    fn bridge_direct_policy_rejects_sideband_triggers() {
        let control = SessionEventBridgeControlSnapshot {
            ui_routed_sessions: HashSet::new(),
            encoding: "UTF-8".to_string(),
            scrollback_limit: 1000,
        };
        assert!(bridge_output_can_go_direct(&control, 0, "s1", false));
        assert!(!bridge_output_can_go_direct(&control, 0, "s1", true));
        assert!(bridge_output_may_contain_sideband_trigger(b"**\x18B"));
        assert!(bridge_output_may_contain_sideband_trigger(b"::TRZSZ:"));
    }

    #[test]
    fn bridge_direct_policy_honors_ui_routed_sessions() {
        let mut routed = HashSet::new();
        routed.insert("s1".to_string());
        let control = SessionEventBridgeControlSnapshot {
            ui_routed_sessions: routed,
            encoding: "UTF-8".to_string(),
            scrollback_limit: 1000,
        };
        assert!(!bridge_output_can_go_direct(&control, 0, "s1", false));
        assert!(bridge_output_can_go_direct(&control, 0, "s2", false));
    }

    #[test]
    fn bridge_direct_policy_yields_under_frame_backpressure() {
        let control = SessionEventBridgeControlSnapshot {
            ui_routed_sessions: HashSet::new(),
            encoding: "UTF-8".to_string(),
            scrollback_limit: 1000,
        };

        assert!(bridge_output_can_go_direct(
            &control,
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE - 1,
            "s1",
            false
        ));
        assert!(!bridge_output_can_go_direct(
            &control,
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE,
            "s1",
            false
        ));
        assert!(bridge_output_is_backpressured(
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE,
            &control,
            "s1",
            false
        ));
        assert!(!bridge_output_is_backpressured(
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE,
            &control,
            "s1",
            true
        ));
    }

    #[test]
    fn bridge_ui_queue_coalesces_a_burst_into_one_wake() {
        let (queue, mut wake_rx) = SessionEventBridgeQueue::new_with_wake();

        // Unarmed: the consumer is already draining, so a push must cost nothing.
        queue.push(SessionEvent::Output {
            session_id: "s1".to_string(),
            data: b"before the arm".to_vec(),
        });
        assert_eq!(queue.wake_count(), 0);

        queue.arm_wake();
        for index in 0..64 {
            queue.push(SessionEvent::Output {
                session_id: "s1".to_string(),
                data: format!("chunk {index}").into_bytes(),
            });
        }

        assert_eq!(
            queue.wake_count(),
            1,
            "a flood must cost one wake per drain cycle, not one per event"
        );
        assert!(matches!(wake_rx.try_recv(), Ok(())));
        assert!(
            wake_rx.try_recv().is_err(),
            "only the first push after an arm should have queued a wake"
        );

        // And the next cycle's arm re-enables delivery.
        queue.arm_wake();
        queue.push(SessionEvent::CwdChanged {
            session_id: "s1".to_string(),
            cwd: "/srv/app".to_string(),
        });
        assert_eq!(queue.wake_count(), 2);
    }

    #[test]
    fn bridge_ui_queue_without_a_wake_still_queues() {
        let queue = SessionEventBridgeQueue::new();
        queue.arm_wake();
        queue.push(SessionEvent::CwdChanged {
            session_id: "s1".to_string(),
            cwd: "/srv/app".to_string(),
        });

        assert_eq!(queue.len(), 1);
        assert_eq!(queue.wake_count(), 0);
    }

    #[test]
    fn bridge_ui_queue_drains_metadata_when_output_budget_is_zero() {
        let queue = SessionEventBridgeQueue::new();
        queue.push(SessionEvent::CwdChanged {
            session_id: "s1".to_string(),
            cwd: "/srv/app".to_string(),
        });
        queue.push(SessionEvent::CommandAccepted {
            session_id: "s1".to_string(),
            command: "cargo test".to_string(),
        });

        let drain = queue.drain_with_output_budget(8, 0);

        assert_eq!(drain.events.len(), 2);
        assert!(matches!(
            &drain.events[0],
            SessionEvent::CwdChanged { cwd, .. } if cwd == "/srv/app"
        ));
        assert!(matches!(
            &drain.events[1],
            SessionEvent::CommandAccepted { command, .. } if command == "cargo test"
        ));
    }

    #[test]
    fn bridge_pauses_source_drain_with_high_low_watermark_hysteresis() {
        assert!(bridge_should_pause_source_drain(
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_BACKPRESSURE,
            0,
            false,
        ));
        assert!(bridge_should_pause_source_drain(
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_LOW_WATERMARK + 1,
            0,
            true,
        ));
        assert!(!bridge_should_pause_source_drain(
            SESSION_EVENT_BRIDGE_DIRECT_OUTPUT_LOW_WATERMARK,
            SESSION_EVENT_BRIDGE_UI_OUTPUT_LOW_WATERMARK,
            true,
        ));
        assert!(bridge_should_pause_source_drain(
            0,
            SESSION_EVENT_BRIDGE_UI_OUTPUT_LIMIT,
            false,
        ));
    }

    #[test]
    fn bridge_ui_queue_preserves_output_above_the_old_limit() {
        let queue = SessionEventBridgeQueue::new();
        let first = vec![b'a'; SESSION_EVENT_BRIDGE_UI_OUTPUT_LIMIT];
        let second = vec![b'b'; 32];
        queue.push(SessionEvent::Output {
            session_id: "s1".to_string(),
            data: first.clone(),
        });
        queue.push(SessionEvent::Output {
            session_id: "s1".to_string(),
            data: second.clone(),
        });

        let drain = queue.drain_with_output_budget(8, usize::MAX);
        let output = drain
            .events
            .into_iter()
            .flat_map(|event| match event {
                SessionEvent::Output { data, .. } => data,
                _ => Vec::new(),
            })
            .collect::<Vec<_>>();
        assert_eq!(output.len(), first.len() + second.len());
        assert_eq!(&output[..first.len()], first.as_slice());
        assert_eq!(&output[first.len()..], second.as_slice());
        assert_eq!(drain.stats.dropped_output_bytes, 0);
    }

    #[test]
    fn bridge_sideband_probe_is_bounded_by_events_and_time() {
        let mut probes = HashMap::new();
        let now = Instant::now();
        bridge_arm_sideband_probe(&mut probes, "s1", now);

        assert!(bridge_sideband_probe_active(&mut probes, "s1", now));
        for _ in 0..SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_EVENTS {
            bridge_consume_sideband_probe(&mut probes, "s1");
        }
        assert!(!bridge_sideband_probe_active(&mut probes, "s1", now));

        bridge_arm_sideband_probe(&mut probes, "s2", now);
        assert!(!bridge_sideband_probe_active(
            &mut probes,
            "s2",
            now + SESSION_EVENT_BRIDGE_SIDEBAND_PROBE_WINDOW + Duration::from_millis(1)
        ));
    }
}
