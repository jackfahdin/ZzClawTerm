use std::time::Instant;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

pub(in crate::features) struct RemoteJobTicket<Event> {
    pub job_id: u64,
    pub tx: UnboundedSender<Event>,
}

pub(super) struct RemoteJobState<Event> {
    tx: UnboundedSender<Event>,
    /// Taken once when the pane's drain task starts, which owns delivery from
    /// then on. `None` afterwards, so a second start is a no-op.
    rx: Option<UnboundedReceiver<Event>>,
    pending: bool,
    job_id: u64,
    session_id: Option<String>,
    consecutive_refresh_failures: u8,
    last_refresh_at: Option<Instant>,
}

impl<Event> RemoteJobState<Event> {
    pub(super) fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            tx,
            rx: Some(rx),
            pending: false,
            job_id: 0,
            session_id: None,
            consecutive_refresh_failures: 0,
            last_refresh_at: None,
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending
    }

    pub(super) fn is_pending_for(&self, session_id: &str) -> bool {
        self.pending && self.session_id.as_deref() == Some(session_id)
    }

    pub(super) fn last_refresh_at(&self) -> Option<Instant> {
        self.last_refresh_at
    }

    pub(super) fn consecutive_refresh_failures(&self) -> u8 {
        self.consecutive_refresh_failures
    }

    pub(super) fn begin(&mut self, session_id: String) -> RemoteJobTicket<Event> {
        self.job_id = self.job_id.wrapping_add(1).max(1);
        self.session_id = Some(session_id);
        self.pending = true;
        RemoteJobTicket {
            job_id: self.job_id,
            tx: self.tx.clone(),
        }
    }

    pub(super) fn mark_refresh_started(&mut self) {
        self.last_refresh_at = Some(Instant::now());
    }

    pub(super) fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<Event>> {
        self.rx.take()
    }

    pub(super) fn complete_if_matches(&mut self, job_id: u64, session_id: &str) -> bool {
        if self.job_id != job_id || self.session_id.as_deref() != Some(session_id) {
            return false;
        }
        self.pending = false;
        self.session_id = None;
        true
    }

    pub(super) fn reset_refresh_failures(&mut self) {
        self.consecutive_refresh_failures = 0;
    }

    pub(super) fn record_refresh_failure(&mut self, terminal: bool) -> u8 {
        self.consecutive_refresh_failures = if terminal {
            3
        } else {
            self.consecutive_refresh_failures.saturating_add(1)
        };
        self.consecutive_refresh_failures
    }

    pub(super) fn reset_for_session_switch(&mut self) {
        self.pending = false;
        self.session_id = None;
        self.consecutive_refresh_failures = 0;
        self.last_refresh_at = None;
    }
}
