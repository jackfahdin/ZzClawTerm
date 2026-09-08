use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{
        FluentBuilder, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled,
    },
    px, rgb, svg,
};
use nyaterm_ui::{NyaSwitch, NyaTabItem, NyaTabs};

use crate::features::{NyaTermApp, connections::ConnectionEditorToggle};
use crate::models::{
    ConnectionEditorField, ConnectionEditorPasswordSource, ConnectionEditorRdpTab,
    ConnectionEditorSelect,
};

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
            NyaSwitch::new(id)
                .checked(checked)
                .on_click(move |_, window, cx| {
                    on_click(&gpui::ClickEvent::default(), window, cx);
                }),
        )
}

pub(super) fn connection_editor_rdp_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<NyaTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
    } = section;
    let auth_values = ["none".to_string(), "password".to_string()];
    let auth_tabs = NyaTabs::new("connection-rdp-auth-tabs")
        .items([
            NyaTabItem::new(t!("dialog.noAuthentication")),
            NyaTabItem::new(t!("dialog.password")),
        ])
        .selected_index(if editor.auth_mode == "none" { 0 } else { 1 })
        .on_select(cx.listener(move |this, index: &usize, _, cx| {
            let Some(value) = auth_values.get(*index) else {
                return;
            };
            this.set_connection_editor_select_value(
                ConnectionEditorSelect::Authentication,
                Some(value.as_str()),
                cx,
            );
        }));
    let password_source_tabs = NyaTabs::new("connection-rdp-password-source-tabs")
        .items([
            NyaTabItem::new(t!("dialog.askWhenConnecting")),
            NyaTabItem::new(t!("dialog.directPassword")),
            NyaTabItem::new(t!("dialog.savedPassword")),
        ])
        .selected_index(match editor.password_source {
            ConnectionEditorPasswordSource::Ask => 0,
            ConnectionEditorPasswordSource::Direct => 1,
            ConnectionEditorPasswordSource::Saved => 2,
        })
        .on_select(cx.listener(|this, index, _, cx| {
            let source = match *index {
                0 => ConnectionEditorPasswordSource::Ask,
                1 => ConnectionEditorPasswordSource::Direct,
                _ => ConnectionEditorPasswordSource::Saved,
            };
            this.set_connection_editor_password_source(source, cx);
        }));
    let advanced_tabs = NyaTabs::new("connection-rdp-advanced-tabs")
        .items([
            NyaTabItem::new(t!("dialog.rdpSecurity")),
            NyaTabItem::new(t!("dialog.rdpDisplay")),
            NyaTabItem::new(t!("dialog.rdpClipboard")),
            NyaTabItem::new(t!("dialog.rdpReconnect")),
        ])
        .selected_index(match editor.rdp_advanced_tab {
            ConnectionEditorRdpTab::Security => 0,
            ConnectionEditorRdpTab::Display => 1,
            ConnectionEditorRdpTab::Clipboard => 2,
            ConnectionEditorRdpTab::Reconnect => 3,
        })
        .on_select(cx.listener(|this, index, _, cx| {
            let tab = match *index {
                0 => ConnectionEditorRdpTab::Security,
                1 => ConnectionEditorRdpTab::Display,
                2 => ConnectionEditorRdpTab::Clipboard,
                _ => ConnectionEditorRdpTab::Reconnect,
            };
            this.set_connection_editor_rdp_tab(tab, cx);
        }));

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .gap_3()
                .child(div().min_w_0().flex_1().child(editor_field(
                    palette,
                    required(t!("dialog.host")),
                    ConnectionEditorField::Host,
                    fields,
                    cx,
                )))
                .child(div().w(px(150.)).flex_none().child(editor_stepper_field(
                    palette,
                    required(t!("dialog.port")),
                    ConnectionEditorField::Port,
                    fields,
                    cx,
                ))),
        )
        .child(editor_field(
            palette,
            required(t!("dialog.username")),
            ConnectionEditorField::Username,
            fields,
            cx,
        ))
        .child(editor_field(
            palette,
            t!("dialog.rdpDomain"),
            ConnectionEditorField::Domain,
            fields,
            cx,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight(500.))
                        .text_color(rgb(palette.text_muted))
                        .child(t!("dialog.authentication")),
                )
                .child(auth_tabs)
                .when(editor.auth_mode != "none", |this| {
                    this.child(password_source_tabs)
                        .when(
                            editor.password_source == ConnectionEditorPasswordSource::Direct,
                            |this| {
                                this.child(editor_field(
                                    palette,
                                    t!("dialog.password"),
                                    ConnectionEditorField::Password,
                                    fields,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            editor.password_source == ConnectionEditorPasswordSource::Saved,
                            |this| {
                                this.child(connection_editor_select(
                                    ConnectionEditorRenderContext {
                                        palette,
                                        fields,
                                        cx,
                                    },
                                    "connection-editor-rdp-saved-password",
                                    t!("dialog.savedPassword"),
                                    ConnectionEditorSelect::SavedPassword,
                                ))
                            },
                        )
                }),
        )
        .child(
            div()
                .id("connection-editor-rdp-advanced-toggle")
                .h(px(28.))
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .cursor_pointer()
                .hover(|this| this.text_color(rgb(palette.text)))
                .child(
                    svg()
                        .size(px(14.))
                        .path(if editor.advanced_open {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/fe/forward.svg"
                        })
                        .text_color(rgb(palette.text_muted)),
                )
                .child(t!("dialog.advancedConfig"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_connection_editor_flag(ConnectionEditorToggle::Advanced, cx);
                })),
        )
        .when(editor.advanced_open, |this| {
            this.child(advanced_tabs).child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .p_3()
                    .when(
                        editor.rdp_advanced_tab == ConnectionEditorRdpTab::Security,
                        |this| {
                            this.flex()
                                .flex_col()
                                .gap_3()
                                .child(rdp_switch_row(
                                    palette,
                                    "connection-rdp-use-nla",
                                    t!("dialog.rdpUseNla"),
                                    t!("dialog.rdpUseNlaDesc"),
                                    editor.rdp_security.use_nla,
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_connection_editor_flag(
                                            ConnectionEditorToggle::RdpUseNla,
                                            cx,
                                        )
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
                        },
                    )
                    .when(
                        editor.rdp_advanced_tab == ConnectionEditorRdpTab::Display,
                        |this| {
                            this.flex()
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
                                .child(
                                    div()
                                        .flex()
                                        .gap_3()
                                        .child(div().min_w_0().flex_1().child(
                                            editor_stepper_field(
                                                palette,
                                                t!("dialog.rdpWidth"),
                                                ConnectionEditorField::RdpDisplayWidth,
                                                fields,
                                                cx,
                                            ),
                                        ))
                                        .child(div().min_w_0().flex_1().child(
                                            editor_stepper_field(
                                                palette,
                                                t!("dialog.rdpHeight"),
                                                ConnectionEditorField::RdpDisplayHeight,
                                                fields,
                                                cx,
                                            ),
                                        )),
                                )
                        },
                    )
                    .when(
                        editor.rdp_advanced_tab == ConnectionEditorRdpTab::Clipboard,
                        |this| {
                            this.child(connection_editor_select(
                                ConnectionEditorRenderContext {
                                    palette,
                                    fields,
                                    cx,
                                },
                                "connection-editor-rdp-clipboard-mode",
                                t!("dialog.rdpClipboard"),
                                ConnectionEditorSelect::RdpClipboardMode,
                            ))
                        },
                    )
                    .when(
                        editor.rdp_advanced_tab == ConnectionEditorRdpTab::Reconnect,
                        |this| {
                            this.flex()
                                .flex_col()
                                .gap_3()
                                .child(rdp_switch_row(
                                    palette,
                                    "connection-rdp-auto-reconnect",
                                    t!("dialog.rdpAutoReconnect"),
                                    t!("dialog.rdpAutoReconnectDesc"),
                                    editor.rdp_reconnect.enabled,
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_connection_editor_flag(
                                            ConnectionEditorToggle::RdpReconnect,
                                            cx,
                                        )
                                    }),
                                ))
                                .child(editor_stepper_field(
                                    palette,
                                    t!("dialog.rdpReconnectAttempts"),
                                    ConnectionEditorField::RdpReconnectAttempts,
                                    fields,
                                    cx,
                                ))
                        },
                    ),
            )
        })
}
