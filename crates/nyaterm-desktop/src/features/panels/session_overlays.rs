use rust_i18n::t;

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _,
    div, prelude::*, px, rgb, rgba,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::NyaNumberInputOptions;

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::widgets::{session_info_row, small_button};

use super::TAB_PRESET_COLORS;

impl NyaTermApp {
    pub(in crate::features) fn rename_session_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rename_draft = self.session.dialog_rename_draft().to_string();
        self.text_input_box(
            "session.rename",
            &rename_draft,
            TextInputSetup::placeholder(t!("tabCtx.renamePlaceholder")),
            cx,
        )
        .into_any_element()
    }

    pub(in crate::features) fn tab_color_picker_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let active_color = self
            .session
            .active_id()
            .and_then(|session_id| self.session.tab_color(session_id));
        let mut swatches = div().mt_3().grid().grid_cols(6).gap_2();
        for (name, color) in TAB_PRESET_COLORS {
            let selected = active_color == Some(color);
            swatches = swatches.child(
                div()
                    .id(SharedString::from(format!("tab-color-{name}")))
                    .size(px(28.))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        rgb(0xffffff)
                    } else {
                        rgb(0x1f2937)
                    })
                    .bg(rgb(color))
                    .cursor_pointer()
                    .hover(|this| this.border_color(rgb(palette.text)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_active_session_tab_color(Some(color), cx);
                    })),
            );
        }

        div()
            .id(SharedString::from("tab-color-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(self.session.dialog_color_picker_focus())
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(this.session.dialog_color_picker_focus(), cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                cx.stop_propagation();
                if event.keystroke.key == "escape" {
                    this.close_tab_color_picker(cx);
                }
            }))
            .child(
                div()
                    .id(SharedString::from("tab-color-dialog"))
                    .w(px(300.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.bg))
                    .shadow_lg()
                    .p_4()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight(800.))
                            .text_color(rgb(palette.text))
                            .child(t!("tabCtx.setColor")),
                    )
                    .child(swatches)
                    .child(
                        div()
                            .mt_3()
                            .text_xs()
                            .text_color(rgb(palette.text_muted))
                            .child(t!("tabCtx.colorHint")),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(small_button(
                                palette,
                                "tab-color-reset",
                                t!("tabCtx.resetColor"),
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_session_tab_color(None, cx);
                                }),
                            ))
                            .child(small_button(
                                palette,
                                "tab-color-cancel",
                                t!("common.cancel"),
                                cx.listener(|this, _, _, cx| {
                                    this.close_tab_color_picker(cx);
                                }),
                            )),
                    ),
            )
    }

    pub(in crate::features) fn session_info_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let details = self.active_session_info_details().unwrap_or_default();
        let title = details
            .iter()
            .find(|(label, _)| *label == t!("sessionInfo.name"))
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| t!("tabCtx.sessionInfo").to_string());
        let mut rows = div().mt_4().flex().flex_col().gap_2();
        if details.is_empty() {
            rows = rows.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.input))
                    .p_3()
                    .text_sm()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("tabCtx.noSessionDetails")),
            );
        } else {
            for (label, value) in details {
                rows = rows.child(session_info_row(palette, label, value));
            }
        }

        div()
            .id(SharedString::from("session-info-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(self.session.dialog_session_info_focus())
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(this.session.dialog_session_info_focus(), cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                cx.stop_propagation();
                if event.keystroke.key == "escape" {
                    this.close_active_session_info(cx);
                }
            }))
            .child(
                div()
                    .id(SharedString::from("session-info-dialog"))
                    .w(px(520.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.bg))
                    .shadow_lg()
                    .p_4()
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(10.))
                                    .mt_1()
                                    .rounded_full()
                                    .bg(rgb(palette.success)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight(800.))
                                            .text_color(rgb(palette.text))
                                            .child(t!("tabCtx.sessionInfo")),
                                    )
                                    .child(
                                        div()
                                            .mt_1()
                                            .text_xs()
                                            .text_color(rgb(palette.text_muted))
                                            .child(truncate_preview(&title, 56)),
                                    ),
                            ),
                    )
                    .child(rows)
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(small_button(
                                palette,
                                "session-info-copy",
                                t!("common.copyToClipboard"),
                                cx.listener(|this, _, _, cx| {
                                    this.copy_active_session_info(cx);
                                }),
                            ))
                            .child(small_button(
                                palette,
                                "session-info-close",
                                t!("common.close"),
                                cx.listener(|this, _, _, cx| {
                                    this.close_active_session_info(cx);
                                }),
                            )),
                    ),
            )
    }

    pub(in crate::features) fn startup_command_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme_palette();
        let command_draft = self.session.dialog_startup_command_draft().to_string();
        let command_input = self
            .text_input_box(
                "session.startup-command",
                &command_draft,
                TextInputSetup::placeholder(t!("tabCtx.commandRequired")),
                cx,
            )
            .into_any_element();
        let delay_ms = self.session.dialog_startup_command_delay_ms();

        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("tabCtx.commandInput")),
            )
            .child(command_input)
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("tabCtx.commandDelay")),
            )
            .child(
                div()
                    .h(px(36.))
                    .px_3()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.input))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div().w(px(180.)).child(
                            self.number_input_box(
                                "session.number.startup-delay",
                                delay_ms.to_string().as_str(),
                                NyaNumberInputOptions::default()
                                    .range(0.0, 60_000.0)
                                    .step(100.0)
                                    .suffix("ms"),
                                cx,
                            ),
                        ),
                    )
                    .child(div().flex().items_center().gap_2().child(small_button(
                        palette,
                        "startup-delay-zero",
                        "0",
                        cx.listener(|this, _, _, cx| {
                            this.session.dialog_reset_startup_command_delay();
                            this.reset_number_input("session.number.startup-delay", "0", cx);
                            cx.notify();
                        }),
                    ))),
            )
            .when(command_draft.trim().is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(palette.danger))
                        .child(t!("tabCtx.commandRequired")),
                )
            })
            .into_any_element()
    }
}
