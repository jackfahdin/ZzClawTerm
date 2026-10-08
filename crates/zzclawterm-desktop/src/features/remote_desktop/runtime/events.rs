use super::errors::format_rdp_error;
use super::input::POINTER_MOVE_INTERVAL;
use crate::features::ZzClawTermApp;
use crate::features::remote_desktop::state::resize::should_disable_dynamic_resize_after_state;
use crate::models::TransferJobKind;
use crate::models::TransferJobState;
use crate::models::TransferJobStatus;
use futures::StreamExt as _;
use gpui::ClipboardItem;
use gpui::Context;
use gpui::Window;
use std::time::Duration;
use std::time::Instant;
use zzclawterm_remote_desktop::ClipboardOrigin;
use zzclawterm_remote_desktop::RDP_FRAMEBUFFER_LIMITS;
use zzclawterm_remote_desktop::RdpCapability;
use zzclawterm_remote_desktop::RdpClipboardTransferStatus;
use zzclawterm_remote_desktop::RdpError;
use zzclawterm_remote_desktop::RdpErrorKind;
use zzclawterm_remote_desktop::RdpFrameEvent;
use zzclawterm_remote_desktop::RdpRuntimeEvent;
use zzclawterm_remote_desktop::RdpSessionState;
use zzclawterm_remote_desktop::RemoteDesktopError;
use zzclawterm_remote_desktop::RemoteDesktopViewState;
use zzclawterm_remote_desktop::VNC_FRAMEBUFFER_LIMITS;
use zzclawterm_remote_desktop::VncRuntimeEvent;
use zzclawterm_remote_desktop::VncServerCapabilities;
use zzclawterm_remote_desktop::VncSessionState;
use zzclawterm_store::StoreDomain;
use zzclawterm_store::store_request;
use zzclawterm_transport::SftpTransferProgress;

/// Cadence for remote-desktop maintenance when no pointer move is waiting.
///
/// Finer than the shortest thing it services (`RESIZE_DEBOUNCE`), so a debounce still
/// resolves promptly after the user stops; the clipboard and metrics intervals gate
/// themselves, so this only costs a cheap check for those.
pub(super) const MAINTENANCE_INTERVAL: Duration = Duration::from_millis(100);

pub(super) const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(5);

