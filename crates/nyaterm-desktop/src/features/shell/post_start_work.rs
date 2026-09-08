//! Bootstrap work that has to wait for the app to fall calm.
//!
//! Two things want to run once session starts have settled: restoring the terminal
//! window layout, and starting an auto-recording. Both open files or the config
//! database, so both were gated on the app being calm -- no pending start, no output
//! pressure, no window geometry churn, and past the connect-settle hold.
//!
//! That combination is why neither could simply be hung off the session-start drain
//! the way the restore *queue* was in `64e1cc3f`. A start settling is the moment they
//! become *eligible*, but connect settle is still active for
//! `CONNECT_SETTLE_HOLD` afterwards, so a one-shot fired at that moment would find the
//! gate closed, skip, and never come back. Hence a clock that retries until the gate
//! opens, and retires as soon as there is nothing left to do.

use std::time::{Duration, Instant};

use gpui::Context;

use crate::features::NyaTermApp;

/// How long to wait before re-checking a closed gate.
///
/// Sized against `CONNECT_SETTLE_HOLD` (750ms), the longest of the holds: a few
/// retries cover it, and nothing here is latency-sensitive enough to want finer.
const POST_START_RETRY_INTERVAL: Duration = Duration::from_millis(250);

/// Whether deferred bootstrap work may run now.
///
/// The same four conditions the idle plane applied: its outer
/// `runtime_idle_plane_allowed(demote_idle)` gate plus the inner repeat of
/// `!start_has_pending`.
fn post_start_work_is_allowed(
    start_pending: bool,
    output_pressure: bool,
    geometry_busy: bool,
    connect_settle: bool,
) -> bool {
    !start_pending && !output_pressure && !geometry_busy && !connect_settle
}

impl NyaTermApp {
    /// Run deferred bootstrap work once the app is calm enough for it.
    ///
    /// Idempotent. Armed at window open (the layout restore is outstanding from boot)
    /// and from the session-start drain, which is when both become eligible.
    pub(in crate::features) fn ensure_post_start_work_clock(&mut self, cx: &mut Context<Self>) {
        if self.shell.post_start_work_clock_is_armed() || !self.has_post_start_work() {
            return;
        }
        self.shell.set_post_start_work_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(keep_running) = this.update(cx, |this, cx| this.drive_post_start_work(cx))
                else {
                    break;
                };
                if !keep_running {
                    break;
                }
                cx.background_executor()
                    .timer(POST_START_RETRY_INTERVAL)
                    .await;
            }
        })
        .detach();
    }

    pub(in crate::features) fn has_post_start_work(&self) -> bool {
        !self.terminal.terminal_windows_restore_is_complete()
            || self.recording.has_pending_auto_start()
    }

    /// Returns whether the clock should keep running.
    fn drive_post_start_work(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.has_post_start_work() {
            self.shell.set_post_start_work_clock_armed(false);
            return false;
        }
        let now = Instant::now();
        if !post_start_work_is_allowed(
            self.session.start_has_pending(),
            self.runtime_output_pressure_active(),
            self.shell_persistence_geometry_is_busy(now),
            self.connect_settle_is_active(now),
        ) {
            return true;
        }

        // Layout restore opens the config DB.
        if !self.terminal.terminal_windows_restore_is_complete() {
            self.try_restore_terminal_window_layout(cx);
            if self.terminal.terminal_window_tree_is_some() {
                self.reconcile_terminal_windows();
            }
        }
        // Auto-recording opens files.
        if let Some((session_id, session_name)) = self.recording.take_pending_auto_start() {
            self.maybe_auto_start_recording(&session_id, &session_name, cx);
        }

        // `try_restore_terminal_window_layout` may not complete in one pass, so ask
        // again rather than assuming this call finished the job.
        if self.has_post_start_work() {
            return true;
        }
        self.shell.set_post_start_work_clock_armed(false);
        false
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use super::{POST_START_RETRY_INTERVAL, post_start_work_is_allowed};
    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::features::shell::event_pump::CONNECT_SETTLE_HOLD;

    fn app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-post-start-{}-{}",
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

    /// A closed gate must mean "come back", not "give up".
    ///
    /// This is the assertion the two predicate tests below cannot make: they check the
    /// gate, while the decision to retry lives in the caller. Returning `false` here --
    /// the naive one-shot -- is exactly the bug that made these two items unsuitable
    /// for the session-start hook the restore *queue* uses, and it would strand the
    /// window-layout restore for the rest of the session.
    #[test]
    fn a_closed_gate_keeps_the_clock_rather_than_giving_up() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            assert!(
                app.has_post_start_work(),
                "a fresh app has its terminal window layout restore outstanding"
            );
            app.enter_connect_settle();

            app.ensure_post_start_work_clock(cx);
            assert!(app.shell.post_start_work_clock_is_armed());
            assert!(
                app.drive_post_start_work(cx),
                "work is outstanding and the gate is shut, so the clock must be kept"
            );
            assert!(
                app.shell.post_start_work_clock_is_armed(),
                "and it must still be armed for that retry to happen"
            );
        });
    }

    #[test]
    fn deferred_bootstrap_work_waits_for_every_hold() {
        assert!(post_start_work_is_allowed(false, false, false, false));
        assert!(
            !post_start_work_is_allowed(true, false, false, false),
            "a session still connecting must hold the layout restore"
        );
        assert!(
            !post_start_work_is_allowed(false, true, false, false),
            "output pressure must hold it: this opens the config DB"
        );
        assert!(
            !post_start_work_is_allowed(false, false, true, false),
            "a window drag or resize must hold it"
        );
        assert!(
            !post_start_work_is_allowed(false, false, false, true),
            "connect settle must hold it, which is why a one-shot on the start-settled \
             hook would never have run"
        );
    }

    /// The retry has to be short enough that a few passes cover the longest hold, or
    /// the layout restore lands visibly late after a connect.
    #[test]
    fn the_retry_covers_the_connect_settle_hold() {
        assert!(POST_START_RETRY_INTERVAL < CONNECT_SETTLE_HOLD);
    }
}
