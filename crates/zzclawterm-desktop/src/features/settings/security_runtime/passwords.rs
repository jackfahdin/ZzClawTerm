use rust_i18n::t;

use gpui::{Context, IntoElement as _, KeyDownEvent, Window};
use zzclawterm_core::SavedPassword;
use zzclawterm_core::models::credentials::PasswordUpdate;
use zzclawterm_ui::ZzClawDialogWindowExt as _;

use crate::features::{ZzClawTermApp, formatting::compact_id, runtime_jobs::await_blocking_job};
use crate::models::{SecurityAuthTab, SecurityPasswordEditorState, SecurityUnlockAction};

use super::jobs::{SecurityStoreLocation, load_security_catalog};

impl ZzClawTermApp {
    pub(in crate::features) fn open_connection_account_editor(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_security_password_editor(id, window, cx);
        let origin = self.connection_state.editor_token();
        if let Some(editor) = self.security.password_editor_mut() {
            editor.connection_origin = origin;
        }
    }

    pub(in crate::features) fn open_security_password_editor(
        &mut self,
        password_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.forget_text_inputs("security.editor.pw-");
        let editor = if let Some(password_id) = password_id {
            let Some(entry) = self
                .security
                .passwords()
                .iter()
                .find(|entry| entry.id == password_id)
                .cloned()
            else {
                self.security.set_status("password is no longer available");
                cx.notify();
                return;
            };
            SecurityPasswordEditorState {
                id: Some(entry.id),
                name: entry.name,
                username: entry.username,
                password_update: PasswordUpdate::Keep,
                references_expanded: false,
                connection_origin: None,
                error_field: None,
                has_password: entry.has_password,
                error: None,
            }
        } else {
            SecurityPasswordEditorState {
                id: None,
                name: String::new(),
                username: String::new(),
                password_update: PasswordUpdate::Replace(Default::default()),
                references_expanded: false,
                connection_origin: None,
                error_field: None,
                has_password: false,
                error: None,
            }
        };
        self.security
            .open_password_editor(editor, "password editor opened".to_string());
        let title = if self
            .security
            .password_editor()
            .is_some_and(|editor| editor.id.is_some())
        {
            t!("passwordManager.editTitle")
        } else {
            t!("passwordManager.newTitle")
        }
        .to_string();
        self.open_guarded_form_dialog(
            (
                title,
                440.,
                t!("common.save").to_string(),
                |app, _, cx| {
                    app.security
                        .password_editor()
                        .cloned()
                        .map(|editor| {
                            app.security_password_editor_view(editor, cx)
                                .into_any_element()
                        })
                        .unwrap_or_else(|| gpui::div().into_any_element())
                },
                |app, window, cx| {
                    app.save_security_password_editor(window, cx);
                    app.security.password_editor().is_none()
                },
                |app, cx| app.close_security_password_editor(cx),
                |app| app.security.editor_busy(),
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn close_security_password_editor(&mut self, cx: &mut Context<Self>) {
        if self.security.editor_busy() {
            return;
        }
        self.forget_text_inputs("security.editor.pw-");
        if let Some(id) = self
            .security
            .password_editor()
            .and_then(|editor| editor.id.clone())
        {
            self.security.hide_revealed_password(&id);
        }
        self.security.close_password_editor();
        cx.notify();
    }

    pub(in crate::features) fn handle_security_password_editor_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.mark_user_activity();
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.alt || keystroke.modifiers.control {
            return false;
        }
        match keystroke.key.as_str() {
            "escape" => {
                if !self.security.editor_busy() {
                    self.close_security_password_editor(cx);
                    window.close_nya_dialog(cx);
                }
                true
            }
            "enter" => {
                let input_focused = self.text_input_fields_snapshot().iter().any(|(id, field)| {
                    id.starts_with("security.editor.pw-")
                        && field.read(cx).component_focus_handle(cx).is_focused(window)
                });
                if input_focused || self.security.password_editor_focus().is_focused(window) {
                    self.save_security_password_editor(window, cx);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(in crate::features) fn save_security_password_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.security.password_editor().cloned() else {
            return;
        };
        let name = editor.name.trim().to_string();
        if name.is_empty() {
            if let Some(editor) = self.security.password_editor_mut() {
                editor.error = Some(t!("passwordManager.nameRequired").to_string());
                editor.error_field = Some("pw-name");
            }
            self.focus_account_editor_field("pw-name", window, cx);
            cx.notify();
            return;
        }
        let has_password = match &editor.password_update {
            PasswordUpdate::Keep => editor.has_password,
            PasswordUpdate::Replace(value) => !value.is_empty(),
            PasswordUpdate::Remove => false,
        };
        if !has_password && editor.username.trim().is_empty() {
            if let Some(editor) = self.security.password_editor_mut() {
                editor.error = Some(t!("passwordManager.credentialsRequired").to_string());
                editor.error_field = Some("pw-username");
            }
            self.focus_account_editor_field("pw-username", window, cx);
            cx.notify();
            return;
        }
        if matches!(&editor.password_update, PasswordUpdate::Replace(value) if value.is_empty())
            && editor.id.is_some()
        {
            if let Some(editor) = self.security.password_editor_mut() {
                editor.error = Some(t!("authForm.passwordRequired").to_string());
                editor.error_field = Some("pw-value");
            }
            self.focus_account_editor_field("pw-value", window, cx);
            cx.notify();
            return;
        }
        let update = match editor.password_update.clone() {
            PasswordUpdate::Replace(value) if value.is_empty() => PasswordUpdate::Remove,
            update => update,
        };
        if editor.id.is_some() && !matches!(update, PasswordUpdate::Keep) {
            let Some(token) = self.security.account_editor_token() else {
                return;
            };
            if !self.require_security_secrets_unlocked(
                window,
                cx,
                Some(SecurityUnlockAction::SavePasswordEditor(token)),
            ) {
                return;
            }
        }
        let origin = editor.connection_origin;
        let is_new = editor.id.is_none();
        let entry = SavedPassword {
            sort_order: 0,
            id: editor.id.clone().unwrap_or_default(),
            name,
            username: editor.username.trim().to_string(),
            password: None,
            has_password: false,
        };
        let Some(request_id) = self.security.begin_editor_request() else {
            return;
        };
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let task = scheduler.submit_task("saved-password-save", move |_| {
                let store = location.open()?;
                let id = store
                    .save_password_with_update(entry, update)
                    .map_err(|error| error.to_string())?;
                let catalog = load_security_catalog(&store)?;
                Ok::<_, String>((id, catalog))
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let mut close = false;
            let _ = this.update(cx, |this, cx| {
                if !this.security.finish_editor_request(request_id) {
                    return;
                }
                match result {
                    Ok((id, catalog)) => {
                        this.forget_text_inputs("security.editor.pw-");
                        this.security.hide_revealed_password(&id);
                        this.security.replace_catalog_state(catalog);
                        this.request_shared_state_refresh(
                            crate::app_shell::SharedStateDomain::Security,
                            cx,
                        );
                        this.security.finish_password_editor(format!(
                            "password saved ({})",
                            compact_id(&id)
                        ));
                        if is_new
                            && origin.is_some()
                            && this.connection_state.editor_token() == origin
                        {
                            let select = if this.connection_state.active_editor_draft().is_some_and(
                                |draft| {
                                    matches!(
                                        draft.kind,
                                        crate::models::ConnectionKindTab::Ssh
                                            | crate::models::ConnectionKindTab::Telnet
                                    )
                                },
                            ) {
                                crate::models::ConnectionEditorSelect::Account
                            } else {
                                crate::models::ConnectionEditorSelect::SavedPassword
                            };
                            this.set_connection_editor_select_value(select, Some(&id), cx);
                            this.connection_state.set_editor_credential_overlay(None);
                        }
                        this.shell.set_status("password saved".to_string());
                        close = true;
                    }
                    Err(error) => {
                        if let Some(editor) = this.security.password_editor_mut() {
                            editor.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
            if close {
                let _ = cx.update(|window, cx| {
                    window.close_nya_dialog(cx);
                    let _ = this.update(cx, |this, cx| {
                        if origin.is_some() && this.connection_state.editor_token() == origin {
                            let select = if this.connection_state.active_editor_draft().is_some_and(
                                |draft| {
                                    matches!(
                                        draft.kind,
                                        crate::models::ConnectionKindTab::Ssh
                                            | crate::models::ConnectionKindTab::Telnet
                                    )
                                },
                            ) {
                                crate::models::ConnectionEditorSelect::Account
                            } else {
                                crate::models::ConnectionEditorSelect::SavedPassword
                            };
                            this.focus_connection_editor_select(select, window, cx);
                        }
                    });
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn request_delete_security_password(
        &mut self,
        password_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_security_secrets_unlocked(
            window,
            cx,
            Some(SecurityUnlockAction::DeletePassword(password_id.clone())),
        ) {
            return;
        }
        let label = self
            .security
            .passwords()
            .iter()
            .find(|entry| entry.id == password_id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| password_id.clone());
        self.open_security_delete_dialog(
            SecurityAuthTab::Passwords,
            password_id,
            label,
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn reveal_security_password(
        &mut self,
        password_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Tauri PasswordManagementTab: eye toggles reveal; hide does not need unlock.
        if self.security.hide_revealed_password(&password_id) {
            self.security.set_status("password hidden");
            cx.notify();
            return;
        }
        if !self.require_security_secrets_unlocked(
            window,
            cx,
            Some(SecurityUnlockAction::RevealPassword(password_id.clone())),
        ) {
            return;
        }
        self.load_security_password(password_id, false, cx);
        cx.notify();
    }

    pub(in crate::features) fn copy_security_password(
        &mut self,
        password_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.require_security_secrets_unlocked(
            window,
            cx,
            Some(SecurityUnlockAction::CopyPassword(password_id.clone())),
        ) {
            return;
        }
        if let Some(value) = self
            .security
            .revealed_password(&password_id)
            .map(str::to_string)
        {
            if value.is_empty() {
                self.security.set_status("password has no secret");
                self.notify_operation(
                    "copy-password",
                    zzclawterm_ui::notification::ZzClawNotificationKind::Warning,
                    rust_i18n::t!("common.nothingToCopy").to_string(),
                    cx,
                );
            } else {
                self.copy_text_with_feedback("copy-password", value, cx);
                self.security.set_status("password copied");
            }
            cx.notify();
            return;
        }
        self.load_security_password(password_id, true, cx);
        cx.notify();
    }

    fn load_security_password(&mut self, password_id: String, copy: bool, cx: &mut Context<Self>) {
        let editor_origin = self.security.account_editor_token();
        let request_password_id = password_id.clone();
        let request_id = self.security.begin_password_request(password_id.clone());
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        let origin_window = cx
            .active_window()
            .or_else(|| self.shell.main_window().map(Into::into));
        cx.spawn(async move |this, cx| {
            let task = scheduler.submit_task("saved-password-reveal", move |_| {
                let store = location.open()?;
                store
                    .load_decrypted_password_by_id(&password_id)
                    .map_err(|error| error.to_string())
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if editor_origin.is_some() && this.security.account_editor_token() != editor_origin
                {
                    return;
                }
                if !this
                    .security
                    .finish_password_request(request_id, &request_password_id)
                {
                    return;
                }
                match result {
                    Ok(Some(entry)) => {
                        let value = entry.password.unwrap_or_default().into_secret();
                        if value.is_empty() {
                            this.security.set_status("password has no secret");
                            if copy {
                                this.notify_operation_at(
                                    origin_window,
                                    "copy-password",
                                    zzclawterm_ui::notification::ZzClawNotificationKind::Warning,
                                    rust_i18n::t!("common.nothingToCopy").to_string(),
                                    cx,
                                );
                            }
                        } else {
                            if !copy {
                                this.security
                                    .reveal_password(request_password_id.clone(), value.clone());
                            }
                            if copy {
                                this.copy_text_with_feedback_at(
                                    origin_window,
                                    "copy-password",
                                    value,
                                    cx,
                                );
                                this.security.set_status("password revealed and copied");
                            } else {
                                this.security.set_status("password revealed");
                            }
                        }
                    }
                    Ok(None) => {
                        this.security.set_status("password not found");
                        if copy {
                            this.notify_operation_at(
                                origin_window,
                                "copy-password",
                                zzclawterm_ui::notification::ZzClawNotificationKind::Error,
                                rust_i18n::t!("common.copyFailed").to_string(),
                                cx,
                            );
                        }
                    }
                    Err(error) => {
                        this.security.set_status(error);
                        if copy {
                            this.notify_operation_at(
                                origin_window,
                                "copy-password",
                                zzclawterm_ui::notification::ZzClawNotificationKind::Error,
                                rust_i18n::t!("common.copyFailed").to_string(),
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::features) fn set_account_password_update(
        &mut self,
        update: PasswordUpdate,
        cx: &mut Context<Self>,
    ) {
        if self.security.editor_busy() {
            return;
        }
        self.forget_text_inputs("security.editor.pw-value");
        if let Some(editor) = self.security.password_editor_mut() {
            editor.password_update = update;
            editor.error = None;
            editor.error_field = None;
        }
        cx.notify();
    }

    fn focus_account_editor_field(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_text_input_if_present(&format!("security.editor.{id}"), window, cx);
    }
}
