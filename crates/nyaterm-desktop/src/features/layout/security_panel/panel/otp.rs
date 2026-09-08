use rust_i18n::t;

use gpui::{Context, FontWeight, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;

use crate::features::NyaTermApp;
use crate::features::formatting::compact_id;
use crate::theme::ThemePalette;
use crate::widgets::empty_panel;

use super::super::super::view_helpers::format_otp_code_display;
use super::security_auth_body_base;

impl NyaTermApp {
    pub(super) fn security_otp_body(
        &mut self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let mut body = security_auth_body_base("security-otp-body");
        let actions_disabled =
            self.security.otp_editor().is_some() || self.security.otp_qr_importing();
        body = body.child(
            div()
                .flex_none()
                .h(px(28.))
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(palette.text))
                        .child(t!("otpManager.title")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            nyaterm_ui::NyaIconButton::new("security-otp-scan-qr", "icons/qr.svg")
                                .tooltip(if self.security.otp_qr_importing() {
                                    t!("otpManager.scanningQr")
                                } else {
                                    t!("otpManager.scanQr")
                                })
                                .disabled(actions_disabled)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.import_security_otp_from_qr(window, cx);
                                })),
                        )
                        .child(
                            nyaterm_ui::NyaIconButton::new("security-add-otp", "icons/plus.svg")
                                .tooltip(t!("otpManager.add"))
                                .disabled(actions_disabled)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_security_otp_editor(None, window, cx);
                                })),
                        ),
                ),
        );

        if self.security.otp_entries().is_empty() {
            return body.child(empty_panel(
                t!("otpManager.noEntries"),
                self.theme_palette(),
            ));
        }

        let entries = self.security.otp_entries().to_vec();
        let entry_count = entries.len();
        let mut rows = div()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .overflow_hidden();
        for (index, entry) in entries.into_iter().enumerate() {
            let id = entry.id.clone();
            let visible = self.security.otp_code_visible(&entry.id);
            let is_totp = entry.otp_type.eq_ignore_ascii_case("totp");
            let code = self
                .security
                .revealed_otp_code(&entry.id)
                .map(str::to_string)
                .unwrap_or_default();
            let code_display = if code.is_empty() {
                "--- ---".to_string()
            } else {
                format_otp_code_display(&code)
            };
            let period = entry.period.max(1);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            let remaining = period - now % period;
            let progress = (remaining as f32 / period as f32).clamp(0., 1.);
            let issuer = if entry.issuer.trim().is_empty() {
                compact_id(&entry.id)
            } else {
                entry.issuer.clone()
            };
            let toggle_id = entry.id.clone();
            let edit_id = entry.id.clone();
            let send_id = entry.id.clone();
            let delete_id = entry.id.clone();
            let copy_id = entry.id.clone();
            let generate_id = entry.id.clone();
            let can_send = self.security_otp_can_send_to_terminal();
            rows = rows.child(
                div()
                    .when(index + 1 < entry_count, |this| {
                        this.border_b_1().border_color(rgb(palette.border))
                    })
                    .px_3()
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .hover(|this| this.bg(rgb(palette.hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_xs()
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(palette.text))
                                    .overflow_hidden()
                                    .child(truncate_preview(&issuer, 28)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .font_family(crate::features::shell::gpui_code_font_family())
                                    .text_size(px(10.))
                                    .font_weight(FontWeight(700.))
                                    .text_color(rgb(if is_totp {
                                        palette.link
                                    } else {
                                        palette.warning
                                    }))
                                    .child(format!("[{}]", entry.otp_type.to_uppercase())),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .overflow_hidden()
                            .child(truncate_preview(&entry.username, 34)),
                    )
                    .child(
                        div()
                            .grid()
                            .grid_cols(4)
                            .gap_1()
                            .child(
                                nyaterm_ui::NyaIconButton::new(
                                    format!("security-otp-view-{id}"),
                                    if visible {
                                        "icons/eye-off.svg"
                                    } else {
                                        "icons/eye.svg"
                                    },
                                )
                                .tooltip(t!(if visible {
                                    "otpManager.hideCodes"
                                } else {
                                    "otpManager.showCodes"
                                }))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.toggle_security_otp_code_visibility(
                                            toggle_id.clone(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .child(
                                nyaterm_ui::NyaIconButton::new(
                                    format!("security-otp-edit-{id}"),
                                    "icons/edit.svg",
                                )
                                .tooltip(t!("common.edit"))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.open_security_otp_editor(
                                            Some(edit_id.clone()),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .child(
                                nyaterm_ui::NyaIconButton::new(
                                    format!("security-otp-send-{id}"),
                                    "icons/send.svg",
                                )
                                .tooltip(if can_send {
                                    t!("otp.sendToTerminal")
                                } else {
                                    t!("otpManager.noActiveTerminal")
                                })
                                .disabled(!can_send)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.send_security_otp_to_terminal(
                                            send_id.clone(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .child(
                                nyaterm_ui::NyaIconButton::new(
                                    format!("security-otp-del-{id}"),
                                    "icons/delete.svg",
                                )
                                .tooltip(t!("common.delete"))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.request_delete_security_otp(
                                            delete_id.clone(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            ),
                    )
                    .when(visible, |this| {
                        this.child(
                            div()
                                .mt_1()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(palette.border))
                                .bg(rgb(palette.input))
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .font_family(
                                                    crate::features::shell::gpui_code_font_family(),
                                                )
                                                .text_lg()
                                                .font_weight(FontWeight(700.))
                                                .text_color(rgb(palette.text))
                                                .child(code_display),
                                        )
                                        .child(
                                            nyaterm_ui::NyaIconButton::new(
                                                format!("security-otp-copy-{id}"),
                                                "icons/copy.svg",
                                            )
                                            .tooltip(t!("otp.copyCode"))
                                            .disabled(code.is_empty())
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| {
                                                    this.copy_security_otp_code(
                                                        copy_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                            ),
                                        ),
                                )
                                .when(is_totp, |this| {
                                    this.child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(palette.text_muted))
                                            .child(format!("{remaining}s")),
                                    )
                                    .child(
                                        div()
                                            .h(px(4.))
                                            .rounded_full()
                                            .bg(rgb(palette.border))
                                            .child(
                                                div()
                                                    .h_full()
                                                    .w(gpui::relative(progress))
                                                    .rounded_full()
                                                    .bg(rgb(palette.link)),
                                            ),
                                    )
                                })
                                .when(!is_totp, |this| {
                                    this.child(
                                        nyaterm_ui::NyaButton::new(
                                            format!("security-otp-generate-{id}"),
                                            t!("otp.generateCode"),
                                        )
                                        .small()
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.generate_security_otp_code(
                                                    generate_id.clone(),
                                                    window,
                                                    cx,
                                                );
                                            }),
                                        ),
                                    )
                                }),
                        )
                    }),
            );
        }
        body.child(rows)
    }
}
