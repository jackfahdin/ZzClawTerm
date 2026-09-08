use rust_i18n::t;

use gpui::{
    AppContext as _, Context, FontWeight, IntoElement, Point, Render, Window, div, prelude::*, px,
    rgb,
};
use nyaterm_core::truncate_preview;

use crate::features::NyaTermApp;
use crate::theme::ThemePalette;
use crate::widgets::empty_panel;

use super::{security_auth_body_base, security_tab_toolbar};

#[derive(Clone)]
struct SecurityCredentialDragPayload {
    id: String,
    label: String,
}

struct SecurityCredentialDragPreview {
    label: String,
    position: Point<gpui::Pixels>,
}

impl Render for SecurityCredentialDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(self.position.x)
            .top(self.position.y)
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(0x202938))
            .text_xs()
            .text_color(rgb(0xf1f5f9))
            .child(self.label.clone())
    }
}

impl NyaTermApp {
    pub(super) fn security_credentials_body(
        &mut self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let compact = self.security_list_compact();
        let mut body = security_auth_body_base("security-credentials-body");
        body = body.child(security_tab_toolbar(
            palette,
            t!("credentialManager.title"),
            "security-add-credential",
            t!("credentialManager.add"),
            self.security.credential_editor().is_none(),
            cx.listener(|this, _, window, cx| {
                this.open_security_credential_editor(None, window, cx);
            }),
        ));
        if self.security.credentials().is_empty() {
            body = body.child(empty_panel(
                t!("credentialManager.noCredentials"),
                self.theme_palette(),
            ));
        } else {
            let entries = self.security.credentials().to_vec();
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
                let is_revealed = self.security.revealed_credential(&entry.id).is_some();
                let secret = self
                    .security
                    .revealed_credential(&entry.id)
                    .map(str::to_string)
                    .unwrap_or_default();
                let username_copy_id = entry.id.clone();
                let password_copy_id = entry.id.clone();
                let drag_id = entry.id.clone();
                let drop_id = entry.id.clone();
                let is_drop_before = self
                    .security
                    .credential_drop_target()
                    .is_some_and(|target| target.id == entry.id && !target.after);
                let is_drop_after = self
                    .security
                    .credential_drop_target()
                    .is_some_and(|target| target.id == entry.id && target.after);
                rows = rows.child(
                    div()
                        .min_h(px(48.))
                        .when(index + 1 < entry_count, |this| {
                            this.border_b_1().border_color(rgb(palette.border))
                        })
                        .px_3()
                        .py_2()
                        .flex()
                        .when(compact, |this| this.flex_col().items_stretch())
                        .when(!compact, |this| this.items_center())
                        .gap_2()
                        .when(is_drop_before, |this| {
                            this.border_t_2().border_color(rgb(palette.link))
                        })
                        .when(is_drop_after, |this| {
                            this.border_b_2().border_color(rgb(palette.link))
                        })
                        .hover(|this| this.bg(rgb(palette.hover)))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .id(format!("security-cred-drag-{id}"))
                                        .flex_none()
                                        .cursor_move()
                                        .child(crate::features::view_widgets::mono_icon(
                                            "icons/drag.svg",
                                            rgb(palette.text_dimmed).into(),
                                            14.,
                                        ))
                                        .on_drag(
                                            SecurityCredentialDragPayload {
                                                id: drag_id,
                                                label: entry.name.clone(),
                                            },
                                            |payload, position, _, cx| {
                                                cx.new(|_| SecurityCredentialDragPreview {
                                                    label: payload.label.clone(),
                                                    position,
                                                })
                                            },
                                        ),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .gap(px(1.))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .flex_1()
                                                        .text_xs()
                                                        .font_weight(FontWeight(600.))
                                                        .text_color(rgb(palette.text))
                                                        .overflow_hidden()
                                                        .child(truncate_preview(&entry.name, 24)),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(10.))
                                                        .text_color(if entry.enabled {
                                                            rgb(palette.success)
                                                        } else {
                                                            rgb(palette.text_muted)
                                                        })
                                                        .child(t!(if entry.enabled {
                                                            "credentialManager.enabled"
                                                        } else {
                                                            "credentialManager.disabled"
                                                        })),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .text_size(px(10.))
                                                        .text_color(rgb(palette.text_dimmed))
                                                        .overflow_hidden()
                                                        .child(truncate_preview(&entry.username, 28)),
                                                )
                                                .when(!entry.username.is_empty(), |this| {
                                                    this.child(
                                                        nyaterm_ui::NyaIconButton::new(
                                                            format!(
                                                                "security-cred-user-copy-{id}"
                                                            ),
                                                            "icons/copy.svg",
                                                        )
                                                        .icon_size(px(11.))
                                                        .tooltip(
                                                            t!("common.copyToClipboard"),
                                                        )
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.copy_security_credential_username(
                                                                    username_copy_id.clone(),
                                                                    cx,
                                                                );
                                                            },
                                                        )),
                                                    )
                                                }),
                                        )
                                        .when(is_revealed, |this| {
                                            this.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .font_family(
                                                                crate::features::shell::gpui_code_font_family(),
                                                            )
                                                            .text_size(px(10.))
                                                            .text_color(rgb(palette.text_muted))
                                                            .child(truncate_preview(&secret, 28)),
                                                    )
                                                    .when(!secret.is_empty(), |this| {
                                                        this.child(
                                                            nyaterm_ui::NyaIconButton::new(
                                                                format!(
                                                                    "security-cred-pass-copy-{id}"
                                                                ),
                                                                "icons/copy.svg",
                                                            )
                                                            .icon_size(px(11.))
                                                            .tooltip(
                                                                t!("common.copyToClipboard"),
                                                            )
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.copy_security_credential_password(
                                                                        password_copy_id.clone(),
                                                                        cx,
                                                                    );
                                                                },
                                                            )),
                                                        )
                                                    }),
                                            )
                                        }),
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
                                        format!("security-cred-show-{id}"),
                                        if is_revealed {
                                            "icons/eye-off.svg"
                                        } else {
                                            "icons/eye.svg"
                                        },
                                    )
                                    .tooltip(t!(if is_revealed {
                                        "credentialManager.hidePassword"
                                    } else {
                                        "credentialManager.showPassword"
                                    }))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.reveal_security_credential_password(
                                                reveal_id.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-cred-edit-{id}"),
                                        "icons/edit.svg",
                                    )
                                    .tooltip(t!("common.edit"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.open_security_credential_editor(
                                                Some(edit_id.clone()),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    nyaterm_ui::NyaIconButton::new(
                                        format!("security-cred-del-{id}"),
                                        "icons/delete.svg",
                                    )
                                    .tooltip(t!("common.delete"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.request_delete_security_credential(
                                                delete_id.clone(),
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                ),
                        )
                        .on_drag_move(cx.listener({
                            let target_id = drop_id.clone();
                            move |this,
                                  event: &gpui::DragMoveEvent<SecurityCredentialDragPayload>,
                                  _,
                                  cx| {
                                let _ = event.drag(cx);
                                let after = event.event.position.y
                                    >= event.bounds.origin.y + event.bounds.size.height / 2.;
                                this.security.set_credential_drop_target(Some(
                                    crate::models::SecurityCredentialDropTarget {
                                        id: target_id.clone(),
                                        after,
                                    },
                                ));
                                cx.notify();
                            }
                        }))
                        .on_drop(cx.listener(
                            move |this, payload: &SecurityCredentialDragPayload, _, cx| {
                                let after = this
                                    .security
                                    .credential_drop_target()
                                    .filter(|target| target.id == drop_id)
                                    .is_some_and(|target| target.after);
                                this.reorder_security_credentials(
                                    payload.id.clone(),
                                    drop_id.clone(),
                                    after,
                                    cx,
                                );
                            },
                        )),
                );
            }
            body = body.child(rows);
        }
        body
    }
}
