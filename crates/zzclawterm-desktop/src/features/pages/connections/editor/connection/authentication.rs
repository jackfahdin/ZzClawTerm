use gpui::{Context, Div, div, prelude::*, rgb};
use rust_i18n::t;
use zzclawterm_ui::{ZzClawButton, ZzClawTabItem, ZzClawTabs};

use super::super::super::list::{
    ConnectionEditorRenderContext, EditorSecretFieldOptions, connection_editor_select,
    editor_field, editor_secret_field, required,
};
use super::ConnectionEditorSectionContext;
use crate::features::ZzClawTermApp;
use crate::models::ConnectionPasswordEdit;
use crate::models::{
    ConnectionEditorCredentialOverlay, ConnectionEditorField, ConnectionEditorPasswordSource,
    ConnectionEditorSelect, ConnectionKindTab,
};

pub(super) fn login_identity(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
        ..
    } = section;
    let mut body = div()
        .debug_selector(|| "connection-login-identity".to_string())
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().child(t!("authForm.loginIdentity")))
        .child(
            ZzClawTabs::new("connection-account-source")
                .items([
                    ZzClawTabItem::new(t!("authForm.manualIdentity")),
                    ZzClawTabItem::new(t!("authForm.savedIdentity")),
                ])
                .selected_index(usize::from(editor.account_source_saved))
                .on_select(cx.listener(|this, index, _, cx| {
                    this.set_connection_account_source(*index == 1, cx)
                })),
        );
    if editor.account_source_saved {
        let id = editor.account_id.clone();
        body = body.child(
            div()
                .flex()
                .flex_wrap()
                .items_end()
                .gap_2()
                .child(connection_editor_select(
                    ConnectionEditorRenderContext {
                        palette,
                        fields,
                        cx,
                    },
                    "connection-editor-account",
                    t!("dialog.account"),
                    ConnectionEditorSelect::Account,
                ))
                .child(
                    ZzClawButton::new("connection-account-new", t!("passwordManager.add"))
                        .form_control()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_connection_account_editor(None, window, cx)
                        })),
                )
                .child(
                    ZzClawButton::new("connection-account-edit", t!("common.edit"))
                        .form_control()
                        .disabled(fields.account.is_none())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_connection_account_editor(id.clone(), window, cx)
                        })),
                ),
        );
        if fields.account.is_none() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.danger))
                    .child(t!("passwordManager.missingAccount")),
            );
        }
    }
    let account_username = fields
        .account
        .as_ref()
        .filter(|_| editor.account_source_saved)
        .map(|entry| entry.username.trim())
        .filter(|value| !value.is_empty());
    if let Some(username) = account_username {
        body = body
            .child(div().text_xs().child(t!("dialog.username")))
            .child(
                div()
                    .debug_selector(|| "connection-account-username".to_string())
                    .text_xs()
                    .child(username.to_owned()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!(
                        "authForm.usernameFromAccount",
                        name = fields
                            .account
                            .as_ref()
                            .map(|account| account.name.as_str())
                            .unwrap_or_default()
                    )),
            );
    } else {
        body = body.child(editor_field(
            palette,
            if editor.kind == ConnectionKindTab::Ssh {
                required(t!("dialog.username"))
            } else {
                t!("credentialManager.usernameOptionalLabel").into()
            },
            ConnectionEditorField::Username,
            fields,
            cx,
        ));
        if editor.account_source_saved && fields.account.is_some() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("authForm.accountUsernameEmpty")),
            );
        }
    }
    body
}

