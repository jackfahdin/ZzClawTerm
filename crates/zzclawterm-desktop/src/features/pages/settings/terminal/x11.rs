use rust_i18n::t;

use gpui::{ClipboardItem, Context, IntoElement, SharedString, div, prelude::*, px, rgb, svg};
use zzclawterm_ui::{ZzClawButton, ZzClawSelectOption, ZzClawTooltip};

use crate::features::pages::settings::panel::{SettingsPanel, X11ServerPresentation};
use crate::theme::ThemePalette;

use super::super::{
    settings_form_row, settings_form_section, settings_input_action_control, settings_switch,
    settings_switch_with_enabled,
};
use super::helpers::terminal_settings_field_meta;

const X11_PATH_SELECT_DEFAULT: &str = "default";

impl SettingsPanel {
    pub(in crate::features) fn terminal_x11_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let x11_display_input = self
            .existing_text_input_box("settings.terminal.x11-display", false)
            .into_any_element();
        let x11_server_path_input = self
            .existing_text_input_box("settings.terminal.x11-server-path", false)
            .into_any_element();
        let x11_server = self.settings.x11_server.clone();
        let (x11_server_enabled, x11_server_tray_icon, path_is_custom) = {
            let summary = self.settings.summary();
            (
                summary.x11_server_autostart,
                summary.x11_server_tray_icon,
                !summary.x11_server_path.trim().is_empty() || x11_server.path_select_custom,
            )
        };
        let options = self.settings.summary().x11_server_options;
        // The tray-icon switch only reaches a server at spawn time, so a
        // running instance we manage can lag the switch until it restarts.
        let restart_pending = x11_server.running
            && x11_server.started_by_us
            && (x11_server
                .tray_icon
                .is_some_and(|value| value != x11_server_tray_icon)
                || x11_server.options.is_some_and(|value| value != options));
        let preview = zzclawterm_transport::x11_server::x_server_arguments_preview(
            x11_server_tray_icon,
            options,
        );
        let mut dpi_values = vec![0, 96, 120, 144, 168, 192];
        if !dpi_values.contains(&options.dpi) {
            dpi_values.push(options.dpi);
        }
        let dpi_options = dpi_values
            .into_iter()
            .map(|dpi| {
                ZzClawSelectOption::new(
                    dpi.to_string(),
                    if dpi == 0 {
                        t!("settings.x11ServerDpiAuto").to_string()
                    } else {
                        dpi.to_string()
                    },
                )
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(x11_server_status_section(palette, &x11_server))
            .child(settings_form_section(
                palette,
                None,
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(terminal_settings_field_meta(
                                palette,
                                t!("settings.x11Display"),
                                t!("settings.x11DisplayDesc"),
                            ))
                            .child(div().w_full().max_w(px(520.)).child(x11_display_input)),
                    )
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerEnabled"),
                        None,
                        settings_switch_with_enabled(
                            palette,
                            "terminal-x11-server-enabled",
                            x11_server_enabled,
                            cfg!(windows) && !x11_server.action_pending && !x11_server.starting,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_x11_server_autostart(cx);
                            }),
                        ),
                    ))
                    .when(cfg!(windows), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .when(x11_server.running && x11_server.started_by_us, |this| {
                                    this.children(x11_server.arguments.as_ref().map(|args| {
                                        arguments_row(
                                            palette,
                                            "x11-current-arguments",
                                            t!("settings.x11ServerCurrentArgs").to_string(),
                                            args.clone(),
                                        )
                                    }))
                                })
                                .when(x11_server.running && !x11_server.started_by_us, |this| {
                                    this.child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(palette.text_muted))
                                            .child(t!("settings.x11ServerExternalArgs")),
                                    )
                                })
                                .when(
                                    !x11_server.running
                                        || restart_pending
                                        || !x11_server.started_by_us,
                                    |this| {
                                        this.child(arguments_row(
                                            palette,
                                            "x11-next-arguments",
                                            t!("settings.x11ServerNextArgs").to_string(),
                                            preview,
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(palette.text_muted))
                                                .child(t!("settings.x11ServerArgsPlaceholder")),
                                        )
                                    },
                                ),
                        )
                    })
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerPath"),
                        None,
                        self.settings_select_control(
                            "settings.terminal.x11-server-path-select",
                            vec![
                                ZzClawSelectOption::new(
                                    X11_PATH_SELECT_DEFAULT,
                                    t!("settings.x11ServerPathSelectDefault"),
                                ),
                                ZzClawSelectOption::new(
                                    "custom",
                                    t!("settings.x11ServerPathSelectCustom"),
                                ),
                            ],
                            if path_is_custom {
                                "custom"
                            } else {
                                X11_PATH_SELECT_DEFAULT
                            },
                            false,
                            cx,
                        ),
                    ))
                    .when(path_is_custom, |this| {
                        this.child(
                            div()
                                .w_full()
                                .max_w(px(520.))
                                .child(settings_input_action_control(
                                    520.,
                                    x11_server_path_input,
                                    ZzClawButton::new(
                                        "settings-x11-server-path-browse",
                                        t!("settings.browse"),
                                    )
                                    .form_control()
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.prompt_x11_server_path(cx);
                                        },
                                    )),
                                )),
                        )
                    })
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerTrayIcon"),
                        None,
                        settings_switch(
                            palette,
                            "terminal-x11-server-tray-icon",
                            x11_server_tray_icon,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_x11_server_tray_icon(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerClipboard"),
                        Some(t!("settings.x11ServerClipboardDesc").into()),
                        settings_switch(
                            palette,
                            "terminal-x11-clipboard",
                            options.clipboard,
                            cx.listener(|this, _, _, cx| this.toggle_x11_server_clipboard(cx)),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerPrimary"),
                        Some(t!("settings.x11ServerPrimaryDesc").into()),
                        settings_switch_with_enabled(
                            palette,
                            "terminal-x11-primary",
                            options.primary,
                            options.clipboard,
                            cx.listener(|this, _, _, cx| this.toggle_x11_server_primary(cx)),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerOpenGl"),
                        Some(t!("settings.x11ServerOpenGlDesc").into()),
                        settings_switch(
                            palette,
                            "terminal-x11-opengl",
                            options.native_opengl,
                            cx.listener(|this, _, _, cx| this.toggle_x11_server_opengl(cx)),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("settings.x11ServerDpi"),
                        Some(t!("settings.x11ServerDpiDesc").into()),
                        self.settings_select_control(
                            "settings.terminal.x11-server-dpi",
                            dpi_options,
                            &options.dpi.to_string(),
                            false,
                            cx,
                        ),
                    ))
                    .when(restart_pending, |this| {
                        this.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.danger))
                                .child(t!("settings.x11ServerRestartHint")),
                        )
                    }),
            ))
    }
}

