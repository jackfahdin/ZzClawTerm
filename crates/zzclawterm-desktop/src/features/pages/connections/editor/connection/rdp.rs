use crate::models::ConnectionEditorSection;
use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{FluentBuilder, InteractiveElement, ParentElement, Styled},
    px, rgb,
};
use zzclawterm_ui::ZzClawSwitch;

use crate::features::{ZzClawTermApp, connections::ConnectionEditorToggle};
use crate::models::{ConnectionEditorField, ConnectionEditorSelect};

use super::super::super::list::{
    ConnectionEditorRenderContext, connection_editor_select, editor_field, editor_stepper_field,
    required,
};
use super::ConnectionEditorSectionContext;

fn rdp_switch_row(
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

pub(super) fn connection_editor_rdp_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let content = match section.editor.section {
        ConnectionEditorSection::Basic => rdp_basic(section, cx),
        ConnectionEditorSection::Authentication => rdp_authentication(section, cx),
        ConnectionEditorSection::Security => rdp_security(section, cx),
        ConnectionEditorSection::Network => rdp_network(section, cx),
        ConnectionEditorSection::Display => rdp_display(section, cx),
        ConnectionEditorSection::Session => rdp_session(section, cx),
        _ => div(),
    };
    div()
        .debug_selector(|| "connection-editor-rdp-section".to_string())
        .flex()
        .flex_col()
        .gap_3()
        .child(content)
}

fn rdp_basic(
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

fn rdp_authentication(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<ZzClawTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
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
                .flex()
                .when(!narrow, |this| this.flex_wrap())
                .when(narrow, |this| this.flex_col())
                .gap_3()
                .child(div().min_w(px(180.)).flex_1().child(editor_field(
                    palette,
                    required(t!("dialog.username")),
                    ConnectionEditorField::Username,
                    fields,
                    cx,
                )))
                .child(div().min_w(px(160.)).flex_1().child(editor_field(
                    palette,
                    t!("dialog.rdpDomain"),
                    ConnectionEditorField::Domain,
                    fields,
                    cx,
                ))),
        )
        .child(super::authentication::password_authentication(section, cx))
}

fn rdp_security(
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
        .flex()
        .flex_col()
        .gap_3()
        .child(rdp_switch_row(
            palette,
            "connection-rdp-use-nla",
            t!("dialog.rdpUseNla"),
            t!("dialog.rdpUseNlaDesc"),
            editor.rdp_security.use_nla,
            cx.listener(|this, _, _, cx| {
                this.toggle_connection_editor_flag(ConnectionEditorToggle::RdpUseNla, cx)
            }),
        ))
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-rdp-certificate-policy",
            t!("dialog.rdpCertificatePolicy"),
            ConnectionEditorSelect::RdpCertificatePolicy,
        ))
}

fn rdp_network(
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
        .flex()
        .flex_col()
        .gap_3()
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-rdp-proxy",
            t!("dialog.proxySelect"),
            ConnectionEditorSelect::Proxy,
        ))
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-rdp-jump",
            t!("dialog.proxyJump"),
            ConnectionEditorSelect::ProxyJump,
        ))
}

fn rdp_display(
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
        .flex()
        .flex_col()
        .gap_3()
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-rdp-display-mode",
            t!("dialog.rdpDisplayMode"),
            ConnectionEditorSelect::RdpDisplayMode,
        ))
        .when(editor.rdp_display.mode == "fixed", |this| {
            this.child(
                div()
                    .flex()
                    .gap_3()
                    .child(div().min_w_0().flex_1().child(editor_stepper_field(
                        palette,
                        t!("dialog.rdpWidth"),
                        ConnectionEditorField::RdpDisplayWidth,
                        fields,
                        cx,
                    )))
                    .child(div().min_w_0().flex_1().child(editor_stepper_field(
                        palette,
                        t!("dialog.rdpHeight"),
                        ConnectionEditorField::RdpDisplayHeight,
                        fields,
                        cx,
                    ))),
            )
        })
}

fn rdp_session(
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
        .child(connection_editor_select(
            ConnectionEditorRenderContext {
                palette,
                fields,
                cx,
            },
            "connection-editor-rdp-clipboard-mode",
            t!("dialog.rdpClipboard"),
            ConnectionEditorSelect::RdpClipboardMode,
        ))
        .flex()
        .flex_col()
        .gap_3()
        .child(rdp_switch_row(
            palette,
            "connection-rdp-auto-reconnect",
            t!("dialog.rdpAutoReconnect"),
            t!("dialog.rdpAutoReconnectDesc"),
            editor.rdp_reconnect.enabled,
            cx.listener(|this, _, _, cx| {
                this.toggle_connection_editor_flag(ConnectionEditorToggle::RdpReconnect, cx)
            }),
        ))
        .child(editor_stepper_field(
            palette,
            t!("dialog.rdpReconnectAttempts"),
            ConnectionEditorField::RdpReconnectAttempts,
            fields,
            cx,
        ))
}
