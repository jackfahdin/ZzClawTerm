use rust_i18n::t;

use gpui::{Context, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb};
use nyaterm_ui::NyaSearchInput;

use crate::features::pages::settings::panel::SettingsPanel;
use crate::theme::ThemePalette;
use crate::widgets::small_button;

use super::super::settings_form_section;

mod credential_rows;
mod model_groups;

impl SettingsPanel {
    pub(in crate::features) fn ai_models_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let _query = self.ai.settings_model_query().to_string();
        let Some(search_field) = self.existing_text_input("ai.settings.model-search") else {
            debug_assert!(false, "the AI model search input was never built");
            return div();
        };
        let has_enabled_custom_credential = self
            .ai
            .settings_config()
            .provider_credentials
            .iter()
            .any(|credential| {
                credential.enabled
                    && credential.provider_kind == nyaterm_core::AiProviderKind::OpenaiCompatible
            });
        let enabled_credentials = self
            .ai
            .settings_config()
            .provider_credentials
            .iter()
            .filter(|credential| credential.enabled)
            .count();
        let has_enabled_model = self
            .ai
            .settings_config()
            .models
            .iter()
            .any(|model| model.enabled);
        let refresh_label = if self.ai.discovery_is_pending() {
            t!("common.loading")
        } else {
            t!("ai.refreshModels")
        };

        let model_groups = self.ai_model_groups(palette, cx);
        let credential_rows = self.ai_credential_rows(palette, cx);

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(settings_form_section(
                palette,
                Some(t!("ai.modelList")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div().min_w_0().flex_1().child(
                                    NyaSearchInput::new("ai-settings-model-search", &search_field)
                                        .on_key_down(cx.listener(
                                            |this, event: &KeyDownEvent, _, cx| {
                                                if event.keystroke.key == "escape" {
                                                    cx.stop_propagation();
                                                    this.clear_ai_model_search(cx);
                                                }
                                            },
                                        )),
                                ),
                            )
                            .child(
                                div()
                                    .opacity(
                                        if has_enabled_custom_credential
                                            && !self.ai.discovery_is_pending()
                                        {
                                            1.0
                                        } else {
                                            0.45
                                        },
                                    )
                                    .child(small_button(
                                        palette,
                                        "ai-models-discover",
                                        refresh_label,
                                        cx.listener(move |this, _, _, cx| {
                                            if has_enabled_custom_credential
                                                && !this.ai.discovery_is_pending()
                                            {
                                                this.discover_ai_models(cx);
                                            }
                                        }),
                                    )),
                            ),
                    )
                    .child(model_groups)
                    .when(enabled_credentials == 0, |this| {
                        this.child(ai_models_hint(
                            palette,
                            t!("ai.manualModelNoProvider"),
                            false,
                        ))
                    })
                    .when(!has_enabled_model, |this| {
                        this.child(ai_models_hint(palette, t!("ai.enableOneModelHint"), true))
                    }),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("ai.apiKeys")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().flex().justify_end().child(small_button(
                        palette,
                        "ai-cred-add",
                        t!("common.add"),
                        cx.listener(|this, _, window, cx| {
                            this.add_ai_credential(window, cx);
                        }),
                    )))
                    .child(credential_rows),
            ))
    }
}

fn ai_models_hint(
    palette: ThemePalette,
    text: impl Into<SharedString>,
    warning: bool,
) -> impl IntoElement {
    let text: SharedString = text.into();
    div()
        .text_size(px(11.))
        .text_color(rgb(if warning {
            palette.warning
        } else {
            palette.text_muted
        }))
        .child(text)
}