pub(super) fn password_source(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
        ..
    } = section;
    let uses_identity = matches!(
        editor.kind,
        ConnectionKindTab::Ssh | ConnectionKindTab::Telnet
    );
    let account_available = fields
        .account
        .as_ref()
        .is_some_and(|entry| entry.has_password);
    let tabs = ZzClawTabs::new("connection-password-source-tabs")
        .items([
            ZzClawTabItem::new(t!("dialog.askWhenConnecting")),
            ZzClawTabItem::new(t!("authForm.connectionPassword")),
            ZzClawTabItem::new(t!("authForm.useAccountPassword"))
                .disabled(uses_identity && !account_available),
        ])
        .selected_index(match editor.password_source {
            ConnectionEditorPasswordSource::Ask => 0,
            ConnectionEditorPasswordSource::Direct => 1,
            ConnectionEditorPasswordSource::Saved => 2,
        })
        .on_select(cx.listener(|this, index, _, cx| {
            this.set_connection_editor_password_source(
                match index {
                    0 => ConnectionEditorPasswordSource::Ask,
                    1 => ConnectionEditorPasswordSource::Direct,
                    _ => ConnectionEditorPasswordSource::Saved,
                },
                cx,
            )
        }));
    let mut body = div()
        .debug_selector(|| "connection-password-controls".to_string())
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().child(t!("dialog.passwordSource")))
        .child(tabs);
    if uses_identity && !account_available {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(t!("authForm.accountPasswordUnavailable")),
        );
    }
    match editor.password_source {
        ConnectionEditorPasswordSource::Ask => {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("dialog.askPasswordDescription")),
            )
        }
        ConnectionEditorPasswordSource::Saved => {
            if !uses_identity {
                body = body
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_end()
                            .gap_2()
                            .child(connection_editor_select(
                                ConnectionEditorRenderContext {
                                    palette,
                                    fields,
                                    cx,
                                },
                                "connection-editor-saved-password",
                                t!("dialog.account"),
                                ConnectionEditorSelect::SavedPassword,
                            ))
                            .child(
                                ZzClawButton::new(
                                    "connection-manage-accounts",
                                    t!("dialog.managePasswords"),
                                )
                                .form_control()
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.set_connection_editor_credential_overlay(
                                            Some(ConnectionEditorCredentialOverlay::Passwords),
                                            cx,
                                        )
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette.text_muted))
                            .child(t!("authForm.passwordOnlyReference")),
                    );
            }
            let status = if fields.account.is_none() {
                t!("passwordManager.missingAccount")
            } else if !account_available {
                t!("authForm.passwordNotSaved")
            } else if fields.account_password_locked {
                t!("authForm.passwordLocked")
            } else {
                t!("authForm.passwordSaved")
            };
            body = body.child(div().text_xs().child(status));
        }
        ConnectionEditorPasswordSource::Direct => {
            let has_existing = editor.existing_password.is_some();
            let editing = editor.password_edit == ConnectionPasswordEdit::Replace || !has_existing;
            if editor.password_edit == ConnectionPasswordEdit::Remove {
                body = body.child(div().text_xs().child(t!("authForm.passwordWillRemove")));
            } else if editing {
                body = body.child(editor_secret_field(
                    palette,
                    if has_existing {
                        t!("authForm.newPasswordRequiredLabel")
                    } else {
                        t!("authForm.passwordOptional")
                    },
                    ConnectionEditorField::Password,
                    fields,
                    EditorSecretFieldOptions::new(
                        false,
                        t!("authForm.removePassword"),
                        cx.listener(|this, _, _, cx| this.clear_connection_editor_password(cx)),
                    ),
                    cx,
                ));
            } else {
                body = body.child(div().text_xs().child(
                    if editor.existing_password_locked && fields.account_password_locked {
                        t!("authForm.passwordLocked")
                    } else {
                        t!("authForm.passwordSaved")
                    },
                ));
            }
            let mut actions = div().flex().flex_wrap().gap_2();
            if editor.password_edit == ConnectionPasswordEdit::Keep && has_existing {
                actions = actions
                    .child(
                        ZzClawButton::new(
                            "connection-change-password",
                            t!("authForm.changePassword"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_connection_password(ConnectionPasswordEdit::Replace, cx)
                        })),
                    )
                    .child(
                        ZzClawButton::new(
                            "connection-remove-password",
                            t!("authForm.removePassword"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.edit_connection_password(ConnectionPasswordEdit::Remove, cx)
                        })),
                    );
            } else if has_existing || editor.password_edit == ConnectionPasswordEdit::Remove {
                actions = actions.child(
                    ZzClawButton::new(
                        "connection-undo-password",
                        t!("authForm.undoPasswordChange"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.edit_connection_password(ConnectionPasswordEdit::Keep, cx)
                    })),
                );
            }
            body = body.child(actions);
        }
    }
    if (editor.password_source != ConnectionEditorPasswordSource::Direct
        || (editor.existing_password.is_some()
            && editor.password_edit != ConnectionPasswordEdit::Replace))
        && let Some(error) = fields.errors.get(&ConnectionEditorField::Password)
    {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.danger))
                .child(error.clone()),
        );
    }
    if editor.kind == ConnectionKindTab::Vnc
        && editor.password_source == ConnectionEditorPasswordSource::Direct
    {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(t!("dialog.vncPasswordLimit")),
        );
    }
    body
}

pub(super) fn password_authentication(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> Div {
    let editor = section.editor;
    let protocol_requires_no_auth =
        editor.kind == ConnectionKindTab::Vnc && editor.vnc_security.mode == "none";
    let body = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().child(t!("dialog.authentication")))
        .child(
            ZzClawTabs::new("connection-password-auth")
                .items([
                    ZzClawTabItem::new(t!("dialog.password")).disabled(protocol_requires_no_auth),
                    ZzClawTabItem::new(t!("dialog.noAuthentication")),
                ])
                .selected_index(usize::from(
                    editor.auth_mode == "none" || protocol_requires_no_auth,
                ))
                .on_select(cx.listener(|this, index, _, cx| {
                    this.set_connection_editor_select_value(
                        ConnectionEditorSelect::Authentication,
                        Some(if *index == 0 { "password" } else { "none" }),
                        cx,
                    )
                })),
        );
    if editor.auth_mode == "password"
        && !(editor.kind == ConnectionKindTab::Vnc && editor.vnc_security.mode == "none")
    {
        body.child(password_source(section, cx))
    } else {
        body
    }
}
