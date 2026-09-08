//! Application settings document persistence.
//!
//! Split out of `storage.rs` by domain. Every method here reads or writes the
//! single app settings document and returns the refreshed
//! [`AppSettingsSummary`]; document keys, field names and defaults are
//! unchanged.

use std::collections::HashMap;

use super::{
    ConnectionStore, LEGACY_TEXT_MASTER_KEY, META_MASTER_KEY, META_TABLE, SETTINGS_DEFAULT,
    SETTINGS_TABLE, StorageError, TEXT_DOCS_TABLE, json_bool, json_path, json_string_vec,
    set_nested_json_value, write_json_in_txn,
};
use zzclawterm_core::{
    AppSettingsSummary, CredentialCrypto, DEFAULT_RECORDING_PATH_TEMPLATE,
    DEFAULT_TERMINAL_TIMESTAMP_FORMAT, ExistingFileBehavior, RecordingMode,
    RecordingRotationPolicy, SearchEngineConfig, default_panel_open_mode, default_search_engines,
    normalize_panel_open_mode,
};

impl ConnectionStore {
    pub fn load_app_settings_summary(&self) -> Result<AppSettingsSummary, StorageError> {
        let value = self.load_settings_value()?;
        Ok(AppSettingsSummary {
            theme: json_string(&value, &["appearance", "theme"], "github-dark"),
            background_image_path: json_path(&value, &["appearance", "background_image_path"])
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            background_image_fit: json_string(
                &value,
                &["appearance", "background_image_fit"],
                "cover",
            ),
            background_image_opacity: {
                let raw = json_path(&value, &["appearance", "background_image_opacity"]);
                raw.and_then(|v| v.as_f64())
                    .map(|v| {
                        // Accept both 0..1 float (Tauri) and 0..100 percent.
                        if v <= 1.0 {
                            (v * 100.0).round() as u8
                        } else {
                            v.round() as u8
                        }
                    })
                    .unwrap_or(45)
                    .clamp(0, 100)
            },
            background_content_opacity: {
                let raw = json_path(&value, &["appearance", "background_opacity"]);
                raw.and_then(|v| v.as_f64())
                    .map(|v| {
                        if v <= 1.0 {
                            (v * 100.0).round() as u8
                        } else {
                            v.round() as u8
                        }
                    })
                    .unwrap_or(82)
                    .clamp(0, 100)
            },
            // Tauri UiConfig.language (not translation.target_language).
            language: json_string(&value, &["ui", "language"], "en"),
            terminal_font_family: json_string(
                &value,
                &["appearance", "font_family"],
                "JetBrains Mono",
            ),
            terminal_font_size: json_u16(&value, &["appearance", "font_size"], 16),
            cursor_style: {
                let raw = json_string(&value, &["appearance", "cursor_style"], "block");
                match raw.as_str() {
                    "underline" | "bar" | "block" => raw,
                    _ => "block".to_string(),
                }
            },
            cursor_blink: json_bool(&value, &["appearance", "cursor_blink"], true),
            terminal_theme: {
                json_path(&value, &["appearance", "terminal_theme"])
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            },
            minimum_contrast_ratio: {
                let raw = json_path(&value, &["appearance", "minimum_contrast_ratio"]);
                let num = raw.and_then(|v| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                });
                match num {
                    Some(1.0) => "1".to_string(),
                    Some(3.0) => "3".to_string(),
                    Some(4.5) => "4.5".to_string(),
                    Some(7.0) => "7".to_string(),
                    Some(21.0) => "21".to_string(),
                    _ => "1".to_string(),
                }
            },
            ui_font_family: json_string(&value, &["appearance", "ui_font_family"], "Inter"),
            ui_font_size: json_u16(&value, &["appearance", "ui_font_size"], 16).clamp(12, 24),
            terminal_font_weight: {
                let w = json_u16(&value, &["appearance", "font_weight"], 400);
                match w {
                    300 | 400 | 500 | 600 | 700 | 800 => w,
                    _ => 400,
                }
            },
            terminal_font_weight_bold: {
                let w = json_u16(&value, &["appearance", "font_weight_bold"], 700);
                match w {
                    300 | 400 | 500 | 600 | 700 | 800 => w,
                    _ => 700,
                }
            },
            x11_display: json_string(&value, &["terminal", "x11_display"], ""),
            terminal_scrollback_lines: json_u32(&value, &["terminal", "scrollback_lines"], 5000)
                .clamp(100, 100_000),
            terminal_keep_alive_mode: normalize_keep_alive_mode(&json_string(
                &value,
                &["terminal", "keep_alive_mode"],
                "compatible",
            )),
            terminal_keep_alive_interval: json_u32(
                &value,
                &["terminal", "keep_alive_interval"],
                30,
            )
            .min(600),
            terminal_timestamp_format: normalize_timestamp_format(&json_string(
                &value,
                &["terminal", "timestamp_format"],
                DEFAULT_TERMINAL_TIMESTAMP_FORMAT,
            )),
            terminal_hardware_acceleration: json_bool(
                &value,
                &["terminal", "hardware_acceleration"],
                true,
            ),
            terminal_show_workspace_padding: json_bool(
                &value,
                &["terminal", "show_workspace_padding"],
                false,
            ),
            terminal_show_line_numbers: json_bool(
                &value,
                &["terminal", "show_line_numbers"],
                false,
            ),
            terminal_show_timestamps: json_bool(&value, &["terminal", "show_timestamps"], false),
            terminal_show_multi_line_paste_dialog: json_bool(
                &value,
                &["terminal", "show_multi_line_paste_dialog"],
                true,
            ),
            terminal_paste_image_as_path: json_bool(
                &value,
                &["terminal", "paste_image_as_path"],
                true,
            ),
            terminal_reconnect_restore_cwd: json_bool(
                &value,
                &["terminal", "reconnect_restore_cwd"],
                false,
            ),
            terminal_low_latency_mode: json_bool(&value, &["terminal", "low_latency_mode"], false),
            terminal_zebra_stripes_enabled: json_bool(
                &value,
                &["terminal", "zebra_stripes_enabled"],
                true,
            ),
            terminal_action_links_enabled: json_bool(
                &value,
                &["terminal", "action_links_enabled"],
                false,
            ),
            terminal_action_links_matchers: load_action_links_matchers(&value),
            search_custom_engines: load_search_engines(&value),
            ui_show_notes_panel: json_bool(&value, &["ui", "show_notes_panel"], true),
            ui_show_remote_stats: json_bool(&value, &["ui", "show_remote_stats"], true),
            ui_remote_stats_interval: json_u32(&value, &["ui", "remote_stats_interval"], 3)
                .clamp(1, 60),
            ui_show_gpu_monitor: json_bool(&value, &["ui", "show_gpu_monitor"], false),
            ui_gpu_monitor_interval: json_u32(&value, &["ui", "gpu_monitor_interval"], 3)
                .clamp(3, 120),
            ui_show_ascend_npu_monitor: json_bool(
                &value,
                &["ui", "show_ascend_npu_monitor"],
                false,
            ),
            ui_ascend_npu_monitor_interval: json_u32(
                &value,
                &["ui", "ascend_npu_monitor_interval"],
                3,
            )
            .clamp(3, 120),
            ui_show_process_manager: json_bool(&value, &["ui", "show_process_manager"], true),
            ui_process_manager_interval: json_u32(&value, &["ui", "process_manager_interval"], 5)
                .clamp(3, 120),
            ui_show_docker_manager: json_bool(&value, &["ui", "show_docker_manager"], true),
            ui_docker_manager_interval: json_u32(&value, &["ui", "docker_manager_interval"], 10)
                .clamp(3, 120),
            ui_quick_cmd_view_mode: normalize_quick_cmd_view_mode(&json_string(
                &value,
                &["ui", "quick_cmd_view_mode"],
                "tile",
            )),
            ui_quick_cmd_sort_mode: normalize_quick_cmd_sort_mode(&json_string(
                &value,
                &["ui", "quick_cmd_sort_mode"],
                "created",
            )),
            ui_saved_connections_sort_mode: normalize_saved_connections_sort_mode(&json_string(
                &value,
                &["ui", "saved_connections_sort_mode"],
                "default",
            )),
            ui_saved_connections_expanded_group_ids: json_string_vec(
                &value,
                &["ui", "saved_connections_expanded_group_ids"],
                512,
            ),
            ui_start_workspace_mode: normalize_start_workspace_mode(&json_string(
                &value,
                &["ui", "start_workspace_mode"],
                "workbench",
            )),
            ui_asset_sort_key: json_optional_string(&value, &["ui", "asset_sort_key"]),
            ui_asset_sort_direction: normalize_asset_sort_direction(&json_optional_string(
                &value,
                &["ui", "asset_sort_direction"],
            )),
            ui_header_status_mode: normalize_header_status_mode(&json_string(
                &value,
                &["ui", "header_status_mode"],
                "session",
            )),
            ui_header_status_visible: json_bool(&value, &["ui", "header_status_visible"], true),
            ui_file_explorer_show_hidden_files: json_bool(
                &value,
                &["ui", "file_explorer_show_hidden_files"],
                true,
            ),
            ui_file_explorer_auto_sync_cwd_connection_ids: json_string_vec(
                &value,
                &["ui", "file_explorer_auto_sync_cwd_connection_ids"],
                256,
            ),
            ui_file_explorer_favorite_dirs_by_connection_id: json_string_vec_map(
                &value,
                &["ui", "file_explorer_favorite_dirs_by_connection_id"],
                12,
            ),
            ui_left_panel_width: json_u32(&value, &["ui", "left_width"], 256).clamp(160, 720),
            ui_right_panel_width: json_u32(&value, &["ui", "right_width"], 288).clamp(200, 720),
            ui_transfer_height: json_u32(&value, &["ui", "transfer_height"], 180).clamp(60, 600),
            ui_quick_cmd_height: json_u32(&value, &["ui", "quick_cmd_height"], 180).clamp(36, 520),
            ui_quick_cmd_visible: json_bool(&value, &["ui", "show_quick_cmd_bar"], true),
            ui_serial_send_height: json_u32(&value, &["ui", "serial_send_height"], 180)
                .clamp(60, 520),
            ui_serial_send_visible: json_bool(&value, &["ui", "show_serial_send_panel"], false),
            ui_active_left_panel: json_optional_string(&value, &["ui", "active_left_panel"]),
            ui_active_right_panel: json_optional_string(&value, &["ui", "active_right_panel"]),
            ui_left_panel_collapsed: json_bool(&value, &["ui", "left_panel_collapsed"], false),
            ui_right_panel_collapsed: json_bool(&value, &["ui", "right_panel_collapsed"], false),
            ui_activity_bar_left_top: {
                json_string_vec_with_default(
                    &value,
                    &["ui", "activity_bar_layout", "left_top"],
                    32,
                    default_activity_left_top,
                )
            },
            ui_activity_bar_left_bottom: {
                json_string_vec_with_default(
                    &value,
                    &["ui", "activity_bar_layout", "left_bottom"],
                    32,
                    default_activity_left_bottom,
                )
            },
            ui_activity_bar_right_top: {
                json_string_vec_with_default(
                    &value,
                    &["ui", "activity_bar_layout", "right_top"],
                    32,
                    default_activity_right_top,
                )
            },
            ui_activity_bar_right_bottom: {
                json_string_vec_with_default(
                    &value,
                    &["ui", "activity_bar_layout", "right_bottom"],
                    32,
                    default_activity_right_bottom,
                )
            },
            ui_activity_bar_show_labels: json_bool(
                &value,
                &["ui", "activity_bar_layout", "show_labels"],
                false,
            ),
            ui_activity_bar_hidden_items: json_string_vec(
                &value,
                &["ui", "activity_bar_layout", "hidden_items"],
                64,
            ),
            ui_panel_multi_open: json_bool(&value, &["appearance", "panel_multi_open"], false)
                || json_bool(&value, &["ui", "panel_multi_open"], false),
            ui_panel_open_mode: normalize_panel_open_mode(
                &json_optional_string(&value, &["ui", "panel_open_mode"])
                    .unwrap_or_else(default_panel_open_mode),
            ),
            ui_left_open_panels: json_string_vec(&value, &["ui", "left_open_panels"], 32),
            ui_right_open_panels: json_string_vec(&value, &["ui", "right_open_panels"], 32),
            ui_panel_stack_sizes: json_u32_map(&value, &["ui", "panel_stack_sizes"]),
            interaction_copy_on_select: json_bool(
                &value,
                &["interaction", "copy_on_select"],
                false,
            ),
            interaction_allow_osc52_clipboard_write: json_bool(
                &value,
                &["interaction", "allow_osc52_clipboard_write"],
                false,
            ),
            interaction_right_click_paste: json_bool(
                &value,
                &["interaction", "right_click_paste"],
                false,
            ),
            interaction_terminal_zoom_enabled: json_bool(
                &value,
                &["interaction", "terminal_zoom_enabled"],
                true,
            ),
            interaction_command_suggestions_enabled: json_bool(
                &value,
                &["interaction", "command_suggestions_enabled"],
                true,
            ),
            interaction_command_suggestion_min_chars: json_u32(
                &value,
                &["interaction", "command_suggestion_min_chars"],
                2,
            )
            .clamp(1, 500),
            interaction_command_suggestion_max_chars: json_u32(
                &value,
                &["interaction", "command_suggestion_max_chars"],
                64,
            )
            .clamp(1, 500),
            interaction_word_separators: json_string(
                &value,
                &["interaction", "word_separators"],
                " \t\r\n\"'`~!@#$%^&*()-=+[{]}\\|;:,<.>/?",
            ),
            interaction_duplicate_session_command_delay_ms: json_u32(
                &value,
                &["interaction", "duplicate_session_command_delay_ms"],
                1000,
            )
            .min(60_000),
            interaction_alt_as_meta: json_bool(&value, &["interaction", "alt_as_meta"], false),
            interaction_mac_ime_compatibility: json_bool(
                &value,
                &["interaction", "mac_ime_compatibility"],
                true,
            ),
            interaction_tab_double_click_action: normalize_tab_mouse_action(&json_string(
                &value,
                &["interaction", "tab_double_click_action"],
                "disconnect_session",
            )),
            interaction_tab_middle_click_action: normalize_tab_mouse_action(&json_string(
                &value,
                &["interaction", "tab_middle_click_action"],
                "rename_tab",
            )),
            interaction_tab_right_click_action: normalize_tab_mouse_action(&json_string(
                &value,
                &["interaction", "tab_right_click_action"],
                "none",
            )),
            interaction_default_encoding: normalize_interaction_encoding(&json_string(
                &value,
                &["interaction", "default_encoding"],
                "UTF-8",
            )),
            host_key_policy: normalize_host_key_policy(&json_string(
                &value,
                &["security", "host_key_policy"],
                "prompt",
            )),
            transfer_download_path: json_string(&value, &["transfer", "download_path"], ""),
            transfer_ask_save_location: json_bool(
                &value,
                &["transfer", "ask_save_location"],
                false,
            ),
            transfer_duplicate_strategy: normalize_transfer_duplicate_strategy(&json_string(
                &value,
                &["transfer", "duplicate_strategy"],
                "ask",
            )),
            transfer_editor_type: normalize_transfer_editor_type(&json_string(
                &value,
                &["transfer", "editor_type"],
                "external",
            )),
            transfer_internal_editor_font_size: json_u16(
                &value,
                &["transfer", "internal_editor_font_size"],
                13,
            )
            .clamp(8, 72),
            transfer_default_editor: json_string(&value, &["transfer", "default_editor"], ""),
            transfer_download_threads: json_u32(&value, &["transfer", "download_threads"], 3)
                .clamp(1, 10),
            transfer_upload_threads: json_u32(&value, &["transfer", "upload_threads"], 3)
                .clamp(1, 10),
            transfer_max_retries: json_u32(&value, &["transfer", "max_transfer_retries"], 2)
                .min(10),
            transfer_buffer_size: json_u32(&value, &["transfer", "transfer_buffer_size"], 32)
                .clamp(8, 256),
            transfer_default_file_permissions: normalize_transfer_file_permissions(&json_string(
                &value,
                &["transfer", "default_file_permissions"],
                "644",
            )),
            transfer_preserve_timestamps: json_bool(
                &value,
                &["transfer", "preserve_timestamps"],
                true,
            ),
            transfer_resume_broken_transfer: json_bool(
                &value,
                &["transfer", "resume_broken_transfer"],
                true,
            ),
            recording_path: json_string(&value, &["recording", "base_path"], ""),
            recording_auto_start: json_bool(&value, &["recording", "auto_start"], false),
            recording_default_mode: load_recording_mode(&value),
            recording_path_template: normalize_recording_path_template(&json_string(
                &value,
                &["recording", "path_template"],
                DEFAULT_RECORDING_PATH_TEMPLATE,
            )),
            recording_include_io_labels: json_bool(
                &value,
                &["recording", "include_io_labels"],
                true,
            ),
            recording_include_timestamps: json_bool(
                &value,
                &["recording", "include_timestamps"],
                true,
            ),
            recording_include_session_metadata: json_bool(
                &value,
                &["recording", "include_session_metadata"],
                true,
            ),
            recording_rotation: load_recording_rotation(&value),
            recording_existing_file_behavior: load_existing_file_behavior(&value),
            recording_include_binary_transfer_payloads: json_bool(
                &value,
                &["recording", "include_binary_transfer_payloads"],
                false,
            ),
            recording_memory_limit_bytes: json_u64(
                &value,
                &["recording", "memory_limit_bytes"],
                5 * 1024 * 1024,
            ),
            diagnostics_level: {
                let raw = json_string(&value, &["diagnostics", "level"], "info");
                match raw.as_str() {
                    "warn" | "debug" => raw,
                    _ => "info".to_string(),
                }
            },
            diagnostics_retention_days: {
                let days = u32::from(json_u16(&value, &["diagnostics", "retention_days"], 7));
                match days {
                    3 | 7 | 14 | 30 => days,
                    _ => 7,
                }
            },
            startup_restore: json_bool(&value, &["general", "startup_restore"], false),
            startup_restore_window_layout: json_bool(
                &value,
                &["general", "startup_restore_window_layout"],
                true,
            ),
            minimize_to_tray: json_bool(&value, &["general", "minimize_to_tray"], false),
            confirm_on_close: json_bool(&value, &["general", "confirm_on_close"], true),
            enable_screen_lock: json_bool(&value, &["security", "enable_screen_lock"], false),
            idle_lock_minutes: u32::from(json_u16(&value, &["security", "idle_lock_minutes"], 0)),
            has_master_password: value
                .get("security")
                .and_then(|security| security.get("master_password"))
                .and_then(|master_password| master_password.as_str())
                .is_some_and(|master_password| !master_password.is_empty()),
            keybindings: json_string_map(&value, &["keybindings"]),
        })
    }

    pub fn verify_master_password(&self, password: &str) -> Result<bool, StorageError> {
        let Some(token) = self.load_encrypted_master_password()? else {
            return Ok(true);
        };
        let bootstrap = CredentialCrypto::new(self.portable_key_path.clone(), None);
        let stored = bootstrap.decrypt_settings_secret(&token)?;
        Ok(stored == password)
    }

    pub fn save_master_password(
        &self,
        next_password: Option<&str>,
    ) -> Result<AppSettingsSummary, StorageError> {
        if next_password.is_some_and(str::is_empty) {
            return Err(StorageError::InvalidData(
                "Master password cannot be empty when enabled".to_string(),
            ));
        }

        let bootstrap = CredentialCrypto::new(self.portable_key_path.clone(), None);
        let current_password = self
            .load_encrypted_master_password()?
            .map(|token| bootstrap.decrypt_settings_secret(&token))
            .transpose()?
            .map(Into::into);
        let rewrapped_master_key = self
            .load_master_key_token()?
            .map(|token| {
                CredentialCrypto::new(self.portable_key_path.clone(), current_password.clone())
                    .rewrap_master_key_token(&token, next_password)
            })
            .transpose()?;

        let mut value = self.load_settings_value()?;
        let encoded_password = next_password
            .map(|password| bootstrap.encrypt_settings_secret(password))
            .transpose()?;
        set_nested_json_value(
            &mut value,
            &["security", "master_password"],
            encoded_password
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );

        let txn = self.db.begin_write()?;
        write_json_in_txn(&txn, SETTINGS_TABLE, SETTINGS_DEFAULT, &value)?;
        if let Some(token) = rewrapped_master_key.as_deref() {
            txn.open_table(META_TABLE)?.insert(META_MASTER_KEY, token)?;
            txn.open_table(TEXT_DOCS_TABLE)?
                .insert(LEGACY_TEXT_MASTER_KEY, token)?;
        }
        txn.commit()?;
        self.load_app_settings_summary()
    }

    pub fn save_host_key_policy(&self, policy: &str) -> Result<AppSettingsSummary, StorageError> {
        let policy = normalize_host_key_policy(policy);
        let mut value = self.load_settings_value()?;
        set_nested_json_string(&mut value, &["security", "host_key_policy"], policy);
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_recording_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_string(
            &mut value,
            &["recording", "base_path"],
            settings.recording_path.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "auto_start"],
            serde_json::Value::Bool(settings.recording_auto_start),
        );
        set_nested_json_string(
            &mut value,
            &["recording", "default_mode"],
            recording_mode_value(settings.recording_default_mode).to_string(),
        );
        set_nested_json_string(
            &mut value,
            &["recording", "path_template"],
            normalize_recording_path_template(&settings.recording_path_template),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "include_io_labels"],
            serde_json::Value::Bool(settings.recording_include_io_labels),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "include_timestamps"],
            serde_json::Value::Bool(settings.recording_include_timestamps),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "include_session_metadata"],
            serde_json::Value::Bool(settings.recording_include_session_metadata),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "rotation"],
            recording_rotation_value(&settings.recording_rotation),
        );
        set_nested_json_string(
            &mut value,
            &["recording", "existing_file_behavior"],
            existing_file_behavior_value(settings.recording_existing_file_behavior).to_string(),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "include_binary_transfer_payloads"],
            serde_json::Value::Bool(settings.recording_include_binary_transfer_payloads),
        );
        set_nested_json_value(
            &mut value,
            &["recording", "memory_limit_bytes"],
            serde_json::Value::from(settings.recording_memory_limit_bytes),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_transfer_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_string(
            &mut value,
            &["transfer", "download_path"],
            settings.transfer_download_path.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "ask_save_location"],
            serde_json::Value::Bool(settings.transfer_ask_save_location),
        );
        set_nested_json_string(
            &mut value,
            &["transfer", "duplicate_strategy"],
            normalize_transfer_duplicate_strategy(&settings.transfer_duplicate_strategy),
        );
        set_nested_json_string(
            &mut value,
            &["transfer", "editor_type"],
            normalize_transfer_editor_type(&settings.transfer_editor_type),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "internal_editor_font_size"],
            serde_json::Value::from(settings.transfer_internal_editor_font_size.clamp(8, 72)),
        );
        set_nested_json_string(
            &mut value,
            &["transfer", "default_editor"],
            settings.transfer_default_editor.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "download_threads"],
            serde_json::Value::from(settings.transfer_download_threads.clamp(1, 10)),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "upload_threads"],
            serde_json::Value::from(settings.transfer_upload_threads.clamp(1, 10)),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "max_transfer_retries"],
            serde_json::Value::from(settings.transfer_max_retries.min(10)),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "transfer_buffer_size"],
            serde_json::Value::from(settings.transfer_buffer_size.clamp(8, 256)),
        );
        set_nested_json_string(
            &mut value,
            &["transfer", "default_file_permissions"],
            normalize_transfer_file_permissions(&settings.transfer_default_file_permissions),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "preserve_timestamps"],
            serde_json::Value::Bool(settings.transfer_preserve_timestamps),
        );
        set_nested_json_value(
            &mut value,
            &["transfer", "resume_broken_transfer"],
            serde_json::Value::Bool(settings.transfer_resume_broken_transfer),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_file_explorer_favorite_dirs(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_value(
            &mut value,
            &["ui", "file_explorer_show_hidden_files"],
            serde_json::Value::Bool(settings.ui_file_explorer_show_hidden_files),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "file_explorer_auto_sync_cwd_connection_ids"],
            string_vec_json_value(&settings.ui_file_explorer_auto_sync_cwd_connection_ids, 256),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "file_explorer_favorite_dirs_by_connection_id"],
            string_vec_map_json_value(
                &settings.ui_file_explorer_favorite_dirs_by_connection_id,
                12,
            ),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_quick_command_ui_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_string(
            &mut value,
            &["ui", "quick_cmd_view_mode"],
            normalize_quick_cmd_view_mode(&settings.ui_quick_cmd_view_mode),
        );
        set_nested_json_string(
            &mut value,
            &["ui", "quick_cmd_sort_mode"],
            normalize_quick_cmd_sort_mode(&settings.ui_quick_cmd_sort_mode),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_ui_layout_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_value(
            &mut value,
            &["ui", "left_width"],
            serde_json::Value::from(settings.ui_left_panel_width.clamp(160, 720)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "right_width"],
            serde_json::Value::from(settings.ui_right_panel_width.clamp(200, 720)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "transfer_height"],
            serde_json::Value::from(settings.ui_transfer_height.clamp(60, 600)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "quick_cmd_height"],
            serde_json::Value::from(settings.ui_quick_cmd_height.clamp(36, 520)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_quick_cmd_bar"],
            serde_json::Value::Bool(settings.ui_quick_cmd_visible),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "serial_send_height"],
            serde_json::Value::from(settings.ui_serial_send_height.clamp(60, 520)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_serial_send_panel"],
            serde_json::Value::Bool(settings.ui_serial_send_visible),
        );
        match &settings.ui_active_left_panel {
            Some(panel) if !panel.trim().is_empty() => {
                set_nested_json_string(&mut value, &["ui", "active_left_panel"], panel.clone())
            }
            _ => set_nested_json_value(
                &mut value,
                &["ui", "active_left_panel"],
                serde_json::Value::Null,
            ),
        }
        match &settings.ui_active_right_panel {
            Some(panel) if !panel.trim().is_empty() => {
                set_nested_json_string(&mut value, &["ui", "active_right_panel"], panel.clone())
            }
            _ => set_nested_json_value(
                &mut value,
                &["ui", "active_right_panel"],
                serde_json::Value::Null,
            ),
        }
        set_nested_json_value(
            &mut value,
            &["ui", "left_panel_collapsed"],
            serde_json::Value::Bool(settings.ui_left_panel_collapsed),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "right_panel_collapsed"],
            serde_json::Value::Bool(settings.ui_right_panel_collapsed),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "activity_bar_layout", "left_top"],
            string_vec_json_value(&settings.ui_activity_bar_left_top, 32),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "activity_bar_layout", "left_bottom"],
            string_vec_json_value(&settings.ui_activity_bar_left_bottom, 32),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "activity_bar_layout", "right_top"],
            string_vec_json_value(&settings.ui_activity_bar_right_top, 32),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "activity_bar_layout", "right_bottom"],
            string_vec_json_value(&settings.ui_activity_bar_right_bottom, 32),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "activity_bar_layout", "show_labels"],
            serde_json::Value::Bool(settings.ui_activity_bar_show_labels),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "activity_bar_layout", "hidden_items"],
            string_vec_json_value(&settings.ui_activity_bar_hidden_items, 64),
        );
        let panel_open_mode = normalize_panel_open_mode(&settings.ui_panel_open_mode);
        set_nested_json_string(&mut value, &["ui", "panel_open_mode"], panel_open_mode);
        set_nested_json_value(
            &mut value,
            &["ui", "panel_multi_open"],
            serde_json::Value::Bool(settings.ui_panel_multi_open),
        );
        // Keep the appearance key used by current Tauri builds.
        set_nested_json_value(
            &mut value,
            &["appearance", "panel_multi_open"],
            serde_json::Value::Bool(settings.ui_panel_multi_open),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "left_open_panels"],
            string_vec_json_value(&settings.ui_left_open_panels, 32),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "right_open_panels"],
            string_vec_json_value(&settings.ui_right_open_panels, 32),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "panel_stack_sizes"],
            u32_map_json_value(&settings.ui_panel_stack_sizes),
        );
        set_nested_json_string(
            &mut value,
            &["ui", "saved_connections_sort_mode"],
            normalize_saved_connections_sort_mode(&settings.ui_saved_connections_sort_mode),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "saved_connections_expanded_group_ids"],
            string_vec_json_value(&settings.ui_saved_connections_expanded_group_ids, 512),
        );
        set_nested_json_string(
            &mut value,
            &["ui", "start_workspace_mode"],
            normalize_start_workspace_mode(&settings.ui_start_workspace_mode),
        );
        match &settings.ui_asset_sort_key {
            Some(key) if !key.trim().is_empty() => set_nested_json_string(
                &mut value,
                &["ui", "asset_sort_key"],
                key.trim().to_string(),
            ),
            _ => set_nested_json_value(
                &mut value,
                &["ui", "asset_sort_key"],
                serde_json::Value::Null,
            ),
        }
        match normalize_asset_sort_direction(&settings.ui_asset_sort_direction) {
            Some(direction) => {
                set_nested_json_string(&mut value, &["ui", "asset_sort_direction"], direction)
            }
            None => set_nested_json_value(
                &mut value,
                &["ui", "asset_sort_direction"],
                serde_json::Value::Null,
            ),
        }
        set_nested_json_string(
            &mut value,
            &["ui", "header_status_mode"],
            normalize_header_status_mode(&settings.ui_header_status_mode),
        );
        set_nested_json_bool(
            &mut value,
            &["ui", "header_status_visible"],
            settings.ui_header_status_visible,
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_appearance_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_string(&mut value, &["appearance", "theme"], settings.theme.clone());
        match settings
            .background_image_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(path) => set_nested_json_string(
                &mut value,
                &["appearance", "background_image_path"],
                path.to_string(),
            ),
            None => set_nested_json_value(
                &mut value,
                &["appearance", "background_image_path"],
                serde_json::Value::Null,
            ),
        }
        set_nested_json_string(
            &mut value,
            &["appearance", "background_image_fit"],
            settings.background_image_fit.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["appearance", "background_image_opacity"],
            serde_json::Value::from(settings.background_image_opacity as f64 / 100.0),
        );
        set_nested_json_value(
            &mut value,
            &["appearance", "background_opacity"],
            serde_json::Value::from(settings.background_content_opacity as f64 / 100.0),
        );
        set_nested_json_string(
            &mut value,
            &["appearance", "font_family"],
            settings.terminal_font_family.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["appearance", "font_size"],
            serde_json::Value::from(settings.terminal_font_size),
        );
        set_nested_json_string(
            &mut value,
            &["appearance", "cursor_style"],
            match settings.cursor_style.as_str() {
                "underline" | "bar" => settings.cursor_style.clone(),
                _ => "block".to_string(),
            },
        );
        set_nested_json_value(
            &mut value,
            &["appearance", "cursor_blink"],
            serde_json::Value::from(settings.cursor_blink),
        );
        match settings
            .terminal_theme
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(theme) => set_nested_json_string(
                &mut value,
                &["appearance", "terminal_theme"],
                theme.to_string(),
            ),
            None => set_nested_json_value(
                &mut value,
                &["appearance", "terminal_theme"],
                serde_json::Value::Null,
            ),
        }
        let contrast = match settings.minimum_contrast_ratio.as_str() {
            "3" => 3.0,
            "4.5" => 4.5,
            "7" => 7.0,
            "21" => 21.0,
            _ => 1.0,
        };
        set_nested_json_value(
            &mut value,
            &["appearance", "minimum_contrast_ratio"],
            serde_json::Value::from(contrast),
        );
        set_nested_json_string(
            &mut value,
            &["appearance", "ui_font_family"],
            if settings.ui_font_family.trim().is_empty() {
                "Inter".to_string()
            } else {
                settings.ui_font_family.clone()
            },
        );
        set_nested_json_value(
            &mut value,
            &["appearance", "ui_font_size"],
            serde_json::Value::from(settings.ui_font_size.clamp(12, 24)),
        );
        let font_weight = match settings.terminal_font_weight {
            300 | 400 | 500 | 600 | 700 | 800 => settings.terminal_font_weight,
            _ => 400,
        };
        let font_weight_bold = match settings.terminal_font_weight_bold {
            300 | 400 | 500 | 600 | 700 | 800 => settings.terminal_font_weight_bold,
            _ => 700,
        };
        set_nested_json_value(
            &mut value,
            &["appearance", "font_weight"],
            serde_json::Value::from(font_weight),
        );
        set_nested_json_value(
            &mut value,
            &["appearance", "font_weight_bold"],
            serde_json::Value::from(font_weight_bold),
        );
        set_nested_json_string(
            &mut value,
            &["terminal", "x11_display"],
            settings.x11_display.clone(),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_terminal_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_string(
            &mut value,
            &["terminal", "x11_display"],
            settings.x11_display.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "scrollback_lines"],
            serde_json::Value::from(settings.terminal_scrollback_lines.clamp(100, 100_000)),
        );
        set_nested_json_string(
            &mut value,
            &["terminal", "keep_alive_mode"],
            normalize_keep_alive_mode(&settings.terminal_keep_alive_mode),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "keep_alive_interval"],
            serde_json::Value::from(settings.terminal_keep_alive_interval.min(600)),
        );
        set_nested_json_string(
            &mut value,
            &["terminal", "timestamp_format"],
            normalize_timestamp_format(&settings.terminal_timestamp_format),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "hardware_acceleration"],
            serde_json::Value::Bool(settings.terminal_hardware_acceleration),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "show_workspace_padding"],
            serde_json::Value::Bool(settings.terminal_show_workspace_padding),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "show_line_numbers"],
            serde_json::Value::Bool(settings.terminal_show_line_numbers),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "show_timestamps"],
            serde_json::Value::Bool(settings.terminal_show_timestamps),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "show_multi_line_paste_dialog"],
            serde_json::Value::Bool(settings.terminal_show_multi_line_paste_dialog),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "paste_image_as_path"],
            serde_json::Value::Bool(settings.terminal_paste_image_as_path),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "reconnect_restore_cwd"],
            serde_json::Value::Bool(settings.terminal_reconnect_restore_cwd),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "low_latency_mode"],
            serde_json::Value::Bool(settings.terminal_low_latency_mode),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "zebra_stripes_enabled"],
            serde_json::Value::Bool(settings.terminal_zebra_stripes_enabled),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "action_links_enabled"],
            serde_json::Value::Bool(settings.terminal_action_links_enabled),
        );
        set_nested_json_value(
            &mut value,
            &["terminal", "action_links_matchers"],
            serde_json::json!({
                "ipv4": settings.terminal_action_links_matchers.ipv4,
                "archive": settings.terminal_action_links_matchers.archive,
                "host_port": settings.terminal_action_links_matchers.host_port,
            }),
        );
        set_nested_json_value(
            &mut value,
            &["search", "custom_engines"],
            search_engines_to_json(&settings.search_custom_engines),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_notes_panel"],
            serde_json::Value::Bool(settings.ui_show_notes_panel),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_remote_stats"],
            serde_json::Value::Bool(settings.ui_show_remote_stats),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "remote_stats_interval"],
            serde_json::Value::from(settings.ui_remote_stats_interval.clamp(1, 60)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_gpu_monitor"],
            serde_json::Value::Bool(settings.ui_show_gpu_monitor),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "gpu_monitor_interval"],
            serde_json::Value::from(settings.ui_gpu_monitor_interval.clamp(3, 120)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_ascend_npu_monitor"],
            serde_json::Value::Bool(settings.ui_show_ascend_npu_monitor),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "ascend_npu_monitor_interval"],
            serde_json::Value::from(settings.ui_ascend_npu_monitor_interval.clamp(3, 120)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_process_manager"],
            serde_json::Value::Bool(settings.ui_show_process_manager),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "process_manager_interval"],
            serde_json::Value::from(settings.ui_process_manager_interval.clamp(3, 120)),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "show_docker_manager"],
            serde_json::Value::Bool(settings.ui_show_docker_manager),
        );
        set_nested_json_value(
            &mut value,
            &["ui", "docker_manager_interval"],
            serde_json::Value::from(settings.ui_docker_manager_interval.clamp(3, 120)),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_interaction_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        let min_chars = settings
            .interaction_command_suggestion_min_chars
            .clamp(1, 500);
        let max_chars = settings
            .interaction_command_suggestion_max_chars
            .clamp(min_chars, 500);
        set_nested_json_value(
            &mut value,
            &["interaction", "copy_on_select"],
            serde_json::Value::Bool(settings.interaction_copy_on_select),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "allow_osc52_clipboard_write"],
            serde_json::Value::Bool(settings.interaction_allow_osc52_clipboard_write),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "right_click_paste"],
            serde_json::Value::Bool(settings.interaction_right_click_paste),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "terminal_zoom_enabled"],
            serde_json::Value::Bool(settings.interaction_terminal_zoom_enabled),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "command_suggestions_enabled"],
            serde_json::Value::Bool(settings.interaction_command_suggestions_enabled),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "command_suggestion_min_chars"],
            serde_json::Value::from(min_chars),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "command_suggestion_max_chars"],
            serde_json::Value::from(max_chars),
        );
        set_nested_json_string(
            &mut value,
            &["interaction", "word_separators"],
            settings.interaction_word_separators.clone(),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "duplicate_session_command_delay_ms"],
            serde_json::Value::from(
                settings
                    .interaction_duplicate_session_command_delay_ms
                    .min(60_000),
            ),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "alt_as_meta"],
            serde_json::Value::Bool(settings.interaction_alt_as_meta),
        );
        set_nested_json_value(
            &mut value,
            &["interaction", "mac_ime_compatibility"],
            serde_json::Value::Bool(settings.interaction_mac_ime_compatibility),
        );
        set_nested_json_string(
            &mut value,
            &["interaction", "tab_double_click_action"],
            normalize_tab_mouse_action(&settings.interaction_tab_double_click_action),
        );
        set_nested_json_string(
            &mut value,
            &["interaction", "tab_middle_click_action"],
            normalize_tab_mouse_action(&settings.interaction_tab_middle_click_action),
        );
        set_nested_json_string(
            &mut value,
            &["interaction", "tab_right_click_action"],
            normalize_tab_mouse_action(&settings.interaction_tab_right_click_action),
        );
        set_nested_json_string(
            &mut value,
            &["interaction", "default_encoding"],
            normalize_interaction_encoding(&settings.interaction_default_encoding),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_general_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_value(
            &mut value,
            &["general", "startup_restore"],
            serde_json::Value::Bool(settings.startup_restore),
        );
        set_nested_json_value(
            &mut value,
            &["general", "startup_restore_window_layout"],
            serde_json::Value::Bool(settings.startup_restore_window_layout),
        );
        set_nested_json_value(
            &mut value,
            &["general", "minimize_to_tray"],
            serde_json::Value::Bool(settings.minimize_to_tray),
        );
        set_nested_json_value(
            &mut value,
            &["general", "confirm_on_close"],
            serde_json::Value::Bool(settings.confirm_on_close),
        );
        // UI language lives under ui.language (Tauri UiConfig.language).
        let language = match settings.language.as_str() {
            "zh-CN" | "zh" => "zh-CN",
            "zh-TW" => "zh-TW",
            "ja" => "ja",
            _ => "en",
        };
        set_nested_json_string(&mut value, &["ui", "language"], language.to_string());
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_diagnostics_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        let level = match settings.diagnostics_level.as_str() {
            "warn" | "debug" => settings.diagnostics_level.as_str(),
            _ => "info",
        };
        let retention = match settings.diagnostics_retention_days {
            3 | 7 | 14 | 30 => settings.diagnostics_retention_days,
            _ => 7,
        };
        set_nested_json_string(&mut value, &["diagnostics", "level"], level.to_string());
        set_nested_json_value(
            &mut value,
            &["diagnostics", "retention_days"],
            serde_json::Value::from(retention),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn load_open_tabs(
        &self,
    ) -> Result<Vec<zzclawterm_core::models::RestorableOpenTab>, StorageError> {
        let value = self.load_settings_value()?;
        let Some(raw) = json_path(&value, &["ui", "open_tabs"]) else {
            return Ok(Vec::new());
        };
        if raw.is_null() {
            return Ok(Vec::new());
        }
        match serde_json::from_value::<Vec<zzclawterm_core::models::RestorableOpenTab>>(raw.clone())
        {
            Ok(tabs) => Ok(tabs),
            Err(_) => Ok(Vec::new()),
        }
    }

    pub fn save_open_tabs(
        &self,
        tabs: &[zzclawterm_core::models::RestorableOpenTab],
    ) -> Result<(), StorageError> {
        let mut value = self.load_settings_value()?;
        let encoded = serde_json::to_value(tabs)?;
        set_nested_json_value(&mut value, &["ui", "open_tabs"], encoded);
        self.save_settings_value(&value)?;
        Ok(())
    }

    pub fn load_terminal_window_layout(
        &self,
    ) -> Result<Option<zzclawterm_core::models::RestorableTerminalWindowNode>, StorageError> {
        let value = self.load_settings_value()?;
        let Some(raw) = json_path(&value, &["ui", "terminal_window_layout"]) else {
            return Ok(None);
        };
        if raw.is_null() {
            return Ok(None);
        }
        match serde_json::from_value(raw.clone()) {
            Ok(node) => Ok(Some(node)),
            Err(_) => Ok(None),
        }
    }

    pub fn save_terminal_window_layout(
        &self,
        layout: Option<&zzclawterm_core::models::RestorableTerminalWindowNode>,
    ) -> Result<(), StorageError> {
        let mut value = self.load_settings_value()?;
        let encoded = match layout {
            Some(node) => serde_json::to_value(node)?,
            None => serde_json::Value::Null,
        };
        set_nested_json_value(&mut value, &["ui", "terminal_window_layout"], encoded);
        self.save_settings_value(&value)?;
        Ok(())
    }

    pub fn load_workspace_pane_layout(
        &self,
    ) -> Result<Option<zzclawterm_core::models::RestorableWorkspacePaneNode>, StorageError> {
        let value = self.load_settings_value()?;
        let Some(raw) = json_path(&value, &["ui", "workspace_pane_layout"]) else {
            return Ok(None);
        };
        if raw.is_null() {
            return Ok(None);
        }
        match serde_json::from_value(raw.clone()) {
            Ok(node) => Ok(Some(node)),
            Err(_) => Ok(None),
        }
    }

    pub fn save_workspace_pane_layout(
        &self,
        layout: Option<&zzclawterm_core::models::RestorableWorkspacePaneNode>,
    ) -> Result<(), StorageError> {
        let mut value = self.load_settings_value()?;
        let encoded = match layout {
            Some(node) => serde_json::to_value(node)?,
            None => serde_json::Value::Null,
        };
        set_nested_json_value(&mut value, &["ui", "workspace_pane_layout"], encoded);
        self.save_settings_value(&value)?;
        Ok(())
    }

    pub fn save_screen_lock_settings(
        &self,
        settings: &AppSettingsSummary,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        set_nested_json_value(
            &mut value,
            &["security", "enable_screen_lock"],
            serde_json::Value::Bool(settings.enable_screen_lock),
        );
        set_nested_json_value(
            &mut value,
            &["security", "idle_lock_minutes"],
            serde_json::Value::from(settings.idle_lock_minutes),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }

    pub fn save_keybindings(
        &self,
        keybindings: &HashMap<String, String>,
    ) -> Result<AppSettingsSummary, StorageError> {
        let mut value = self.load_settings_value()?;
        let mut object = serde_json::Map::new();
        for (id, keys) in keybindings {
            // This map is an extensibility/compatibility boundary. Validation
            // belongs to the desktop registry; the store must retain unknown
            // IDs and invalid legacy values verbatim so a newer version can
            // diagnose or recover them.
            object.insert(id.clone(), serde_json::Value::String(keys.clone()));
        }
        set_nested_json_value(
            &mut value,
            &["keybindings"],
            serde_json::Value::Object(object),
        );
        self.save_settings_value(&value)?;
        self.load_app_settings_summary()
    }
}

fn default_activity_left_bottom() -> Vec<String> {
    vec!["syncBackupHistory".to_string(), "settings".to_string()]
}

fn default_activity_left_top() -> Vec<String> {
    vec![
        "fileExplorer".to_string(),
        "notes".to_string(),
        "network".to_string(),
        "securityAuth".to_string(),
    ]
}

fn default_activity_right_bottom() -> Vec<String> {
    vec![
        "quickCmdBar".to_string(),
        "serialSend".to_string(),
        "recording".to_string(),
        "lock".to_string(),
    ]
}

fn default_activity_right_top() -> Vec<String> {
    vec![
        "savedConnections".to_string(),
        "aiAssistant".to_string(),
        "activeSessions".to_string(),
        "commandHistory".to_string(),
        "resourceMonitor".to_string(),
        "processManager".to_string(),
        "dockerManager".to_string(),
    ]
}

fn json_optional_string(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    json_path(value, path)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn json_string(value: &serde_json::Value, path: &[&str], fallback: &str) -> String {
    json_path(value, path)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn json_string_map(value: &serde_json::Value, path: &[&str]) -> HashMap<String, String> {
    json_path(value, path)
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_string_vec_map(
    value: &serde_json::Value,
    path: &[&str],
    limit_per_key: usize,
) -> HashMap<String, Vec<String>> {
    json_path(value, path)
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    let values = value
                        .as_array()?
                        .iter()
                        .filter_map(|entry| {
                            entry
                                .as_str()
                                .map(str::trim)
                                .filter(|entry| !entry.is_empty())
                                .map(ToOwned::to_owned)
                        })
                        .fold(Vec::<String>::new(), |mut values, entry| {
                            if !values.iter().any(|existing| existing == &entry) {
                                values.push(entry);
                            }
                            values
                        })
                        .into_iter()
                        .take(limit_per_key)
                        .collect::<Vec<_>>();
                    (!values.is_empty()).then(|| (key.clone(), values))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_string_vec_with_default(
    value: &serde_json::Value,
    path: &[&str],
    limit: usize,
    default: fn() -> Vec<String>,
) -> Vec<String> {
    if json_path(value, path).is_some() {
        json_string_vec(value, path, limit)
    } else {
        default()
    }
}

fn json_u16(value: &serde_json::Value, path: &[&str], fallback: u16) -> u16 {
    let Some(value) = json_path(value, path) else {
        return fallback;
    };
    if let Some(number) = value.as_u64() {
        return number.try_into().unwrap_or(fallback);
    }
    if let Some(number) = value.as_f64()
        && number.is_finite()
        && number >= 0.0
        && number <= f64::from(u16::MAX)
    {
        return number.round() as u16;
    }
    fallback
}

fn json_u32(value: &serde_json::Value, path: &[&str], fallback: u32) -> u32 {
    let Some(value) = json_path(value, path) else {
        return fallback;
    };
    if let Some(number) = value.as_u64() {
        return number.try_into().unwrap_or(fallback);
    }
    if let Some(number) = value.as_f64()
        && number.is_finite()
        && number >= 0.0
        && number <= f64::from(u32::MAX)
    {
        return number.round() as u32;
    }
    fallback
}

fn json_u32_map(value: &serde_json::Value, path: &[&str]) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    let Some(object) = json_path(value, path).and_then(|value| value.as_object()) else {
        return map;
    };
    for (key, raw) in object {
        let number = raw
            .as_u64()
            .or_else(|| raw.as_f64().map(|value| value.round() as u64));
        if let Some(number) = number
            && number > 0
        {
            map.insert(key.clone(), number.min(u32::MAX as u64) as u32);
        }
    }
    map
}

fn json_u64(value: &serde_json::Value, path: &[&str], fallback: u64) -> u64 {
    let Some(value) = json_path(value, path) else {
        return fallback;
    };
    if let Some(number) = value.as_u64() {
        return number;
    }
    if let Some(number) = value.as_f64()
        && number.is_finite()
        && number >= 0.0
        && number <= u64::MAX as f64
    {
        return number.round() as u64;
    }
    fallback
}

fn load_action_links_matchers(
    value: &serde_json::Value,
) -> zzclawterm_core::ActionLinksMatcherSettings {
    let defaults = zzclawterm_core::ActionLinksMatcherSettings::default();
    let Some(obj) = json_path(value, &["terminal", "action_links_matchers"]) else {
        return defaults;
    };
    zzclawterm_core::ActionLinksMatcherSettings {
        ipv4: obj
            .get("ipv4")
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.ipv4),
        archive: obj
            .get("archive")
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.archive),
        host_port: obj
            .get("host_port")
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.host_port),
    }
}

fn load_search_engines(value: &serde_json::Value) -> Vec<SearchEngineConfig> {
    let Some(arr) = json_path(value, &["search", "custom_engines"]).and_then(|v| v.as_array())
    else {
        return default_search_engines();
    };
    let mut engines = Vec::new();
    for item in arr {
        if !item.is_object() {
            continue;
        }
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let url_template = item
            .get("url_template")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let show_in_menu = item
            .get("show_in_menu")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let icon = item
            .get("icon")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
        engines.push(SearchEngineConfig {
            name,
            url_template,
            icon,
            show_in_menu,
        });
    }
    engines
}

fn normalize_keep_alive_mode(mode: &str) -> String {
    match mode.trim() {
        "strict" | "disabled" => mode.trim().to_string(),
        _ => "compatible".to_string(),
    }
}

fn normalize_timestamp_format(format: &str) -> String {
    let trimmed = format.trim();
    if trimmed.is_empty() {
        return DEFAULT_TERMINAL_TIMESTAMP_FORMAT.to_string();
    }
    trimmed.chars().take(64).collect()
}

fn normalize_recording_path_template(template: &str) -> String {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        DEFAULT_RECORDING_PATH_TEMPLATE.to_string()
    } else {
        trimmed.to_string()
    }
}

fn load_recording_mode(value: &serde_json::Value) -> RecordingMode {
    match json_string(value, &["recording", "default_mode"], "transcript").as_str() {
        "raw" => RecordingMode::Raw,
        _ => RecordingMode::Transcript,
    }
}

fn recording_mode_value(mode: RecordingMode) -> &'static str {
    match mode {
        RecordingMode::Transcript => "transcript",
        RecordingMode::Raw => "raw",
    }
}

