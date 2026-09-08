use super::browser_filter::transfer_browser_search_text_for_key;
use gpui::{Context, KeyDownEvent, Window};

use crate::features::NyaTermApp;

impl NyaTermApp {
    pub(in crate::features::pages::transfers) fn handle_transfer_browser_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mark_user_activity();
        let keystroke = &event.keystroke;
        let modified_for_location = (keystroke.modifiers.platform || keystroke.modifiers.control)
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift;
        if modified_for_location && keystroke.key.eq_ignore_ascii_case("l") {
            cx.stop_propagation();
            self.begin_transfer_browser_path_edit(window, cx);
            return;
        }

        if !self.transfer.rename_dialog_is_open()
            && let Some(text) = transfer_browser_search_text_for_key(event)
        {
            cx.stop_propagation();
            self.focus_transfer_browser_search(Some(text), window, cx);
            return;
        }

        let modified_for_select_all = (keystroke.modifiers.platform || keystroke.modifiers.control)
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift;

        if modified_for_select_all && keystroke.key.eq_ignore_ascii_case("a") {
            cx.stop_propagation();
            self.select_all_visible_transfer_entries(cx);
            return;
        }

        let unmodified = !keystroke.modifiers.alt
            && !keystroke.modifiers.control
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.shift;

        if unmodified && keystroke.key.eq_ignore_ascii_case("enter") {
            cx.stop_propagation();
            if let Some(entry) = self.selected_transfer_entry() {
                if entry.is_directory() {
                    self.open_transfer_browser_entry_directory(entry, window, cx);
                } else {
                    self.open_transfer_default(entry, window, cx);
                }
            } else {
                self.shell
                    .set_status("select a remote item before opening".to_string());
                cx.notify();
            }
            return;
        }

        if unmodified && keystroke.key.eq_ignore_ascii_case("backspace") {
            cx.stop_propagation();
            self.open_transfer_parent_directory(window, cx);
            return;
        }

        if unmodified && keystroke.key.eq_ignore_ascii_case("f5") {
            cx.stop_propagation();
            self.refresh_transfer_browser(window, cx);
            return;
        }

        if keystroke.key == "delete" && unmodified && !self.selected_transfer_entries().is_empty() {
            cx.stop_propagation();
            self.open_selected_transfer_delete_dialog(window, cx);
        }
    }
}
