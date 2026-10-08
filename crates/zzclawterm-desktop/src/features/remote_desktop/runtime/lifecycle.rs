use super::events::set_rdp_view_error;
use super::events::set_remote_view_error;
use crate::features::ZzClawTermApp;
use gpui::AppContext as _;
use gpui::Context;
use gpui::IntoElement as _;
use std::time::Duration;
use std::time::Instant;
use zzclawterm_remote_desktop::RdpError;
use zzclawterm_remote_desktop::RdpErrorKind;
use zzclawterm_remote_desktop::RdpSessionConfig;
use zzclawterm_remote_desktop::RemoteDesktopViewState;
use zzclawterm_remote_desktop::VncError;
use zzclawterm_remote_desktop::VncSessionConfig;
use zzclawterm_store::StoreDomain;
use zzclawterm_store::store_request;

impl ZzClawTermApp {
    pub(in crate::features) fn settle_remote_desktop_restore(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.remote_desktop.restore_pending.clone() else {
            return;
        };
        if self
            .remote_desktop
            .sessions
            .get(&id)
            .is_some_and(|session| {
                matches!(
                    session.state,
                    RemoteDesktopViewState::Connecting | RemoteDesktopViewState::Reconnecting
                )
            })
        {
            return;
        }
        self.remote_desktop.restore_pending = None;
        let Some(window) = self.shell.main_window() else {
            return;
        };
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.pump_startup_restore_queue_if_ready(window, cx);
                });
            });
        });
    }

    pub(super) fn prompt_remote_desktop_password(
        &mut self,
        session_id: &str,
        reset_attempts: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
            session.state = RemoteDesktopViewState::Connecting;
        }
        let Some(window) = self.shell.main_window() else {
            return;
        };
        let app = cx.weak_entity();
        let session_id = session_id.to_string();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    use gpui::{ParentElement as _, Styled as _};
                    use zzclawterm_ui::{
                        ZzClawDialogWindowExt as _, ZzClawInput, ZzClawInputState,
                    };
                    if window.has_active_nya_dialog(cx) {
                        if let Some(session) = app.remote_desktop.sessions.get_mut(&session_id) {
                            session.state = RemoteDesktopViewState::Disconnected;
                        }
                        app.settle_remote_desktop_restore(cx);
                        return;
                    }
                    let input = cx.new(|cx| ZzClawInputState::new(cx, "").masked(true));
                    let render_input = input.clone();
                    let cancel_input = input.clone();
                    let cancel_id = session_id.clone();
                    let focus = input.read(cx).focus_handle();
                    app.open_form_dialog(
                        (
                            rust_i18n::t!("sshAuth.missingPassword").to_string(),
                            400.,
                            rust_i18n::t!("common.confirm").to_string(),
                            move |_, _, _| {
                                gpui::div()
                                    .w_full()
                                    .h(gpui::px(36.))
                                    .child(ZzClawInput::new(&render_input))
                                    .into_any_element()
                            },
                            move |app, _, cx| {
                                let password = input.read(cx).value(cx);
                                if password.is_empty() {
                                    return false;
                                }
                                input.update(cx, |input, cx| input.clear(cx));
                                if let Some(metadata) = app.session.metadata_mut(&session_id) {
                                    match &mut metadata.launch_config {
                                        crate::models::SessionLaunchConfig::Rdp(config) => {
                                            config.password = Some(password.into())
                                        }
                                        crate::models::SessionLaunchConfig::Vnc(config) => {
                                            config.password = Some(password.into())
                                        }
                                        _ => return true,
                                    }
                                } else {
                                    return true;
                                }
                                app.retry_rdp_runtime(&session_id, cx);
                                app.settle_remote_desktop_restore(cx);
                                let _ = reset_attempts;
                                true
                            },
                            move |app, cx| {
                                cancel_input.update(cx, |input, cx| input.clear(cx));
                                if let Some(session) =
                                    app.remote_desktop.sessions.get_mut(&cancel_id)
                                {
                                    session.state = RemoteDesktopViewState::Disconnected;
                                }
                                app.settle_remote_desktop_restore(cx);
                                cx.notify();
                            },
                        ),
                        window,
                        cx,
                    );
                    window.focus(&focus, cx);
                });
            });
        });
    }

    pub(in crate::features) fn create_rdp_runtime(
        &mut self,
        config: RdpSessionConfig,
    ) -> Result<String, RdpError> {
        self.remote_desktop.create_rdp_session(config)
    }

    pub(in crate::features) fn create_failed_rdp_runtime(&mut self, error: RdpError) -> String {
        let session_id = zzclawterm_core::uuid();
        self.remote_desktop
            .insert_failed_session(session_id.clone(), error.kind, error.message);
        session_id
    }

    pub(in crate::features) fn create_vnc_runtime(
        &mut self,
        config: VncSessionConfig,
    ) -> Result<String, VncError> {
        self.remote_desktop.create_vnc_session(config)
    }

    pub(in crate::features) fn create_failed_vnc_runtime(&mut self, error: VncError) -> String {
        let session_id = zzclawterm_core::uuid();
        self.remote_desktop.insert_connecting(session_id.clone());
        if let Some(session) = self.remote_desktop.sessions.get_mut(&session_id) {
            set_remote_view_error(session, error.into());
        }
        session_id
    }

    pub(in crate::features) fn retry_rdp_runtime(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        let route_missing = !self.remote_desktop.routes.contains_key(session_id);
        if route_missing {
            let connection = self
                .session
                .metadata(session_id)
                .and_then(|metadata| metadata.source_connection_id.as_ref())
                .and_then(|id| {
                    self.connection_state
                        .connections()
                        .iter()
                        .find(|connection| &connection.id == id)
                })
                .cloned()
                .filter(|connection| {
                    connection.network.as_ref().is_some_and(|network| {
                        network.proxy_id.is_some() || network.proxy_jump_id.is_some()
                    })
                });
            if let Some(connection) = connection {
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.state = RemoteDesktopViewState::Connecting;
                }
                let session_id = session_id.to_string();
                self.prepare_remote_route(
                    connection,
                    move |app, result, cx| {
                        if !app.session.has_session(&session_id) {
                            app.settle_remote_desktop_restore(cx);
                            return;
                        }
                        match result {
                            Ok(route) => {
                                if let Some(metadata) = app.session.metadata_mut(&session_id) {
                                    match &mut metadata.launch_config {
                                        crate::models::SessionLaunchConfig::Rdp(config) => {
                                            config.relay = Some(route.endpoint.clone())
                                        }
                                        crate::models::SessionLaunchConfig::Vnc(config) => {
                                            config.relay = Some(route.endpoint.clone())
                                        }
                                        _ => {}
                                    }
                                }
                                app.remote_desktop.routes.insert(session_id.clone(), route);
                                app.retry_rdp_runtime(&session_id, cx);
                            }
                            Err(error) => {
                                if let Some(session) =
                                    app.remote_desktop.sessions.get_mut(&session_id)
                                {
                                    session.state = RemoteDesktopViewState::Failed;
                                }
                                app.notify_background_operation(
                                    "remote-route",
                                    zzclawterm_ui::notification::ZzClawNotificationKind::Error,
                                    error,
                                    cx,
                                );
                            }
                        }
                        app.settle_remote_desktop_restore(cx);
                        cx.notify();
                    },
                    cx,
                );
                return;
            }
        }
        if self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        }) {
            self.restart_vnc_runtime(session_id, true, cx);
        } else {
            self.restart_rdp_runtime(session_id, true, cx);
        }
    }

    pub(super) fn restart_rdp_runtime(
        &mut self,
        session_id: &str,
        reset_attempts: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(metadata) = self.session.metadata(session_id).cloned() else {
            return;
        };
        let mut config = match metadata.launch_config {
            crate::models::SessionLaunchConfig::Rdp(config) => config,
            _ => return,
        };
        if config.password.is_none()
            && let Some(connection_id) = metadata.source_connection_id.as_deref()
            && let Some(connection) = self
                .connection_state
                .connections()
                .iter()
                .find(|connection| connection.id == connection_id)
        {
            config.password = inline_remote_desktop_password(connection.auth.as_ref());
            if config.password.is_none()
                && let Some(password_id) = remote_desktop_password_id(connection.auth.as_ref())
            {
                self.request_remote_desktop_restart_password(
                    session_id.to_string(),
                    password_id,
                    reset_attempts,
                    false,
                    cx,
                );
                return;
            }
        }
        if config.password.is_none() {
            self.prompt_remote_desktop_password(session_id, reset_attempts, cx);
            return;
        }
        let reconnect_attempts = if reset_attempts {
            0
        } else {
            self.remote_desktop
                .sessions
                .get(session_id)
                .map_or(0, |session| session.reconnect_attempts)
        };
        let dynamic_resize_disabled = self
            .remote_desktop
            .sessions
            .get(session_id)
            .is_some_and(|session| session.dynamic_resize_disabled);
        let route = self.remote_desktop.routes.remove(session_id);
        let _ = self.close_rdp_runtime(session_id);
        if let Some(route) = route {
            self.remote_desktop
                .routes
                .insert(session_id.to_string(), route);
        }
        match self
            .remote_desktop
            .manager
            .create_session_with_id(session_id.to_string(), config)
        {
            Ok(_) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.reconnect_attempts = reconnect_attempts;
                    session.dynamic_resize_disabled = dynamic_resize_disabled;
                }
                if let Some(metadata) = self.session.metadata_mut(session_id) {
                    metadata.disconnected = false;
                }
                self.shell.set_status("RDP reconnecting".to_string());
            }
            Err(error) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    set_rdp_view_error(session, error.kind, error.message);
                }
            }
        }
    }

    pub(super) fn restart_vnc_runtime(
        &mut self,
        session_id: &str,
        reset_attempts: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(metadata) = self.session.metadata(session_id).cloned() else {
            return;
        };
        let mut config = match metadata.launch_config {
            crate::models::SessionLaunchConfig::Vnc(config) => config,
            _ => return,
        };
        if config.password.is_none()
            && let Some(connection_id) = metadata.source_connection_id.as_deref()
            && let Some(connection) = self
                .connection_state
                .connections()
                .iter()
                .find(|connection| connection.id == connection_id)
        {
            config.password = inline_remote_desktop_password(connection.auth.as_ref());
            if config.password.is_none()
                && let Some(password_id) = remote_desktop_password_id(connection.auth.as_ref())
            {
                self.request_remote_desktop_restart_password(
                    session_id.to_string(),
                    password_id,
                    reset_attempts,
                    true,
                    cx,
                );
                return;
            }
        }
        if config.password.is_none()
            && config.security.mode != zzclawterm_remote_desktop::VncSecurityMode::None
        {
            self.prompt_remote_desktop_password(session_id, reset_attempts, cx);
            return;
        }
        let reconnect_attempts = if reset_attempts {
            0
        } else {
            self.remote_desktop
                .sessions
                .get(session_id)
                .map_or(0, |session| session.reconnect_attempts)
        };
        let route = self.remote_desktop.routes.remove(session_id);
        let _ = self.close_vnc_runtime(session_id);
        if let Some(route) = route {
            self.remote_desktop
                .routes
                .insert(session_id.to_string(), route);
        }
        match self
            .remote_desktop
            .vnc_manager
            .create_session_with_id(session_id.to_string(), config)
        {
            Ok(_) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.reconnect_attempts = reconnect_attempts;
                }
                if let Some(metadata) = self.session.metadata_mut(session_id) {
                    metadata.disconnected = false;
                }
                self.shell.set_status("VNC reconnecting".to_string());
            }
            Err(error) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    set_remote_view_error(session, error.into());
                }
            }
        }
    }

    pub(super) fn request_remote_desktop_restart_password(
        &mut self,
        session_id: String,
        password_id: String,
        reset_attempts: bool,
        vnc: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop.sessions.get_mut(&session_id) {
            session.state = RemoteDesktopViewState::Connecting;
        }
        let response_session_id = session_id.clone();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Security, move |store| {
                store.load_decrypted_password_by_id(&password_id)
            }),
            move |this, event, cx| {
                let password = match event.outcome {
                    Ok(Some(entry)) => entry
                        .password
                        .filter(|password| !password.trim().is_empty()),
                    Ok(None) => None,
                    Err(error) => {
                        this.shell.set_status(format!(
                            "remote desktop reconnect could not load saved password: {error}"
                        ));
                        if let Some(session) =
                            this.remote_desktop.sessions.get_mut(&response_session_id)
                        {
                            session.state = RemoteDesktopViewState::Failed;
                        }
                        this.settle_remote_desktop_restore(cx);
                        cx.notify();
                        return;
                    }
                };
                let Some(password) = password else {
                    this.prompt_remote_desktop_password(&response_session_id, reset_attempts, cx);
                    cx.notify();
                    return;
                };
                let Some(metadata) = this.session.metadata_mut(&response_session_id) else {
                    return;
                };
                match &mut metadata.launch_config {
                    crate::models::SessionLaunchConfig::Rdp(config) if !vnc => {
                        config.password = Some(password);
                    }
                    crate::models::SessionLaunchConfig::Vnc(config) if vnc => {
                        config.password = Some(password);
                    }
                    _ => return,
                }
                if vnc {
                    this.restart_vnc_runtime(&response_session_id, reset_attempts, cx);
                } else {
                    this.restart_rdp_runtime(&response_session_id, reset_attempts, cx);
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(in crate::features) fn close_remote_desktop_runtime(
        &mut self,
        session_id: &str,
    ) -> anyhow::Result<()> {
        match self
            .session
            .metadata(session_id)
            .map(|metadata| &metadata.launch_config)
        {
            Some(crate::models::SessionLaunchConfig::Vnc(_)) => self
                .close_vnc_runtime(session_id)
                .map_err(anyhow::Error::from),
            _ => self
                .close_rdp_runtime(session_id)
                .map_err(anyhow::Error::from),
        }
    }

    pub(in crate::features) fn close_rdp_runtime(
        &mut self,
        session_id: &str,
    ) -> Result<(), RdpError> {
        if self.session.active_id() == Some(session_id) {
            crate::features::remote_desktop::keyboard_capture::set_keyboard_capture(
                self.remote_desktop.manager.clone(),
                self.remote_desktop.vnc_manager.clone(),
                None,
            );
            self.release_remote_keys(session_id);
        }
        self.remote_desktop.remove_session(session_id);
        self.remote_desktop.manager.close_detached(session_id)
    }

    pub(in crate::features) fn close_vnc_runtime(
        &mut self,
        session_id: &str,
    ) -> Result<(), VncError> {
        if self.session.active_id() == Some(session_id) {
            crate::features::remote_desktop::keyboard_capture::set_keyboard_capture(
                self.remote_desktop.manager.clone(),
                self.remote_desktop.vnc_manager.clone(),
                None,
            );
            self.release_remote_keys(session_id);
        }
        self.remote_desktop.remove_session(session_id);
        self.remote_desktop.vnc_manager.close_detached(session_id)
    }

    pub(super) fn schedule_rdp_reconnect(&mut self, session_id: &str, error: &RdpError) -> bool {
        if !rdp_error_is_retryable(error.kind) {
            return false;
        }
        let Some(config) =
            self.session
                .metadata(session_id)
                .and_then(|metadata| match &metadata.launch_config {
                    crate::models::SessionLaunchConfig::Rdp(config) => Some(&config.reconnect),
                    _ => None,
                })
        else {
            return false;
        };
        if !config.enabled {
            return false;
        }
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return false;
        };
        if session.reconnect_attempts >= config.max_attempts {
            return false;
        }
        session.reconnect_attempts += 1;
        let delay = rdp_reconnect_delay(session.reconnect_attempts, rand::random_range(0..250));
        session.reconnect_at = Some(Instant::now() + delay);
        session.state = RemoteDesktopViewState::Reconnecting;
        self.shell.set_status(format!(
            "RDP reconnecting in {:.1}s (attempt {}/{})",
            delay.as_secs_f32(),
            session.reconnect_attempts,
            config.max_attempts
        ));
        true
    }

    pub(super) fn drive_rdp_reconnects(&mut self, cx: &mut Context<Self>) -> bool {
        let now = Instant::now();
        let due = self
            .remote_desktop
            .sessions
            .iter()
            .filter(|(_, session)| session.reconnect_at.is_some_and(|deadline| now >= deadline))
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        for session_id in &due {
            if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                session.reconnect_at = None;
            }
            self.restart_rdp_runtime(session_id, false, cx);
        }
        !due.is_empty()
    }
}

