use crate::models::ConnectionEditorSection;
use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{FluentBuilder, InteractiveElement, ParentElement, Styled},
    px, rgb, rgba,
};
use zzclawterm_ui::ZzClawSwitch;

use crate::features::{ZzClawTermApp, connections::ConnectionEditorToggle};
use crate::models::{ConnectionEditorField, ConnectionEditorSelect};

use super::super::super::list::{
    ConnectionEditorRenderContext, connection_editor_select, editor_field, editor_stepper_field,
    required,
};
use super::ConnectionEditorSectionContext;

fn vnc_switch_row(
    palette: crate::theme::ThemePalette,
    id: &'static str,
    label: impl Into<SharedString>,
    description: impl Into<SharedString>,
    checked: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let description: SharedString = description.into();
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .px_3()
        .py_2()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(div().text_xs().font_weight(FontWeight(500.)).child(label))
                .child(
                    div()
                        .mt_1()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(description),
                ),
        )
        .child(
            ZzClawSwitch::new(id)
                .checked(checked)
                .on_click(move |_, window, cx| {
                    on_click(&gpui::ClickEvent::default(), window, cx);
                }),
        )
}

pub(super) fn connection_editor_vnc_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let content = match section.editor.section {
        ConnectionEditorSection::Basic => vnc_basic(section, cx),
        ConnectionEditorSection::Authentication => vnc_authentication(section, cx),
        ConnectionEditorSection::Network => vnc_network(section, cx),
        ConnectionEditorSection::Display => vnc_display(section, cx),
        ConnectionEditorSection::Security => vnc_security(section, cx),
        ConnectionEditorSection::Session => vnc_session(section, cx),
        _ => div(),
    };
    div()
        .debug_selector(|| "connection-editor-vnc-section".to_string())
        .flex()
        .flex_col()
        .gap_3()
        .child(content)
}

fn vnc_basic(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        fields,
        narrow,
        ..
    } = section;
    div().flex().flex_col().gap_3().child(
        div()
            .flex()
            .when(narrow, |this| this.flex_col())
            .gap_3()
            .child(div().min_w_0().flex_1().child(editor_field(
                palette,
                required(t!("dialog.host")),
                ConnectionEditorField::Host,
                fields,
                cx,
            )))
            .child(
                div()
                    .when(!narrow, |this| this.w(px(150.)).flex_none())
                    .when(narrow, |this| this.w_full())
                    .child(editor_stepper_field(
                        palette,
                        required(t!("dialog.port")),
                        ConnectionEditorField::Port,
                        fields,
                        cx,
                    )),
            ),
    )
}

fn vnc_authentication(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette, fields, ..
    } = section;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(super::authentication::password_authentication(section, cx))
        .child(editor_field(
            palette,
            t!("credentialManager.usernameOptionalLabel"),
            ConnectionEditorField::Username,
            fields,
            cx,
        ))
}

fn vnc_network(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette, fields, ..
    } = section;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-vnc-proxy",
            t!("dialog.proxySelect"),
            ConnectionEditorSelect::Proxy,
        ))
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-vnc-jump",
            t!("dialog.proxyJump"),
            ConnectionEditorSelect::ProxyJump,
        ))
}

fn vnc_display(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette, fields, ..
    } = section;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .min_w(px(180.))
                .flex_1()
                .child(connection_editor_select(
                    ConnectionEditorRenderContext {
                        palette,
                        fields,
                        cx,
                    },
                    "connection-editor-vnc-scale-mode",
                    t!("dialog.vncScaleMode"),
                    ConnectionEditorSelect::VncScaleMode,
                )),
        )
}

fn vnc_security(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
        ..
    } = section;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .min_w(px(180.))
                .flex_1()
                .child(connection_editor_select(
                    ConnectionEditorRenderContext {
                        palette,
                        fields,
                        cx,
                    },
                    "connection-editor-vnc-security-mode",
                    t!("dialog.vncSecurityMode"),
                    ConnectionEditorSelect::VncSecurityMode,
                )),
        )
        .when(editor.vnc_security.mode == "none", |this| {
            this.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.warning))
                    .bg(rgba((palette.warning << 8) | 0x18))
                    .p_2()
                    .text_size(px(10.))
                    .text_color(rgb(palette.warning))
                    .child(t!("dialog.vncUnencryptedWarning")),
            )
        })
}

fn vnc_session(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
        narrow,
        ..
    } = section;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .grid()
                .grid_cols(if narrow { 1 } else { 2 })
                .gap_2()
                .child(vnc_switch_row(
                    palette,
                    "connection-vnc-clipboard",
                    t!("dialog.vncClipboard"),
                    t!("dialog.vncClipboardDesc"),
                    editor.vnc_clipboard.enabled,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_connection_editor_flag(
                            ConnectionEditorToggle::VncClipboard,
                            cx,
                        );
                    }),
                ))
                .child(vnc_switch_row(
                    palette,
                    "connection-vnc-shared",
                    t!("dialog.vncSharedSession"),
                    t!("dialog.vncSharedSessionDesc"),
                    editor.vnc_shared,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_connection_editor_flag(ConnectionEditorToggle::VncShared, cx);
                    }),
                ))
                .child(vnc_switch_row(
                    palette,
                    "connection-vnc-view-only",
                    t!("dialog.vncViewOnly"),
                    t!("dialog.vncViewOnlyDesc"),
                    editor.vnc_view_only,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_connection_editor_flag(ConnectionEditorToggle::VncViewOnly, cx);
                    }),
                ))
                .child(vnc_switch_row(
                    palette,
                    "connection-vnc-reconnect",
                    t!("dialog.vncReconnect"),
                    t!("dialog.vncReconnectDesc"),
                    editor.vnc_reconnect.enabled,
                    cx.listener(|this, _, _, cx| {
                        this.toggle_connection_editor_flag(
                            ConnectionEditorToggle::VncReconnect,
                            cx,
                        );
                    }),
                )),
        )
        .child(editor_stepper_field(
            palette,
            t!("dialog.vncReconnectAttempts"),
            ConnectionEditorField::VncReconnectAttempts,
            fields,
            cx,
        ))
}
