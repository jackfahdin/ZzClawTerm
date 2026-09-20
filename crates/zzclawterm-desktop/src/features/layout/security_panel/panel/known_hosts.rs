use rust_i18n::t;

use gpui::{Context, FontWeight, div, prelude::*, px, rgb};
use zzclawterm_core::truncate_preview;

use crate::features::ZzClawTermApp;
use crate::theme::ThemePalette;
use crate::widgets::empty_panel;

use super::security_auth_body_base;

impl ZzClawTermApp {
    pub(super) fn security_known_hosts_body(
        &mut self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let compact = self.security_list_compact();
        let loading = self.security.known_hosts_loading();
        let busy = self.security.known_hosts_busy();
        let entries = self.security.known_hosts().to_vec();
        let mut body = security_auth_body_base("security-known-hosts-body").child(
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
                        .child(t!("securityAuth.knownHostsTitle").to_string()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            zzclawterm_ui::ZzClawIconButton::new(
                                "security-known-hosts-refresh",
                                "icons/fe/refresh.svg",
                            )
                            .tooltip(t!("common.refresh"))
                            .disabled(loading || busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_security_known_hosts(cx);
                            })),
                        )
                        .child(
                            zzclawterm_ui::ZzClawIconButton::new(
                                "security-known-hosts-clear",
                                "icons/delete.svg",
                            )
                            .tooltip(t!("securityAuth.clearKnownHosts"))
                            .disabled(loading || busy)
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.request_clear_security_known_hosts(window, cx);
                                },
                            )),
                        ),
                ),
        );

        if entries.is_empty() {
            body = body.child(empty_panel(
                if loading {
                    t!("common.loading")
                } else {
                    t!("securityAuth.noKnownHosts")
                },
                self.theme_palette(),
            ));
            return body;
        }

        let entry_count = entries.len();
        let mut rows = div()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .overflow_hidden();
        for (index, entry) in entries.into_iter().enumerate() {
            let id = entry.id.clone();
            let delete_id = entry.id.clone();
            let label = entry.host_identifier.clone();
            let patterns = if entry.host_patterns.is_empty() {
                entry.host_identifier.clone()
            } else {
                entry.host_patterns.join(", ")
            };
            let fingerprint = entry.fingerprint;
            let marker = entry.marker.unwrap_or_default();
            rows = rows.child(
                div()
                    .min_h(px(54.))
                    .when(index + 1 < entry_count, |this| {
                        this.border_b_1().border_color(rgb(palette.border))
                    })
                    .px_3()
                    .py_2()
                    .flex()
                    .when(compact, |this| this.flex_col().items_stretch())
                    .when(!compact, |this| this.items_center())
                    .gap_2()
                    .hover(|this| this.bg(rgb(palette.hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(palette.text))
                                    .child(truncate_preview(&patterns, 48)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(truncate_preview(
                                        &format!("{} {}", marker, entry.key_type),
                                        48,
                                    )),
                            )
                            .child(
                                div()
                                    .font_family(crate::features::shell::gpui_code_font_family())
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(truncate_preview(&fingerprint, 56)),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .when(compact, |this| this.w_full().justify_end().pt_1())
                            .child(
                                zzclawterm_ui::ZzClawIconButton::new(
                                    format!("security-known-host-delete-{id}"),
                                    "icons/delete.svg",
                                )
                                .tooltip(t!("common.delete"))
                                .disabled(loading || busy)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.request_delete_security_known_host(
                                            delete_id.clone(),
                                            label.clone(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            ),
                    ),
            );
        }
        body.child(rows)
    }
}
