//! Large-output protection recovery accounting.
//!
//! A terminal that has been flooded enters degraded rendering and may show a recovery
//! notice; both come back on deadlines (`92eff82a`). Advancing those deadlines used to
//! be the runtime tick's visual plane, which meant recovery resolution tracked the
//! tick cadence rather than the deadlines it was measuring.
//!
//! Like the AI agent loop, this cannot be event-driven: `tick_render_degradation`
//! recovers only after output has been *absent* for a calm window, and absence emits
//! nothing. So it stays a poll, scoped to the sessions that actually have recovery
//! outstanding, and stops as soon as none do.

use std::time::{Duration, Instant};

use gpui::Context;

use crate::features::NyaTermApp;
use crate::features::shell::event_pump::terminal_performance_pressure;

/// How often to advance recovery accounting while any is outstanding.
///
/// Finer than `TERMINAL_RENDER_DEGRADATION_RECOVERY_CALM` (400ms), the shorter of the
/// two deadlines it services, so the calm window is what decides when decorations come
/// back rather than this interval.
const RECOVERY_TICK_INTERVAL: Duration = Duration::from_millis(100);

/// Visible session ids, without blanks or repeats.
fn dedup_non_empty(session_ids: &[&str]) -> Vec<String> {
    let mut ids = Vec::with_capacity(session_ids.len());
    for session_id in session_ids {
        if !session_id.is_empty() && !ids.iter().any(|id| id == session_id) {
            ids.push((*session_id).to_string());
        }
    }
    ids
}

impl NyaTermApp {
    /// Advance recovery deadlines while any visible session has some outstanding.
    ///
    /// Idempotent. Armed from the data-plane drain, because entering degraded mode is
    /// a consequence of output being applied.
    pub(in crate::features) fn ensure_terminal_recovery_clock(&mut self, cx: &mut Context<Self>) {
        if self.shell.terminal_recovery_clock_is_armed()
            || !self.visible_terminal_performance_recovery_due()
        {
            return;
        }
        self.shell.set_terminal_recovery_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RECOVERY_TICK_INTERVAL).await;
                let Ok(keep_running) = this.update(cx, |this, cx| {
                    this.tick_terminal_recovery(cx);
                    let running = this.visible_terminal_performance_recovery_due();
                    if !running {
                        this.shell.set_terminal_recovery_clock_armed(false);
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

    fn tick_terminal_recovery(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let pressure = terminal_performance_pressure(self, now);
        let visible_session_ids = self.visible_terminal_session_ids();
        // Deduplicating: the same session can be visible in two panes, and ticking it
        // twice would double-advance its recovery deadlines.
        let session_ids = dedup_non_empty(&visible_session_ids);
        let surface_paint_sessions = self.terminal.tick_session_performance(
            session_ids.iter().map(String::as_str),
            pressure,
            now,
        );
        for session_id in surface_paint_sessions {
            self.notify_terminal_surface_only(Some(session_id.as_str()), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RECOVERY_TICK_INTERVAL, dedup_non_empty};
    use crate::models::TERMINAL_RENDER_DEGRADATION_RECOVERY_CALM;

    /// Moved here with the function it covers: a session visible in two panes must be
    /// ticked once, or its recovery deadlines advance twice as fast as the clock.
    #[test]
    fn recovery_ticks_each_visible_session_once() {
        assert_eq!(
            dedup_non_empty(&["a", "b", "a", ""]),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(dedup_non_empty(&[]).is_empty());
    }

    /// The clock has to be finer than the deadline it is measuring, or the calm window
    /// is decided by the polling interval instead.
    #[test]
    fn the_clock_is_finer_than_the_calm_window() {
        assert!(RECOVERY_TICK_INTERVAL < TERMINAL_RENDER_DEGRADATION_RECOVERY_CALM);
    }
}
