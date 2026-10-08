use gpui::{Context, Window};
use rust_i18n::t;

use super::state::TransferDeleteOutcome;
use crate::features::ZzClawTermApp;

impl ZzClawTermApp {
    pub(super) fn finish_transfer_delete_batch(
        &mut self,
        session: Option<&str>,
        outcome: TransferDeleteOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = session {
            self.transfer.invalidate_delete_batch(session, &outcome);
        }
        if self.session.active_id() == session {
            if !outcome.deleted.is_empty() {
                self.transfer.browser.selected_remote_path = None;
                self.transfer.browser.selected_remote_paths.clear();
                if let Some(session) = session {
                    self.transfer.clear_tree_selection(session);
                }
            }
            let current = self.transfer.browser_remote_file_path();
            if let Some(target) = outcome.refresh_target(&current) {
                if let Some(session) = session {
                    self.transfer.take_delete_refresh_pending(session);
                }
                if target == current {
                    self.refresh_transfer_browser(window, cx);
                } else {
                    self.open_transfer_browser_directory_path(target, cx);
                }
            }
            self.request_missing_expanded_tree_listings(cx);
        }
        self.shell.set_status(if outcome.errors.is_empty() {
            format!("Deleted {} item(s)", outcome.deleted.len())
        } else {
            format!(
                "{}: {}",
                t!(
                    "fileExplorer.deleteFailedCount",
                    count = outcome.errors.len()
                ),
                outcome.errors[0]
            )
        });
        cx.notify();
    }
}
