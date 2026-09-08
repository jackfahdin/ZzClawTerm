//! The caret blink clock.
//!
//! Blink used to be driven from the runtime tick's visual plane, which made its
//! period a function of the tick cadence rather than of the setting. The quiet
//! cadence (500ms) is coarser than the blink interval (530ms), so a tick at *t*
//! toggled and armed *t+530*, the tick at *t+500* was too early, and the next landed
//! at *t+1000* -- a ~1000ms half-period instead of 530ms, precisely when the user is
//! idle at a prompt looking at the caret. `1c3d9e85` bought that back by clamping the
//! tick delay to the blink deadline; this module removes the need for the clamp by
//! giving blink its own timer.

use std::time::{Duration, Instant};

use gpui::Context;

use crate::features::NyaTermApp;

/// Blink half-period. Matches the Tauri build's caret.
pub(in crate::features) const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// Whether the caret should be toggling at all.
///
/// Two independent sources ask for blink: the user's setting, and the terminal
/// itself via DECSCUSR. Both paint paths already OR them together, so the clock has
/// to as well.
fn cursor_blink_clock_should_run(
    has_visible_session: bool,
    setting_enabled: bool,
    terminal_requested: bool,
) -> bool {
    has_visible_session && (setting_enabled || terminal_requested)
}

/// Whether this tick may advance the phase, as opposed to holding it.
///
/// The visual plane held the phase under output pressure (`runtime_cursor_blink_allowed`)
/// and skipped blink notifies during connect settle, so first frames after a connect
/// were not competing with caret repaints. Holding rather than stopping is the point:
/// the caret keeps its current state instead of flickering mid-flood.
fn cursor_blink_phase_may_advance(output_pressure: bool, connect_settle: bool) -> bool {
    !output_pressure && !connect_settle
}

impl NyaTermApp {
    /// Start the blink clock if it should be running and is not already.
    ///
    /// Idempotent and cheap, so it can be called from anywhere a blink input might
    /// have changed. The clock stops itself once nothing wants a blinking caret, so
    /// an app with no visible terminal costs nothing.
    pub(in crate::features) fn ensure_cursor_blink_clock(&mut self, cx: &mut Context<Self>) {
        if self.shell.cursor_blink_clock_is_armed() || !self.cursor_blink_should_run() {
            return;
        }
        self.shell.set_cursor_blink_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CURSOR_BLINK_INTERVAL).await;
                let Ok(keep_running) = this.update(cx, |this, cx| this.tick_cursor_blink(cx))
                else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    fn cursor_blink_should_run(&self) -> bool {
        let visible = self.visible_terminal_session_ids();
        cursor_blink_clock_should_run(
            !visible.is_empty(),
            self.settings.summary().cursor_blink,
            self.terminal
                .visible_cursor_blink_requested(visible.iter().copied()),
        )
    }

