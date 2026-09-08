use rust_i18n::t;

use gpui::{Context, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;

use crate::features::NyaTermApp;
use crate::theme::ThemePalette;
use crate::widgets::empty_panel;

use super::{security_auth_body_base, security_tab_toolbar};

impl NyaTermApp {
    pub(super) fn security_keys_body(
        &mut self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let compact = self.security_list_compact();
        let mut body = security_auth_body_base("security-keys-body");
        body = body.child(security_tab_toolbar(
            palette,
            t!("securityAuth.keyManagement"),
            "security-add-key",
            t!("securityAuth.addKey"),
            self.security.key_editor().is_none(),
            cx.listener(|this, _, window, cx| {
                this.open_security_key_editor(None, window, cx);
            }),
        ));
        if self.security.ssh_keys().is_empty() {
            body = body.child(empty_panel(t!("securityAuth.noKeys"), self.theme_palette()));
        } else {
            let entries = self.security.ssh_keys().to_vec();
            let entry_count = entries.len();
            let mut rows = div()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .overflow_hidden();
            for (index, key) in entries.into_iter().enumerate() {
                let key_id = key.id.clone();
                let view_id = key.id.clone();
                let edit_id = key.id.clone();
                let delete_id = key.id.clone();
                rows = rows.child(
                    div()
                        .min_h(px(42.))
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
                            div().min_w_0().flex_1().flex().flex_col().child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(palette.text))
                                    .overflow_hidden()
                                    .child(truncate_preview(&key.name, 28)),
                            ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .when(compact, |this| this.w_full().justify_end().pt_1())
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-key-view-{key_id}"),
                                        "icons/private-key.svg",
                                    )
                                    .tooltip(t!("settings.viewPrivateKey"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.view_security_private_key(
                                                view_id.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-key-edit-{key_id}"),
                                        "icons/edit.svg",
                                    )
                                    .tooltip(t!("common.edit"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.open_security_key_editor(
                                                Some(edit_id.clone()),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-key-del-{key_id}"),
                                        "icons/delete.svg",
                                    )
                                    .tooltip(t!("common.delete"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.request_delete_security_key(
                                                delete_id.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                ),
                        ),
                );
            }
            body = body.child(rows);
        }
        body
    }
}
