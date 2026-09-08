use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{
        FluentBuilder, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled,
    },
    px, rgb, svg,
};

use crate::features::{NyaTermApp, connections::ConnectionEditorToggle};
use crate::models::{
    ConnectionEditorField, ConnectionEditorPasswordSource, ConnectionEditorSelect,
    ConnectionEditorTelnetTab,
};
use nyaterm_ui::{NyaSwitch, NyaTabItem, NyaTabs};

use super::super::super::list::{
    ConnectionEditorRenderContext, connection_editor_select, editor_field, editor_stepper_field,
    required,
};

use super::ConnectionEditorSectionContext;

fn telnet_switch_row(
    palette: crate::theme::ThemePalette,
    id: &'static str,
    label: impl Into<SharedString>,
    description: impl Into<SharedString>,
    checked: bool,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let description: SharedString = description.into();
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.bg))
        .px_3()
        .py_2()
        .opacity(if enabled { 1.0 } else { 0.55 })
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight(500.))
                        .text_color(rgb(palette.text))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(description),
                ),
        )
        .child(
            div().mt(px(2.)).flex_none().child(
                NyaSwitch::new(id)
                    .checked(checked)
                    .disabled(!enabled)
                    .on_click(move |_, window, cx| {
                        if enabled {
                            on_click(&gpui::ClickEvent::default(), window, cx);
                        }
                    }),
            ),
        )
}

