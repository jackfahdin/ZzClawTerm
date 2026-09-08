use rust_i18n::t;

use std::borrow::Cow;

use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_ui::NyaSelectOption;

use crate::features::{pages::settings::panel::SettingsPanel, shell::TAB_MOUSE_ACTIONS};
use crate::theme::ThemePalette;

use super::super::{settings_form_row, settings_form_section, settings_switch};

impl SettingsPanel {
    pub(in crate::features) fn interaction_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let encoding = self.settings.summary().interaction_default_encoding.clone();
        // Built before the form, which reads `self` throughout: creating the
        // box needs it mutably.
        let word_separators_input =
            self.existing_text_input_box("settings.interaction.word-separators", false);
        let double_action = self
            .settings
            .summary()
            .interaction_tab_double_click_action
            .clone();
        let middle_action = self
            .settings
            .summary()
            .interaction_tab_middle_click_action
            .clone();
        let right_action = self
            .settings
            .summary()
            .interaction_tab_right_click_action
            .clone();
        let _delay_ms = self
            .settings
            .summary()
            .interaction_duplicate_session_command_delay_ms;
        let _min_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_min_chars;
        let _max_chars = self
            .settings
            .summary()
            .interaction_command_suggestion_max_chars;
        let suggestions_enabled = self
            .settings
            .summary()
            .interaction_command_suggestions_enabled;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(settings_form_section(
                palette,
                Some(t!("settings.interactionClipboardMouse")),
                Some(t!("settings.interactionClipboardMouseDesc")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("settings.copyOnSelect"),
                        Some(SharedString::from(t!("settings.copyOnSelectDesc"))),
                        settings_switch(
                            palette,
                            "interaction-copy-select",
                            self.settings.summary().interaction_copy_on_select,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_interaction_copy_on_select(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.allowOsc52ClipboardWrite"),
                        Some(SharedString::from(t!(
                            "settings.allowOsc52ClipboardWriteDesc"
                        ))),
                        settings_switch(
                            palette,
                            "interaction-osc52-clipboard",
                            self.settings
                                .summary()
                                .interaction_allow_osc52_clipboard_write,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_osc52_clipboard_write(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.rightClickPaste"),
                        Some(SharedString::from(t!("settings.rightClickPasteDesc"))),
                        settings_switch(
                            palette,
                            "interaction-right-paste",
                            self.settings.summary().interaction_right_click_paste,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_interaction_right_click_paste(cx);
                            }),
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("settings.terminalZoomEnabled")),
                Some(t!("settings.terminalZoomEnabledDesc")),
                settings_form_row(
                    palette,
                    t!("settings.terminalZoomEnabled"),
                    Some(SharedString::from(t!("settings.terminalZoomEnabledDesc"))),
                    settings_switch(
                        palette,
                        "interaction-terminal-zoom",
                        self.settings.summary().interaction_terminal_zoom_enabled,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_terminal_zoom_enabled(cx);
                        }),
                    ),
                ),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("settings.interactionCommandInput")),
                Some(t!("settings.interactionCommandInputDesc")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("settings.commandSuggestions"),
                        Some(SharedString::from(t!("settings.commandSuggestionsDesc"))),
                        settings_switch(
                            palette,
                            "interaction-cmd-suggestions",
                            suggestions_enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_command_suggestions(cx);
                            }),
                        ),
                    ))
                    .when(suggestions_enabled, |this| {
                        this.child(settings_form_row(
                            palette,
                            t!("settings.commandSuggestionsMinChars"),
                            Some(SharedString::from(t!(
                                "settings.commandSuggestionsMinCharsDesc"
                            ))),
                            self.existing_number_input_box(
                                "settings.number.command-suggestion-min-chars",
                            ),
                        ))
                        .child(settings_form_row(
                            palette,
                            t!("settings.commandSuggestionsMaxChars"),
                            Some(SharedString::from(t!(
                                "settings.commandSuggestionsMaxCharsDesc"
                            ))),
                            self.existing_number_input_box(
                                "settings.number.command-suggestion-max-chars",
                            ),
                        ))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(settings_field_meta(
                                palette,
                                t!("settings.wordSeparators"),
                                t!("settings.wordSeparatorsDesc"),
                            ))
                            .child(div().w_full().max_w(px(640.)).child(word_separators_input)),
                    )
                    .child(settings_form_row(
                        palette,
                        t!("settings.duplicateSessionCommandDelay"),
                        Some(SharedString::from(t!(
                            "settings.duplicateSessionCommandDelayDesc"
                        ))),
                        self.existing_number_input_box(
                            "settings.number.duplicate-session-command-delay",
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.altAsMeta"),
                        Some(SharedString::from(t!("settings.altAsMetaDesc"))),
                        settings_switch(
                            palette,
                            "interaction-alt-meta",
                            self.settings.summary().interaction_alt_as_meta,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_alt_as_meta(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.macImeCompatibility"),
                        Some(SharedString::from(t!("settings.macImeCompatibilityDesc"))),
                        settings_switch(
                            palette,
                            "interaction-mac-ime",
                            self.settings.summary().interaction_mac_ime_compatibility,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_mac_ime_compatibility(cx);
                            }),
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("settings.tabMouseActions")),
                Some(t!("settings.tabMouseActionsDesc")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.tab_mouse_action_settings_field(
                        TabMouseActionPresentation {
                            label: t!("settings.tabDoubleClickAction"),
                            description: t!("settings.tabDoubleClickActionDesc"),
                            id: "settings.interaction.tab-double",
                            current: &double_action,
                        },
                        cx,
                    ))
                    .child(self.tab_mouse_action_settings_field(
                        TabMouseActionPresentation {
                            label: t!("settings.tabMiddleClickAction"),
                            description: t!("settings.tabMiddleClickActionDesc"),
                            id: "settings.interaction.tab-middle",
                            current: &middle_action,
                        },
                        cx,
                    ))
                    .child(self.tab_mouse_action_settings_field(
                        TabMouseActionPresentation {
                            label: t!("settings.tabRightClickAction"),
                            description: t!("settings.tabRightClickActionDesc"),
                            id: "settings.interaction.tab-right",
                            current: &right_action,
                        },
                        cx,
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("settings.interactionEncoding")),
                Some(t!("settings.interactionEncodingDesc")),
                self.settings_select_field(
                    "settings.interaction.default-encoding",
                    t!("settings.defaultEncoding"),
                    Some(SharedString::from(t!("settings.defaultEncodingDesc"))),
                    (
                        vec![
                            NyaSelectOption::new("UTF-8", "UTF-8"),
                            NyaSelectOption::new("GBK", "GBK"),
                        ],
                        encoding,
                        false,
                    ),
                    cx,
                ),
            ))
    }

    fn tab_mouse_action_settings_field(
        &mut self,
        presentation: TabMouseActionPresentation<'_>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let TabMouseActionPresentation {
            label,
            description,
            id,
            current,
        } = presentation;
        let selected = normalized_tab_mouse_action(current).to_string();
        let options = TAB_MOUSE_ACTIONS
            .iter()
            .map(|action| NyaSelectOption::new(*action, t!(tab_mouse_action_i18n_key(action))))
            .collect();

        self.settings_select_field(
            id,
            label,
            Some(SharedString::from(description)),
            (options, selected, false),
            cx,
        )
    }
}

