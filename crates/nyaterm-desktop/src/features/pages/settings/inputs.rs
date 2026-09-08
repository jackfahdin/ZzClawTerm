//! Building the inputs each Settings tab draws.
//!
//! Creating an input is a mutation: `text_input` and `number_input` build the entity
//! and its change subscription on first use, so a render that calls them writes to
//! authoritative state on the first frame that shows the field. Every Settings
//! section did exactly that, unconditionally, so the first paint of each tab created
//! between one and seven inputs.
//!
//! The seeds and options live here now rather than at the render sites, which look
//! their inputs up by id and cannot create one. That is a relocation, not a
//! duplication: the render sites lost those arguments entirely.
//!
//! One consequence worth naming, because it looks like a behaviour change and is not:
//! `number_input` returns early for an id it already holds, so options derived from
//! state -- `range(1.0, max_chars)`, `disabled(!debounce_enabled)` -- were only ever
//! honoured on the frame that created the input. Building at tab activation keeps that
//! exactly, because activation is the moment that first frame used to be.

use rust_i18n::t;

use gpui::Context;
use nyaterm_ui::NyaNumberInputOptions;

use crate::features::NyaTermApp;
use crate::features::text_inputs::{TextInputSetup, secret_input_setup};
use nyaterm_core::RecordingRotationPolicy;

use crate::models::{
    AiActionEditorField, AiActionListKind, AiInputField, CloudSyncInputField, SettingsTab,
    TranslateInputField,
};

impl NyaTermApp {
    /// Build every input the given tab draws.
    ///
    /// Called when the Settings page opens and whenever a tab is activated. Cheap to
    /// repeat: each `ensure_*` returns early for an input that already exists.
    pub(in crate::features) fn ensure_settings_tab_inputs(
        &mut self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) {
        match tab {
            SettingsTab::General => {}
            SettingsTab::Appearance => self.ensure_appearance_inputs(cx),
            SettingsTab::Interaction => self.ensure_interaction_inputs(cx),
            SettingsTab::Keybindings => self.ensure_keybinding_inputs(cx),
            SettingsTab::TerminalGeneral => self.ensure_terminal_inputs(cx),
            SettingsTab::Search => self.ensure_expanded_search_engine_inputs(cx),
            SettingsTab::Translation => self.ensure_translation_inputs(cx),
            SettingsTab::AiGeneral => self.ensure_ai_general_inputs(cx),
            SettingsTab::AiModels => self.ensure_ai_model_inputs(cx),
            SettingsTab::AiRules => self.ensure_ai_rule_inputs(cx),
            SettingsTab::Transfer => self.ensure_transfer_inputs(cx),
            SettingsTab::Security => self.ensure_security_inputs(cx),
            SettingsTab::SyncBackup => self.ensure_cloud_sync_settings_inputs(cx),
        }
    }

    /// Activate a settings tab from somewhere other than the panel's own tab strip.
    ///
    /// `open_page` and the tab strip both build the tab's inputs and publish the switch
    /// to the panel. A jump that only set the active tab would leave the panel on its
    /// previous snapshot, and once it caught up it would land on a tab whose inputs were
    /// never built, drawing every field it owns as an empty box.
    pub(in crate::features) fn focus_settings_tab(
        &mut self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) {
        self.ensure_settings_tab_inputs(tab, cx);
        self.shell.set_settings_active_tab(tab);
        self.request_settings_panel_refresh(cx);
    }

    /// The AI General tab owns the request and external-agent registry inputs.
    pub(in crate::features) fn ensure_ai_settings_inputs(&mut self, cx: &mut Context<Self>) {
        let settings = self.ai.settings_config().clone();
        for (field, value) in [
            (AiInputField::RequestUserAgent, settings.request_user_agent),
            (
                AiInputField::CodexExecutable,
                settings.codex.executable_path.unwrap_or_default(),
            ),
            (
                AiInputField::CodexDefaultModel,
                settings.codex.default_model.unwrap_or_default(),
            ),
            (
                AiInputField::CodexConfigDirectory,
                settings.codex.config_directory.unwrap_or_default(),
            ),
            (
                AiInputField::ClaudeExecutable,
                settings.claude_code.executable_path.unwrap_or_default(),
            ),
            (
                AiInputField::ClaudeDefaultModel,
                settings.claude_code.default_model.unwrap_or_default(),
            ),
            (
                AiInputField::ClaudeConfigDirectory,
                settings.claude_code.config_directory.unwrap_or_default(),
            ),
        ] {
            self.ensure_text_input(
                format!("ai.input.{}", field.input_key()),
                &value,
                TextInputSetup::default(),
                cx,
            );
        }
    }

