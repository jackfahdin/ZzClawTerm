use rust_i18n::t;

use gpui::{Context, KeyDownEvent, Window};
use nyaterm_ui::NyaDialogWindowExt as _;

use crate::features::{NyaTermApp, runtime_jobs::await_blocking_job, text_inputs::TextInputSetup};
use crate::models::{NavItem, SecurityAuthTab, SecurityUnlockAction, SettingsTab};

use super::jobs::{SecurityStoreLocation, load_security_catalog};

impl NyaTermApp {
    pub(in crate::features) fn security_secrets_locked(&self) -> bool {
        self.settings.summary().has_master_password && !self.security.secrets_unlocked()
    }

    pub(in crate::features) fn require_security_secrets_unlocked(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        pending_action: Option<SecurityUnlockAction>,
    ) -> bool {
        if self.settings.summary().has_master_password && self.security.secrets_unlocked() {
            return true;
        }
        self.security.set_pending_unlock_action(pending_action);
        self.open_security_unlock_prompt(window, cx);
        false
    }

    pub(in crate::features) fn open_security_unlock_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.summary().has_master_password {
            self.security.show_master_required_prompt();
            self.forget_text_inputs("security.unlock.password");
            cx.notify();
            return;
        }
        self.security.show_unlock_prompt();
        self.forget_text_inputs("security.unlock.password");
        let field = self.text_input("security.unlock.password", "", TextInputSetup::masked(), cx);
        window.focus(&field.read(cx).focus_handle(), cx);
        cx.notify();
    }

    pub(in crate::features) fn cancel_security_unlock_prompt(&mut self, cx: &mut Context<Self>) {
        self.security.cancel_unlock_prompt();
        self.forget_text_inputs("security.unlock.password");
        cx.notify();
    }

    pub(in crate::features) fn close_security_master_required_prompt(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.security.close_master_required_prompt();
        cx.notify();
    }

    pub(in crate::features) fn open_security_settings_from_prompt(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.security.close_master_required_prompt();
        self.shell.set_settings_active_tab(SettingsTab::Security);
        self.open_page(NavItem::Settings, cx);
    }

    pub(in crate::features) fn lock_security_secrets(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.security.lock_secrets();
        self.forget_text_inputs("security.unlock.password");
        window.close_all_nya_dialogs(cx);
        cx.notify();
    }

    pub(in crate::features) fn submit_security_unlock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.summary().has_master_password {
            self.security.show_master_required_prompt();
            self.forget_text_inputs("security.unlock.password");
            cx.notify();
            return;
        }
        let Some((request_id, password)) = self.security.begin_unlock_request() else {
            return;
        };
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let task = scheduler.submit_task("security-unlock", move |_| {
                let store = location.open()?;
                store
                    .verify_master_password(password.expose_secret())
                    .map_err(|error| error.to_string())
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let mut pending_action = None;
            let _ = this.update(cx, |this, cx| {
                if !this.security.finish_unlock_request(request_id) {
                    return;
                }
                match result {
                    Ok(true) => {
                        pending_action = this.security.complete_unlock();
                        this.forget_text_inputs("security.unlock.password");
                    }
                    Ok(false) => {
                        this.reset_text_input("security.unlock.password", "", cx);
                        let error = t!("secretUnlock.wrongPassword").to_string();
                        this.security.reject_unlock(error, "unlock rejected");
                    }
                    Err(error) => {
                        this.reset_text_input("security.unlock.password", "", cx);
                        this.security.reject_unlock(error, "unlock failed");
                    }
                }
                cx.notify();
            });
            if let Some(action) = pending_action {
                let _ = cx.update(|window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.execute_security_unlock_action(action, window, cx);
                    });
                });
            }
        })
        .detach();
    }

    pub(in crate::features) fn handle_security_unlock_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mark_user_activity();
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.alt || keystroke.modifiers.control {
            return;
        }
        match keystroke.key.as_str() {
            "enter" => self.submit_security_unlock(window, cx),
            "escape" => self.cancel_security_unlock_prompt(cx),
            _ => {}
        }
    }

    pub(in crate::features) fn apply_security_unlock_password_input(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.security.apply_unlock_input(text);
        cx.notify();
    }

    fn execute_security_unlock_action(
        &mut self,
        action: SecurityUnlockAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            SecurityUnlockAction::ViewPrivateKey(id) => {
                self.view_security_private_key(id, window, cx);
            }
            SecurityUnlockAction::OpenPasswordEditor(id) => {
                self.open_security_password_editor(id, window, cx);
            }
            SecurityUnlockAction::RevealPassword(id) => {
                self.reveal_security_password(id, window, cx);
            }
            SecurityUnlockAction::CopyPassword(id) => {
                self.copy_security_password(id, window, cx);
            }
            SecurityUnlockAction::DeletePassword(id) => {
                self.request_delete_security_password(id, window, cx);
            }
            SecurityUnlockAction::OpenCredentialEditor(id) => {
                self.open_security_credential_editor(id, window, cx);
            }
            SecurityUnlockAction::ToggleCredentialEnabled(id) => {
                self.toggle_security_credential_list_enabled(id, window, cx);
            }
            SecurityUnlockAction::RevealCredential(id) => {
                self.reveal_security_credential_password(id, window, cx);
            }
            SecurityUnlockAction::DeleteCredential(id) => {
                self.request_delete_security_credential(id, window, cx);
            }
        }
    }

    pub(in crate::features) fn set_security_auth_tab(
        &mut self,
        tab: SecurityAuthTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.security.set_auth_tab(tab);
        window.close_all_nya_dialogs(cx);
        cx.notify();
    }

    pub(in crate::features) fn refresh_security_catalog(&mut self, cx: &mut Context<Self>) {
        let location = SecurityStoreLocation::new(self.store_blocking_client());
        let scheduler = self.blocking_jobs.clone();
        cx.spawn(async move |this, cx| {
            let task = scheduler.submit_task("security-catalog-refresh", move |_| {
                let store = location.open()?;
                load_security_catalog(&store)
            });
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(catalog) => this.security.replace_catalog_state(catalog),
                    Err(error) => this.security.set_status(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
