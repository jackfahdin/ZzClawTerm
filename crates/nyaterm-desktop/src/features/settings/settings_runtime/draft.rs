use gpui::Context;
use nyaterm_store::{StoreDomain, store_request};
use nyaterm_transport::SftpDuplicatePolicy;

use crate::features::NyaTermApp;
use crate::features::app_state::SettingsDraftSnapshot;
use crate::models::TranslationSecretDraft;

impl NyaTermApp {
    pub(in crate::features) fn begin_settings_draft(&mut self, cx: &mut Context<Self>) {
        if self.shell.has_settings_draft() {
            return;
        }
        let (translation_settings, translation_secret_draft) =
            self.translation.settings_draft_snapshot();
        let (cloud_sync_settings, cloud_sync_secret_draft) =
            self.cloud_sync.settings_draft_snapshot();
        let (ai_settings, ai_model_draft, ai_base_url_draft, ai_secret_draft) =
            self.ai.settings_draft_snapshot();
        let master_password = self.settings.master_password();
        self.shell
            .set_settings_draft_snapshot(SettingsDraftSnapshot {
                settings: self.settings.summary().clone(),
                ai_settings,
                ai_model_draft,
                ai_base_url_draft,
                ai_secret_draft,
                cloud_sync_settings,
                cloud_sync_secret_draft,
                translation_settings,
                translation_secret_draft,
                keyword_highlights: self.settings.keyword_config().clone(),
                master_password_enabled: master_password.enabled,
                master_password_draft: master_password.draft.to_owned().into(),
            });
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn settings_draft_dirty(&self) -> bool {
        let Some(snapshot) = self.shell.settings_draft_snapshot() else {
            return false;
        };
        let master_password = self.settings.master_password();
        snapshot.settings != *self.settings.summary()
            || !self.ai.settings_draft_matches(
                &snapshot.ai_settings,
                &snapshot.ai_model_draft,
                &snapshot.ai_base_url_draft,
                &snapshot.ai_secret_draft,
            )
            || !self.cloud_sync.settings_draft_matches(
                &snapshot.cloud_sync_settings,
                &snapshot.cloud_sync_secret_draft,
            )
            || !self.translation.settings_draft_matches(
                &snapshot.translation_settings,
                &snapshot.translation_secret_draft,
            )
            || snapshot.keyword_highlights != *self.settings.keyword_config()
            || snapshot.master_password_enabled != master_password.enabled
            || snapshot.master_password_draft.expose_secret() != master_password.draft
    }

    /// Returns true when a settings save should stay in the in-memory draft.
    pub(in crate::features) fn defer_settings_persistence(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.request_settings_panel_refresh(cx);
        if !self.shell.has_settings_draft() {
            return false;
        }
        self.settings
            .update_store_status("settings draft changed", true);
        self.shell
            .set_status("settings draft changed; apply to persist".to_string());
        cx.notify();
        true
    }

    pub(in crate::features) fn pending_settings_cloud_error(&self) -> Option<String> {
        let settings = self.cloud_sync.pending_settings();
        if !settings.enabled {
            return None;
        }
        let master_password = self.settings.master_password();
        if !master_password.enabled {
            return Some("Enable a master password before enabling cloud sync".to_string());
        }
        if !self.settings.summary().has_master_password && master_password.draft.is_empty() {
            return Some("Enter a master password before enabling cloud sync".to_string());
        }
        let missing = match settings.provider.as_str() {
            "webdav" if settings.webdav.endpoint.trim().is_empty() => {
                Some("WebDAV endpoint is required")
            }
            "s3" if settings.s3.endpoint.trim().is_empty() => Some("S3 endpoint is required"),
            "s3" if settings.s3.bucket.trim().is_empty() => Some("S3 bucket is required"),
            "s3" if settings
                .s3
                .access_key_id
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
                != settings
                    .s3
                    .secret_access_key
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty() =>
            {
                Some("S3 access key and secret must be provided together")
            }
            "gitee_snippet" if settings.gitee_snippet.api_endpoint.trim().is_empty() => {
                Some("Gitee Snippet API endpoint is required")
            }
            "gitee_snippet" if settings.gitee_snippet.gist_id.trim().is_empty() => {
                Some("Gitee Snippet ID is required")
            }
            "gitee_snippet"
                if settings
                    .gitee_snippet
                    .access_token
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty() =>
            {
                Some("Gitee Snippet token is required")
            }
            "google_drive" => drive_validation_error(
                settings.google_drive.refresh_token.as_deref(),
                settings.google_drive.client_id.as_deref(),
                settings.google_drive.client_secret.as_deref(),
            ),
            "onedrive" => drive_validation_error(
                settings.onedrive.refresh_token.as_deref(),
                settings.onedrive.client_id.as_deref(),
                settings.onedrive.client_secret.as_deref(),
            ),
            "aliyun_drive" => drive_validation_error(
                settings.aliyun_drive.refresh_token.as_deref(),
                settings.aliyun_drive.client_id.as_deref(),
                settings.aliyun_drive.client_secret.as_deref(),
            ),
            "github_gist" if settings.github_gist.gist_id.trim().is_empty() => {
                Some("GitHub Gist ID is required")
            }
            "github_gist"
                if settings
                    .github_gist
                    .access_token
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty() =>
            {
                Some("GitHub Gist token is required")
            }
            _ => None,
        };
        missing.map(str::to_string)
    }

    pub(in crate::features) fn block_cloud_sync_for_settings_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_draft_dirty() {
            return false;
        }
        self.cloud_sync
            .set_status("apply settings before running cloud sync");
        self.shell.set_status(self.cloud_sync.status().to_string());
        cx.notify();
        true
    }