    /// Masking comes from `TranslateInputField::is_secret` rather than a list repeated
    /// here, so a new `*-api-key` or `*-app-key` variant cannot be added to the enum and
    /// silently seed an unmasked input.
    pub(in crate::features) fn ensure_translation_inputs(&mut self, cx: &mut Context<Self>) {
        let (settings, secret_draft) = self.translation.settings_draft_snapshot();
        for (field, value) in [
            (
                TranslateInputField::DeeplApiKey,
                secret_draft.deepl_api_key.expose_secret().to_owned(),
            ),
            (
                TranslateInputField::BaiduAppId,
                settings.baidu_app_id.clone(),
            ),
            (
                TranslateInputField::BaiduAppKey,
                secret_draft.baidu_app_key.expose_secret().to_owned(),
            ),
            (TranslateInputField::AliAppId, settings.ali_app_id.clone()),
            (
                TranslateInputField::AliAppKey,
                secret_draft.ali_app_key.expose_secret().to_owned(),
            ),
            (
                TranslateInputField::YoudaoAppId,
                settings.youdao_app_id.clone(),
            ),
            (
                TranslateInputField::YoudaoAppKey,
                secret_draft.youdao_app_key.expose_secret().to_owned(),
            ),
        ] {
            self.ensure_text_input(
                format!("translation.input.{}", field.input_key()),
                &value,
                secret_input_setup(field.is_secret()),
                cx,
            );
        }
    }

