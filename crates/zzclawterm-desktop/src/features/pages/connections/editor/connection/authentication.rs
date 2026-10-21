use gpui::{Context, Div, div, prelude::*, rgb};
use rust_i18n::t;
use zzclawterm_core::ConnectionUsernameSource;
use zzclawterm_ui::{ZzClawButton, ZzClawTabItem, ZzClawTabs};

use super::super::super::list::{
    ConnectionEditorFields, ConnectionEditorRenderContext, EditorSecretFieldOptions,
    connection_editor_select, editor_field, editor_secret_field, required,
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
    let account_username = fields
        .account
        .as_ref()
        .map(|entry| entry.username.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let follows_account =
        editor.username_source == ConnectionUsernameSource::Account && account_username.is_some();
    if let Some(input) = fields.get(&ConnectionEditorField::Username) {
        let shown = if follows_account {
            account_username.clone().unwrap_or_default()
        } else {
            editor.username.clone()
        };
        input.update(cx, |state, cx| {
            state.set_readonly(follows_account, cx);
            if state.value(cx) != shown {
                state.set_content_silent(&shown, cx);
            }
        });
    }
    let id = editor.account_id.clone();
    let mut body = div()
        .debug_selector(|| "connection-login-identity".to_string())
        .flex()
        .flex_col()
        .gap_2()
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
    if editor.account_id.is_some() && fields.account.is_none() {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.danger))
                .child(t!("passwordManager.missingAccount")),
        );
    }
    let mut username_row = div().flex().flex_wrap().items_end().gap_2().child(
        div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .child(editor_field(
                palette,
                if editor.kind == ConnectionKindTab::Ssh {
                    required(t!("dialog.username"))
                } else {
                    t!("credentialManager.usernameOptionalLabel").into()
                },
                ConnectionEditorField::Username,
                fields,
                cx,
            )),
    );
    if follows_account {
        username_row = username_row
            .debug_selector(|| "connection-account-username".to_string())
            .child(
                ZzClawButton::new("connection-username-override", t!("common.edit"))
                    .form_control()
                    .on_click(
                        cx.listener(|this, _, _, cx| this.use_connection_editor_username(cx)),
                    ),
            );
    } else if account_username.is_some()
        && editor.username_source == ConnectionUsernameSource::Connection
    {
        username_row = username_row.child(
            ZzClawButton::new(
                "connection-username-restore",
                t!("authForm.restoreAccountUsername"),
            )
            .form_control()
            .on_click(
                cx.listener(|this, _, _, cx| this.follow_connection_editor_account_username(cx)),
            ),
        );
    }
    body = body.child(username_row);
    if follows_account {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(t!("authForm.usernameProvidedByAccount")),
        );
    } else if account_username.is_some()
        && editor.username_source == ConnectionUsernameSource::Connection
    {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(t!("authForm.usernameForConnection")),
        );
    } else if fields.account.is_some() && account_username.is_none() {
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .child(t!("authForm.accountUsernameFallbackDescription")),
        );
    }
    body
}

fn account_password_prompt(fields: &ConnectionEditorFields, has_password: bool) -> String {
    let Some(account) = fields.account.as_ref() else {
        return t!("passwordManager.missingAccount").to_string();
    };
    if has_password && fields.account_password_locked {
        return t!("authForm.passwordLocked").to_string();
    }
    let name = account.name.clone();
    if has_password {
        t!("authForm.accountPasswordDescription", name = name).to_string()
    } else {
        t!("authForm.accountPasswordMissingDescription", name = name).to_string()
    }
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
    let account_linked = uses_identity && editor.account_id.is_some();
    let mut items = vec![
        if account_linked {
            ZzClawTabItem::new(t!("authForm.useAccountPassword"))
        } else {
            ZzClawTabItem::new(t!("dialog.askWhenConnecting"))
        },
        ZzClawTabItem::new(t!("authForm.connectionPassword")),
    ];
    if !uses_identity {
        items.push(ZzClawTabItem::new(t!("authForm.useAccountPassword")));
    }
    let selected_index = if uses_identity {
        match editor.password_source {
            ConnectionEditorPasswordSource::Direct => Some(1),
            ConnectionEditorPasswordSource::Saved => Some(0),
            ConnectionEditorPasswordSource::Ask if account_available => None,
            ConnectionEditorPasswordSource::Ask => Some(0),
        }
    } else {
        Some(match editor.password_source {
            ConnectionEditorPasswordSource::Ask => 0,
            ConnectionEditorPasswordSource::Direct => 1,
            ConnectionEditorPasswordSource::Saved => 2,
        })
    };
    let tabs = ZzClawTabs::new("connection-password-source-tabs")
        .items(items)
        .selected_index_if_visible(selected_index)
        .on_select(cx.listener(move |this, index, _, cx| {
            let source = if uses_identity {
                match index {
                    0 if account_available => ConnectionEditorPasswordSource::Saved,
                    0 => ConnectionEditorPasswordSource::Ask,
                    _ => ConnectionEditorPasswordSource::Direct,
                }
            } else {
                match index {
                    0 => ConnectionEditorPasswordSource::Ask,
                    1 => ConnectionEditorPasswordSource::Direct,
                    _ => ConnectionEditorPasswordSource::Saved,
                }
            };
            this.set_connection_editor_password_source(source, cx);
        }));
    let mut body = div()
        .debug_selector(|| "connection-password-controls".to_string())
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().child(t!("dialog.passwordSource")))
        .child(tabs);
    match editor.password_source {
        ConnectionEditorPasswordSource::Ask => {
            let message = if account_linked && !account_available {
                account_password_prompt(fields, false)
            } else if editor.password_source_demoted {
                t!("authForm.passwordSourceAdjusted").to_string()
            } else {
                t!("dialog.askPasswordDescription").to_string()
            };
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(message),
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
            let status = if uses_identity {
                account_password_prompt(fields, account_available)
            } else if fields.account.is_none() {
                t!("passwordManager.missingAccount").to_string()
            } else if !account_available {
                t!("authForm.passwordNotSaved").to_string()
            } else if fields.account_password_locked {
                t!("authForm.passwordLocked").to_string()
            } else {
                t!("authForm.passwordSaved").to_string()
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
