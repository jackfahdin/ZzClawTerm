use rust_i18n::t;

use gpui::{Context, IntoElement, SharedString, div, prelude::*, px};
use nyaterm_core::{ExistingFileBehavior, RecordingMode, RecordingRotationPolicy};
use nyaterm_ui::NyaSelectOption;

use crate::features::pages::settings::panel::SettingsPanel;
use crate::widgets::small_button;

use super::super::{
    settings_form_row, settings_form_section, settings_input_action_control, settings_switch,
};

impl SettingsPanel {
    pub(in crate::features) fn recording_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        // Built before the form, which reads `self` throughout: creating the
        // box needs it mutably.
        let recording_path_input = self
            .existing_text_input_box("settings.recording.path", false)
            .into_any_element();
        let recording_template_input = self
            .existing_text_input_box("settings.recording.path-template", false)
            .into_any_element();
        let _memory_mib =
            (self.settings.summary().recording_memory_limit_bytes / (1024 * 1024)).max(1);
        let rotation_value = match self.settings.summary().recording_rotation {
            RecordingRotationPolicy::Daily => "daily",
            RecordingRotationPolicy::Size { .. } => "size",
            RecordingRotationPolicy::Session => "session",
        };
        let _rotation_size_mib = match self.settings.summary().recording_rotation {
            RecordingRotationPolicy::Size { max_bytes } => (max_bytes / (1024 * 1024)).max(1),
            _ => 50,
        };
        let mode_value = match self.settings.summary().recording_default_mode {
            RecordingMode::Raw => "raw",
            RecordingMode::Transcript => "transcript",
        };
        let existing_value = match self.settings.summary().recording_existing_file_behavior {
            ExistingFileBehavior::Append => "append",
            ExistingFileBehavior::Overwrite => "overwrite",
            ExistingFileBehavior::Unique => "unique",
        };

        div().flex().flex_col().gap_3().child(settings_form_section(
            palette,
            Some(t!("settings.recordingSettings")),
            Some(t!("settings.recordingSettingsDesc")),
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingDefaultMode"),
                    Some(SharedString::from(t!("settings.recordingDefaultModeDesc"))),
                    self.settings_select_control(
                        "settings.recording.default-mode",
                        vec![
                            NyaSelectOption::new(
                                "transcript",
                                t!("settings.recordingModeTranscript"),
                            ),
                            NyaSelectOption::new("raw", t!("settings.recordingModeRaw")),
                        ],
                        mode_value,
                        false,
                        cx,
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingPath"),
                    Some(SharedString::from(t!("settings.recordingPathDesc"))),
                    settings_input_action_control(
                        260.,
                        recording_path_input,
                        small_button(
                            palette,
                            "settings-recording-path-browse",
                            t!("settings.browse"),
                            cx.listener(|this, _, _, cx| {
                                this.prompt_recording_path_setting(cx);
                            }),
                        ),
                    ),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(gpui::rgb(palette.text))
                                .child(t!("settings.recordingPathTemplate")),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(gpui::rgb(palette.text_dimmed))
                                .child(t!("settings.recordingPathTemplateDesc")),
                        )
                        .child(
                            div()
                                .w_full()
                                .max_w(px(576.))
                                .child(recording_template_input),
                        ),
                )
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingAutoStart"),
                    Some(SharedString::from(t!("settings.recordingAutoStartDesc"))),
                    settings_switch(
                        palette,
                        "settings-recording-auto",
                        self.settings.summary().recording_auto_start,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_recording_auto_start(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingIncludeMetadata"),
                    Some(SharedString::from(t!(
                        "settings.recordingIncludeMetadataDesc"
                    ))),
                    settings_switch(
                        palette,
                        "settings-recording-metadata",
                        self.settings.summary().recording_include_session_metadata,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_recording_session_metadata(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingIncludeIoLabels"),
                    Some(SharedString::from(t!(
                        "settings.recordingIncludeIoLabelsDesc"
                    ))),
                    settings_switch(
                        palette,
                        "settings-recording-labels",
                        self.settings.summary().recording_include_io_labels,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_recording_io_labels(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingIncludeTimestamps"),
                    Some(SharedString::from(t!(
                        "settings.recordingIncludeTimestampsDesc"
                    ))),
                    settings_switch(
                        palette,
                        "settings-recording-timestamps",
                        self.settings.summary().recording_include_timestamps,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_recording_timestamps(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingRotation"),
                    Some(SharedString::from(t!("settings.recordingRotationDesc"))),
                    self.settings_select_control(
                        "settings.recording.rotation",
                        vec![
                            NyaSelectOption::new(
                                "session",
                                t!("settings.recordingRotationSession"),
                            ),
                            NyaSelectOption::new("daily", t!("settings.recordingRotationDaily")),
                            NyaSelectOption::new("size", t!("settings.recordingRotationSize")),
                        ],
                        rotation_value,
                        false,
                        cx,
                    ),
                ))
                .when(
                    matches!(
                        self.settings.summary().recording_rotation,
                        RecordingRotationPolicy::Size { .. }
                    ),
                    |this| {
                        this.child(settings_form_row(
                            palette,
                            t!("settings.recordingRotationSizeLimit"),
                            Some(SharedString::from(t!(
                                "settings.recordingRotationSizeLimitDesc"
                            ))),
                            self.existing_number_input_box(
                                "settings.number.recording-rotation-size",
                            ),
                        ))
                    },
                )
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingExistingFileBehavior"),
                    Some(SharedString::from(t!(
                        "settings.recordingExistingFileBehaviorDesc"
                    ))),
                    self.settings_select_control(
                        "settings.recording.existing-file",
                        vec![
                            NyaSelectOption::new("unique", t!("settings.recordingExistingUnique")),
                            NyaSelectOption::new("append", t!("settings.recordingExistingAppend")),
                            NyaSelectOption::new(
                                "overwrite",
                                t!("settings.recordingExistingOverwrite"),
                            ),
                        ],
                        existing_value,
                        false,
                        cx,
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingIncludeBinaryTransfers"),
                    Some(SharedString::from(t!(
                        "settings.recordingIncludeBinaryTransfersDesc"
                    ))),
                    settings_switch(
                        palette,
                        "settings-recording-binary-transfer-payloads",
                        self.settings
                            .summary()
                            .recording_include_binary_transfer_payloads,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_recording_binary_transfer_payloads(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.recordingMemoryLimit"),
                    Some(SharedString::from(t!("settings.recordingMemoryLimitDesc"))),
                    self.existing_number_input_box("settings.number.recording-memory-limit"),
                )),
        ))
    }
}
