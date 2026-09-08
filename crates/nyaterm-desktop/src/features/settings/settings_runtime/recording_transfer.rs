use gpui::Context;
use nyaterm_core::{ExistingFileBehavior, RecordingMode, RecordingRotationPolicy};
use nyaterm_transport::SftpDuplicatePolicy;

use crate::features::{NyaTermApp, transfers::duplicate_policy_label};

use super::general_interaction::SettingsSaveKind;

impl NyaTermApp {
    pub(in crate::features) fn update_host_key_policy(
        &mut self,
        policy: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_host_key_policy(policy);
        if self.defer_settings_persistence(cx) {
            self.shell
                .set_status(format!("host key policy staged as {policy}"));
            return;
        }
        self.queue_settings_save(SettingsSaveKind::HostKey, cx);
    }

    pub(in crate::features) fn toggle_recording_auto_start(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_recording_auto_start();
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn set_recording_default_mode(
        &mut self,
        mode: RecordingMode,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_default_mode(mode);
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn apply_recording_path_template(
        &mut self,
        template: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_path_template(template);
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn toggle_recording_io_labels(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_recording_io_labels();
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn toggle_recording_timestamps(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_recording_timestamps();
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn toggle_recording_session_metadata(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings.toggle_recording_session_metadata();
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn set_recording_rotation(
        &mut self,
        rotation: RecordingRotationPolicy,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_rotation(rotation);
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn set_recording_rotation_size_mib(
        &mut self,
        value_mib: u64,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_rotation_size_mib(value_mib);
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn set_recording_existing_file_behavior(
        &mut self,
        behavior: ExistingFileBehavior,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_existing_file_behavior(behavior);
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn toggle_recording_binary_transfer_payloads(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings.toggle_recording_binary_transfer_payloads();
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn set_recording_memory_limit(
        &mut self,
        value_mib: u64,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_memory_limit_mib(value_mib);
        self.save_recording_settings(cx);
    }

    pub(in crate::features) fn save_recording_settings(&mut self, cx: &mut Context<Self>) {
        self.recording
            .set_memory_limit(self.settings.summary().recording_memory_limit_bytes as usize);
        if self.defer_settings_persistence(cx) {
            return;
        }
        self.queue_settings_save(SettingsSaveKind::Recording, cx);
    }

    pub(in crate::features) fn update_transfer_duplicate_policy(
        &mut self,
        policy: SftpDuplicatePolicy,
        cx: &mut Context<Self>,
    ) {
        self.transfer.set_duplicate_policy(policy);
        self.settings
            .set_transfer_duplicate_strategy(duplicate_policy_label(policy).to_string());
        self.save_transfer_settings("transfer duplicate policy saved", cx);
    }

    pub(in crate::features) fn update_transfer_editor_type(
        &mut self,
        editor_type: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_editor_type(editor_type);
        self.save_transfer_settings("transfer editor preference saved", cx);
    }

    pub(in crate::features) fn adjust_transfer_internal_editor_font_size(
        &mut self,
        delta: i16,
        cx: &mut Context<Self>,
    ) {
        if self
            .settings
            .adjust_transfer_internal_editor_font_size(delta)
        {
            self.save_transfer_settings("internal editor font size saved", cx);
            cx.notify();
        }
    }

    pub(in crate::features) fn toggle_transfer_ask_save_location(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings.toggle_transfer_ask_save_location();
        self.save_transfer_settings("transfer save-location preference saved", cx);
    }

    pub(in crate::features) fn toggle_transfer_preserve_timestamps(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings.toggle_transfer_preserve_timestamps();
        self.save_transfer_settings("transfer timestamp preference saved", cx);
    }

    pub(in crate::features) fn toggle_transfer_resume_broken(&mut self, cx: &mut Context<Self>) {
        self.settings.toggle_transfer_resume_broken();
        self.save_transfer_settings("transfer resume preference saved", cx);
    }

    pub(in crate::features) fn set_transfer_download_threads(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_download_threads(value);
        self.save_transfer_settings("transfer download concurrency saved", cx);
    }

    pub(in crate::features) fn set_transfer_upload_threads(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_upload_threads(value);
        self.save_transfer_settings("transfer upload concurrency saved", cx);
    }

    pub(in crate::features) fn set_transfer_max_retries(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_max_retries(value);
        self.save_transfer_settings("transfer retry setting saved", cx);
    }

    pub(in crate::features) fn set_transfer_buffer_size(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_buffer_size(value);
        self.save_transfer_settings("transfer buffer setting saved", cx);
    }

    pub(in crate::features) fn apply_transfer_file_permissions(
        &mut self,
        permissions: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_file_permissions(&permissions);
        cx.notify();
    }

    pub(in crate::features) fn save_transfer_settings(
        &mut self,
        success_status: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.defer_settings_persistence(cx) {
            return;
        }
        let _ = success_status;
        self.queue_settings_save(SettingsSaveKind::Transfer, cx);
    }

    /// Apply an edit from the default editor command box.
    pub(in crate::features) fn apply_transfer_default_editor(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_default_editor(text);
        self.shell
            .set_status("transfer editor command edited".to_string());
        cx.notify();
    }

    /// Apply an edit from the download path box.
    pub(in crate::features) fn apply_transfer_download_path(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_transfer_download_path(text);
        cx.notify();
    }

    /// Apply an edit from the recording path box.
    pub(in crate::features) fn apply_recording_path(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.settings.set_recording_path(text);
        cx.notify();
    }
}