fn load_existing_file_behavior(value: &serde_json::Value) -> ExistingFileBehavior {
    match json_string(value, &["recording", "existing_file_behavior"], "unique").as_str() {
        "append" => ExistingFileBehavior::Append,
        "overwrite" => ExistingFileBehavior::Overwrite,
        _ => ExistingFileBehavior::Unique,
    }
}

fn existing_file_behavior_value(behavior: ExistingFileBehavior) -> &'static str {
    match behavior {
        ExistingFileBehavior::Unique => "unique",
        ExistingFileBehavior::Append => "append",
        ExistingFileBehavior::Overwrite => "overwrite",
    }
}

fn load_recording_rotation(value: &serde_json::Value) -> RecordingRotationPolicy {
    let Some(raw) = json_path(value, &["recording", "rotation"]) else {
        return RecordingRotationPolicy::Session;
    };
    let rotation_type = raw
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("session");
    match rotation_type {
        "daily" => RecordingRotationPolicy::Daily,
        "size" => RecordingRotationPolicy::Size {
            max_bytes: raw
                .get("max_bytes")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(50 * 1024 * 1024)
                .max(1),
        },
        _ => RecordingRotationPolicy::Session,
    }
}

fn recording_rotation_value(rotation: &RecordingRotationPolicy) -> serde_json::Value {
    match rotation {
        RecordingRotationPolicy::Session => serde_json::json!({ "type": "session" }),
        RecordingRotationPolicy::Daily => serde_json::json!({ "type": "daily" }),
        RecordingRotationPolicy::Size { max_bytes } => serde_json::json!({
            "type": "size",
            "max_bytes": (*max_bytes).max(1),
        }),
    }
}