    pub(in crate::features) fn ensure_cloud_sync_settings_inputs(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        for field in CloudSyncInputField::ALL {
            let value = self.cloud_sync.input_value(field);
            self.ensure_text_input(
                format!("cloud-sync.input.{}", field.input_key()),
                &value,
                secret_input_setup(field.is_secret()),
                cx,
            );
        }
        let form_enabled = self.cloud_sync_form_enabled();
        let settings = self.cloud_sync.settings();
        let debounce_enabled = form_enabled && settings.enabled && settings.auto_push_on_change;
        self.ensure_number_input(
            "cloud-sync.number.debounce",
            &settings.sync_debounce_seconds.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, 3_600.0)
                .step(1.0)
                .disabled(!debounce_enabled),
            cx,
        );
    }

    /// Every action row the rules tab draws, in the order it draws them.
    ///
    /// Both lists are drawn together, so both are built together.
    fn ai_action_ids(&self) -> Vec<(AiActionListKind, String)> {
        let mut ids = Vec::new();
        for kind in [AiActionListKind::Terminal, AiActionListKind::File] {
            let actions = match kind {
                AiActionListKind::Terminal => self.ai.settings_config().terminal_ai_actions.clone(),
                AiActionListKind::File => self.ai.settings_config().file_ai_actions.clone(),
            };
            for action in actions {
                ids.push((kind, action.id.clone()));
            }
        }
        ids
    }

    fn ensure_appearance_inputs(&mut self, cx: &mut Context<Self>) {
        let terminal_font_size = self.settings.summary().terminal_font_size.to_string();
        self.ensure_number_input(
            "appearance.number.terminal-font-size",
            &terminal_font_size,
            NyaNumberInputOptions::default().range(8.0, 72.0).step(1.0),
            cx,
        );
        let ui_font_size = self.settings.summary().ui_font_size.to_string();
        self.ensure_number_input(
            "appearance.number.ui-font-size",
            &ui_font_size,
            NyaNumberInputOptions::default().range(12.0, 24.0).step(1.0),
            cx,
        );
    }

    fn ensure_interaction_inputs(&mut self, cx: &mut Context<Self>) {
        let word_separators = self.settings.summary().interaction_word_separators.clone();
        self.ensure_text_input(
            "settings.interaction.word-separators",
            &word_separators,
            TextInputSetup::default(),
            cx,
        );
        let summary = self.settings.summary();
        let min_chars = summary.interaction_command_suggestion_min_chars;
        let max_chars = summary.interaction_command_suggestion_max_chars;
        let delay_ms = summary.interaction_duplicate_session_command_delay_ms;
        self.ensure_number_input(
            "settings.number.command-suggestion-min-chars",
            &min_chars.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, max_chars as f64)
                .step(1.0),
            cx,
        );
        self.ensure_number_input(
            "settings.number.command-suggestion-max-chars",
            &max_chars.to_string(),
            NyaNumberInputOptions::default()
                .range(min_chars as f64, 500.0)
                .step(1.0),
            cx,
        );
        self.ensure_number_input(
            "settings.number.duplicate-session-command-delay",
            &delay_ms.to_string(),
            NyaNumberInputOptions::default()
                .range(0.0, 60_000.0)
                .step(100.0)
                .suffix("ms"),
            cx,
        );
    }

    fn ensure_keybinding_inputs(&mut self, cx: &mut Context<Self>) {
        let search = self.settings.keybinding_presentation().search_draft;
        self.ensure_text_input(
            "settings.keybindings.search",
            &search,
            TextInputSetup::placeholder(t!("settings.keybindingsSearch")),
            cx,
        );
    }

    fn ensure_terminal_inputs(&mut self, cx: &mut Context<Self>) {
        let summary = self.settings.summary().clone();
        self.ensure_text_input(
            "settings.terminal.x11-display",
            &summary.x11_display,
            TextInputSetup::placeholder(t!("settings.x11DisplayPlaceholder")),
            cx,
        );
        self.ensure_text_input(
            "settings.terminal.timestamp-format",
            &summary.terminal_timestamp_format,
            TextInputSetup::placeholder("[HH:mm:ss]"),
            cx,
        );
        for (id, value, min, max, step) in [
            (
                "settings.number.terminal-scrollback-lines",
                summary.terminal_scrollback_lines as f64,
                100.0,
                100_000.0,
                100.0,
            ),
            (
                "settings.number.terminal-keep-alive-interval",
                summary.terminal_keep_alive_interval as f64,
                0.0,
                600.0,
                5.0,
            ),
            (
                "settings.number.remote-stats-interval",
                summary.ui_remote_stats_interval as f64,
                1.0,
                60.0,
                1.0,
            ),
            (
                "settings.number.gpu-monitor-interval",
                summary.ui_gpu_monitor_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
            (
                "settings.number.ascend-npu-monitor-interval",
                summary.ui_ascend_npu_monitor_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
            (
                "settings.number.process-manager-interval",
                summary.ui_process_manager_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
            (
                "settings.number.docker-manager-interval",
                summary.ui_docker_manager_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
        ] {
            self.ensure_number_input(
                id,
                &format!("{}", value as i64),
                NyaNumberInputOptions::default().range(min, max).step(step),
                cx,
            );
        }
    }

    fn ensure_transfer_inputs(&mut self, cx: &mut Context<Self>) {
        let summary = self.settings.summary().clone();
        for (id, value, min, max, step) in [
            (
                "settings.number.transfer-download-threads",
                summary.transfer_download_threads as f64,
                1.0,
                10.0,
                1.0,
            ),
            (
                "settings.number.transfer-upload-threads",
                summary.transfer_upload_threads as f64,
                1.0,
                10.0,
                1.0,
            ),
            (
                "settings.number.transfer-max-retries",
                summary.transfer_max_retries as f64,
                0.0,
                10.0,
                1.0,
            ),
            (
                "settings.number.transfer-buffer-size",
                summary.transfer_buffer_size as f64,
                8.0,
                256.0,
                8.0,
            ),
        ] {
            self.ensure_number_input(
                id,
                &format!("{}", value as i64),
                NyaNumberInputOptions::default().range(min, max).step(step),
                cx,
            );
        }

        for (id, value, placeholder) in [
            (
                "settings.transfer.default-permissions",
                summary.transfer_default_file_permissions.clone(),
                "644".to_string(),
            ),
            (
                "settings.transfer.default-editor",
                summary.transfer_default_editor.clone(),
                t!("settings.defaultEditor").to_string(),
            ),
            (
                "settings.transfer.download-path",
                summary.transfer_download_path.clone(),
                t!("settings.downloadPath").to_string(),
            ),
            (
                "settings.recording.path",
                summary.recording_path.clone(),
                t!("settings.recordingPath").to_string(),
            ),
            (
                "settings.recording.path-template",
                summary.recording_path_template.clone(),
                nyaterm_core::DEFAULT_RECORDING_PATH_TEMPLATE.to_string(),
            ),
        ] {
            self.ensure_text_input(id, &value, TextInputSetup::placeholder(placeholder), cx);
        }

        // Recording lives on the Transfer tab, and both of its inputs are in MiB while
        // the settings they mirror are in bytes.
        let memory_mib =
            (self.settings.summary().recording_memory_limit_bytes / (1024 * 1024)).max(1);
        let rotation_size_mib = match self.settings.summary().recording_rotation {
            RecordingRotationPolicy::Size { max_bytes } => (max_bytes / (1024 * 1024)).max(1),
            _ => 50,
        };
        self.ensure_number_input(
            "settings.number.recording-rotation-size",
            &rotation_size_mib.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, 102_400.0)
                .step(1.0)
                .suffix("MiB"),
            cx,
        );
        self.ensure_number_input(
            "settings.number.recording-memory-limit",
            &memory_mib.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, 512.0)
                .step(1.0)
                .suffix("MiB"),
            cx,
        );
    }

    fn ensure_security_inputs(&mut self, cx: &mut Context<Self>) {
        let master_password_draft = self.settings.master_password().draft.to_owned();
        self.ensure_text_input(
            "settings.security.master-password",
            &master_password_draft,
            crate::features::text_inputs::secret_input_setup(true),
            cx,
        );
        let idle_minutes = self.settings.summary().idle_lock_minutes;
        self.ensure_number_input(
            "settings.number.idle-lock-minutes",
            &idle_minutes.to_string(),
            NyaNumberInputOptions::default()
                .range(0.0, 1440.0)
                .step(1.0)
                .suffix(t!("common.minutes").to_string()),
            cx,
        );
    }

    fn ensure_ai_general_inputs(&mut self, cx: &mut Context<Self>) {
        self.ensure_ai_settings_inputs(cx);
        let config = self.ai.settings_config().clone();
        for (id, value, min, max, step) in [
            (
                "ai.number.context-line-limit",
                config.context_line_limit as f64,
                50.0,
                500.0,
                50.0,
            ),
            (
                "ai.number.timeout-ms",
                config.timeout_ms as f64,
                5_000.0,
                300_000.0,
                1_000.0,
            ),
            (
                "ai.number.agent-steps",
                config.max_agent_steps.unwrap_or(10) as f64,
                1.0,
                50.0,
                1.0,
            ),
            (
                "ai.number.agent-step-timeout-ms",
                config.agent_step_timeout_ms.unwrap_or(30_000) as f64,
                5_000.0,
                120_000.0,
                1_000.0,
            ),
            (
                "ai.number.terminal-output-lines",
                config.terminal_output_lines as f64,
                0.0,
                100.0,
                1.0,
            ),
        ] {
            self.ensure_number_input(
                id,
                &format!("{}", value as i64),
                NyaNumberInputOptions::default().range(min, max).step(step),
                cx,
            );
        }
    }

    fn ensure_ai_model_inputs(&mut self, cx: &mut Context<Self>) {
        let credential_ids: Vec<String> = self
            .ai
            .settings_config()
            .provider_credentials
            .iter()
            .map(|credential| credential.id.clone())
            .collect();
        for credential_id in credential_ids {
            self.ensure_ai_credential_inputs(&credential_id, cx);
        }
        let query = self.ai.settings_model_query().to_string();
        self.ensure_text_input(
            "ai.settings.model-search",
            &query,
            TextInputSetup::placeholder(t!("ai.searchModels")),
            cx,
        );
    }

    /// Build the inputs a provider credential row draws.
    ///
    /// Three fields for the credential itself, plus the manual-model box its group
    /// draws: `ai_model_groups` builds one group per enabled credential, keyed by the
    /// credential id, and looks that box up whether or not the group is expanded.
    /// Adding a credential reveals all four, so the add is also a boundary that builds
    /// them; keep the ids in step with the lookups in `ai::models`.
    pub(in crate::features) fn ensure_ai_credential_inputs(
        &mut self,
        credential_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(credential) = self
            .ai
            .settings_config()
            .provider_credentials
            .iter()
            .find(|credential| credential.id == credential_id)
            .cloned()
        else {
            return;
        };
        let secret = self
            .ai
            .settings_credential_secret_drafts()
            .get(credential_id)
            .cloned()
            .unwrap_or_default();
        for (suffix, value, secret_field) in [
            ("name", credential.name.clone(), false),
            (
                "base-url",
                credential.base_url.clone().unwrap_or_default(),
                false,
            ),
            ("api-key", secret, true),
        ] {
            self.ensure_text_input(
                format!("ai.credential.{credential_id}.{suffix}"),
                &value,
                secret_input_setup(secret_field),
                cx,
            );
        }
        let manual_draft = self.ai.settings_manual_model_draft(credential_id);
        self.ensure_text_input(
            format!("ai.settings.manual-model.{credential_id}"),
            &manual_draft,
            TextInputSetup::placeholder(t!("ai.manualModelPlaceholder").to_string()),
            cx,
        );
    }

    fn ensure_ai_rule_inputs(&mut self, cx: &mut Context<Self>) {
        // One name and one prompt per action, all drawn together.
        for (kind, action_id) in self.ai_action_ids() {
            self.ensure_ai_action_inputs(kind, &action_id, cx);
        }
        let file_size_mb =
            (self.ai.settings_config().max_ai_file_size_bytes / (1024 * 1024)).max(1);
        self.ensure_number_input(
            "ai.number.file-size-mb",
            &file_size_mb.to_string(),
            NyaNumberInputOptions::default().range(1.0, 256.0).step(1.0),
            cx,
        );
    }

    /// Build the name and prompt inputs one action row draws.
    ///
    /// Adding an action reveals a row, so the add is also a boundary that builds them;
    /// keep the ids in step with the lookups in `ai::rules`.
    pub(in crate::features) fn ensure_ai_action_inputs(
        &mut self,
        kind: AiActionListKind,
        action_id: &str,
        cx: &mut Context<Self>,
    ) {
        let actions = match kind {
            AiActionListKind::Terminal => self.ai.settings_config().terminal_ai_actions.clone(),
            AiActionListKind::File => self.ai.settings_config().file_ai_actions.clone(),
        };
        let Some(action) = actions.iter().find(|action| action.id == action_id) else {
            return;
        };
        let name = action.name.clone();
        let prompt = action.prompt.clone();
        self.ensure_text_input(
            Self::ai_action_text_input_id(kind, action_id, AiActionEditorField::Name),
            &name,
            TextInputSetup::placeholder(""),
            cx,
        );
        self.ensure_text_input(
            Self::ai_action_text_input_id(kind, action_id, AiActionEditorField::Prompt),
            &prompt,
            TextInputSetup::multi_line(""),
            cx,
        );
    }
}

/// Every tab, so the tests below cannot silently miss one that gets added.
#[cfg(test)]
pub(in crate::features::pages::settings) const ALL_SETTINGS_TABS: [SettingsTab; 13] = [
    SettingsTab::General,
    SettingsTab::Appearance,
    SettingsTab::Interaction,
    SettingsTab::Keybindings,
    SettingsTab::TerminalGeneral,
    SettingsTab::Search,
    SettingsTab::Translation,
    SettingsTab::AiGeneral,
    SettingsTab::AiModels,
    SettingsTab::AiRules,
    SettingsTab::Transfer,
    SettingsTab::Security,
    SettingsTab::SyncBackup,
];

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gpui::{
        AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div,
    };
    use nyaterm_core::{AppRuntime, RuntimeMode};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::{AiActionEditorField, AiActionListKind, NavItem, SettingsTab};
    use crate::test_support::TestConfigDir;

    use super::ALL_SETTINGS_TABS;

    fn app(cx: &mut TestAppContext, root: &Path) -> Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.to_path_buf(),
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

    fn hosted(cx: &mut TestAppContext, root: &Path) -> Entity<NyaTermApp> {
        let app = app(cx, root);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            app.open_page(NavItem::Settings, cx);
            app.flush_settings_panel_snapshots(cx);
        });
        app
    }

    /// Activating a tab must build its app-owned inputs and publish those handles
    /// into the panel snapshot. Rendering is covered by the hosted SettingsPanel
    /// tests; this pins the ownership boundary that render relies on.
    #[test]
    fn every_settings_tab_builds_its_inputs_and_snapshot_handles() {
        let test_dir = TestConfigDir::new("nyaterm-settings-inputs");
        let mut cx = TestAppContext::single();
        let app = hosted(&mut cx, test_dir.path());

        for tab in ALL_SETTINGS_TABS {
            cx.update_entity(&app, |app, cx| {
                app.ensure_settings_tab_inputs(tab, cx);
                app.shell.set_settings_active_tab(tab);
                app.request_settings_panel_refresh(cx);
                app.flush_settings_panel_snapshots(cx);
            });
            let after_first = cx.update_entity(&app, |app, cx| {
                app.settings_panel
                    .read(cx)
                    .snapshot()
                    .expect("tab snapshot")
                    .text_inputs
                    .len()
            });
            cx.update_entity(&app, |app, cx| {
                app.flush_settings_panel_snapshots(cx);
            });
            assert_eq!(
                cx.update_entity(&app, |app, cx| {
                    app.settings_panel
                        .read(cx)
                        .snapshot()
                        .expect("tab snapshot")
                        .text_inputs
                        .len()
                }),
                after_first,
                "flushing the {tab:?} tab again changed its input handle set"
            );
        }
    }

    /// Static tab inputs outlive the page; only the row-scoped ones are released.
    ///
    /// `finish_settings_page` forgets exactly three prefixes -- `ai.settings.action.`,
    /// `ai.settings.manual-model.` and `keyword.highlight.` -- and nothing else, so a
    /// number or text input belonging to a tab lives for as long as the app does. That
    /// predates this change: they used to be created by the first render of their tab
    /// and were never forgotten either. Pinning it here so the panel batch, which will
    /// carry these handles in a snapshot, does not mistake it for a leak.
    ///
    /// The release half of the lifecycle is proven by these tests reaching teardown at
    /// all: `TestAppContext` fails on any entity still reachable when it drops, so a
    /// clean exit is the assertion that nothing but the app retains them.
    #[test]
    fn static_settings_inputs_outlive_the_page() {
        let test_dir = TestConfigDir::new("nyaterm-settings-inputs");
        let mut cx = TestAppContext::single();
        let app = hosted(&mut cx, test_dir.path());

        cx.update_entity(&app, |app, cx| {
            app.ensure_settings_tab_inputs(SettingsTab::Security, cx);
            app.shell.set_settings_active_tab(SettingsTab::Security);
            app.request_settings_panel_refresh(cx);
            app.flush_settings_panel_snapshots(cx);
        });

        let probe = cx.update_entity(&app, |app, _| {
            app.existing_text_input("settings.security.master-password")
                .expect("activation built the master-password input")
                .downgrade()
        });

        cx.update_entity(&app, |app, cx| {
            app.cancel_settings(cx);
            app.flush_settings_panel_snapshots(cx);
        });

        assert!(
            probe.upgrade().is_some(),
            "a static tab input is app-lifetime; page close must not release it"
        );
    }

    /// A window with nothing in it, so a `Window` can be handed to actions that need
    /// one without mounting the settings panel.
    ///
    /// Painting a settings tab that holds a registry input never parks -- see the
    /// ignored `every_settings_surface_draws_only_inputs_it_built` in `panel.rs` -- so
    /// these boundary tests assert on the published snapshot instead of on a paint.
    struct BareHost;

    impl Render for BareHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn flush(app: &Entity<NyaTermApp>, vcx: &mut VisualTestContext) {
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| app.flush_settings_panel_snapshots(cx));
        });
    }

    #[track_caller]
    fn assert_built(
        app: &Entity<NyaTermApp>,
        vcx: &mut VisualTestContext,
        text: &[&str],
        numbers: &[&str],
    ) {
        flush(app, vcx);
        vcx.update(|_, cx| {
            let panel = app.read(cx).settings_panel.read(cx);
            let snapshot = panel.snapshot().expect("the panel has a snapshot");
            for id in text {
                assert!(
                    snapshot.text_inputs.contains_key(*id),
                    "{id} is drawn by its row but no boundary built it"
                );
            }
            for id in numbers {
                assert!(
                    snapshot.number_inputs.contains_key(*id),
                    "{id} is drawn by its row but no boundary built it"
                );
            }
        });
    }

    #[track_caller]
    fn assert_released(app: &Entity<NyaTermApp>, vcx: &mut VisualTestContext, text: &[&str]) {
        flush(app, vcx);
        vcx.update(|_, cx| {
            let panel = app.read(cx).settings_panel.read(cx);
            let snapshot = panel.snapshot().expect("the panel has a snapshot");
            for id in text {
                assert!(
                    !snapshot.text_inputs.contains_key(*id),
                    "{id} outlived the row that revealed it"
                );
            }
        });
    }

    /// Every boundary that reveals a row must build the inputs that row draws.
    ///
    /// The tab-level list is covered by `ensure_settings_tab_inputs`; these are the
    /// four boundaries that reveal a field without changing tab, each of which used to
    /// leave the row's inputs unbuilt so it drew an empty box (and tripped a
    /// `debug_assert!` in a debug build).
    #[test]
    fn reveal_boundaries_build_the_inputs_their_rows_draw() {
        let test_dir = TestConfigDir::new("nyaterm-settings-inputs");
        let mut cx = TestAppContext::single();
        let app = hosted(&mut cx, test_dir.path());
        let (_, vcx) = cx.add_window_view(|_, _| BareHost);
        let vcx: &mut VisualTestContext = vcx;

        // A custom engine row. The ids carry the row index, so adding or removing an
        // engine forgets the whole prefix and whatever row is open must be rebuilt.
        let engine_ids = [
            "settings.search-engine.0.name",
            "settings.search-engine.0.url",
        ];
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.focus_settings_tab(SettingsTab::Search, cx)
            });
        });
        assert_released(&app, vcx, &engine_ids);
        vcx.update(|_, cx| app.update(cx, |app, cx| app.add_search_engine(cx)));
        assert_built(&app, vcx, &engine_ids, &[]);
        vcx.update(|_, cx| app.update(cx, |app, cx| app.expand_search_engine(0, cx)));
        assert_released(&app, vcx, &engine_ids);
        vcx.update(|_, cx| app.update(cx, |app, cx| app.expand_search_engine(0, cx)));
        assert_built(&app, vcx, &engine_ids, &[]);

        // A freshly added provider credential draws three fields.
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.focus_settings_tab(SettingsTab::AiModels, cx)
            });
        });
        vcx.update(|window, cx| app.update(cx, |app, cx| app.add_ai_credential(window, cx)));
        let credential_id = vcx.update(|_, cx| {
            app.read(cx)
                .ai
                .settings_config()
                .provider_credentials
                .last()
                .expect("the credential was added")
                .id
                .clone()
        });
        assert_built(
            &app,
            vcx,
            &[
                &format!("ai.credential.{credential_id}.name"),
                &format!("ai.credential.{credential_id}.base-url"),
                &format!("ai.credential.{credential_id}.api-key"),
            ],
            &[],
        );

        // A freshly added action draws a name and a prompt, in both lists.
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.focus_settings_tab(SettingsTab::AiRules, cx)
            });
        });
        for kind in [AiActionListKind::Terminal, AiActionListKind::File] {
            vcx.update(|window, cx| app.update(cx, |app, cx| app.add_ai_action(kind, window, cx)));
            let action_id = vcx.update(|_, cx| {
                let config = app.read(cx).ai.settings_config().clone();
                let actions = match kind {
                    AiActionListKind::Terminal => config.terminal_ai_actions,
                    AiActionListKind::File => config.file_ai_actions,
                };
                actions.last().expect("the action was added").id.clone()
            });
            assert_built(
                &app,
                vcx,
                &[
                    &NyaTermApp::ai_action_text_input_id(
                        kind,
                        &action_id,
                        AiActionEditorField::Name,
                    ),
                    &NyaTermApp::ai_action_text_input_id(
                        kind,
                        &action_id,
                        AiActionEditorField::Prompt,
                    ),
                ],
                &[],
            );
        }

        // Enabling cloud sync without a master password redirects to Security, and that
        // jump is neither `open_page` nor the tab strip. Security has not been visited
        // above, so its inputs only exist if the jump built them.
        vcx.update(|_, cx| app.update(cx, |app, cx| app.toggle_cloud_sync_enabled(cx)));
        vcx.update(|_, cx| {
            assert_eq!(
                app.read(cx).shell.settings_active_tab(),
                SettingsTab::Security,
                "enabling cloud sync without a master password must land on Security"
            );
        });
        assert_built(
            &app,
            vcx,
            &["settings.security.master-password"],
            &["settings.number.idle-lock-minutes"],
        );
    }
}
