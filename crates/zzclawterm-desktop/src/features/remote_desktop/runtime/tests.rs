use std::time::{Duration, Instant};

use gpui::Modifiers;
use zzclawterm_core::ConnectionAuth;
use zzclawterm_remote_desktop::{
    CursorPosition, RdpError, RdpErrorKind, RdpServerCapabilities, RemoteDesktopViewState,
    VncServerCapabilities, VncSessionState,
};

use super::{
    MAINTENANCE_INTERVAL, POINTER_MOVE_INTERVAL, RESIZE_DEBOUNCE, clear_rdp_reconnect_after_frame,
    defer_rdp_pointer_move, inline_remote_desktop_password, rdp_error_is_retryable,
    rdp_key_modifiers, rdp_reconnect_delay, record_remote_cursor_position_if_sent,
    remote_committed_text_supported, remote_desktop_password_id, remote_desktop_periodic_delay,
    remote_modifier_transitions, remote_state_clears_input, secure_attention_available,
    vnc_capabilities_for_state, vnc_input_allowed, vnc_keysym_for_key,
};
use crate::features::remote_desktop::state::RemoteDesktopSessionState;

#[test]
fn vnc_retry_without_password_opens_and_cancels_dialog_without_leasing_root() {
    use gpui::{AppContext as _, Context, IntoElement, Render, TestAppContext, Window};
    use zzclawterm_core::test_support::TestTempDir;
    use zzclawterm_remote_desktop::VncSessionConfig;
    use zzclawterm_ui::{ZzClawDialogWindowExt as _, zzclaw_root};

    use crate::features::test_support::app_with_visible_local_session;
    use crate::models::SessionLaunchConfig;

    struct EmptyView;
    impl Render for EmptyView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
        }
    }

    let root = TestTempDir::new("zzclawterm-vnc-retry-password-dialog");
    let mut cx = TestAppContext::single();
    let _test_app_cleanup = crate::test_support::TestAppCleanup::new(&cx);
    let app = app_with_visible_local_session(&mut cx, root.path(), "vnc-retry");
    app.update(&mut cx, |app, cx| app.sync_component_theme(cx));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| EmptyView);
        zzclaw_root(view, window, cx)
    });
    cx.update(|window, cx| {
        let handle = window.window_handle().downcast().unwrap();
        app.update(cx, |app, cx| {
            app.shell.remember_main_window(handle);
            app.session.metadata_mut("vnc-retry").unwrap().launch_config =
                SessionLaunchConfig::Vnc(VncSessionConfig {
                    username: String::new(),
                    relay: None,
                    name: "VNC retry".into(),
                    host: "127.0.0.1".into(),
                    port: 5900,
                    password: None,
                    security: Default::default(),
                    display: Default::default(),
                    clipboard: Default::default(),
                    reconnect: Default::default(),
                    shared: true,
                    view_only: false,
                });
            app.remote_desktop.insert_connecting("vnc-retry".into());
            app.retry_rdp_runtime("vnc-retry", cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(window.has_active_nya_dialog(cx));
        assert_eq!(
            app.read(cx).remote_desktop.sessions["vnc-retry"].state,
            RemoteDesktopViewState::Connecting,
        );
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!window.has_active_nya_dialog(cx));
        assert_eq!(
            app.read(cx).remote_desktop.sessions["vnc-retry"].state,
            RemoteDesktopViewState::Disconnected,
        );
    });
}

#[test]
fn reconnect_classification_only_accepts_transient_failures() {
    for kind in [
        RdpErrorKind::Timeout,
        RdpErrorKind::ConnectionRefused,
        RdpErrorKind::Tls,
        RdpErrorKind::Transport,
        RdpErrorKind::Session,
    ] {
        assert!(rdp_error_is_retryable(kind), "{kind:?}");
    }
    for kind in [
        RdpErrorKind::Authentication,
        RdpErrorKind::CertificateRejected,
        RdpErrorKind::Negotiation,
        RdpErrorKind::Clipboard,
        RdpErrorKind::HelperMissing,
        RdpErrorKind::HelperCrashed,
        RdpErrorKind::Ipc,
        RdpErrorKind::Protocol,
        RdpErrorKind::Unsupported,
        RdpErrorKind::ResourceExhausted,
    ] {
        assert!(!rdp_error_is_retryable(kind), "{kind:?}");
    }
}

#[test]
fn reconnect_password_selection_resolves_locked_values_by_id() {
    let auth = ConnectionAuth {
        mode: "password".to_string(),
        password: Some("masked-or-encrypted".to_string().into()),
        password_id: Some("pw-rdp".to_string()),
        has_password: true,
        ..ConnectionAuth::default()
    };

    assert_eq!(inline_remote_desktop_password(Some(&auth)), None);
    assert_eq!(
        remote_desktop_password_id(Some(&auth)).as_deref(),
        Some("pw-rdp")
    );
}

#[test]
fn reconnect_backoff_caps_and_bounds_jitter() {
    let expected = [1, 2, 4, 8, 15, 30, 30];
    for (index, seconds) in expected.into_iter().enumerate() {
        assert_eq!(
            rdp_reconnect_delay(index as u32 + 1, 0),
            Duration::from_secs(seconds)
        );
    }
    assert_eq!(rdp_reconnect_delay(1, 999), Duration::from_millis(1_249));
}

/// A waiting pointer move is the one thing here that needs a fine cadence.
///
/// Its send is budgeted against `POINTER_MOVE_INTERVAL`, so a flush on any coarser
/// schedule is a visibly late cursor -- which is what the runtime tick's 500ms
/// quiet interval was doing, since `runtime_quiet_tick_allowed` has no
/// remote-desktop term. Everything else here debounces or gates itself.
#[test]
fn a_waiting_pointer_move_gets_the_fine_cadence() {
    assert_eq!(
        remote_desktop_periodic_delay(true),
        POINTER_MOVE_INTERVAL,
        "a coalesced pointer move must not wait longer than its own send interval"
    );
    assert_eq!(remote_desktop_periodic_delay(false), MAINTENANCE_INTERVAL);
    assert!(
        MAINTENANCE_INTERVAL < RESIZE_DEBOUNCE,
        "the maintenance cadence has to be finer than the shortest thing it              services, or a resize debounce resolves late"
    );
}

#[test]
fn deferred_rdp_pointer_move_tracks_locally_before_protocol_flush() {
    let mut pending_pointer = Some((3, 4));
    let mut cursor_position = CursorPosition { x: 1, y: 2 };

    defer_rdp_pointer_move(&mut pending_pointer, &mut cursor_position, (9, 7));

    assert_eq!(pending_pointer, Some((9, 7)));
    assert_eq!(cursor_position, CursorPosition { x: 9, y: 7 });
}

#[test]
fn remote_cursor_position_advances_only_after_pointer_send_succeeds() {
    let mut cursor_position = CursorPosition { x: 1, y: 2 };

    assert!(!record_remote_cursor_position_if_sent(
        &mut cursor_position,
        (9, 7),
        false,
    ));
    assert_eq!(cursor_position, CursorPosition { x: 1, y: 2 });

    assert!(record_remote_cursor_position_if_sent(
        &mut cursor_position,
        (9, 7),
        true,
    ));
    assert_eq!(cursor_position, CursorPosition { x: 9, y: 7 });
}

#[test]
fn first_frame_clears_reconnect_attempt_and_error_state() {
    let mut session = RemoteDesktopSessionState {
        reconnect_attempts: 4,
        reconnect_at: Some(Instant::now()),
        error: Some(RdpError::new(RdpErrorKind::Transport, "interrupted").into()),
        ..Default::default()
    };

    clear_rdp_reconnect_after_frame(&mut session);

    assert_eq!(session.reconnect_attempts, 0);
    assert_eq!(session.reconnect_at, None);
    assert_eq!(session.error, None);
}

#[test]
fn committed_text_requires_the_confirmed_protocol_capability() {
    let rdp_supported = RdpServerCapabilities {
        committed_unicode_text: true,
        secure_attention: false,
    };
    let vnc_supported = VncServerCapabilities {
        committed_unicode_keysyms: true,
    };

    assert!(!remote_committed_text_supported(false, None, None));
    assert!(!remote_committed_text_supported(
        false,
        Some(RdpServerCapabilities::default()),
        None,
    ));
    assert!(remote_committed_text_supported(
        false,
        Some(rdp_supported),
        None,
    ));
    assert!(!remote_committed_text_supported(true, None, None));
    assert!(!remote_committed_text_supported(
        true,
        None,
        Some(VncServerCapabilities::default()),
    ));
    assert!(remote_committed_text_supported(
        true,
        None,
        Some(vnc_supported),
    ));
}

#[test]
fn rdp_shifted_punctuation_keeps_the_reported_modifier_state() {
    let reported = Modifiers::default();
    assert!(rdp_key_modifiers("!", reported, true).shift);
    assert!(rdp_key_modifiers("?", reported, true).shift);
    assert!(!rdp_key_modifiers("!", reported, false).shift);
    assert!(!rdp_key_modifiers("a", reported, true).shift);
}

#[test]
fn modifier_transitions_press_in_order_release_in_reverse_and_toggle_capslock() {
    let pressed = remote_modifier_transitions(
        Modifiers::default(),
        Modifiers {
            control: true,
            shift: true,
            platform: true,
            ..Default::default()
        },
        None,
        Some(true),
    );
    assert_eq!(
        pressed
            .iter()
            .map(|transition| (transition.key, transition.pressed))
            .collect::<Vec<_>>(),
        vec![("shift", true), ("control", true), ("platform", true)]
    );

    let released = remote_modifier_transitions(
        Modifiers {
            control: true,
            shift: true,
            platform: true,
            ..Default::default()
        },
        Modifiers::default(),
        Some(true),
        Some(false),
    );
    assert_eq!(
        released
            .iter()
            .map(|transition| (transition.key, transition.pressed))
            .collect::<Vec<_>>(),
        vec![
            ("platform", false),
            ("control", false),
            ("shift", false),
            ("capslock", true),
            ("capslock", false),
        ]
    );
}

#[test]
fn vnc_capability_cache_only_survives_connected_state() {
    let supported = Some(VncServerCapabilities {
        committed_unicode_keysyms: true,
    });

    assert_eq!(
        vnc_capabilities_for_state(&VncSessionState::Connected, supported),
        supported,
    );
    assert_eq!(
        vnc_capabilities_for_state(&VncSessionState::Reconnecting, supported),
        None,
    );
    assert_eq!(
        vnc_capabilities_for_state(&VncSessionState::Disconnected, supported),
        None,
    );
}

#[test]
fn physical_vnc_key_mapping_preserves_shortcut_and_navigation_keys() {
    assert_eq!(vnc_keysym_for_key("c", Some("c")), Some(u32::from('c')));
    assert_eq!(vnc_keysym_for_key("left", None), Some(0xff51));
    assert_eq!(vnc_keysym_for_key("F12", None), Some(0xffc9));
}

#[test]
fn vnc_view_only_is_rejected_before_manager_dispatch() {
    assert!(vnc_input_allowed(false));
    assert!(!vnc_input_allowed(true));
}

#[test]
fn secure_attention_requires_rdp_connected_state_and_confirmed_capability() {
    let supported = Some(RdpServerCapabilities {
        committed_unicode_text: true,
        secure_attention: true,
    });
    assert!(secure_attention_available(
        true,
        &RemoteDesktopViewState::Connected,
        supported,
    ));
    assert!(!secure_attention_available(
        false,
        &RemoteDesktopViewState::Connected,
        supported,
    ));
    assert!(!secure_attention_available(
        true,
        &RemoteDesktopViewState::Connecting,
        supported,
    ));
    assert!(!secure_attention_available(
        true,
        &RemoteDesktopViewState::Connected,
        None,
    ));
    assert!(!secure_attention_available(
        true,
        &RemoteDesktopViewState::Connected,
        Some(RdpServerCapabilities::default()),
    ));
}

#[test]
fn disconnecting_states_clear_local_composition_and_key_suppression() {
    assert!(!remote_state_clears_input(
        &RemoteDesktopViewState::Connected
    ));
    assert!(remote_state_clears_input(
        &RemoteDesktopViewState::Reconnecting
    ));
    assert!(remote_state_clears_input(
        &RemoteDesktopViewState::Disconnected
    ));
    assert!(remote_state_clears_input(&RemoteDesktopViewState::Failed));
}

#[test]
fn texture_failure_isolated_and_inactive_framebuffer_can_rebuild() {
    use crate::features::test_support::app_with_visible_local_session;
    use gpui::TestAppContext;
    use zzclawterm_core::test_support::TestTempDir;
    use zzclawterm_remote_desktop::{
        CursorShape, RDP_FRAMEBUFFER_LIMITS, RemoteDesktopErrorCategory, RemotePoint,
    };

    let root = TestTempDir::new("zzclawterm-remote-texture-failure");
    let mut cx = TestAppContext::single();
    let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
    let app = app_with_visible_local_session(&mut cx, root.path(), "healthy");
    struct EmptyView;
    impl gpui::Render for EmptyView {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::div()
        }
    }
    let (_, cx) = cx.add_window_view(|_, _| EmptyView);
    cx.update(|window, cx| {
        app.update(cx, |app, _| {
            app.remote_desktop.insert_connecting("healthy".into());
            app.remote_desktop.insert_connecting("broken".into());
            app.reset_rdp_framebuffer("healthy", 1, 8, 8, RDP_FRAMEBUFFER_LIMITS, window);
            let texture = app.remote_desktop.sessions["healthy"].texture.unwrap();
            let pixels = app.remote_desktop.sessions["healthy"]
                .framebuffer
                .as_ref()
                .unwrap()
                .pixels()
                .as_ptr();
            app.remote_desktop.release_inactive_textures("local");
            assert!(app.remote_desktop.sessions["healthy"].texture.is_none());
            assert!(app.remote_desktop.sessions["healthy"].texture_dirty);
            assert_eq!(
                app.remote_desktop.sessions["healthy"]
                    .framebuffer
                    .as_ref()
                    .unwrap()
                    .pixels()
                    .as_ptr(),
                pixels
            );
            for old in app.remote_desktop.pending_texture_removals.drain(..) {
                window.remove_dynamic_texture(old);
            }
            app.prepare_remote_desktop_textures("healthy", window);
            assert_ne!(
                app.remote_desktop.sessions["healthy"].texture,
                Some(texture)
            );
            assert!(!app.remote_desktop.sessions["healthy"].texture_dirty);

            let broken = app.remote_desktop.sessions.get_mut("broken").unwrap();
            broken.reconnect_at = Some(Instant::now());
            // Invalid upload data deterministically exercises a platform upload
            // failure without exhausting CPU/GPU resources.
            broken.cursor_shape = Some(CursorShape {
                shape_id: 1,
                width: 1,
                height: 1,
                hotspot: RemotePoint { x: 0, y: 0 },
                pixels: vec![0; 3].into(),
            });
            app.prepare_remote_desktop_textures("broken", window);
            let broken = &app.remote_desktop.sessions["broken"];
            assert!(broken.uploads_stopped);
            assert!(broken.reconnect_at.is_none());
            assert!(broken.framebuffer.is_none());
            assert!(broken.cursor_shape.is_none());
            assert_eq!(broken.state, RemoteDesktopViewState::Failed);
            assert_eq!(
                broken.error.as_ref().unwrap().category,
                RemoteDesktopErrorCategory::ResourceExhausted
            );
            app.reset_rdp_framebuffer("broken", 2, 8, 8, RDP_FRAMEBUFFER_LIMITS, window);
            assert!(app.remote_desktop.sessions["broken"].framebuffer.is_none());
            assert!(app.remote_desktop.sessions["healthy"].texture.is_some());
            assert!(!app.remote_desktop.sessions["healthy"].uploads_stopped);
        });
    });
}