fn normalize_host_key_policy(policy: &str) -> String {
    match policy {
        "strict" | "accept" | "prompt" => policy.to_string(),
        _ => "prompt".to_string(),
    }
}

fn normalize_interaction_encoding(encoding: &str) -> String {
    if encoding.eq_ignore_ascii_case("gbk") {
        "GBK".to_string()
    } else {
        "UTF-8".to_string()
    }
}

fn normalize_quick_cmd_sort_mode(value: &str) -> String {
    match value.trim() {
        "created" | "name" | "useCount" => value.trim().to_string(),
        _ => "created".to_string(),
    }
}

fn normalize_quick_cmd_view_mode(value: &str) -> String {
    match value.trim() {
        "list" | "compact" | "tile" => value.trim().to_string(),
        _ => "tile".to_string(),
    }
}

fn normalize_saved_connections_sort_mode(value: &str) -> String {
    match value.trim() {
        "name-asc" | "name-desc" => value.trim().to_string(),
        _ => "default".to_string(),
    }
}

fn normalize_start_workspace_mode(value: &str) -> String {
    match value.trim() {
        "assets" => "assets".to_string(),
        _ => "workbench".to_string(),
    }
}

/// Keeps only the two known asset sort directions; any other or absent value
/// leaves the direction unset (`None`) so it round-trips as JSON null.
fn normalize_asset_sort_direction(value: &Option<String>) -> Option<String> {
    match value.as_deref().map(str::trim) {
        Some("asc") => Some("asc".to_string()),
        Some("desc") => Some("desc".to_string()),
        _ => None,
    }
}

