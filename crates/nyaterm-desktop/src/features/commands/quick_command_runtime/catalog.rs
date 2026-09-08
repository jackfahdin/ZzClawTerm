use rust_i18n::t;

use gpui::{Context, KeyDownEvent, Window};
use nyaterm_core::QuickCommandsConfig;
use nyaterm_store::{StoreDomain, store_request};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::{NavItem, QuickCommandSortMode, QuickCommandViewMode};

use super::helpers::{quick_command_sort_mode_setting, quick_command_view_mode_setting};
use crate::features::settings::SettingsSaveKind;

impl NyaTermApp {
    pub(in crate::features) fn finish_quick_command_reorder(
        &mut self,
        config: Option<QuickCommandsConfig>,
        cx: &mut Context<Self>,
    ) {
        self.commands.clear_quick_drop_target();
        let Some(config) = config else {
            cx.notify();
            return;
        };
        self.commands
            .set_quick_sort_mode(QuickCommandSortMode::Custom);
        self.settings
            .set_quick_command_sort_mode("custom".to_string());
        let settings = self.settings.summary().clone();
        self.shell
            .set_status("saving custom quick command order".to_string());
        self.submit_store_request(
            0,
            store_request(StoreDomain::Commands, move |store| {
                store.save_quick_commands(config)?;
                store.save_quick_command_ui_settings(&settings)?;
                Ok(())
            }),
            |this, event, cx| {
                match event.outcome {
                    Ok(()) => {
                        this.settings
                            .update_store_status("custom quick command order saved", true);
                        this.shell
                            .set_status("custom quick command order saved".to_string());
                    }
                    Err(error) => {
                        this.settings.update_store_status(
                            format!("quick command reorder save failed: {error}"),
                            false,
                        );
                        this.shell
                            .set_status(this.settings.store_status().message.to_string());
                        this.refresh_quick_commands(cx);
                    }
                }
                cx.notify();
            },
            cx,
        );
    }

    /// Moves a category one slot among its siblings from the group context menu.
    ///
    /// Persists through `finish_quick_command_reorder`, the same path category
    /// drag-and-drop uses, so both routes agree on ordering and on switching the
    /// list to the custom sort.
    pub(in crate::features) fn move_quick_command_category(
        &mut self,
        category_id: String,
        up: bool,
        cx: &mut Context<Self>,
    ) {
        let config = self.commands.move_quick_category_by_one(&category_id, up);
        if config.is_none() {
            self.shell.set_status(
                "quick command category is already at the end of its group".to_string(),
            );
            cx.notify();
            return;
        }
        self.finish_quick_command_reorder(config, cx);
    }

    pub(in crate::features) fn close_quick_command_toolbar_popovers(&mut self) {
        self.commands.close_quick_toolbar_popovers();
    }

    pub(in crate::features) fn refresh_quick_commands(&mut self, cx: &mut Context<Self>) {
        self.submit_store_request(
            0,
            store_request(StoreDomain::Commands, |store| store.load_quick_commands()),
            |this, event, cx| {
                match event.outcome {
                    Ok(config) => this
                        .commands
                        .replace_quick_command_catalog(config.commands, config.categories),
                    Err(error) => this.settings.update_store_status(
                        format!("quick command refresh failed: {error}"),
                        false,
                    ),
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(in crate::features) fn set_quick_command_view_mode(
        &mut self,
        mode: QuickCommandViewMode,
        cx: &mut Context<Self>,
    ) {
        self.commands.set_quick_view_mode(mode);
        self.settings
            .set_quick_command_view_mode(quick_command_view_mode_setting(mode).to_string());
        self.save_quick_command_ui_settings(cx);
    }

    pub(in crate::features) fn set_quick_command_sort_mode(
        &mut self,
        mode: QuickCommandSortMode,
        cx: &mut Context<Self>,
    ) {
        self.commands.set_quick_sort_mode(mode);
        self.settings
            .set_quick_command_sort_mode(quick_command_sort_mode_setting(mode).to_string());
        self.save_quick_command_ui_settings(cx);
    }

    pub(in crate::features) fn save_quick_command_ui_settings(&mut self, cx: &mut Context<Self>) {
        self.queue_settings_save(SettingsSaveKind::QuickCommands, cx);
    }

    pub(in crate::features) fn apply_quick_command_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.mark_user_activity();
        self.commands.set_quick_search_draft(text);
        cx.notify();
    }

    pub(in crate::features) fn toggle_quick_command_ai_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.commands.toggle_quick_ai_popover();
        if next {
            let prompt = self.commands.quick_ai_prompt_draft().to_string();
            let input = self.text_input(
                "quick-command.ai-prompt",
                &prompt,
                TextInputSetup::placeholder(t!("ai.placeholder")),
                cx,
            );
            window.focus(&input.read(cx).focus_handle(), cx);
        }
        cx.notify();
    }

    pub(in crate::features) fn apply_quick_command_ai_prompt(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.mark_user_activity();
        self.commands.set_quick_ai_prompt_draft(text);
        cx.notify();
    }

    pub(in crate::features) fn submit_quick_command_ai_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.commands.take_quick_ai_prompt() else {
            self.shell
                .set_status("describe a command to generate".to_string());
            cx.notify();
            return;
        };
        self.reset_text_input("quick-command.ai-prompt", "", cx);
        self.close_quick_command_toolbar_popovers();
        self.set_ai_prompt_draft(format!("Generate a shell command for: {prompt}"), cx);
        self.ai
            .set_chat_response_preview("Quick command generation ready");
        self.ai.set_panel_status("quick command AI assist");
        self.ensure_panel_open(NavItem::AiAssistant);
        window.focus(self.ai.chat_focus(), cx);
        cx.notify();
    }

    pub(in crate::features) fn handle_quick_command_ai_prompt_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "enter" => {
                self.submit_quick_command_ai_prompt(window, cx);
            }
            "escape" => {
                self.commands.close_quick_ai_popover();
                cx.notify();
            }
            _ => return false,
        }
        true
    }
}
