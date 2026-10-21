use futures::channel::oneshot;
use gpui::{Context, PathPromptOptions, SharedString};
use rust_i18n::t;

use crate::features::ZzClawTermApp;

use super::general_interaction::SettingsSaveKind;

impl ZzClawTermApp {
    /// Apply an edit from the X11 display box.
    pub(in crate::features) fn apply_terminal_x11_display(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_terminal_x11_display(text);
        cx.notify();
    }

    /// Apply an edit from the X server path box.
    pub(in crate::features) fn apply_terminal_x11_server_path(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_terminal_x11_server_path(text);
        cx.notify();
    }

    pub(in crate::features) fn toggle_x11_server_autostart(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_x11_server_autostart();
        self.save_terminal_settings(cx);
        // The master switch takes effect immediately, not only at the next
        // launch: enabling starts the managed server now, disabling stops it
        // (an external server is never killed).
        if self.settings.summary().x11_server_autostart {
            self.start_x11_server_now(cx);
        } else {
            self.stop_x11_server_now(cx);
        }
    }

    pub(in crate::features) fn toggle_x11_server_tray_icon(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_x11_server_tray_icon();
        self.save_terminal_settings(cx);
    }

    /// The X server path select maps "default" to an empty stored path
    /// (auto-discover) and "custom" to revealing the path input. The custom
    /// mode is transient UI state until the user actually types a path.
    pub(in crate::features) fn select_terminal_x11_server_path_mode(
        &mut self,
        mode: &str,
        cx: &mut Context<Self>,
    ) {
        if mode == "custom" {
            self.settings.set_x11_server_path_select_custom(true);
        } else {
            self.settings.set_x11_server_path_select_custom(false);
            self.settings.set_terminal_x11_server_path(String::new());
            if let Some(input) = self
                .text_input_fields_snapshot()
                .get("settings.terminal.x11-server-path")
            {
                input.update(cx, |input, cx| input.set_content_silent("", cx));
            }
        }
        self.request_settings_panel_refresh(cx);
    }

    /// Custom X server path mode requires an existing `.exe`. Called from
    /// `pending_settings_validation_error` during draft snapshot builds (not
    /// render), so one `try_exists` per check is acceptable.
    pub(in crate::features) fn x11_server_path_validation_error(&self) -> Option<String> {
        let path = self.settings.summary().x11_server_path.trim();
        let custom_mode = !path.is_empty() || self.settings.x11_server_path_select_custom();
        if !custom_mode {
            return None;
        }
        let path_ref = std::path::Path::new(path);
        let valid = !path.is_empty()
            && path_ref
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
            && path_ref.try_exists().unwrap_or(false);
        if valid {
            None
        } else {
            Some(t!("settings.x11ServerPathInvalid").to_string())
        }
    }