struct TabMouseActionPresentation<'a> {
    label: Cow<'static, str>,
    description: Cow<'static, str>,
    id: &'static str,
    current: &'a str,
}

fn settings_field_meta(
    palette: ThemePalette,
    label: impl Into<SharedString>,
    desc: impl Into<SharedString>,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let desc: SharedString = desc.into();
    div()
        .min_w_0()
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight(500.))
                .text_color(rgb(palette.text))
                .child(label),
        )
        .child(
            div()
                .mt_1()
                .text_size(px(11.))
                .text_color(rgb(palette.text_dimmed))
                .child(desc),
        )
}

fn normalized_tab_mouse_action(action: &str) -> &'static str {
    TAB_MOUSE_ACTIONS
        .iter()
        .copied()
        .find(|candidate| *candidate == action)
        .unwrap_or("none")
}

fn tab_mouse_action_i18n_key(action: &str) -> &'static str {
    match action {
        "rename_tab" => "tabCtx.rename",
        "copy_tab_name" => "tabCtx.copyName",
        "copy_server_ip" => "tabCtx.copyIp",
        "duplicate_session" => "tabCtx.duplicate",
        "multiplex_ssh" => "tabCtx.multiplexSsh",
        "reconnect_session" => "tabCtx.reconnect",
        "disconnect_session" => "tabCtx.disconnect",
        "close_tab" => "tabCtx.close",
        _ => "settings.tabMouseActionNone",
    }
}
