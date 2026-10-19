use gpui::{Context, IntoElement, KeyDownEvent, div, prelude::*, px, rgb};
use rust_i18n::t;
use zzclawterm_core::models::credentials::PasswordUpdate;
use zzclawterm_ui::ZzClawButton;

use crate::features::{ZzClawTermApp, text_inputs::TextInputSetup};
use crate::models::SecurityPasswordEditorState;

use super::super::view_helpers::security_editor_field;

impl ZzClawTermApp {
    pub(in crate::features) fn security_password_editor_view(
        &mut self,
        editor: SecurityPasswordEditorState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let busy = self.security.editor_busy();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .track_focus(self.security.password_editor_focus())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.handle_security_password_editor_key_down(event, window, cx) {
                    cx.stop_propagation();
                }
            }));
        for (id, label, value) in [
            (
                "pw-name",
                t!("authForm.accountName").to_string(),
                editor.name.clone(),
            ),
            (
                "pw-username",
                t!("credentialManager.usernameOptionalLabel").to_string(),
                editor.username.clone(),
            ),
        ] {
            body = body.child(security_editor_field(
                self,
                id,
                label,
                value,
                TextInputSetup::default(),
                cx,
            ));
            if editor.error_field == Some(id) {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette.danger))
                        .child(editor.error.clone().unwrap_or_default()),
                );
            }
        }
        let status = match &editor.password_update {
            PasswordUpdate::Remove => t!("authForm.passwordWillRemove"),
            PasswordUpdate::Replace(_) => t!("authForm.newPassword"),
            PasswordUpdate::Keep if editor.has_password => t!("authForm.passwordSaved"),
            PasswordUpdate::Keep => t!("authForm.passwordNotSaved"),
        };
        body = body.child(div().text_xs().child(status));
        if let PasswordUpdate::Replace(value) = &editor.password_update {
            body = body.child(div().flex().items_end().gap_1().child(
                div().min_w_0().flex_1().child(security_editor_field(
                    self,
                    "pw-value",
                    if editor.id.is_some() {
                        t!("authForm.newPasswordRequiredLabel")
                    } else {
                        t!("authForm.passwordOptional")
                    },
                    value.expose_secret().to_owned(),
                    TextInputSetup::new_password(),
                    cx,
                )),
            ));
            if editor.error_field == Some("pw-value") {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette.danger))
                        .child(editor.error.clone().unwrap_or_default()),
                );
            }
        }
        let mut actions = div().flex().flex_wrap().gap_2();
        if matches!(editor.password_update, PasswordUpdate::Keep) {
            actions = actions.child(
                ZzClawButton::new("account-change-password", t!("authForm.changePassword"))
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_account_password_update(
                            PasswordUpdate::Replace(Default::default()),
                            cx,
                        )
                    })),
            );
            if editor.has_password {
                actions = actions.child(
                    ZzClawButton::new("account-remove-password", t!("authForm.removePassword"))
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_account_password_update(PasswordUpdate::Remove, cx)
                        })),
                );
            }
        } else if editor.id.is_some() {
            actions = actions.child(
                ZzClawButton::new("account-cancel-password", t!("authForm.undoPasswordChange"))
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_account_password_update(PasswordUpdate::Keep, cx)
                    })),
            );
        }
        if let Some(id) = editor.id.clone() {
            if editor.has_password {
                let copy_id = id.clone();
                let reveal_id = id.clone();
                actions = actions
                    .child(
                        ZzClawButton::new(
                            "account-show-original",
                            t!("authForm.viewOriginalPassword"),
                        )
                        .disabled(busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.reveal_security_password(reveal_id.clone(), window, cx)
                            },
                        )),
                    )
                    .child(
                        ZzClawButton::new(
                            "account-copy-original",
                            t!("authForm.copyOriginalPassword"),
                        )
                        .disabled(busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.copy_security_password(copy_id.clone(), window, cx)
                            },
                        )),
                    );
            }
            body = body.child(actions);
            if let Some(value) = self.security.revealed_password(&id) {
                body = body.child(div().text_xs().whitespace_normal().child(value.to_owned()));
            }
            let mut seen = std::collections::HashSet::new();
            let names = self
                .connection_state
                .connections()
                .iter()
                .filter(|connection| {
                    connection.auth.as_ref().is_some_and(|auth| {
                        auth.account_id.as_deref() == Some(id.as_str())
                            || auth.password_id.as_deref() == Some(id.as_str())
                    }) && seen.insert(connection.id.clone())
                })
                .map(|connection| connection.name.clone())
                .collect::<Vec<_>>();
            body = body.child(
                ZzClawButton::new(
                    "account-references",
                    t!("authForm.references", count = names.len()),
                )
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(editor) = this.security.password_editor_mut() {
                        editor.references_expanded = !editor.references_expanded;
                    }
                    cx.notify();
                })),
            );
            if editor.references_expanded {
                for name in names {
                    body = body.child(div().text_xs().child(name));
                }
            }
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("authForm.nextAuthentication")),
            );
        } else {
            body = body.child(actions);
        }
        if editor.error_field.is_none()
            && let Some(error) = editor.error
        {
            body = body.child(div().text_xs().text_color(rgb(palette.danger)).child(error));
        }
        body.text_size(px(12.))
    }
}
