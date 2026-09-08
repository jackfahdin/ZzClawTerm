use rust_i18n::t;

use gpui::{Context, IntoElement, SharedString, div, prelude::*};
use nyaterm_ui::NyaSelectOption;

use crate::features::pages::settings::panel::SettingsPanel;
use crate::widgets::small_button;

use super::super::{settings_form_row, settings_input_action_control};

impl SettingsPanel {
    pub(in crate::features::pages::settings) fn transfer_editor_settings_rows(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let editor_type = self.settings.summary().transfer_editor_type.clone();
        let default_editor_input = self
            .existing_text_input_box("settings.transfer.default-editor", false)
            .into_any_element();
        let editor_type_label = if editor_type == "internal" {
            t!("settings.editorTypeInternal")
        } else {
            t!("settings.editorTypeExternal")
        };
        let selected_editor_type = if editor_type == "internal" {
            "internal"
        } else {
            "external"
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(settings_form_row(
                palette,
                t!("settings.editorType"),
                Some(SharedString::from(editor_type_label)),
                self.settings_select_control(
                    "settings.transfer.editor-type",
                    vec![
                        NyaSelectOption::new("external", t!("settings.editorTypeExternal")),
                        NyaSelectOption::new("internal", t!("settings.editorTypeInternal")),
                    ],
                    selected_editor_type,
                    false,
                    cx,
                ),
            ))
            .when(editor_type == "external", |this| {
                this.child(settings_form_row(
                    palette,
                    t!("settings.defaultEditor"),
                    Some(SharedString::from(t!("settings.defaultEditorDesc"))),
                    settings_input_action_control(
                        260.,
                        default_editor_input,
                        small_button(
                            palette,
                            "settings-transfer-editor-browse",
                            t!("settings.browse"),
                            cx.listener(|this, _, _, cx| {
                                this.prompt_transfer_default_editor_setting(cx);
                            }),
                        ),
                    ),
                ))
            })
    }
}
