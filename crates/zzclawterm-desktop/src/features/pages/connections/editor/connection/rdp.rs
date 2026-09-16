use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{
        FluentBuilder, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled,
    },
    px, rgb, svg,
};
use zzclawterm_ui::{ZzClawSwitch, ZzClawTabItem, ZzClawTabs};

use crate::features::{ZzClawTermApp, connections::ConnectionEditorToggle};
use crate::models::{
    ConnectionEditorCredentialOverlay, ConnectionEditorField, ConnectionEditorPasswordSource,
    ConnectionEditorRdpTab, ConnectionEditorSelect,
};

use super::super::super::list::{
    ConnectionEditorRenderContext, EditorSecretFieldOptions, connection_editor_select,
    editor_field, editor_secret_field, editor_stepper_field, required,
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
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
        baud_popover_open: _,
    } = section;
    let auth_tabs = ZzClawTabs::new("connection-rdp-auth-tabs")
        .items([
            ZzClawTabItem::new(t!("dialog.directPassword")),
            ZzClawTabItem::new(t!("dialog.savedPassword")),
            ZzClawTabItem::new(t!("dialog.askWhenConnecting")),
            ZzClawTabItem::new(t!("dialog.noAuthentication")),
        ])
        .selected_index(if editor.auth_mode == "none" {
            3
        } else {
            match editor.password_source {
                ConnectionEditorPasswordSource::Direct => 0,
                ConnectionEditorPasswordSource::Saved => 1,
                ConnectionEditorPasswordSource::Ask => 2,
            }
        })
        .on_select(cx.listener(move |this, index: &usize, _, cx| {
            if *index == 3 {
                this.set_connection_editor_select_value(
                    ConnectionEditorSelect::Authentication,
                    Some("none"),
                    cx,
                );
            } else {
                this.set_connection_editor_select_value(
                    ConnectionEditorSelect::Authentication,
                    Some("password"),
                    cx,
                );
                let source = match *index {
                    0 => ConnectionEditorPasswordSource::Direct,
                    1 => ConnectionEditorPasswordSource::Saved,
                    _ => ConnectionEditorPasswordSource::Ask,
                };
                this.set_connection_editor_password_source(source, cx);
            }
        }));
    let advanced_tabs = ZzClawTabs::new("connection-rdp-advanced-tabs")
        .items([
            ZzClawTabItem::new(t!("dialog.rdpSecurity")),
            ZzClawTabItem::new(t!("panel.network")),
            ZzClawTabItem::new(t!("dialog.rdpDisplay")),
            ZzClawTabItem::new(t!("dialog.rdpClipboard")),
            ZzClawTabItem::new(t!("dialog.rdpReconnect")),
        ])
        .selected_index(match editor.rdp_advanced_tab {
            ConnectionEditorRdpTab::Security => 0,
            ConnectionEditorRdpTab::Network => 1,
            ConnectionEditorRdpTab::Display => 2,
            ConnectionEditorRdpTab::Clipboard => 3,
            ConnectionEditorRdpTab::Reconnect => 4,
        })
        .on_select(cx.listener(|this, index, _, cx| {
            let tab = match *index {
                0 => ConnectionEditorRdpTab::Security,
                1 => ConnectionEditorRdpTab::Network,
                2 => ConnectionEditorRdpTab::Display,
                3 => ConnectionEditorRdpTab::Clipboard,
                _ => ConnectionEditorRdpTab::Reconnect,
            };
            this.set_connection_editor_rdp_tab(tab, cx);
        }));

    div()
        .debug_selector(|| "connection-editor-rdp-section".to_string())
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
        .child(
            div()
                .flex()
                .flex_wrap()
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
                    this.when(
                        editor.password_source == ConnectionEditorPasswordSource::Direct,
                        |this| {
                            this.child(editor_secret_field(
                                palette,
                                t!("dialog.password"),
                                ConnectionEditorField::Password,
                                fields,
                                EditorSecretFieldOptions::new(
                                    false,
                                    t!("passwordManager.showPassword"),
                                    t!("dialog.clearPassword"),
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_connection_editor_password_visibility(cx);
                                    }),
                                    cx.listener(|this, _, _, cx| {
                                        this.clear_connection_editor_password(cx);
                                    }),
                                ),
                                cx,
                            ))
                        },
                    )
                    .when(
                        editor.password_source == ConnectionEditorPasswordSource::Saved,
                        |this| {
                            this.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_end()
                                    .gap_2()
                                    .child(div().min_w(px(180.)).flex_1().child(
                                        connection_editor_select(
                                            ConnectionEditorRenderContext {
                                                palette,
                                                fields,
                                                cx,
                                            },
                                            "connection-editor-rdp-saved-password",
                                            t!("dialog.savedPassword"),
                                            ConnectionEditorSelect::SavedPassword,
                                        ),
                                    ))
                                    .child(
                                        zzclawterm_ui::ZzClawButton::new(
                                            "connection-editor-rdp-manage-passwords",
                                            t!("dialog.managePasswords"),
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.set_connection_editor_credential_overlay(
                                                Some(ConnectionEditorCredentialOverlay::Passwords),
                                                cx,
                                            );
                                            }),
                                        ),
                                    ),
                            )
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
                        .path(if editor.advanced.rdp {
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
        .when(editor.advanced.rdp, |this| {
            this.child(advanced_tabs).child(
                div()
                    .id("connection-editor-rdp-advanced")
                    .debug_selector(|| "connection-editor-rdp-advanced".to_string())
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
                        editor.rdp_advanced_tab == ConnectionEditorRdpTab::Network,
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
                                .when(editor.rdp_display.mode == "fixed", |this| {
                                    this.child(
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
                                })
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