    /// Native file picker for the custom X server executable, committed into
    /// both the summary and the visible path box.
    pub(in crate::features) fn prompt_x11_server_path(&mut self, cx: &mut Context<Self>) {
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                t!("settings.x11ServerPath").into_owned(),
            )),
        };
        let receiver = cx.prompt_for_paths(options);
        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(path) = path {
                    let path = path.display().to_string();
                    this.apply_terminal_x11_server_path(path.clone(), cx);
                    this.reset_text_input("settings.terminal.x11-server-path", &path, cx);
                    this.save_terminal_settings(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Start the managed X server now, backing the master switch's immediate
    /// effect.
    ///
    /// `ensure_x11_server` blocks for up to ~10s, so it runs on the
    /// blocking-job scheduler; completion flips the pending flag and refreshes
    /// the panel, which re-reads `managed_x11_status` for the status icon.
    pub(in crate::features) fn start_x11_server_now(&mut self, cx: &mut Context<Self>) {
        if !cfg!(windows) || self.settings.x11_server_action_pending() {
            return;
        }
        let configured_path = self.settings.summary().x11_server_path.trim().to_string();
        let tray_icon = self.settings.summary().x11_server_tray_icon;
        self.settings.set_x11_server_action_pending(true);
        self.run_x11_server_action("x11-server-start", cx, move || {
            let path = std::path::PathBuf::from(&configured_path);
            let path = (!path.as_os_str().is_empty()).then_some(path);
            match zzclawterm_transport::ensure_x11_server(path.as_deref(), tray_icon) {
                Ok(info) => tracing::info!(
                    display = info.display,
                    started_by_us = info.started_by_us,
                    "managed X server is ready"
                ),
                Err(error) => tracing::warn!(%error, "managed X server start failed"),
            }
        });
    }

    /// Stop the managed X server now, backing the master switch's immediate
    /// effect. Only a server ZzClawTerm spawned is killed; an external server
    /// is left running.
    pub(in crate::features) fn stop_x11_server_now(&mut self, cx: &mut Context<Self>) {
        if !cfg!(windows) || self.settings.x11_server_action_pending() {
            return;
        }
        self.settings.set_x11_server_action_pending(true);
        self.run_x11_server_action("x11-server-stop", cx, || {
            zzclawterm_transport::stop_managed_x11_server();
        });
    }

    fn run_x11_server_action(
        &mut self,
        name: &'static str,
        cx: &mut Context<Self>,
        action: impl FnOnce() + Send + 'static,
    ) {
        let (sender, receiver) = oneshot::channel();
        if let Err(error) = self.blocking_jobs.submit_detached(name, move |_| {
            action();
            let _ = sender.send(());
        }) {
            tracing::warn!(%error, "failed to queue the X server action job");
            self.settings.set_x11_server_action_pending(false);
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let _ = receiver.await;
            let _ = this.update(cx, |this, cx| {
                this.settings.set_x11_server_action_pending(false);
                this.request_settings_panel_refresh(cx);
            });
        })
        .detach();
    }

    /// Warm up the managed X server once after startup when the user opted in.
    ///
    /// The spawn/poll in `ensure_x11_server` blocks for up to ~10s, so it runs
    /// on the blocking-job scheduler; a failure is logged, never surfaced.
    pub(in crate::features) fn maybe_autostart_x11_server(&self) {
        if !cfg!(windows) {
            return;
        }
        let summary = self.settings.summary();
        if !summary.x11_server_autostart {
            return;
        }
        let configured_path = summary.x11_server_path.trim().to_string();
        let tray_icon = summary.x11_server_tray_icon;
        if let Err(error) = self
            .blocking_jobs
            .submit_detached("x11-server-autostart", move |_| {
                let path = std::path::PathBuf::from(&configured_path);
                let path = (!path.as_os_str().is_empty()).then_some(path);
                match zzclawterm_transport::ensure_x11_server(path.as_deref(), tray_icon) {
                    Ok(info) => tracing::info!(
                        display = info.display,
                        started_by_us = info.started_by_us,
                        "managed X server is ready"
                    ),
                    Err(error) => tracing::warn!(%error, "managed X server autostart failed"),
                }
            })
        {
            tracing::warn!(%error, "failed to queue the X server autostart job");
        }
    }

    pub(in crate::features) fn toggle_terminal_zebra_stripes(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_terminal_zebra_stripes();
        self.save_terminal_settings(cx);
        let session_ids = self
            .visible_terminal_session_ids()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        for session_id in session_ids {
            self.notify_terminal_surface_only(Some(session_id.as_str()), cx);
        }
    }

    pub(in crate::features) fn set_terminal_scrollback_lines(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_terminal_scrollback_lines(value);
        if !self.shell.has_settings_draft() {
            self.enforce_terminal_scrollback_limit();
        }
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_terminal_keep_alive_interval(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_terminal_keep_alive_interval(value);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_terminal_keep_alive_mode(
        &mut self,
        mode: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_terminal_keep_alive_mode(mode);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn apply_terminal_timestamp_format(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_terminal_timestamp_format(text);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn toggle_terminal_workspace_padding(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings.toggle_terminal_workspace_padding();
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn toggle_terminal_line_numbers(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_terminal_line_numbers();
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn toggle_terminal_timestamps(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_terminal_timestamps();
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn toggle_terminal_reconnect_restore_cwd(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings.toggle_terminal_reconnect_restore_cwd();
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn toggle_multi_line_paste_dialog(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_multi_line_paste_dialog();
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn toggle_paste_image_as_path(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_paste_image_as_path();
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_remote_stats_interval(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_remote_stats_interval(value);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_gpu_monitor_interval(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_gpu_monitor_interval(value);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_ascend_npu_monitor_interval(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_ascend_npu_monitor_interval(value);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_process_manager_interval(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_process_manager_interval(value);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn set_docker_manager_interval(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_docker_manager_interval(value);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn reset_remote_monitoring_intervals(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let defaults = zzclawterm_core::AppSettingsSummary::default();
        self.settings
            .set_remote_stats_interval(defaults.ui_remote_stats_interval);
        self.settings
            .set_gpu_monitor_interval(defaults.ui_gpu_monitor_interval);
        self.settings
            .set_ascend_npu_monitor_interval(defaults.ui_ascend_npu_monitor_interval);
        self.settings
            .set_process_manager_interval(defaults.ui_process_manager_interval);
        self.settings
            .set_docker_manager_interval(defaults.ui_docker_manager_interval);
        self.sync_remote_monitoring_inputs(cx);
        self.save_terminal_settings(cx);
    }

    pub(in crate::features) fn sync_remote_monitoring_inputs(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings.summary();
        let inputs = self.number_input_fields_snapshot();
        for (id, value) in [
            (
                "settings.number.remote-stats-interval",
                settings.ui_remote_stats_interval,
            ),
            (
                "settings.number.gpu-monitor-interval",
                settings.ui_gpu_monitor_interval,
            ),
            (
                "settings.number.ascend-npu-monitor-interval",
                settings.ui_ascend_npu_monitor_interval,
            ),
            (
                "settings.number.process-manager-interval",
                settings.ui_process_manager_interval,
            ),
            (
                "settings.number.docker-manager-interval",
                settings.ui_docker_manager_interval,
            ),
        ] {
            if let Some(input) = inputs.get(id) {
                input.update(cx, |input, cx| {
                    input.set_content_silent(&value.to_string(), cx)
                });
            }
        }
    }

    pub(in crate::features) fn save_terminal_settings(&mut self, cx: &mut Context<Self>) {
        if self.defer_settings_domain_persistence(
            crate::features::settings::SettingsPersistenceDomain::Terminal,
            cx,
        ) {
            return;
        }
        self.enforce_terminal_scrollback_limit();
        self.queue_settings_save(SettingsSaveKind::Terminal, cx);
    }
}