pub(super) fn connection_editor_telnet_section(
    section: ConnectionEditorSectionContext<'_>,
    cx: &mut Context<NyaTermApp>,
) -> gpui::Div {
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
    } = section;
    let auth_values = ["none".to_string(), "password".to_string()];
    let auth_tabs = NyaTabs::new("connection-telnet-auth-tabs")
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
    let password_source_tabs = NyaTabs::new("connection-telnet-password-source-tabs")
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
    let tabs = NyaTabs::new("connection-telnet-tabs")
        .items([
            NyaTabItem::new(t!("dialog.telnetInputSettings")),
            NyaTabItem::new(t!("dialog.telnetCompatibility")),
        ])
        .selected_index(
            if editor.telnet_advanced_tab == ConnectionEditorTelnetTab::Input {
                0
            } else {
                1
            },
        )
        .on_select(cx.listener(|this, index, _, cx| {
            let tab = match *index {
                0 => ConnectionEditorTelnetTab::Input,
                _ => ConnectionEditorTelnetTab::Compatibility,
            };
            this.set_connection_editor_telnet_tab(tab, cx);
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
            t!("dialog.username"),
            ConnectionEditorField::Username,
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
                                    "connection-editor-telnet-saved-password",
                                    t!("dialog.savedPassword"),
                                    ConnectionEditorSelect::SavedPassword,
                                ))
                            },
                        )
                }),
        )
        .child(
            div()
                .id("connection-telnet-advanced-toggle")
                .h(px(28.))
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .text_xs()
                .text_color(rgb(palette.text_muted))
                .hover(|this| this.text_color(rgb(palette.text)))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_connection_editor_flag(ConnectionEditorToggle::Advanced, cx);
                }))
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
                .child(t!("dialog.advancedConfig")),
        )
        .when(editor.advanced_open, |this| {
            this.child(tabs)
                .when(
                    editor.telnet_advanced_tab == ConnectionEditorTelnetTab::Input,
                    |this| {
                        this.child(
                            div()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(palette.border))
                                .bg(rgb(palette.bg))
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight(600.))
                                        .child(t!("dialog.telnetInputBehavior")),
                                )
                                .child(
                                    div()
                                        .grid()
                                        .grid_cols(3)
                                        .gap_2()
                                        .child(connection_editor_select(
                                            ConnectionEditorRenderContext {
                                                palette,
                                                fields,
                                                cx,
                                            },
                                            "connection-editor-telnet-backspace",
                                            t!("dialog.backspaceMode"),
                                            ConnectionEditorSelect::Backspace,
                                        ))
                                        .child(connection_editor_select(
                                            ConnectionEditorRenderContext {
                                                palette,
                                                fields,
                                                cx,
                                            },
                                            "connection-editor-telnet-enter-mode",
                                            t!("dialog.telnetEnterMode"),
                                            ConnectionEditorSelect::TelnetEnterMode,
                                        ))
                                        .child(connection_editor_select(
                                            ConnectionEditorRenderContext {
                                                palette,
                                                fields,
                                                cx,
                                            },
                                            "connection-editor-telnet-encoding",
                                            t!("connection.encoding"),
                                            ConnectionEditorSelect::Encoding,
                                        )),
                                ),
                        )
                    },
                )
                .when(
                    editor.telnet_advanced_tab == ConnectionEditorTelnetTab::Compatibility,
                    |this| {
                        this.child(
                            div()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(palette.border))
                                .bg(rgb(palette.bg))
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(palette.text_muted))
                                        .child(t!("dialog.telnetRawTcpCliDesc")),
                                )
                                .child(telnet_switch_row(
                                    palette,
                                    "connection-telnet-raw-tcp",
                                    t!("dialog.telnetRawTcpCli"),
                                    t!("dialog.telnetRawTcpCliLongDesc"),
                                    editor.raw_tcp_cli,
                                    true,
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_connection_editor_flag(
                                            ConnectionEditorToggle::RawTcp,
                                            cx,
                                        );
                                    }),
                                ))
                                .child(
                                    div()
                                        .grid()
                                        .grid_cols(2)
                                        .gap_2()
                                        .child(telnet_switch_row(
                                            palette,
                                            "connection-telnet-local-echo",
                                            t!("dialog.telnetLocalEcho"),
                                            t!("dialog.telnetLocalEchoDesc"),
                                            editor.local_echo,
                                            true,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_connection_editor_flag(
                                                    ConnectionEditorToggle::LocalEcho,
                                                    cx,
                                                );
                                            }),
                                        ))
                                        .child(telnet_switch_row(
                                            palette,
                                            "connection-telnet-local-line-edit",
                                            t!("dialog.telnetLocalLineEdit"),
                                            t!("dialog.telnetLocalLineEditDesc"),
                                            editor.local_line_edit,
                                            true,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_connection_editor_flag(
                                                    ConnectionEditorToggle::LocalLineEdit,
                                                    cx,
                                                );
                                            }),
                                        ))
                                        .child(telnet_switch_row(
                                            palette,
                                            "connection-telnet-force-character",
                                            t!("dialog.telnetForceCharAtATime"),
                                            t!("dialog.telnetForceCharAtATimeDesc"),
                                            editor.force_character_at_a_time,
                                            true,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_connection_editor_flag(
                                                    ConnectionEditorToggle::ForceCharacterAtATime,
                                                    cx,
                                                );
                                            }),
                                        ))
                                        .child(telnet_switch_row(
                                            palette,
                                            "connection-telnet-send-naws",
                                            t!("dialog.telnetSendNaws"),
                                            t!("dialog.telnetSendNawsDesc"),
                                            editor.send_naws,
                                            !editor.raw_tcp_cli,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_connection_editor_flag(
                                                    ConnectionEditorToggle::SendNaws,
                                                    cx,
                                                );
                                            }),
                                        ))
                                        .child(telnet_switch_row(
                                            palette,
                                            "connection-telnet-send-sga",
                                            t!("dialog.telnetSendSga"),
                                            t!("dialog.telnetSendSgaDesc"),
                                            editor.send_sga,
                                            !editor.raw_tcp_cli,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_connection_editor_flag(
                                                    ConnectionEditorToggle::SendSga,
                                                    cx,
                                                );
                                            }),
                                        )),
                                )
                                .child(
                                    div()
                                        .mt_3()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight(600.))
                                                .text_color(rgb(palette.text))
                                                .child(t!("dialog.telnetAutoLogin")),
                                        )
                                        .child(
                                            div()
                                                .grid()
                                                .grid_cols(2)
                                                .gap_2()
                                                .child(telnet_switch_row(
                                                    palette,
                                                    "connection-telnet-auto-login-enabled",
                                                    t!("dialog.enabled"),
                                                    t!("dialog.telnetAutoLoginDesc"),
                                                    editor.telnet_auto_login_enabled,
                                                    true,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.toggle_connection_editor_flag(
                                                            ConnectionEditorToggle::TelnetAutoLoginEnabled,
                                                            cx,
                                                        );
                                                    }),
                                                ))
                                                .child(telnet_switch_row(
                                                    palette,
                                                    "connection-telnet-auto-login-wake",
                                                    t!("dialog.telnetAutoLoginWakeEnter"),
                                                    t!("dialog.telnetAutoLoginWakeEnterDesc"),
                                                    editor.telnet_auto_login_send_wake_enter,
                                                    editor.telnet_auto_login_enabled,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.toggle_connection_editor_flag(
                                                            ConnectionEditorToggle::TelnetAutoLoginSendWakeEnter,
                                                            cx,
                                                        );
                                                    }),
                                                )),
                                        )
                                        .child(
                                            div()
                                                .grid()
                                                .grid_cols(2)
                                                .gap_2()
                                                .child(editor_stepper_field(
                                                    palette,
                                                    t!("dialog.telnetAutoLoginTimeout"),
                                                    ConnectionEditorField::TelnetAutoLoginTimeout,
                                                    fields,
                                                    cx,
                                                ))
                                                .child(editor_stepper_field(
                                                    palette,
                                                    t!("dialog.telnetAutoLoginMaxRetries"),
                                                    ConnectionEditorField::TelnetAutoLoginMaxRetries,
                                                    fields,
                                                    cx,
                                                )),
                                        )
                                        .child(editor_field(
                                            palette,
                                            t!("dialog.telnetAutoLoginUsernamePrompt"),
                                            ConnectionEditorField::TelnetAutoLoginUsernamePrompt,
                                            fields,
                                            cx,
                                        ))
                                        .child(editor_field(
                                            palette,
                                            t!("dialog.telnetAutoLoginPasswordPrompt"),
                                            ConnectionEditorField::TelnetAutoLoginPasswordPrompt,
                                            fields,
                                            cx,
                                        ))
                                        .child(editor_field(
                                            palette,
                                            t!("dialog.telnetAutoLoginSuccessPrompt"),
                                            ConnectionEditorField::TelnetAutoLoginSuccessPrompt,
                                            fields,
                                            cx,
                                        ))
                                        .child(editor_field(
                                            palette,
                                            t!("dialog.telnetAutoLoginFailurePrompt"),
                                            ConnectionEditorField::TelnetAutoLoginFailurePrompt,
                                            fields,
                                            cx,
                                        )),
                                ),
                        )
                    },
                )
        })
}