    pub(in crate::features) fn block_import_for_settings_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_draft_dirty() {
            return false;
        }
        self.shell
            .set_status("apply or cancel settings before importing".to_string());
        self.settings
            .update_store_status(self.shell.status().to_string(), false);
        cx.notify();
        true
    }

    pub(in crate::features) fn rebase_open_settings_draft(&mut self, cx: &mut Context<Self>) {
        if !self.shell.has_settings_draft() {
            return;
        }
        self.shell.clear_settings_draft_snapshot();
        self.settings.rebase_master_password();
        self.begin_settings_draft(cx);
    }

    pub(in crate::features) fn apply_settings_draft(
        &mut self,
        close_after_apply: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.shell.has_settings_draft() {
            if close_after_apply {
                self.finish_settings_page(cx);
            }
            self.request_settings_panel_refresh(cx);
            return;
        }
        if let Some(error) = self.pending_settings_cloud_error() {
            self.settings.update_store_status(error.clone(), false);
            self.shell
                .set_status(format!("settings apply blocked: {error}"));
            cx.notify();
            self.request_settings_panel_refresh(cx);
            return;
        }

        let settings = self.settings.summary().clone();
        let ai_settings = self.pending_ai_settings();
        let cloud_sync_settings = self.cloud_sync.pending_settings();
        let translation_settings = self.translation.pending_settings();
        let keyword_highlights = self.settings.keyword_config().clone();
        let master_password = self.settings.master_password();
        let master_password_update = if master_password.draft.is_empty() {
            (self.settings.summary().has_master_password && !master_password.enabled)
                .then_some(None)
        } else {
            Some(Some(master_password.draft.to_string()))
        };
        self.settings
            .update_store_status("applying settings", false);
        self.submit_store_request(
            0,
            store_request(StoreDomain::Settings, move |store| {
                if let Some(next_password) = master_password_update.as_ref() {
                    store.save_master_password(next_password.as_deref())?;
                }
                store.save_appearance_settings(&settings)?;
                store.save_terminal_settings(&settings)?;
                store.save_interaction_settings(&settings)?;
                store.save_general_settings(&settings)?;
                // The header-status mode and visibility are edited on the General tab
                // but stored by the UI-layout writer, which is otherwise driven by
                // layout gestures through `persist_ui_layout`. Without this the draft's
                // choice is never written, and the `load_app_settings_summary` below
                // hands the stale value straight back to `apply_gpui_settings`.
                store.save_ui_layout_settings(&settings)?;
                store.save_diagnostics_settings(&settings)?;
                store.save_screen_lock_settings(&settings)?;
                store.save_recording_settings(&settings)?;
                store.save_transfer_settings(&settings)?;
                store.save_host_key_policy(&settings.host_key_policy)?;
                store.save_keybindings(&settings.keybindings)?;
                let saved_keyword_highlights =
                    store.save_keyword_highlights(&keyword_highlights)?;
                let saved_translation_settings =
                    store.save_translation_settings(translation_settings)?;
                let saved_cloud_sync_settings =
                    store.save_cloud_sync_settings(cloud_sync_settings)?;
                let saved_ai_settings = store.save_ai_settings(ai_settings)?;
                if !settings.startup_restore_window_layout {
                    store.save_terminal_window_layout(None)?;
                    store.save_workspace_pane_layout(None)?;
                }
                Ok((
                    store.load_app_settings_summary()?,
                    saved_keyword_highlights,
                    saved_translation_settings,
                    saved_cloud_sync_settings,
                    saved_ai_settings,
                ))
            }),
            move |this, event, cx| match event.outcome {
                Ok((
                    saved_settings,
                    saved_keyword_highlights,
                    saved_translation_settings,
                    saved_cloud_sync_settings,
                    saved_ai_settings,
                )) => {
                    this.apply_gpui_settings(saved_settings, cx);
                    this.settings.rebase_master_password();
                    this.ai.replace_settings_config(saved_ai_settings, true);
                    this.cloud_sync
                        .replace_settings(saved_cloud_sync_settings, Default::default());
                    this.translation.replace_settings(
                        saved_translation_settings,
                        TranslationSecretDraft::default(),
                    );
                    this.settings
                        .replace_keyword_config(saved_keyword_highlights);
                    this.sync_ai_drafts_from_active_profile();
                    this.recording.set_memory_limit(
                        this.settings.summary().recording_memory_limit_bytes as usize,
                    );
                    this.transfer
                        .set_duplicate_policy(SftpDuplicatePolicy::from_legacy_value(
                            &this.settings.summary().transfer_duplicate_strategy,
                        ));
                    this.sync_terminal_encodings_from_settings();
                    this.enforce_terminal_scrollback_limit();
                    if !this
                        .settings
                        .summary()
                        .interaction_command_suggestions_enabled
                    {
                        this.terminal.clear_command_tracking();
                    }
                    this.invalidate_terminal_cell_metrics(cx);
                    this.refresh_visible_terminal_surfaces(cx);
                    this.shell.clear_settings_draft_snapshot();
                    this.settings.update_store_status("settings applied", true);
                    this.shell.set_status("settings applied".to_string());
                    if close_after_apply {
                        this.finish_settings_page(cx);
                    } else {
                        this.begin_settings_draft(cx);
                        cx.notify();
                    }
                    this.request_settings_panel_refresh(cx);
                }
                Err(error) => {
                    let message = format!("settings apply failed: {error}");
                    this.settings.update_store_status(message.clone(), false);
                    this.shell.set_status(message);
                    this.request_settings_panel_refresh(cx);
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(in crate::features) fn cancel_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.shell.take_settings_draft_snapshot() {
            self.apply_gpui_settings(snapshot.settings, cx);
            self.ai.restore_settings_draft(
                snapshot.ai_settings,
                snapshot.ai_model_draft,
                snapshot.ai_base_url_draft,
                snapshot.ai_secret_draft,
            );
            self.cloud_sync.replace_settings(
                snapshot.cloud_sync_settings,
                snapshot.cloud_sync_secret_draft,
            );
            self.translation.replace_settings(
                snapshot.translation_settings,
                snapshot.translation_secret_draft,
            );
            self.settings
                .replace_keyword_config(snapshot.keyword_highlights);
            self.settings.restore_master_password_draft(
                snapshot.master_password_enabled,
                snapshot.master_password_draft,
            );
            self.recording
                .set_memory_limit(self.settings.summary().recording_memory_limit_bytes as usize);
            self.transfer
                .set_duplicate_policy(SftpDuplicatePolicy::from_legacy_value(
                    &self.settings.summary().transfer_duplicate_strategy,
                ));
            self.sync_terminal_encodings_from_settings();
            self.invalidate_terminal_cell_metrics(cx);
            self.invalidate_paint_theme_caches();
            self.sync_ai_drafts_from_active_profile();
            self.refresh_visible_terminal_surfaces(cx);
        }
        self.finish_settings_page(cx);
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn confirm_settings_draft(&mut self, cx: &mut Context<Self>) {
        if self.settings_draft_dirty() {
            self.apply_settings_draft(true, cx);
        } else {
            self.shell.clear_settings_draft_snapshot();
            self.finish_settings_page(cx);
        }
        self.request_settings_panel_refresh(cx);
    }

    pub(in crate::features) fn toggle_settings_master_password(&mut self, cx: &mut Context<Self>) {
        self.shell.set_status(
            match self
                .settings
                .toggle_master_password(self.cloud_sync.settings().enabled)
            {
                Ok(true) => "master password enabled; enter a password".to_string(),
                Ok(false) => "master password removal staged".to_string(),
                Err(error) => error.to_string(),
            },
        );
        cx.notify();
    }

    /// Apply an edit from the master password box.
    pub(in crate::features) fn apply_settings_master_password(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.edit_master_password_draft(text) {
            return;
        }
        self.shell
            .set_status("master password edited; apply to persist".to_string());
        cx.notify();
    }

    fn finish_settings_page(&mut self, cx: &mut Context<Self>) {
        self.cancel_github_gist_auth(cx);
        self.ai.close_settings_editors();
        self.settings.clear_keyword_highlight_edit();
        self.forget_text_inputs("ai.settings.action.");
        self.forget_text_inputs("ai.settings.manual-model.");
        self.forget_text_inputs("keyword.highlight.");
        if self.shell.finish_settings_navigation() {
            self.persist_ui_layout();
        }
        self.shell.set_status("settings closed".to_string());
        cx.notify();
    }
}

fn drive_validation_error(
    refresh_token: Option<&str>,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Option<&'static str> {
    if refresh_token.unwrap_or("").trim().is_empty() {
        return Some("Drive refresh token is required");
    }
    if client_id.unwrap_or("").trim().is_empty() {
        return Some("Drive client ID is required");
    }
    if client_secret.unwrap_or("").trim().is_empty() {
        return Some("Drive client secret is required");
    }
    None
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::features::settings::SettingsSaveKind;
    use crate::models::HeaderStatusMode;

    /// A real store runtime, because these tests are about what reaches disk.
    fn app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-settings-draft-{}-{}",
            std::process::id(),
            uuid()
        ));
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.clone(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    fn stored_header_status(
        app: &gpui::Entity<NyaTermApp>,
        cx: &mut TestAppContext,
    ) -> (String, bool) {
        let summary = cx
            .update_entity(app, |app, _| {
                app.store_blocking_client()
                    .request_fn(nyaterm_store::StoreDomain::Settings, |store| {
                        store.load_app_settings_summary()
                    })
            })
            .expect("load stored settings");
        (
            summary.ui_header_status_mode,
            summary.ui_header_status_visible,
        )
    }

    /// Applying a draft must write the header-status mode it changed.
    ///
    /// `persist_header_status_settings` deliberately defers while a draft is open,
    /// telling the user to apply settings to persist. Apply then reloads the summary
    /// from the store and replaces the in-memory one wholesale, so a value its save
    /// batch failed to write comes back as whatever is still on disk -- which is the
    /// reported symptom: the header reverts the instant settings are applied.
    ///
    /// **The assertion is on disk, not on the summary.** In this harness the store
    /// reply is not delivered inside `run_until_parked`, so the in-memory summary keeps
    /// the value the draft set and an assertion on it passes whether or not anything
    /// was written -- which it did, before the disk check was added. Disk is also the
    /// more fundamental property: a value that never lands there is lost at the next
    /// launch regardless of what this session shows.
    ///
    /// `DateTime` rather than `Session`, because `session` is the stored default -- a
    /// test that set the mode to the default could not tell a successful apply from a
    /// dropped one.
    #[test]
    fn applying_a_draft_persists_the_header_status_mode() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        assert_eq!(
            stored_header_status(&app, &mut cx),
            ("session".to_string(), true),
            "fixture baseline"
        );

        cx.update_entity(&app, |app, cx| {
            app.begin_settings_draft(cx);
            app.set_header_status_mode(HeaderStatusMode::DateTime, cx);
            assert_eq!(
                app.settings.summary().ui_header_status_mode,
                "datetime",
                "the draft changes the in-memory summary immediately"
            );
            app.apply_settings_draft(false, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            stored_header_status(&app, &mut cx),
            ("datetime".to_string(), true),
            "apply must write the header status, not leave the stored value behind for \
             its own reload to restore"
        );
    }

    #[test]
    fn canceling_a_draft_restores_ui_typography_source() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);

        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let original_family = app.gpui_ui_font().family;
            let original_size = app.settings.summary().ui_font_size;

            app.begin_settings_draft(cx);
            app.update_ui_font_family("Noto Sans", cx);
            app.set_ui_font_size_from_input(20, cx);

            assert_eq!(app.gpui_ui_font().family, "Noto Sans");
            assert_eq!(app.settings.summary().ui_font_size, 20);

            app.cancel_settings(cx);

            assert_eq!(app.gpui_ui_font().family, original_family);
            assert_eq!(app.settings.summary().ui_font_size, original_size);
        });
    }

    /// And must write a header status turned back on from hidden.
    ///
    /// This is the direction the bug was reported in: the header reads as "hidden"
    /// again the moment settings are applied. Seeded through the store so the stale
    /// on-disk value is genuinely `false` rather than the `true` default, which is what
    /// makes the revert visible at all.
    #[test]
    fn applying_a_draft_persists_a_header_status_turned_back_on() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            // No draft open, so this takes the immediate-persist path.
            app.set_header_status_visible(false, cx);
            app.queue_settings_save(SettingsSaveKind::UiLayout, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            stored_header_status(&app, &mut cx),
            ("session".to_string(), false),
            "the seed must reach disk, or there is no stale value to revert to"
        );

        cx.update_entity(&app, |app, cx| {
            app.begin_settings_draft(cx);
            app.set_header_status_mode(HeaderStatusMode::Host, cx);
            app.apply_settings_draft(false, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            stored_header_status(&app, &mut cx),
            ("host".to_string(), true),
            "apply must write the header back on, not leave it hidden on disk"
        );
    }
}
