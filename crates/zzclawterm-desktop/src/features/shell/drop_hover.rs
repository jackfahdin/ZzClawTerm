//! Clearing a drop-target highlight when the drag goes away.
//!
//! A drag over the terminal or the transfer browser sets a hover highlight, while a
//! session-tab drag records its source so the source tab can be dimmed. Dropping clears
//! that transient state through the drop handler, but a drag can also end *without* a
//! drop on our element. Esc and platform cancellation clear the state immediately;
//! an ordinary release outside still needs a poll of `has_active_drag`.
//!
//! What changes is the scope. The runtime tick's visual plane checked
//! `has_active_drag` on every tick regardless, and both hover flags had to be named in
//! the tick's due-work predicate so a stale highlight would be noticed at all. Now the
//! poll exists only while a highlight is actually up -- a span measured in the seconds a
//! user spends dragging -- and nothing wakes for it otherwise.

use std::time::Duration;

use gpui::Context;

use crate::features::ZzClawTermApp;

/// How often to look for the drag having ended while a highlight is up.
///
/// A frame: the highlight is a visual affordance, so noticing within one paint is both
/// enough and the finest that could matter.
const DROP_HOVER_POLL_INTERVAL: Duration = Duration::from_millis(16);

impl ZzClawTermApp {
    /// Watch for the drag ending while a drop highlight is showing.
    ///
    /// Idempotent, and armed from the places that set a highlight.
    pub(in crate::features) fn ensure_drop_hover_clock(&mut self, cx: &mut Context<Self>) {
        if self.shell.drop_hover_clock_is_armed() || !self.has_drop_hover() {
            return;
        }
        self.shell.set_drop_hover_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(DROP_HOVER_POLL_INTERVAL)
                    .await;
                let Ok(keep_running) = this.update(cx, |this, cx| this.tick_drop_hover(cx)) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::features) fn has_drop_hover(&self) -> bool {
        self.shell.terminal_file_drop_hover_is_pending()
            || self.transfer.browser_external_drop_hover_is_pending()
            || self.session.tab_drag_is_pending()
            || self.security.drop_target().is_some()
            || self.commands.quick_drop_target().is_some()
            || self.connection_state.list_drop_target().is_some()
    }

    /// Returns whether the clock should keep running.
    fn tick_drop_hover(&mut self, cx: &mut Context<Self>) -> bool {
        if cx.has_active_drag() {
            // Still dragging: the highlight is correct, so keep watching.
            let running = self.has_drop_hover();
            if !running {
                self.shell.set_drop_hover_clock_armed(false);
            }
            return running;
        }
        if let Some(controller) = &self.desktop_controller {
            let _ = controller.update(cx, |controller, _| {
                controller.update_tab_drag(|drag| drag.cancel_without_release(self.workspace_id));
            });
        }
        self.clear_drop_hover(cx);
        let running = self.has_drop_hover();
        if !running {
            self.shell.set_drop_hover_clock_armed(false);
        }
        running
    }

    pub(in crate::features) fn clear_drop_hover(&mut self, cx: &mut Context<Self>) {
        self.shell.finish_tab_strip_motion();
        let mut dirty = self.shell.clear_terminal_file_drop_hover();
        dirty |= self.shell.clear_terminal_window_drop();
        let transfer_dirty = self.transfer.set_browser_external_drop_hover(false);
        dirty |= transfer_dirty;
        dirty |= self.session.clear_tab_drag();
        dirty |= self.security.clear_drop_target();
        dirty |= self.commands.clear_quick_drop_target();
        let connection_dirty = self.connection_state.list_drop_target().is_some();
        if connection_dirty {
            self.connection_state.clear_list_drop_target();
            self.defer_connection_panel_snapshot_flush(cx);
            dirty = true;
        }
        if dirty {
            if transfer_dirty {
                self.defer_transfer_panel_snapshot_flush(cx);
            }
            cx.notify();
        }
    }
}