fn normalize_tab_mouse_action(action: &str) -> String {
    match action {
        "none" | "rename_tab" | "copy_tab_name" | "copy_server_ip" | "duplicate_session"
        | "multiplex_ssh" | "reconnect_session" | "disconnect_session" | "close_tab" => {
            action.to_string()
        }
        _ => "none".to_string(),
    }
}

fn normalize_transfer_duplicate_strategy(strategy: &str) -> String {
    match strategy {
        "ask" | "overwrite" | "skip" | "rename" => strategy.to_string(),
        _ => "ask".to_string(),
    }
}

fn normalize_transfer_editor_type(editor_type: &str) -> String {
    match editor_type {
        "external" | "internal" => editor_type.to_string(),
        _ => "external".to_string(),
    }
}

fn normalize_transfer_file_permissions(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_start_matches("0o")
        .trim_start_matches('0');
    let normalized = if trimmed.is_empty() { "0" } else { trimmed };
    if (3..=4).contains(&normalized.len()) && normalized.chars().all(|ch| matches!(ch, '0'..='7')) {
        normalized.to_string()
    } else {
        "644".to_string()
    }
}

fn search_engines_to_json(engines: &[SearchEngineConfig]) -> serde_json::Value {
    serde_json::Value::Array(
        engines
            .iter()
            .map(|engine| {
                serde_json::json!({
                    "name": engine.name,
                    "url_template": engine.url_template,
                    "icon": engine.icon,
                    "show_in_menu": engine.show_in_menu,
                })
            })
            .collect(),
    )
}

