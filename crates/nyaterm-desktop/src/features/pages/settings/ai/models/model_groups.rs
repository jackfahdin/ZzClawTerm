use rust_i18n::t;

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px,
    rgb,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::NyaScrollable;

use crate::features::pages::settings::panel::SettingsPanel;
use crate::widgets::small_button;

use super::super::super::settings_switch;

impl SettingsPanel {
    pub(super) fn ai_model_groups(
        &mut self,
        palette: crate::theme::ThemePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let credentials: Vec<_> = self
            .ai
            .settings_config()
            .provider_credentials
            .iter()
            .filter(|credential| credential.enabled)
            .cloned()
            .collect();
        let query = self.ai.settings_model_query().trim().to_ascii_lowercase();
        let mut groups: Vec<(
            String,
            String,
            nyaterm_core::AiProviderCredential,
            Vec<nyaterm_core::AiModelConfigItem>,
        )> = credentials
            .iter()
            .map(|credential| {
                (
                    credential.id.clone(),
                    credential.name.clone(),
                    credential.clone(),
                    Vec::new(),
                )
            })
            .collect();

        for model in
            self.ai.settings_config().models.iter().filter(|model| {
                query.is_empty() || model.name.to_ascii_lowercase().contains(&query)
            })
        {
            let group_index = model
                .credential_id
                .as_deref()
                .and_then(|credential_id| {
                    credentials
                        .iter()
                        .position(|credential| credential.id == credential_id)
                })
                .or_else(|| {
                    model.provider_kind.as_ref().and_then(|kind| {
                        credentials
                            .iter()
                            .position(|credential| &credential.provider_kind == kind)
                    })
                });
            if let Some(index) = group_index {
                groups[index].3.push(model.clone());
            }
        }
        for group in &mut groups {
            group
                .3
                .sort_by_key(|model| (!model.enabled, model.name.to_ascii_lowercase()));
        }

        let collapsed = self.ai.settings_model_collapsed_groups().clone();
        let manual_drafts = self.ai.settings_manual_model_drafts().clone();
        let manual_badge = t!("ai.manualModelBadge");
        let custom_provider = t!("ai.customProvider");
        let add_label = t!("common.add");
        let delete_label = t!("common.delete");

        if groups.is_empty() {
            return div()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .px_3()
                .py_8()
                .text_center()
                .text_size(px(11.))
                .text_color(rgb(palette.text_muted))
                .child(t!("ai.noModels"))
                .into_any_element();
        }

        let rows = groups.into_iter().fold(
            div().flex().flex_col(),
            |rows, (group_key, mut label, credential, models)| {
                if label.trim().is_empty() {
                    label = custom_provider.to_string();
                }
                let is_collapsed = collapsed.contains(&group_key);
                let enabled_in_group = models.iter().filter(|model| model.enabled).count();
                let total_in_group = models.len();
                let group_key_toggle = group_key.clone();
                let group_key_add = group_key.clone();
                let credential_id = credential.id.clone();
                let draft = manual_drafts.get(&group_key).cloned().unwrap_or_default();
                let manual_click = cx.listener({
                    let group = group_key.clone();
                    move |this, _, window, cx| {
                        this.focus_ai_manual_model_input(group.clone(), window, cx);
                    }
                });
                let manual_key = cx.listener({
                    let group = group_key.clone();
                    move |this, event: &KeyDownEvent, window, cx| {
                        if this.handle_ai_manual_model_key_down(&group, event, window, cx) {
                            cx.stop_propagation();
                        }
                    }
                });
                let manual_input = self.existing_text_input_box(
                    format!("ai.settings.manual-model.{group_key}"),
                    false,
                );

                rows.child(
                    div()
                        .id(SharedString::from(format!("ai-model-group-{group_key}")))
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "ai-model-group-header-{group_key}"
                                )))
                                .px_3()
                                .py_2()
                                .flex()
                                .items_center()
                                .gap_2()
                                .bg(rgb(palette.input))
                                .cursor_pointer()
                                .hover(|this| this.bg(rgb(palette.hover)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_ai_model_group(group_key_toggle.clone(), cx);
                                }))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(if is_collapsed { ">" } else { "v" }),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight(600.))
                                        .text_color(rgb(palette.text))
                                        .overflow_hidden()
                                        .child(label),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(palette.text_muted))
                                        .child(format!("{enabled_in_group}/{total_in_group}")),
                                ),
                        )
                        .when(!is_collapsed, |this| {
                            let group_for_focus = group_key_add.clone();
                            let group_for_add = group_key_add.clone();
                            this.child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .pl_8()
                                    .border_t_1()
                                    .border_color(rgb(palette.border))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "ai-manual-model-click-{group_for_focus}"
                                            )))
                                            .min_w_0()
                                            .flex_1()
                                            .on_click(manual_click)
                                            .on_key_down(manual_key)
                                            .child(manual_input),
                                    )
                                    .child(
                                        div()
                                            .opacity(if draft.trim().is_empty() {
                                                0.45
                                            } else {
                                                1.0
                                            })
                                            .child(small_button(
                                                palette,
                                                format!("ai-manual-add-{group_for_add}"),
                                                add_label.clone(),
                                                cx.listener({
                                                    let credential_id = credential_id.clone();
                                                    let group = group_for_add.clone();
                                                    move |this, _, _, cx| {
                                                        let name = this
                                                            .ai
                                                            .settings_manual_model_drafts()
                                                            .get(&group)
                                                            .cloned()
                                                            .unwrap_or_default();
                                                        if !name.trim().is_empty() {
                                                            this.add_ai_manual_model(
                                                                credential_id.clone(),
                                                                name,
                                                                cx,
                                                            );
                                                            this.clear_ai_manual_model_draft(
                                                                &group, cx,
                                                            );
                                                        }
                                                    }
                                                }),
                                            )),
                                    ),
                            )
                            .children(models.into_iter().map(|model| {
                                let model_id = model.id.clone();
                                let model_id_delete = model.id.clone();
                                let is_manual = model.source == nyaterm_core::AiModelSource::Manual;
                                div()
                                    .id(SharedString::from(format!("ai-model-row-{}", model.id)))
                                    .px_3()
                                    .py_2()
                                    .pl_8()
                                    .border_t_1()
                                    .border_color(rgb(palette.border))
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(palette.text))
                                                    .child(truncate_preview(&model.name, 48)),
                                            )
                                            .when(is_manual, |this| {
                                                this.child(
                                                    div()
                                                        .rounded_sm()
                                                        .border_1()
                                                        .border_color(rgb(palette.border))
                                                        .px_1()
                                                        .text_size(px(10.))
                                                        .text_color(rgb(palette.text_muted))
                                                        .child(manual_badge.clone()),
                                                )
                                            }),
                                    )
                                    .child(settings_switch(
                                        palette,
                                        format!("ai-model-enabled-{}", model.id),
                                        model.enabled,
                                        cx.listener(move |this, _, _, cx| {
                                            this.toggle_ai_model_enabled(model_id.clone(), cx);
                                        }),
                                    ))
                                    .when(is_manual, |this| {
                                        this.child(small_button(
                                            palette,
                                            format!("ai-model-delete-{}", model.id),
                                            delete_label.clone(),
                                            cx.listener(move |this, _, _, cx| {
                                                this.remove_ai_manual_model(
                                                    model_id_delete.clone(),
                                                    cx,
                                                );
                                            }),
                                        ))
                                    })
                            }))
                        }),
                )
            },
        );

        div()
            .id("ai-model-groups-scroll")
            .max_h(px(352.))
            .overflow_y_scrollbar()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .child(rows)
            .into_any_element()
    }
}
