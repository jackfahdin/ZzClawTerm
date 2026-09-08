use rust_i18n::t;

use gpui::{
    Context, div,
    prelude::{ParentElement, Styled},
    px,
};

use crate::features::NyaTermApp;
use crate::models::{ConnectionEditorField, ConnectionEditorSelect};

use super::super::super::list::{
    ConnectionEditorRenderContext, connection_editor_select, editor_field, required,
};

use super::ConnectionEditorSectionContext;

pub(super) fn connection_editor_serial_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<NyaTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor: _,
        fields,
    } = section;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_end()
                .gap_3()
                .child(div().min_w_0().flex_1().child(connection_editor_select(
                    ConnectionEditorRenderContext {
                        palette,
                        fields,
                        cx,
                    },
                    "connection-editor-serial-port",
                    required(t!("dialog.serialPort")),
                    ConnectionEditorSelect::SerialPort,
                )))
                .child(
                    div()
                        .w(px(216.))
                        .flex_none()
                        .flex()
                        .items_end()
                        .gap_1()
                        .child(div().min_w_0().flex_1().child(editor_field(
                            palette,
                            t!("dialog.baudRate"),
                            ConnectionEditorField::BaudRate,
                            fields,
                            cx,
                        )))
                        .child(div().w(px(72.)).flex_none().child(connection_editor_select(
                            ConnectionEditorRenderContext {
                                palette,
                                fields,
                                cx,
                            },
                            "connection-editor-baud-preset",
                            "",
                            ConnectionEditorSelect::BaudRate,
                        ))),
                ),
        )
        .child(
            div()
                .flex()
                .items_end()
                .gap_3()
                .child(div().w(px(72.)).flex_none().child(connection_editor_select(
                    ConnectionEditorRenderContext {
                        palette,
                        fields,
                        cx,
                    },
                    "connection-editor-data-bits",
                    t!("dialog.dataBits"),
                    ConnectionEditorSelect::DataBits,
                )))
                .child(
                    div()
                        .min_w(px(112.))
                        .flex_1()
                        .child(connection_editor_select(
                            ConnectionEditorRenderContext {
                                palette,
                                fields,
                                cx,
                            },
                            "connection-editor-parity",
                            t!("dialog.parity"),
                            ConnectionEditorSelect::Parity,
                        )),
                )
                .child(div().w(px(72.)).flex_none().child(connection_editor_select(
                    ConnectionEditorRenderContext {
                        palette,
                        fields,
                        cx,
                    },
                    "connection-editor-stop-bits",
                    t!("dialog.stopBits"),
                    ConnectionEditorSelect::StopBits,
                )))
                .child(
                    div()
                        .w(px(144.))
                        .flex_none()
                        .child(connection_editor_select(
                            ConnectionEditorRenderContext {
                                palette,
                                fields,
                                cx,
                            },
                            "connection-editor-serial-backspace",
                            t!("dialog.backspaceMode"),
                            ConnectionEditorSelect::Backspace,
                        )),
                ),
        )
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-serial-encoding",
            t!("connection.encoding"),
            ConnectionEditorSelect::Encoding,
        ))
}