/// Managed X server status: a large tinted icon (running = accent color, not
/// running = muted) carrying the details in a hover tooltip. Off Windows the
/// system X server serves forwarding, so a hint is shown instead.
fn x11_server_status_section(
    palette: ThemePalette,
    status: &X11ServerPresentation,
) -> impl IntoElement {
    let status_text = if status.starting {
        t!("settings.x11ServerStarting")
    } else if status.error.is_some() {
        t!("settings.x11ServerFailed")
    } else if !status.known {
        t!("settings.x11ServerStatusNeverStarted")
    } else if status.running && status.started_by_us {
        t!("settings.x11ServerStatusStartedByApp")
    } else if status.running {
        t!("settings.x11ServerStatusExternal")
    } else {
        t!("settings.x11ServerStatusNotRunning")
    };

    let icon_color = if status.running {
        palette.success
    } else {
        palette.text_muted
    };
    let mut tooltip_lines = vec![status_text.to_string()];
    if let Some(display) = status.display {
        tooltip_lines.push(format!(
            "{}: {display}",
            t!("settings.x11ServerDisplayLabel")
        ));
    }
    if let Some(error) = &status.error {
        tooltip_lines.push(error.clone());
    }
    if status.running {
        tooltip_lines.push(String::new());
        tooltip_lines.push(t!("settings.x11ServerListening").to_string());
        if !status.listeners_known {
            tooltip_lines.push(t!("settings.x11ServerListenersUnknown").to_string());
        } else if status.listening_displays.is_empty() {
            tooltip_lines.push(t!("settings.x11ServerNoListeners").to_string());
        } else {
            tooltip_lines.extend(status.listening_displays.iter().cloned());
            tooltip_lines.push(t!("settings.x11ServerFirewallHint").to_string());
        }
    }
    let tooltip_text: SharedString = tooltip_lines.join("\n").into();

    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .child(
            div()
                .id("terminal-x11-server-status-icon")
                .py_2()
                .tooltip(move |window, cx| {
                    ZzClawTooltip::new(tooltip_text.clone()).build(window, cx)
                })
                .child(
                    svg()
                        .size(px(56.))
                        .path("icons/settings/x11.svg")
                        .text_color(rgb(icon_color)),
                ),
        )
        .when(!cfg!(windows), |this| {
            this.child(
                div()
                    .pt_1()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(t!("settings.x11ServerSystemProvided")),
            )
        });

    settings_form_section(palette, Some(t!("settings.x11ServerStatus")), None, body)
}

/// Display the same argv used for spawning; copying never reads the auth file.
fn arguments_row(
    palette: ThemePalette,
    id: &'static str,
    label: String,
    arguments: String,
) -> impl IntoElement {
    let copy = arguments.clone();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_muted))
                        .child(label),
                )
                .child(
                    ZzClawButton::new(id, t!("settings.x11ServerCopyArgs")).on_click(
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                        },
                    ),
                ),
        )
        .child(
            div()
                .w_full()
                .text_size(px(11.))
                .text_color(rgb(palette.text_muted))
                .child(arguments),
        )
}