fn set_nested_json_string(value: &mut serde_json::Value, path: &[&str], new_value: String) {
    set_nested_json_value(value, path, serde_json::Value::String(new_value));
}

fn set_nested_json_bool(value: &mut serde_json::Value, path: &[&str], new_value: bool) {
    set_nested_json_value(value, path, serde_json::Value::Bool(new_value));
}

/// The title bar's centre reading, defaulting to the session it always showed.
fn normalize_header_status_mode(value: &str) -> String {
    match value.trim() {
        "resources" | "host" | "datetime" | "gpu" | "npu" => value.trim().to_string(),
        _ => "session".to_string(),
    }
}

fn string_vec_json_value(values: &[String], limit: usize) -> serde_json::Value {
    serde_json::Value::Array(
        values
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .fold(Vec::<String>::new(), |mut values, value| {
                if !values.iter().any(|existing| existing == value) {
                    values.push(value.to_string());
                }
                values
            })
            .into_iter()
            .take(limit)
            .map(serde_json::Value::String)
            .collect(),
    )
}

fn string_vec_map_json_value(
    map: &HashMap<String, Vec<String>>,
    limit_per_key: usize,
) -> serde_json::Value {
    let object = map
        .iter()
        .filter_map(|(key, values)| {
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let values = values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .fold(Vec::<String>::new(), |mut values, value| {
                    if !values.iter().any(|existing| existing == value) {
                        values.push(value.to_string());
                    }
                    values
                })
                .into_iter()
                .take(limit_per_key)
                .map(serde_json::Value::String)
                .collect::<Vec<_>>();
            (!values.is_empty()).then(|| (key.to_string(), serde_json::Value::Array(values)))
        })
        .collect();
    serde_json::Value::Object(object)
}

fn u32_map_json_value(map: &HashMap<String, u32>) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (key, value) in map {
        if *value > 0 {
            object.insert(key.clone(), serde_json::json!(*value));
        }
    }
    serde_json::Value::Object(object)
}
