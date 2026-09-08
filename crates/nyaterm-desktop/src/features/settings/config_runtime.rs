use rust_i18n::t;

use gpui::{
    AnyElement, Context, FontWeight, KeyDownEvent, PathPromptOptions, SharedString, Window, div,
    prelude::*, rgb,
};
use nyaterm_store::ConnectionStore;
use nyaterm_store::{BootstrapSnapshot, LoadBootstrap};
use nyaterm_transport::SftpDuplicatePolicy;

use crate::features::{NyaTermApp, runtime_jobs::await_blocking_job, text_inputs::TextInputSetup};
use crate::models::{
    ConfigPathPromptKind, ConfigPathPromptResult, SnapshotPasswordPromptKind,
    TranslationSecretDraft,
};

impl NyaTermApp {
    pub(in crate::features) fn prompt_encrypted_portable_snapshot_export(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_local_snapshot_password_dialog(SnapshotPasswordPromptKind::Export, window, cx);
    }

    pub(in crate::features) fn prompt_encrypted_portable_snapshot_import(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.block_import_for_settings_draft(cx) {
            return;
        }
        if self.session.active_id().is_some() || self.session.start_has_pending() {
            self.shell
                .set_status("close active session before importing config".to_string());
            cx.notify();
            return;
        }
        self.open_local_snapshot_password_dialog(SnapshotPasswordPromptKind::Import, window, cx);
    }

    fn open_local_snapshot_password_dialog(
        &mut self,
        kind: SnapshotPasswordPromptKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_stale_local_snapshot_password_prompt(cx);
        if !self.settings.begin_snapshot_password_prompt(kind) {
            self.shell
                .set_status("backup or sync prompt is already open".to_string());
            cx.notify();
            return;
        }

        self.forget_text_inputs("snapshot-password.");
        let field = self.text_input("snapshot-password.value", "", TextInputSetup::masked(), cx);
        self.shell.set_status(
            match kind {
                SnapshotPasswordPromptKind::Export => "enter password for encrypted .nya export",
                SnapshotPasswordPromptKind::Import => "enter password for encrypted .nya import",
                SnapshotPasswordPromptKind::CloudForcePush
                | SnapshotPasswordPromptKind::CloudForcePull
                | SnapshotPasswordPromptKind::CloudProviderPush
                | SnapshotPasswordPromptKind::CloudProviderPull
                | SnapshotPasswordPromptKind::CloudProviderForcePush
                | SnapshotPasswordPromptKind::CloudProviderForcePull
                | SnapshotPasswordPromptKind::CloudRecoverCurrent
                | SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                    "enter password for encrypted cloud sync snapshot"
                }
            }
            .to_string(),
        );
        self.settings
            .set_store_message("awaiting .nya master password");

