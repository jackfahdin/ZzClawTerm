use rust_i18n::t;

use gpui::{
    Context, FontWeight, div,
    prelude::{FluentBuilder, ParentElement, Styled},
    px, rgb,
};
use nyaterm_ui::NyaSwitch;

use crate::features::{NyaTermApp, connections::ConnectionEditorToggle};
use crate::models::ConnectionEditorSelect;

use super::super::super::list::{ConnectionEditorRenderContext, connection_editor_select};
use super::ConnectionEditorSectionContext;

pub(super) fn connection_editor_recording_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<NyaTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
    } = section;
    let use_global = editor.recording.is_none();
    let auto_start = editor
        .recording
        .as_ref()
        .and_then(|settings| settings.auto_start)
        .unwrap_or(false);

    div()
        .border_t_1()
        .border_color(rgb(palette.border))
        .pt_3()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight(500.))
                                .child(t!("dialog.connectionRecording")),
                        )
                        .child(
                            div()
                                .mt_1()
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .child(t!("dialog.connectionRecordingDesc")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(palette.text_muted))
                                .child(t!("dialog.recordingUseGlobal")),
                        )
                        .child(
                            NyaSwitch::new("connection-recording-use-global")
                                .checked(use_global)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_connection_editor_flag(
                                        ConnectionEditorToggle::RecordingUseGlobal,
                                        cx,
                                    );
                                })),
                        ),
                ),
        )
        .when(!use_global, |this| {
            this.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight(500.))
                                    .child(t!("dialog.recordingAutoStart")),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(t!("dialog.recordingAutoStartDesc")),
                            ),
                    )
                    .child(
                        NyaSwitch::new("connection-recording-auto-start")
                            .checked(auto_start)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_connection_editor_flag(
                                    ConnectionEditorToggle::RecordingAutoStart,
                                    cx,
                                );
                            })),
                    ),
            )
            .child(connection_editor_select(
                ConnectionEditorRenderContext {
                    palette,
                    fields,
                    cx,
                },
                "connection-editor-recording-mode",
                t!("dialog.recordingMode"),
                ConnectionEditorSelect::RecordingMode,
            ))
        })
}