    /// One blink half-period. Returns whether the clock should keep running.
    fn tick_cursor_blink(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.cursor_blink_should_run() {
            self.shell.set_cursor_blink_clock_armed(false);
            // Leave the caret visible; a stopped clock must not strand it hidden.
            if self.shell.set_cursor_blink_on(true) {
                self.notify_active_terminal_surface(cx);
            }
            return false;
        }
        let now = Instant::now();
        if !cursor_blink_phase_may_advance(
            self.runtime_output_pressure_active(),
            self.connect_settle_is_active(now),
        ) {
            return true;
        }
        self.shell.toggle_cursor_blink_phase();
        // Blink is terminal-local; this must never rebuild the full shell.
        self.notify_active_terminal_surface(cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{AppContext as _, TestAppContext};

    use crate::features::test_support::app_with_visible_local_session;
    use crate::test_support::TestConfigDir;

    use super::{
        CURSOR_BLINK_INTERVAL, cursor_blink_clock_should_run, cursor_blink_phase_may_advance,
    };

    const SESSION_ID: &str = "cursor-blink-session";

    fn app_with_blinking_caret(
        cx: &mut TestAppContext,
        root: &TestConfigDir,
    ) -> gpui::Entity<crate::features::NyaTermApp> {
        app_with_visible_local_session(cx, root.path(), SESSION_ID)
    }

    /// The bug Phase 0 could only paper over: the caret's half-period must be the
    /// blink interval, not whatever the tick cadence happened to be.
    ///
    /// The clock is the deadline now, so advancing just short of the interval must
    /// not toggle and advancing onto it must. A driver that toggles on some other
    /// schedule -- a 500ms tick, say -- fails one of these two.
    #[test]
    fn the_caret_toggles_exactly_one_interval_apart() {
        let root = TestConfigDir::new("nyaterm-cursor-blink");
        let mut cx = TestAppContext::single();
        let app = app_with_blinking_caret(&mut cx, &root);
        cx.update_entity(&app, |app, cx| {
            app.ensure_cursor_blink_clock(cx);
            assert!(app.shell.cursor_blink_on(), "the caret starts visible");
        });

        cx.executor()
            .advance_clock(CURSOR_BLINK_INTERVAL - Duration::from_millis(1));
        cx.run_until_parked();
        cx.update_entity(&app, |app, _| {
            assert!(
                app.shell.cursor_blink_on(),
                "toggling early would make the caret faster than its setting"
            );
        });

        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        cx.update_entity(&app, |app, _| {
            assert!(!app.shell.cursor_blink_on(), "the caret should be off now");
        });

        cx.executor().advance_clock(CURSOR_BLINK_INTERVAL);
        cx.run_until_parked();
        cx.update_entity(&app, |app, _| {
            assert!(
                app.shell.cursor_blink_on(),
                "and back on one interval later, without drifting"
            );
        });
    }

    /// The clock keeps running on its own; nothing else has to drive it.
    ///
    /// There is no runtime tick in this fixture, so several periods of blinking here
    /// can only be the clock re-arming itself.
    #[test]
    fn the_clock_keeps_its_own_cadence_with_no_runtime_tick() {
        let root = TestConfigDir::new("nyaterm-cursor-blink");
        let mut cx = TestAppContext::single();
        let app = app_with_blinking_caret(&mut cx, &root);
        cx.update_entity(&app, |app, cx| app.ensure_cursor_blink_clock(cx));

        for expected_on in [false, true, false, true] {
            cx.executor().advance_clock(CURSOR_BLINK_INTERVAL);
            cx.run_until_parked();
            cx.update_entity(&app, |app, _| {
                assert_eq!(app.shell.cursor_blink_on(), expected_on);
            });
        }
    }

    /// Turning the setting off leaves the caret visible rather than stranded hidden.
    #[test]
    fn the_clock_stops_and_leaves_the_caret_visible() {
        let root = TestConfigDir::new("nyaterm-cursor-blink");
        let mut cx = TestAppContext::single();
        let app = app_with_blinking_caret(&mut cx, &root);
        cx.update_entity(&app, |app, cx| app.ensure_cursor_blink_clock(cx));

        // Toggle into the hidden half of the cycle first, so a stop that forgot to
        // restore the phase would leave no caret at all.
        cx.executor().advance_clock(CURSOR_BLINK_INTERVAL);
        cx.run_until_parked();
        cx.update_entity(&app, |app, _| {
            assert!(!app.shell.cursor_blink_on());
            let mut summary = app.settings.summary().clone();
            summary.cursor_blink = false;
            app.settings.replace_summary(summary);
        });

        cx.executor().advance_clock(CURSOR_BLINK_INTERVAL);
        cx.run_until_parked();
        cx.update_entity(&app, |app, _| {
            assert!(
                app.shell.cursor_blink_on(),
                "a stopped clock must leave the caret painted, not hidden"
            );
            assert!(
                !app.shell.cursor_blink_clock_is_armed(),
                "and it must actually stop rather than spin at 530ms forever"
            );
        });
    }

    #[test]
    fn resetting_cursor_blink_phase_makes_hidden_caret_visible() {
        let root = TestConfigDir::new("nyaterm-cursor-blink");
        let mut cx = TestAppContext::single();
        let app = app_with_blinking_caret(&mut cx, &root);

        cx.update_entity(&app, |app, _| {
            app.shell.set_cursor_blink_on(false);
            assert!(app.shell.reset_cursor_blink_phase());
            assert!(app.shell.cursor_blink_on());
            assert!(!app.shell.reset_cursor_blink_phase());
        });
    }

    #[test]
    fn the_clock_runs_for_either_blink_source() {
        assert!(cursor_blink_clock_should_run(true, true, false));
        assert!(
            cursor_blink_clock_should_run(true, false, true),
            "a terminal asking for a blinking caret must run the clock even with the \
             setting off -- both paint paths honour the request, so a solid caret here \
             is the bug this replaces"
        );
        assert!(!cursor_blink_clock_should_run(true, false, false));
    }

    #[test]
    fn the_clock_stops_when_no_terminal_is_visible() {
        assert!(
            !cursor_blink_clock_should_run(false, true, true),
            "nothing is painting a caret, so nothing should be waking to toggle one"
        );
    }

    #[test]
    fn the_phase_is_held_rather_than_advanced_while_busy() {
        assert!(cursor_blink_phase_may_advance(false, false));
        assert!(
            !cursor_blink_phase_may_advance(true, false),
            "output pressure must hold the phase, as the visual plane did"
        );
        assert!(
            !cursor_blink_phase_may_advance(false, true),
            "connect settle must hold the phase so first frames are not competing"
        );
    }
}