impl ZzClawTermApp {
    /// Deliver RDP and VNC session events as the helper processes produce them.
    ///
    /// Started once at window open. Before this the runtime tick polled every
    /// session queue, which capped remote-desktop framerate at the tick cadence:
    /// `has_protocol_runtime_sessions()` keeps the tick off the 500ms quiet
    /// interval, but that still left 50ms idle / 16ms under pressure, so a helper
    /// delivering 60fps was sampled at 20-60fps.
    ///
    /// Frame deltas stay ordered in bounded queues; only the wake signal is a
    /// channel (see `models::event_wake`). `update_in` rather
    /// than `update` because applying a frame needs the `Window` for its dynamic
    /// texture.
    pub(in crate::features) fn start_remote_desktop_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut wake_rx) = self.remote_desktop.take_wake_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            loop {
                // Arm before draining, so a frame enqueued in between still
                // signals rather than waiting for the next one.
                let drained = this.update_in(cx, |this, window, cx| {
                    this.remote_desktop.arm_event_wake();
                    // Any event means a session exists; the periodic clock is scoped
                    // to that, and every reconnect is scheduled from an event handler.
                    this.ensure_remote_desktop_periodic_clock(cx);
                    this.drain_remote_desktop_queues(window, cx)
                });
                match drained {
                    Err(_) => break,
                    // A frame can arrive while the previous one is being applied.
                    Ok(true) => {
                        // Bound consecutive UI updates even with continuously full queues.
                        cx.background_executor()
                            .timer(Duration::from_millis(1))
                            .await;
                        continue;
                    }
                    Ok(false) => {}
                }
                if wake_rx.next().await.is_none() {
                    break;
                }
            }
        })
        .detach();
    }

    /// The queue half: everything the helper processes push.
    pub(super) fn drain_remote_desktop_queues(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        for texture in self.remote_desktop.pending_texture_removals.drain(..) {
            window.remove_dynamic_texture(texture);
        }
        let mut ids = self
            .remote_desktop
            .sessions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        ids.sort_unstable();
        if !ids.is_empty() {
            let offset = self.remote_desktop.drain_cursor % ids.len();
            ids.rotate_left(offset);
        }
        let started = Instant::now();
        let mut dirty = false;
        let mut root_dirty = false;
        for session_id in ids {
            self.remote_desktop.drain_cursor = self.remote_desktop.drain_cursor.wrapping_add(1);
            let drain = self.remote_desktop.manager.drain_batch(&session_id);
            let vnc_drain = self.remote_desktop.vnc_manager.drain_batch(&session_id);
            if drain.control.is_empty()
                && drain.frames.is_empty()
                && drain.cursors.is_empty()
                && vnc_drain.control.is_empty()
                && vnc_drain.frames.is_empty()
                && vnc_drain.cursors.is_empty()
            {
                continue;
            }
            if self.remote_desktop.metrics_enabled {
                self.remote_desktop.metrics_control_events += drain.control.len();
                self.remote_desktop.metrics_frame_updates += drain.frames.len();
                self.remote_desktop.metrics_control_events += vnc_drain.control.len();
                self.remote_desktop.metrics_control_events += vnc_drain.cursors.len();
                self.remote_desktop.metrics_frame_updates += vnc_drain.frames.len();
            }
            dirty = true;
            root_dirty |= !drain.control.is_empty() || !vnc_drain.control.is_empty();
            let previous_state = self
                .remote_desktop
                .sessions
                .get(&session_id)
                .map(|session| session.state);
            for event in drain.control {
                self.apply_rdp_control_event(&session_id, event, window, cx);
            }
            self.apply_remote_cursor_batch(&session_id, drain.cursors, window);
            self.apply_rdp_frame_batch(&session_id, drain.frames, window);
            for event in vnc_drain.control {
                self.apply_vnc_control_event(&session_id, event, window, cx);
            }
            self.apply_remote_cursor_batch(&session_id, vnc_drain.cursors, window);
            self.apply_rdp_frame_batch(&session_id, vnc_drain.frames, window);
            root_dirty |= previous_state
                != self
                    .remote_desktop
                    .sessions
                    .get(&session_id)
                    .map(|session| session.state);
            if self.session.active_id() == Some(session_id.as_str())
                && let Some(surface) = self.remote_desktop.surfaces.get(&session_id)
            {
                surface.update(cx, |_, cx| cx.notify());
            }
            if self
                .remote_desktop
                .sessions
                .get(&session_id)
                .is_some_and(|session| {
                    matches!(
                        session.state,
                        RemoteDesktopViewState::Failed | RemoteDesktopViewState::Disconnected
                    )
                })
            {
                self.remote_desktop.routes.remove(&session_id);
            }
            if started.elapsed() >= Duration::from_millis(4) {
                break;
            }
        }
        self.settle_remote_desktop_restore(cx);
        if root_dirty {
            cx.notify();
        }
        dirty
    }

    /// The time-based half, still driven by the runtime tick.
    ///
    /// None of these is a queue read: a pointer batch flushes after a hold, a
    /// resize is debounced, the reconnect ladder waits out a backoff, the
    /// clipboard is polled on an interval, and metrics report on one. Giving each
    /// its own timer is Phase 2 of the runtime-tick plan.
    /// Drive remote-desktop maintenance on its own cadence while a session exists.
    ///
    /// These six are all genuinely time-based -- a coalesced pointer move flushes after
    /// a hold, a resize is debounced, the reconnect ladder waits out a backoff, and
    /// the clipboard and metrics report on intervals -- so this stays a poll. What was
    /// wrong was *whose* cadence it used: `runtime_quiet_tick_allowed` has no
    /// remote-desktop term, so an otherwise-idle app with a live RDP session ran this
    /// at the 500ms quiet interval, and the trailing pointer move of a gesture --
    /// budgeted at `POINTER_MOVE_INTERVAL`, 8ms -- landed up to half a second late.
    ///
    /// Armed from the remote-desktop event drain. Every reconnect is scheduled by an
    /// event handler, and a connecting session always reports at least one state
    /// change, so an event is a reliable point to start from.
    pub(in crate::features) fn ensure_remote_desktop_periodic_clock(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.remote_desktop.periodic_clock_is_armed() || !self.remote_desktop.has_sessions() {
            return;
        }
        self.remote_desktop.set_periodic_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(delay) = this.update(cx, |this, _| {
                    remote_desktop_periodic_delay(this.remote_desktop.pointer_flush_is_pending())
                }) else {
                    break;
                };
                cx.background_executor().timer(delay).await;
                // `update_in`: keyboard-capture sync needs the window.
                let Ok(keep_running) = this.update_in(cx, |this, window, cx| {
                    if this.drive_remote_desktop_periodic(window, cx) {
                        cx.notify();
                    }
                    let running = this.remote_desktop.has_sessions();
                    if !running {
                        this.remote_desktop.set_periodic_clock_armed(false);
                    }
                    running
                }) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::features) fn drive_remote_desktop_periodic(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut dirty = self.drive_rdp_pointer_flush();
        dirty |= self.drive_rdp_resize_debounce();
        dirty |= self.drive_rdp_reconnects(cx);
        self.sync_rdp_keyboard_capture(window);
        dirty |= self.poll_active_rdp_clipboard(cx);
        self.report_rdp_metrics();
        dirty
    }

    pub(super) fn apply_vnc_control_event(
        &mut self,
        session_id: &str,
        event: VncRuntimeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            VncRuntimeEvent::ServerKeyRequest(request) => self.handle_vnc_key_request(request, cx),
            VncRuntimeEvent::ServerKeyAuthenticated(request) => {
                let previous = self
                    .remote_desktop
                    .sessions
                    .get(session_id)
                    .and_then(|session| session.vnc_trust_previous.clone());
                if let Some(trust) = self.remote_desktop.vnc_manager.trust_state(session_id) {
                    self.submit_store_request(
                        0,
                        store_request(StoreDomain::Security, move |store| {
                            trust
                                .commit_if_current(&request, |current| {
                                    store.remember_vnc_known_host_if_current(
                                        &request.host,
                                        request.port,
                                        &request.sha256_fingerprint,
                                        previous.as_deref(),
                                        current,
                                    )
                                })
                                .transpose()
                                .map(|_| ())
                        }),
                        |_, _, _| {},
                        cx,
                    );
                }
            }

            VncRuntimeEvent::State { state, message, .. } => {
                let vnc_server_capabilities = vnc_capabilities_for_state(
                    &state,
                    self.remote_desktop
                        .vnc_manager
                        .server_capabilities(session_id),
                );
                let state = RemoteDesktopViewState::from(&state);
                if remote_state_clears_input(&state)
                    && let Some(input) = self.remote_desktop.inputs.get(session_id)
                {
                    input.update(cx, |input, cx| input.clear(cx));
                }
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    if remote_state_clears_input(&state) {
                        session.keys = Default::default();
                        session.modifiers = Default::default();
                    }
                    if state != RemoteDesktopViewState::Connected {
                        session.vnc_key_request = None;
                    }
                    session.vnc_server_capabilities = vnc_server_capabilities;
                    session.state = state;
                }
                if let Some(message) = message {
                    self.shell.set_status(message);
                }
            }
            VncRuntimeEvent::Frame {
                event:
                    RdpFrameEvent::Reset {
                        epoch,
                        width,
                        height,
                    },
                ..
            } => {
                self.reset_rdp_framebuffer(
                    session_id,
                    epoch,
                    width,
                    height,
                    VNC_FRAMEBUFFER_LIMITS,
                    window,
                );
            }
            VncRuntimeEvent::Frame { .. } => {}
            VncRuntimeEvent::Clipboard { text, .. } => {
                if self.session.active_id() == Some(session_id) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            VncRuntimeEvent::Error { error, .. } => {
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.keys = Default::default();
                    session.modifiers = Default::default();
                    session.vnc_server_capabilities = None;
                    session.vnc_key_request = None;
                    set_remote_view_error(session, error.into());
                }
            }
        }
    }

    pub(super) fn report_rdp_metrics(&mut self) {
        if !self.remote_desktop.metrics_enabled {
            return;
        }
        let now = Instant::now();
        if now.saturating_duration_since(self.remote_desktop.metrics_last_report)
            < METRICS_REPORT_INTERVAL
        {
            return;
        }
        tracing::debug!(
            active_sessions = self.remote_desktop.sessions.len(),
            control_events = self.remote_desktop.metrics_control_events,
            frame_updates = self.remote_desktop.metrics_frame_updates,
            "RDP runtime metrics"
        );
        self.remote_desktop.metrics_last_report = now;
        self.remote_desktop.metrics_control_events = 0;
        self.remote_desktop.metrics_frame_updates = 0;
    }

    pub(super) fn apply_rdp_control_event(
        &mut self,
        session_id: &str,
        event: RdpRuntimeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RdpRuntimeEvent::State { state, message, .. } => {
                let server_capabilities = matches!(state, RdpSessionState::Connected)
                    .then(|| self.remote_desktop.manager.server_capabilities(session_id))
                    .flatten();
                let view_state = rdp_view_state(&state);
                if remote_state_clears_input(&view_state)
                    && let Some(input) = self.remote_desktop.inputs.get(session_id)
                {
                    input.update(cx, |input, cx| input.clear(cx));
                }
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    if remote_state_clears_input(&view_state) {
                        session.keys = Default::default();
                        session.modifiers = Default::default();
                    }
                    if should_disable_dynamic_resize_after_state(
                        &state,
                        session.last_resize_sent_at,
                        Instant::now(),
                    ) {
                        session.dynamic_resize_disabled = true;
                        session.pending_resize = None;
                    }
                    if let RdpSessionState::Failed(error) = &state {
                        session.error = Some(error.clone().into());
                    }
                    session.server_capabilities = server_capabilities;
                    session.state = view_state;
                }
                if let Some(message) = message {
                    self.shell.set_status(message);
                }
                if matches!(
                    state,
                    RdpSessionState::Disconnected | RdpSessionState::Failed(_)
                ) {
                    self.stop_rdp_clipboard_jobs(session_id, cx);
                }
            }
            RdpRuntimeEvent::Frame {
                event:
                    RdpFrameEvent::Reset {
                        epoch,
                        width,
                        height,
                    },
                ..
            } => {
                self.reset_rdp_framebuffer(
                    session_id,
                    epoch,
                    width,
                    height,
                    RDP_FRAMEBUFFER_LIMITS,
                    window,
                );
            }
            RdpRuntimeEvent::Frame { .. } => {}
            RdpRuntimeEvent::Cursor { event, .. } => {
                self.apply_remote_cursor_batch(session_id, vec![event], window);
            }
            RdpRuntimeEvent::Clipboard { text, .. } => {
                if self.session.active_id() != Some(session_id) {
                    return;
                }
                let accepted = self
                    .remote_desktop
                    .sessions
                    .get_mut(session_id)
                    .and_then(|session| {
                        session
                            .clipboard
                            .accept(ClipboardOrigin::Remote, &text)
                            .ok()
                            .flatten()
                    })
                    .is_some();
                if accepted {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            RdpRuntimeEvent::ClipboardTransfer { progress, .. } => {
                let status = match progress.status {
                    RdpClipboardTransferStatus::Running => TransferJobStatus::Running,
                    RdpClipboardTransferStatus::Completed => TransferJobStatus::Completed,
                    RdpClipboardTransferStatus::Failed => TransferJobStatus::Failed,
                    RdpClipboardTransferStatus::Cancelled => TransferJobStatus::Cancelled,
                };
                let sample = SftpTransferProgress {
                    remote_path: "RDP clipboard".to_string(),
                    local_path: std::path::PathBuf::new(),
                    bytes_transferred: progress.transferred_bytes,
                    total_bytes: Some(progress.total_bytes),
                    item_count_completed: Some(progress.completed_files),
                    item_count_total: Some(progress.total_files),
                };
                if let Some(job) = self.transfer.transfer_job_mut(&progress.id) {
                    job.status = status;
                    job.detail = progress.error.unwrap_or_default();
                    job.update_progress(sample);
                } else {
                    self.transfer.enqueue_transfer_job(TransferJobState {
                        id: progress.id,
                        session_id: Some(session_id.to_string()),
                        kind: TransferJobKind::RdpClipboard {
                            file_name: progress.name.clone(),
                        },
                        status,
                        detail: progress.error.unwrap_or_default(),
                        created_at_ms: TransferJobState::now_ms(),
                        display_name: progress.name,
                        entries: Vec::new(),
                        summary: None,
                        progress: Some(sample),
                        control: None,
                        speed: Default::default(),
                    });
                }
                self.defer_transfer_panel_snapshot_flush(cx);
            }
            RdpRuntimeEvent::CertificateRequest(request) => {
                self.handle_rdp_certificate_request(session_id, request, cx);
            }
            RdpRuntimeEvent::Capability { capability, .. } => {
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.capability = Some(capability);
                }
                if capability == RdpCapability::DynamicResizeUnavailable {
                    self.shell
                        .set_status("RDP server does not support dynamic resize".to_string());
                }
            }
            RdpRuntimeEvent::Error { error, fatal, .. } => {
                if fatal {
                    self.stop_rdp_clipboard_jobs(session_id, cx);
                }
                let should_reconnect = fatal && self.schedule_rdp_reconnect(session_id, &error);
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.error = Some(error.clone().into());
                    if fatal {
                        session.keys = Default::default();
                        session.modifiers = Default::default();
                        session.server_capabilities = None;
                    }
                    if fatal && !should_reconnect {
                        session.state = RemoteDesktopViewState::Failed;
                    }
                }
                if !should_reconnect {
                    self.shell.set_status(format_rdp_error(&error));
                }
            }
        }
    }
}