pub(super) fn rdp_error_is_retryable(kind: RdpErrorKind) -> bool {
    matches!(
        kind,
        RdpErrorKind::Timeout
            | RdpErrorKind::ConnectionRefused
            | RdpErrorKind::Tls
            | RdpErrorKind::Transport
            | RdpErrorKind::Session
    )
}

pub(super) fn rdp_reconnect_delay(attempt: u32, jitter_ms: u64) -> Duration {
    const BACKOFF_SECONDS: [u64; 6] = [1, 2, 4, 8, 15, 30];
    let index = attempt.saturating_sub(1) as usize;
    Duration::from_secs(BACKOFF_SECONDS[index.min(BACKOFF_SECONDS.len() - 1)])
        + Duration::from_millis(jitter_ms.min(249))
}

pub(super) fn clear_rdp_reconnect_after_frame(
    session: &mut crate::features::remote_desktop::state::RemoteDesktopSessionState,
) {
    session.reconnect_attempts = 0;
    session.reconnect_at = None;
    session.error = None;
}

pub(super) fn inline_remote_desktop_password(
    auth: Option<&zzclawterm_core::ConnectionAuth>,
) -> Option<zzclawterm_core::SecretString> {
    let auth = auth?;
    if auth.mode == "none" {
        return None;
    }
    if let Some(password) = auth
        .password
        .as_deref()
        .filter(|password| !password.trim().is_empty())
    {
        return (!auth.has_password).then(|| password.to_owned().into());
    }
    None
}

pub(super) fn remote_desktop_password_id(
    auth: Option<&zzclawterm_core::ConnectionAuth>,
) -> Option<String> {
    let auth = auth?;
    if auth.mode == "none"
        || auth
            .password
            .as_deref()
            .is_some_and(|password| !password.trim().is_empty() && !auth.has_password)
    {
        return None;
    }
    auth.password_id
        .as_deref()
        .map(str::trim)
        .filter(|password_id| !password_id.is_empty())
        .map(ToString::to_string)
}
