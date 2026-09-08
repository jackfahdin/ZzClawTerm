use std::path::Path;

use gpui::AppContext as _;
use nyaterm_core::{AiExecutionProfile, AppRuntime, RuntimeMode};
use nyaterm_transport::LocalSessionConfig;

use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
use crate::models::{SessionLaunchConfig, SessionRuntimeMetadata};

use super::NyaTermApp;

/// Build a visible local terminal session for GPUI tests.
pub(in crate::features) fn app_with_visible_local_session(
    cx: &mut gpui::TestAppContext,
    root: &Path,
    session_id: &str,
) -> gpui::Entity<NyaTermApp> {
    let runtime = AppRuntime::from_parts_for_test(
        RuntimeMode::Portable,
        root.to_path_buf(),
        root.join("config"),
        root.join("logs"),
        root.join("cache"),
        None,
    );
    let stores = UiStoreHandles {
        startup_restore: cx.new(|_| StartupRestoreStore::default()),
        overlays: cx.new(|_| OverlayStore::default()),
    };
    let app = cx.new(|cx| NyaTermApp::new(runtime, stores, cx));
    cx.update_entity(&app, |app, _| {
        let mut summary = app.settings.summary().clone();
        summary.cursor_blink = true;
        app.settings.replace_summary(summary);
        app.session.register_session_metadata(
            session_id,
            SessionRuntimeMetadata {
                ssh_config: None,
                ssh_multiplex_key: None,
                source_connection_id: None,
                ai_execution_profile: AiExecutionProfile::Posix,
                launch_config: SessionLaunchConfig::Local(LocalSessionConfig {
                    name: "Local session".to_string(),
                    ..LocalSessionConfig::default()
                }),
                disconnected: false,
            },
        );
        app.session.select_active_session(session_id);
        app.terminal
            .seed_session_view(session_id.to_string(), String::new(), "UTF-8");
        app.shell.show_workspace();
        assert!(
            !app.visible_terminal_session_ids().is_empty(),
            "the fixture must have a visible session"
        );
    });
    app
}