        let title = match kind {
            SnapshotPasswordPromptKind::Export => t!("runtimePrompt.snapshotExport"),
            SnapshotPasswordPromptKind::Import => t!("runtimePrompt.snapshotImport"),
            SnapshotPasswordPromptKind::CloudForcePush
            | SnapshotPasswordPromptKind::CloudForcePull
            | SnapshotPasswordPromptKind::CloudProviderPush
            | SnapshotPasswordPromptKind::CloudProviderPull
            | SnapshotPasswordPromptKind::CloudProviderForcePush
            | SnapshotPasswordPromptKind::CloudProviderForcePull
            | SnapshotPasswordPromptKind::CloudRecoverCurrent
            | SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                t!("runtimePrompt.cloudPush")
            }
        };
        self.open_form_dialog(
            (
                title.to_string(),
                448.,
                t!("runtimePrompt.submit").to_string(),
                |app, _, cx| app.local_snapshot_password_dialog_content(cx),
                |app, _, cx| app.submit_local_snapshot_password_dialog(cx),
                |app, cx| app.cancel_snapshot_password_prompt(cx),
            ),
            window,
            cx,
        );
        window.focus(&field.read(cx).focus_handle(), cx);
        cx.notify();
    }

    fn clear_stale_local_snapshot_password_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.settings.snapshot_password_prompt() else {
            return;
        };
        if matches!(
            prompt.kind,
            SnapshotPasswordPromptKind::Export | SnapshotPasswordPromptKind::Import
        ) {
            let _ = self.settings.take_snapshot_password_prompt();
            self.forget_text_inputs("snapshot-password.");
            self.settings.set_store_message("config picker cancelled");
            cx.notify();
        }
    }

    fn local_snapshot_password_dialog_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(prompt) = self.settings.snapshot_password_prompt() else {
            return div().into_any_element();
        };
        let palette = self.theme_palette();
        let description = t!("runtimePrompt.localSnapshotDescription");
        let password_input = self.text_input_box(
            "snapshot-password.value",
            &prompt.value,
            TextInputSetup::masked(),
            cx,
        );

        div()
            .flex()
            .flex_col()
            .gap_3()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.handle_snapshot_password_key_down(event, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.text))
                    .child(description),
            )
            .child(password_input)
            .into_any_element()
    }

    fn submit_local_snapshot_password_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.settings.take_snapshot_password_prompt() else {
            return true;
        };
        let password: nyaterm_core::SecretString = state.value.trim().to_owned().into();
        if password.is_empty() {
            self.settings.restore_snapshot_password_prompt(state.kind);
            self.reset_text_input("snapshot-password.value", "", cx);
            self.shell
                .set_status("master password is required for encrypted .nya".to_string());
            cx.notify();
            return false;
        }
        self.forget_text_inputs("snapshot-password.");

        match state.kind {
            SnapshotPasswordPromptKind::Export => {
                self.prompt_encrypted_portable_snapshot_export_path(password, cx);
            }
            SnapshotPasswordPromptKind::Import => {
                self.prompt_encrypted_portable_snapshot_import_path(password, cx);
            }
            SnapshotPasswordPromptKind::CloudForcePush
            | SnapshotPasswordPromptKind::CloudForcePull
            | SnapshotPasswordPromptKind::CloudProviderPush
            | SnapshotPasswordPromptKind::CloudProviderPull
            | SnapshotPasswordPromptKind::CloudProviderForcePush
            | SnapshotPasswordPromptKind::CloudProviderForcePull
            | SnapshotPasswordPromptKind::CloudRecoverCurrent
            | SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                self.settings.restore_snapshot_password_prompt(state.kind);
                self.shell.set_status(
                    "cloud sync password prompt must be submitted from settings".to_string(),
                );
                cx.notify();
                return false;
            }
        }
        true
    }

    pub(in crate::features) fn start_snapshot_password_prompt(
        &mut self,
        kind: SnapshotPasswordPromptKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.begin_snapshot_password_prompt(kind) {
            self.shell
                .set_status("backup or sync prompt is already open".to_string());
            cx.notify();
            return;
        }
        self.forget_text_inputs("snapshot-password.");
        let field = self.text_input("snapshot-password.value", "", TextInputSetup::masked(), cx);
        window.focus(&field.read(cx).focus_handle(), cx);
        self.shell.set_status(
            match kind {
                SnapshotPasswordPromptKind::Export => "enter password for encrypted .nya export",
                SnapshotPasswordPromptKind::Import => "enter password for encrypted .nya import",
                SnapshotPasswordPromptKind::CloudForcePush => {
                    "enter password for forced cloud sync push"
                }
                SnapshotPasswordPromptKind::CloudForcePull => {
                    "enter password for forced cloud sync pull"
                }
                SnapshotPasswordPromptKind::CloudProviderPush => {
                    "enter password for provider cloud sync push"
                }
                SnapshotPasswordPromptKind::CloudProviderPull => {
                    "enter password for provider cloud sync pull"
                }
                SnapshotPasswordPromptKind::CloudProviderForcePush => {
                    "enter password for forced provider cloud sync push"
                }
                SnapshotPasswordPromptKind::CloudProviderForcePull => {
                    "enter password for forced provider cloud sync pull"
                }
                SnapshotPasswordPromptKind::CloudRecoverCurrent => {
                    "enter password to recover cloud sync metadata"
                }
                SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                    "enter password to recover provider cloud sync metadata"
                }
            }
            .to_string(),
        );
        let store_message = match kind {
            SnapshotPasswordPromptKind::CloudForcePush
            | SnapshotPasswordPromptKind::CloudForcePull
            | SnapshotPasswordPromptKind::CloudProviderPush
            | SnapshotPasswordPromptKind::CloudProviderPull
            | SnapshotPasswordPromptKind::CloudProviderForcePush
            | SnapshotPasswordPromptKind::CloudProviderForcePull
            | SnapshotPasswordPromptKind::CloudRecoverCurrent
            | SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                "awaiting cloud sync password".to_string()
            }
            _ => "awaiting .nya master password".to_string(),
        };
        self.settings.set_store_message(store_message);
        cx.notify();
    }

    pub(in crate::features) fn submit_snapshot_password_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.settings.take_snapshot_password_prompt() else {
            return;
        };
        let password: nyaterm_core::SecretString = state.value.trim().to_owned().into();
        if password.is_empty() {
            self.settings.restore_snapshot_password_prompt(state.kind);
            self.reset_text_input("snapshot-password.value", "", cx);
            self.shell
                .set_status("master password is required for encrypted .nya".to_string());
            cx.notify();
            return;
        }
        self.forget_text_inputs("snapshot-password.");

        match state.kind {
            SnapshotPasswordPromptKind::Export => {
                self.prompt_encrypted_portable_snapshot_export_path(password, cx);
            }
            SnapshotPasswordPromptKind::Import => {
                self.prompt_encrypted_portable_snapshot_import_path(password, cx);
            }
            SnapshotPasswordPromptKind::CloudForcePush => {
                self.run_local_cloud_sync_push(password, true, cx);
            }
            SnapshotPasswordPromptKind::CloudForcePull => {
                self.run_local_cloud_sync_pull(password, true, cx);
            }
            SnapshotPasswordPromptKind::CloudProviderPush => {
                self.run_provider_cloud_sync_push(password, false, cx);
            }
            SnapshotPasswordPromptKind::CloudProviderPull => {
                self.run_provider_cloud_sync_pull(password, false, cx);
            }
            SnapshotPasswordPromptKind::CloudProviderForcePush => {
                self.run_provider_cloud_sync_push(password, true, cx);
            }
            SnapshotPasswordPromptKind::CloudProviderForcePull => {
                self.run_provider_cloud_sync_pull(password, true, cx);
            }
            SnapshotPasswordPromptKind::CloudRecoverCurrent => {
                self.run_cloud_sync_recovery(password, false, cx);
            }
            SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                self.run_cloud_sync_recovery(password, true, cx);
            }
        }
    }

    pub(in crate::features) fn cancel_snapshot_password_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.settings.take_snapshot_password_prompt() else {
            return;
        };
        self.forget_text_inputs("snapshot-password.");
        self.shell.set_status(match state.kind {
            SnapshotPasswordPromptKind::Export => "encrypted .nya export cancelled".to_string(),
            SnapshotPasswordPromptKind::Import => "encrypted .nya import cancelled".to_string(),
            SnapshotPasswordPromptKind::CloudForcePush => {
                "forced cloud sync push cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudForcePull => {
                "forced cloud sync pull cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudProviderPush => {
                "provider cloud sync push cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudProviderPull => {
                "provider cloud sync pull cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudProviderForcePush => {
                "forced provider cloud sync push cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudProviderForcePull => {
                "forced provider cloud sync pull cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudRecoverCurrent => {
                "cloud sync metadata recovery cancelled".to_string()
            }
            SnapshotPasswordPromptKind::CloudProviderRecoverCurrent => {
                "provider cloud sync metadata recovery cancelled".to_string()
            }
        });
        self.settings.set_store_message("config picker cancelled");
        cx.notify();
    }

    pub(in crate::features) fn handle_snapshot_password_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.mark_user_activity();
        if !self.settings.snapshot_password_prompt_active() {
            return false;
        }
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || keystroke.modifiers.alt || keystroke.modifiers.control {
            return false;
        }

        match keystroke.key.as_str() {
            "enter" => self.submit_snapshot_password_prompt(cx),
            "escape" => self.cancel_snapshot_password_prompt(cx),
            _ => return false,
        }
        true
    }

    pub(in crate::features) fn apply_snapshot_password_input(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.apply_snapshot_password_input(text) {
            return;
        }
        self.mark_user_activity();
        cx.notify();
    }

    fn prompt_encrypted_portable_snapshot_export_path(
        &mut self,
        master_password: nyaterm_core::SecretString,
        cx: &mut Context<Self>,
    ) {
        if !self
            .settings
            .begin_config_path_prompt(ConfigPathPromptKind::EncryptedPortableExport)
        {
            self.shell
                .set_status("config path picker is already open".to_string());
            cx.notify();
            return;
        }
        let directory = self.runtime.config_dir().to_path_buf();
        let receiver = cx.prompt_for_new_path(&directory, Some("nyaterm-encrypted.nya"));
        let config_dir = self.runtime.config_dir().to_path_buf();
        let portable_key_path = self.runtime.portable_key_path().map(ToOwned::to_owned);
        let scheduler = self.blocking_jobs.clone();
        self.shell
            .set_status("selecting encrypted portable snapshot destination".to_string());
        self.settings
            .set_store_message("selecting encrypted .nya export destination");
        cx.spawn(async move |this, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => {
                    let task = scheduler.submit_task("portable-snapshot-export", move |_| {
                        match ConnectionStore::export_encrypted_portable_snapshot(
                            &config_dir,
                            portable_key_path,
                            &path,
                            "native-local",
                            env!("CARGO_PKG_VERSION"),
                            master_password.expose_secret(),
                        ) {
                            Ok(info) => ConfigPathPromptResult::Exported(info),
                            Err(error) => ConfigPathPromptResult::Failed(error.to_string()),
                        }
                    });
                    await_blocking_job(task)
                        .await
                        .unwrap_or_else(ConfigPathPromptResult::Failed)
                }
                Ok(Ok(None)) => ConfigPathPromptResult::Cancelled,
                Ok(Err(error)) => ConfigPathPromptResult::Failed(error.to_string()),
                Err(_) => ConfigPathPromptResult::Closed,
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_config_path_prompt_result(
                    ConfigPathPromptKind::EncryptedPortableExport,
                    result,
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn prompt_encrypted_portable_snapshot_import_path(
        &mut self,
        master_password: nyaterm_core::SecretString,
        cx: &mut Context<Self>,
    ) {
        if !self
            .settings
            .begin_config_path_prompt(ConfigPathPromptKind::EncryptedPortableImport)
        {
            self.shell
                .set_status("config path picker is already open".to_string());
            cx.notify();
            return;
        }
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from("Select encrypted .nya snapshot")),
        };
        let receiver = cx.prompt_for_paths(options);
        let config_dir = self.runtime.config_dir().to_path_buf();
        let portable_key_path = self.runtime.portable_key_path().map(ToOwned::to_owned);
        let scheduler = self.blocking_jobs.clone();
        self.shell
            .set_status("selecting encrypted portable snapshot to import".to_string());
        self.settings
            .set_store_message("selecting encrypted .nya snapshot");
        cx.spawn(async move |this, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => {
                        let task = scheduler.submit_task("portable-snapshot-import", move |_| {
                            match ConnectionStore::import_encrypted_portable_snapshot(
                                &config_dir,
                                portable_key_path,
                                &path,
                                master_password.expose_secret(),
                            ) {
                                Ok(info) => ConfigPathPromptResult::Imported(info),
                                Err(error) => ConfigPathPromptResult::Failed(error.to_string()),
                            }
                        });
                        await_blocking_job(task)
                            .await
                            .unwrap_or_else(ConfigPathPromptResult::Failed)
                    }
                    None => ConfigPathPromptResult::Cancelled,
                },
                Ok(Ok(None)) => ConfigPathPromptResult::Cancelled,
                Ok(Err(error)) => ConfigPathPromptResult::Failed(error.to_string()),
                Err(_) => ConfigPathPromptResult::Closed,
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_config_path_prompt_result(
                    ConfigPathPromptKind::EncryptedPortableImport,
                    result,
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_config_path_prompt_result(
        &mut self,
        kind: ConfigPathPromptKind,
        result: ConfigPathPromptResult,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.finish_config_path_prompt(kind) {
            return;
        }
        match result {
            ConfigPathPromptResult::Exported(info) => {
                let message = match kind {
                    ConfigPathPromptKind::EncryptedPortableExport => {
                        format!("exported {} byte encrypted .nya snapshot", info.bytes)
                    }
                    ConfigPathPromptKind::EncryptedPortableImport => {
                        format!("exported {} byte encrypted .nya snapshot", info.bytes)
                    }
                };
                self.settings.replace_store_status(
                    info.database_path.display().to_string(),
                    message,
                    true,
                );
                self.shell.set_status(match kind {
                    ConfigPathPromptKind::EncryptedPortableExport => {
                        format!(
                            "encrypted portable snapshot exported to {}",
                            info.backup_path.display()
                        )
                    }
                    ConfigPathPromptKind::EncryptedPortableImport => {
                        format!(
                            "encrypted portable snapshot exported to {}",
                            info.backup_path.display()
                        )
                    }
                });
            }
            ConfigPathPromptResult::Imported(info) => {
                self.refresh_store_from_runtime_and_sync_theme(cx);
                self.rebase_open_settings_draft(cx);
                let safety = info
                    .safety_backup_path
                    .as_ref()
                    .map(|path| format!("; previous db saved to {}", path.display()))
                    .unwrap_or_default();
                let message = match kind {
                    ConfigPathPromptKind::EncryptedPortableImport => {
                        format!(
                            "imported {} byte encrypted .nya snapshot{safety}",
                            info.bytes
                        )
                    }
                    ConfigPathPromptKind::EncryptedPortableExport => {
                        format!(
                            "imported {} byte encrypted .nya snapshot{safety}",
                            info.bytes
                        )
                    }
                };
                self.settings.update_store_status(message, true);
                self.shell.set_status(match kind {
                    ConfigPathPromptKind::EncryptedPortableImport => {
                        format!(
                            "encrypted portable snapshot imported from {}",
                            info.backup_path.display()
                        )
                    }
                    ConfigPathPromptKind::EncryptedPortableExport => {
                        format!(
                            "encrypted portable snapshot imported from {}",
                            info.backup_path.display()
                        )
                    }
                });
            }
            ConfigPathPromptResult::Cancelled => {
                self.shell.set_status(match kind {
                    ConfigPathPromptKind::EncryptedPortableExport => {
                        "encrypted portable snapshot export cancelled".to_string()
                    }
                    ConfigPathPromptKind::EncryptedPortableImport => {
                        "encrypted portable snapshot import cancelled".to_string()
                    }
                });
                self.settings.set_store_message("config picker cancelled");
            }
            ConfigPathPromptResult::Failed(error) => {
                self.shell.set_status(match kind {
                    ConfigPathPromptKind::EncryptedPortableExport => {
                        format!("encrypted portable snapshot export failed: {error}")
                    }
                    ConfigPathPromptKind::EncryptedPortableImport => {
                        format!("encrypted portable snapshot import failed: {error}")
                    }
                });
                self.settings
                    .update_store_status(self.shell.status().to_string(), false);
            }
            ConfigPathPromptResult::Closed => {
                self.shell
                    .set_status("config path picker closed before returning".to_string());
                self.settings.set_store_message("config picker closed");
            }
        }
    }

    pub(in crate::features) fn refresh_store_from_runtime_and_sync_theme(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.submit_store_request(
            0,
            LoadBootstrap,
            |this, event, cx| match event.outcome {
                Ok(snapshot) => {
                    this.apply_store_refresh(snapshot, cx);
                    cx.notify();
                }
                Err(error) => {
                    let message = format!("store refresh failed: {error}");
                    this.settings.update_store_status(message.clone(), false);
                    this.shell.set_status(message);
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn apply_store_refresh(&mut self, snapshot: BootstrapSnapshot, cx: &mut Context<Self>) {
        self.connection_state
            .replace_loaded(snapshot.connections, snapshot.connection_groups);
        self.security.replace_catalog(
            snapshot.ssh_keys,
            snapshot.otp_entries,
            snapshot.saved_passwords,
            snapshot.saved_credentials,
        );
        self.tunnel_state.replace_loaded_catalog(
            snapshot.tunnels,
            snapshot.tunnel_groups,
            snapshot.proxies,
            snapshot.proxy_groups,
        );
        self.commands.replace_loaded(
            snapshot.quick_commands,
            snapshot.quick_command_categories,
            snapshot.command_history,
        );
        self.settings
            .replace_keyword_config(snapshot.keyword_highlights);
        self.apply_gpui_settings(snapshot.settings, cx);
        self.apply_ui_layout_from_settings();
        self.translation.replace_settings(
            snapshot.translation_settings,
            TranslationSecretDraft::default(),
        );
        self.recording
            .set_memory_limit(self.settings.summary().recording_memory_limit_bytes as usize);
        self.ai.replace_settings_config(snapshot.ai_settings, true);
        self.sync_ai_drafts_from_active_profile();
        self.settings.rebase_master_password();
        self.cloud_sync
            .replace_loaded(snapshot.cloud_sync_settings, snapshot.cloud_sync_state);
        self.transfer
            .set_duplicate_policy(SftpDuplicatePolicy::from_legacy_value(
                &self.settings.summary().transfer_duplicate_strategy,
            ));
        self.settings.replace_store_status(
            snapshot.database_path.display().to_string(),
            "redb connection store online".to_string(),
            true,
        );
        // Notes are not part of BootstrapSnapshot because their Markdown bodies
        // are loaded on demand. A backup import or cloud pull may have replaced
        // the entire catalog, so refresh its lightweight tree explicitly.
        self.refresh_notes(cx);
        self.request_settings_panel_refresh(cx);
    }
}