pub(super) fn set_rdp_view_error(
    session: &mut crate::features::remote_desktop::state::RemoteDesktopSessionState,
    kind: RdpErrorKind,
    message: String,
) {
    let error = RdpError::new(kind, message);
    session.error = Some(error.into());
    session.state = RemoteDesktopViewState::Failed;
}

pub(super) fn set_remote_view_error(
    session: &mut crate::features::remote_desktop::state::RemoteDesktopSessionState,
    error: RemoteDesktopError,
) {
    session.error = Some(error);
    session.state = RemoteDesktopViewState::Failed;
}

pub(super) fn vnc_capabilities_for_state(
    state: &VncSessionState,
    capabilities: Option<VncServerCapabilities>,
) -> Option<VncServerCapabilities> {
    matches!(state, VncSessionState::Connected)
        .then_some(capabilities)
        .flatten()
}

pub(super) fn remote_state_clears_input(state: &RemoteDesktopViewState) -> bool {
    matches!(
        state,
        RemoteDesktopViewState::Reconnecting
            | RemoteDesktopViewState::Disconnecting
            | RemoteDesktopViewState::Disconnected
            | RemoteDesktopViewState::Failed
    )
}

pub(super) fn rdp_view_state(state: &RdpSessionState) -> RemoteDesktopViewState {
    state.into()
}
/// How long before the next remote-desktop maintenance pass.
///
/// A waiting pointer move gets `POINTER_MOVE_INTERVAL`, because that is the interval
/// its own send is budgeted against and a late flush is a visibly late cursor.
/// Everything else is happy on the coarser maintenance cadence.
pub(super) fn remote_desktop_periodic_delay(pointer_flush_pending: bool) -> Duration {
    if pointer_flush_pending {
        POINTER_MOVE_INTERVAL
    } else {
        MAINTENANCE_INTERVAL
    }
}
