use rust_i18n::t;

use gpui::{Context, FontWeight, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;

use crate::features::NyaTermApp;
use crate::theme::ThemePalette;
use crate::widgets::empty_panel;

use super::{security_auth_body_base, security_tab_toolbar};

impl NyaTermApp {
    pub(super) fn security_passwords_body(
        &mut self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let compact = self.security_list_compact();
        let mut body = security_auth_body_base("security-passwords-body");
        body = body.child(security_tab_toolbar(
            palette,
            t!("passwordManager.title"),
            "security-add-password",
            t!("passwordManager.add"),
            self.security.password_editor().is_none(),
            cx.listener(|this, _, window, cx| {
                this.open_security_password_editor(None, window, cx);
            }),
        ));
        if self.security.passwords().is_empty() {
            body = body.child(empty_panel(
                t!("passwordManager.noPasswords"),
                self.theme_palette(),
            ));
        } else {
            let entries = self.security.passwords().to_vec();
            let entry_count = entries.len();
            let mut rows = div()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .overflow_hidden();
            for (index, entry) in entries.into_iter().enumerate() {
                let id = entry.id.clone();
                let edit_id = entry.id.clone();
                let delete_id = entry.id.clone();
                let reveal_id = entry.id.clone();
                let copy_id = entry.id.clone();
                let revealed_value = self
                    .security
                    .revealed_password(&entry.id)
                    .map(str::to_string);
                let is_revealed = revealed_value.is_some();
                // Tauri: masked until revealed; revealed shows secret + Copy.
                let secret_line = if is_revealed {
                    revealed_value
                        .clone()
                        .filter(|v| !v.is_empty())
                        .unwrap_or_else(|| t!("secretUnlock.emptySecret").to_string())
                } else if entry.has_password {
                    String::new()
                } else {
                    t!("secretUnlock.emptySecret").to_string()
                };
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
                                        .overflow_hidden()
                                        .child(truncate_preview(&entry.name, 28)),
                                )
                                .when(is_revealed, |this| {
                                    this.child(
                                        div()
                                            .flex()
                                            .items_start()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .font_family(
                                                        crate::features::shell::gpui_code_font_family(),
                                                    )
                                                    .text_size(px(11.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(truncate_preview(&secret_line, 36)),
                                            )
                                            .when(
                                                revealed_value
                                                    .as_ref()
                                                    .is_some_and(|v| !v.is_empty()),
                                                |this| {
                                                    this.child(
                                                        nyaterm_ui::NyaIconButton::new(
                                                            format!("security-pw-copy-{id}"),
                                                            "icons/copy.svg",
                                                        )
                                                        .tooltip(t!("common.copyToClipboard"))
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.copy_security_password(
                                                                    copy_id.clone(),
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        )),
                                                    )
                                                },
                                            ),
                                    )
                                }),
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
                                        format!("security-pw-show-{id}"),
                                        if is_revealed {
                                            "icons/eye-off.svg"
                                        } else {
                                            "icons/eye.svg"
                                        },
                                    )
                                    .tooltip(t!(if is_revealed {
                                        "passwordManager.hidePassword"
                                    } else {
                                        "passwordManager.showPassword"
                                    }))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.reveal_security_password(
                                                reveal_id.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-pw-edit-{id}"),
                                        "icons/edit.svg",
                                    )
                                    .tooltip(t!("common.edit"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.open_security_password_editor(
                                                Some(edit_id.clone()),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-pw-del-{id}"),
                                        "icons/delete.svg",
                                    )
                                    .tooltip(t!("common.delete"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.request_delete_security_password(
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
