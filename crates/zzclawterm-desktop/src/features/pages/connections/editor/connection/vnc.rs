use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, SharedString, div,
    prelude::{
        FluentBuilder, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled,
    },
    px, rgb, rgba, svg,
};
use zzclawterm_ui::{ZzClawSwitch, ZzClawTabItem, ZzClawTabs};

use crate::features::{ZzClawTermApp, connections::ConnectionEditorToggle};
use crate::models::{
    ConnectionEditorCredentialOverlay, ConnectionEditorField, ConnectionEditorPasswordSource,
    ConnectionEditorSelect,
};

use super::super::super::list::{
    ConnectionEditorRenderContext, EditorSecretFieldOptions, connection_editor_select,
    editor_field, editor_secret_field, editor_stepper_field, required,
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
    let ConnectionEditorSectionContext {
        palette,
        editor,
        fields,
        baud_popover_open: _,
    } = section;
    let password_enabled = editor.vnc_security.mode != "none";
    let auth_tabs = ZzClawTabs::new("connection-vnc-auth-tabs")
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
            if !password_enabled {
                return;
            }
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
    div()
        .debug_selector(|| "connection-editor-vnc-section".to_string())
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
                .flex_col()
                .gap_1()
                .opacity(if password_enabled { 1.0 } else { 0.5 })
                .child(auth_tabs),
        )
        .when(editor.auth_mode == "password", |this| {
            this.when(
                editor.password_source == ConnectionEditorPasswordSource::Direct,
                |this| {
                    this.child(editor_secret_field(
                        palette,
                        t!("dialog.password"),
                        ConnectionEditorField::Password,
                        fields,
                        EditorSecretFieldOptions::new(
                            !password_enabled,
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
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(t!("dialog.vncPasswordLimit")),
                    )
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
                                        "connection-editor-vnc-saved-password",
                                        t!("dialog.savedPassword"),
                                        ConnectionEditorSelect::SavedPassword,
                                    )),
                            )
                            .child(
                                zzclawterm_ui::ZzClawButton::new(
                                    "connection-editor-vnc-manage-passwords",
                                    t!("dialog.managePasswords"),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.set_connection_editor_credential_overlay(
                                            Some(ConnectionEditorCredentialOverlay::Passwords),
                                            cx,
                                        );
                                    },
                                )),
                            ),
                    )
                },
            )
        })
        .child(
            div()
                .id("connection-editor-vnc-advanced-toggle")
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
                        .path(if editor.advanced.vnc {
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
        .when(editor.advanced.vnc, |this| {
            this.child(
                div()
                    .id("connection-editor-vnc-advanced")
                    .debug_selector(|| "connection-editor-vnc-advanced".to_string())
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .p_3()
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
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
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
                            ),
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
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
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
                                    this.toggle_connection_editor_flag(
                                        ConnectionEditorToggle::VncShared,
                                        cx,
                                    );
                                }),
                            ))
                            .child(vnc_switch_row(
                                palette,
                                "connection-vnc-view-only",
                                t!("dialog.vncViewOnly"),
                                t!("dialog.vncViewOnlyDesc"),
                                editor.vnc_view_only,
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_connection_editor_flag(
                                        ConnectionEditorToggle::VncViewOnly,
                                        cx,
                                    );
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
                    )),
            )
        })
}
