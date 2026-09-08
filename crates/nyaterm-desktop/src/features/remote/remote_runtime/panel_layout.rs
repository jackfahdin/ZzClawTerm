//! Keeping the remote panes' derived state in step with things they cannot observe.
//!
//! `RemoteOpsFeatureState` maintains its own invariant: every mutator that changes the
//! data, the query, the sort or the tab recomputes the derived list and re-clamps the
//! stored scroll offsets. That is why the render pass no longer needs `&mut` access.
//!
//! One input is outside the state's reach. The process table hides its memory and user
//! columns at narrow widths, and the sort key has to be constrained to the columns that
//! are actually shown -- but the panel width lives in `ShellFeatureState`. So the app
//! pushes it in, from the three places it can change.

use gpui::Context;

use crate::features::NyaTermApp;
use crate::features::remote::ProcessSortColumns;

use crate::features::pages::remote::{ProcessDisplayMode, process_display_mode};

impl NyaTermApp {
    /// Re-sync the Remote panels' GPUI-facing state after the active session changed.
    ///
    /// Called from `activate_session_id`, immediately after
    /// `reset_remote_runtime_for_session_switch`, so the reset and everything that has to
    /// follow it land in one outer GPUI update transaction rather than waiting for a
    /// paint.
    ///
    /// **Deliberately narrow.** This is for Remote-panel state that must follow the
    /// active session, not a general activation side-effect bucket. Today that is refresh
    /// demand: a panel's clock is armed on `(visible || the header wants it) && enabled in
    /// settings && a session with an SSH config`, and the last term just changed --
    /// switching to a non-SSH session has to drop the clocks. The snapshot flush joins
    /// this helper, which keeps every activation caller from switching sessions while
    /// omitting either.
    pub(in crate::features) fn sync_remote_panels_after_activation(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.sync_remote_panel_demand(cx);
        self.defer_remote_panel_snapshot_flush(cx);
    }

    /// Tell the process pane which sort columns the current panel width can show.
    ///
    /// Returns whether anything changed, so a caller mid-drag can skip a repaint.
    /// Idempotent, and cheap: the pane compares before recomputing.
    ///
    /// Called from every width change rather than from `render`, which is what used to
    /// do it: a resize is an event, and doing it here is what lets the render pass be
    /// read-only. Mid-drag included, so a column disappearing still moves the sort key
    /// off it immediately rather than at the end of the gesture.
    pub(in crate::features) fn reconcile_remote_process_sort_columns(&mut self) -> bool {
        let mode = process_display_mode(self.shell.right_panel_width());
        self.remote_ops
            .set_process_sort_columns(ProcessSortColumns {
                allow_memory: !matches!(
                    mode,
                    ProcessDisplayMode::Compact | ProcessDisplayMode::Narrow
                ),
                allow_user: mode == ProcessDisplayMode::Wide,
            })
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};
    use nyaterm_transport::RemoteProcess;

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::features::pages::remote::RemoteMonitorKind;
    use crate::models::{NavItem, RemoteProcessSortKey};

    fn app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-panel-layout-{}-{}",
            std::process::id(),
            uuid()
        ));
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.clone(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    fn register_session(app: &mut NyaTermApp, session_id: &str, ssh: bool) {
        app.session.register_session_metadata(
            session_id,
            crate::models::SessionRuntimeMetadata {
                ssh_config: ssh.then(nyaterm_transport::SshSessionConfig::default),
                ssh_multiplex_key: None,
                source_connection_id: None,
                ai_execution_profile: nyaterm_core::AiExecutionProfile::Posix,
                launch_config: crate::models::SessionLaunchConfig::Local(
                    nyaterm_transport::LocalSessionConfig::default(),
                ),
                disconnected: false,
            },
        );
    }

    fn process(pid: u32, cpu_percent: f64, memory_percent: f64) -> RemoteProcess {
        RemoteProcess {
            pid,
            ppid: 1,
            user: "user".to_string(),
            state: "S".to_string(),
            cpu_percent,
            memory_percent,
            rss_kb: 3,
            vsz_kb: 4,
            elapsed: "00:01".to_string(),
            command: "sleep".to_string(),
            command_line: "sleep 10".to_string(),
        }
    }

    /// Resizing the panel narrow must constrain the sort key, with nothing painting.
    ///
    /// The width -> columns mapping is the one input the pane cannot observe, so this
    /// drives it the way the app does: set the width, call the reconcile, read the
    /// result. `render` used to do this, which is why a test could not tell whether the
    /// constraint was a property of the state or a side effect of drawing.
    #[test]
    fn narrowing_the_right_panel_constrains_the_sort_key_with_no_render() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, _| {
            app.remote_ops
                .apply_processes(vec![process(1, 9.0, 1.0), process(2, 1.0, 9.0)]);
            app.remote_ops
                .toggle_process_sort(RemoteProcessSortKey::Memory);

            // Wide enough for every column.
            app.shell.set_right_panel_width_for_test(700.);
            assert!(!app.reconcile_remote_process_sort_columns());
            assert_eq!(
                app.remote_ops.process_presentation().sort_key,
                RemoteProcessSortKey::Memory,
                "a wide panel keeps the memory sort"
            );
            assert_eq!(app.remote_ops.derived_processes()[0].pid, 2);

            // Below the 430px threshold the memory column is gone.
            app.shell.set_right_panel_width_for_test(360.);
            assert!(
                app.reconcile_remote_process_sort_columns(),
                "the columns changed, so the caller is told to repaint"
            );
            assert_eq!(
                app.remote_ops.process_presentation().sort_key,
                RemoteProcessSortKey::Cpu
            );
            assert_eq!(
                app.remote_ops.derived_processes()[0].pid,
                1,
                "and the rows follow the new key"
            );
        });
    }

    /// A session switch must reconcile Remote-panel refresh demand inside the same
    /// update, with no paint.
    ///
    /// Demand is `(visible || the header wants it) && enabled && a session with an SSH
    /// config`. Before `activate_session_id` took `cx`, only `render` reconciled it, so a
    /// switch to a non-SSH session left the panel clock running until something painted.
    ///
    /// **There is no `window.draw` in this test, and that is the assertion.** Every check
    /// runs inside one `update_entity`, which is the same transaction the real switch
    /// happens in.
    #[test]
    fn a_session_switch_reconciles_remote_panel_demand_without_a_paint() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            let mut summary = app.settings.summary().clone();
            summary.ui_show_gpu_monitor = true;
            app.settings.replace_summary(summary);
            app.open_or_toggle_panel(NavItem::GpuMonitor, cx);

            register_session(app, "ssh-a", true);
            register_session(app, "local-b", false);

            let gpu_polling = |app: &NyaTermApp, cx: &gpui::App| {
                app.remote_panels
                    .entity(RemoteMonitorKind::Gpu)
                    .read(cx)
                    .is_polling()
            };

            assert!(
                !gpu_polling(app, cx),
                "no session is active yet, so nothing polls"
            );

            app.activate_session_id("ssh-a", cx);
            assert!(
                gpu_polling(app, cx),
                "activating an SSH session with the GPU panel open must arm its clock \
                 here, not at the next paint"
            );

            app.activate_session_id("local-b", cx);
            assert!(
                !gpu_polling(app, cx),
                "switching to a session with no SSH config must drop the clock in the \
                 same update"
            );
        });
    }
}
