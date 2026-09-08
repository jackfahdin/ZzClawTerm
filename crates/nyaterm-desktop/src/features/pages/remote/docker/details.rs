use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;
use nyaterm_transport::{DockerContainer, DockerContainerDetails};
use nyaterm_ui::NyaScrollable;

use super::super::panels::RemoteMonitorPanel;
use crate::features::{
    formatting::compact_id, formatting::docker_state_color, shell::gpui_code_font_family,
    view_widgets::modal_dialog_shell,
};
use crate::theme::ThemePalette;
use crate::widgets::{empty_panel, small_button, status_pill};

use super::DockerLabels;

pub(in crate::features::pages::remote) fn docker_details_panel(
    palette: ThemePalette,
    dialog_bg: gpui::Rgba,
    container_id: Option<String>,
    details: Option<DockerContainerDetails>,
    container: Option<DockerContainer>,
    labels: DockerLabels,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let Some(details) = details else {
        let details_id = container_id
            .as_deref()
            .map(compact_id)
            .unwrap_or_else(|| "unknown".to_string());
        let card = div()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text))
                            .child(labels.container_details.clone()),
                    )
                    .when_some(container_id.clone(), |this, _| {
                        this.child(small_button(
                            palette,
                            "docker-details-loading-close",
                            labels.close.clone(),
                            cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.close_docker_details(cx);
                                });
                            }),
                        ))
                    }),
            )
            .child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.section_header))
                    .p_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(labels.loading.clone()),
                    )
                    .child(
                        div()
                            .rounded_sm()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(rgb(0x93c5fd))
                            .bg(rgb(0x17233a))
                            .child(details_id),
                    ),
            );

        return modal_dialog_shell(palette, dialog_bg, "docker-details-modal", 620., card)
            .into_any_element();
    };

    let mut mounts = div().flex().flex_col().gap_1();
    if details.mounts.is_empty() {
        mounts = mounts.child(empty_panel(labels.no_matches.clone(), palette));
    } else {
        for mount in &details.mounts {
            mounts = mounts.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.section_header))
                    .px_2()
                    .py_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .font_family(gpui_code_font_family())
                            .text_size(px(11.))
                            .text_color(rgb(palette.text))
                            .overflow_hidden()
                            .child(format!(
                                "{} -> {}",
                                truncate_preview(&mount.source, 52),
                                truncate_preview(&mount.destination, 52)
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(format!(
                                "{} · {} · {}",
                                mount.kind,
                                mount.mode,
                                if mount.rw { "rw" } else { "ro" }
                            )),
                    ),
            );
        }
    }

    let mut networks = div().flex().flex_col().gap_1();
    if details.networks.is_empty() {
        networks = networks.child(empty_panel(labels.no_matches.clone(), palette));
    } else {
        for network in &details.networks {
            let ip_address = if network.ip_address.trim().is_empty() {
                "no ip".to_string()
            } else {
                network.ip_address.clone()
            };
            networks = networks.child(
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text))
                            .overflow_hidden()
                            .child(truncate_preview(&network.name, 36)),
                    )
                    .child(
                        div()
                            .font_family(gpui_code_font_family())
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(ip_address),
                    ),
            );
        }
    }

    let details_title = container
        .as_ref()
        .map(|container| truncate_preview(&container.name, 40))
        .unwrap_or_else(|| labels.container_details.to_string());
    let details_state = container.as_ref().map(|container| container.state.clone());
    let networks_value = docker_networks_value(&details);
    let mounts_value = docker_mounts_value(&details);
    let mut actions = div().flex().items_center().gap_2();
    if let Some(container_id) = container_id.clone() {
        actions = actions
            .child(small_button(
                palette,
                format!("docker-details-refresh-{}", compact_id(&container_id)),
                labels.refresh.clone(),
                cx.listener(move |panel, _, _window, cx| {
                    panel.with_app(cx, |this, cx| {
                        this.load_docker_details(container_id.clone(), cx);
                    });
                }),
            ))
            .child(small_button(
                palette,
                "docker-details-close",
                labels.close.clone(),
                cx.listener(|panel, _, _, cx| {
                    panel.with_app(cx, |this, cx| {
                        this.close_docker_details(cx);
                    });
                }),
            ));
    }
    let card = div()
        .id("docker-details-scroll")
        .max_h(px(600.))
        .overflow_scrollbar()
        .p_4()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight(700.))
                                .text_color(rgb(palette.text))
                                .child(details_title),
                        )
                        .when_some(details_state, |this, state| {
                            this.child(status_pill(
                                labels.state_label(&state),
                                docker_state_color(palette, &state),
                                rgb(0x17233a),
                            ))
                        }),
                )
                .child(actions),
        )
        .child(
            div()
                .grid()
                .grid_cols(3)
                .gap_2()
                .child(docker_metric(
                    palette,
                    labels.cpu.clone(),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| format!("{:.1}%", stats.cpu_percent))
                        .unwrap_or_else(|| "n/a".to_string()),
                ))
                .child(docker_metric(
                    palette,
                    labels.memory.clone(),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| format!("{:.1}%", stats.memory_percent))
                        .unwrap_or_else(|| "n/a".to_string()),
                ))
                .child(docker_metric(
                    palette,
                    labels.pids.clone(),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| stats.pids.clone())
                        .unwrap_or_else(|| "n/a".to_string()),
                )),
        )
        .when_some(container.clone(), |this, container| {
            this.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.section_header))
                    .p_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text))
                            .child(labels.identity.clone()),
                    )
                    .child(docker_detail_line(
                        palette,
                        labels.container_name.clone(),
                        container.name.clone(),
                        truncate_preview(&container.name, 72),
                        true,
                        labels.copy.clone(),
                        cx,
                    ))
                    .child(docker_detail_line(
                        palette,
                        labels.container_id.clone(),
                        container.id.clone(),
                        truncate_preview(&container.id, 72),
                        true,
                        labels.copy.clone(),
                        cx,
                    ))
                    .child(docker_detail_line(
                        palette,
                        labels.image.clone(),
                        container.image.clone(),
                        truncate_preview(&container.image, 72),
                        true,
                        labels.copy.clone(),
                        cx,
                    ))
                    .child(docker_detail_line(
                        palette,
                        labels.status.clone(),
                        if container.status.trim().is_empty() {
                            container.state.clone()
                        } else {
                            container.status.clone()
                        },
                        truncate_preview(
                            if container.status.trim().is_empty() {
                                &container.state
                            } else {
                                &container.status
                            },
                            72,
                        ),
                        false,
                        labels.copy.clone(),
                        cx,
                    ))
                    .child(docker_detail_line(
                        palette,
                        labels.created_at.clone(),
                        container.created_at.clone(),
                        truncate_preview(&container.created_at, 72),
                        false,
                        labels.copy.clone(),
                        cx,
                    ))
                    .child(docker_detail_line(
                        palette,
                        labels.size.clone(),
                        if container.size.trim().is_empty() {
                            "-".to_string()
                        } else {
                            container.size.clone()
                        },
                        if container.size.trim().is_empty() {
                            "-".to_string()
                        } else {
                            container.size.clone()
                        },
                        false,
                        labels.copy.clone(),
                        cx,
                    )),
            )
        })
        .child(
            div()
                .grid()
                .grid_cols(1)
                .gap_3()
                .child(
                    div()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.section_header))
                        .p_2()
                        .child(docker_detail_line(
                            palette,
                            labels.started_at.clone(),
                            details.started_at.clone(),
                            truncate_preview(&details.started_at, 52),
                            false,
                            labels.copy.clone(),
                            cx,
                        ))
                        .child(docker_detail_line(
                            palette,
                            labels.finished_at.clone(),
                            details.finished_at.clone(),
                            truncate_preview(&details.finished_at, 52),
                            false,
                            labels.copy.clone(),
                            cx,
                        ))
                        .child(docker_detail_line(
                            palette,
                            labels.restart_count.clone(),
                            details.restart_count.to_string(),
                            details.restart_count.to_string(),
                            false,
                            labels.copy.clone(),
                            cx,
                        ))
                        .child(docker_detail_line(
                            palette,
                            labels.entrypoint.clone(),
                            details.entrypoint.clone(),
                            truncate_preview(&details.entrypoint, 72),
                            true,
                            labels.copy.clone(),
                            cx,
                        ))
                        .child(docker_detail_line(
                            palette,
                            labels.command.clone(),
                            details.command.clone(),
                            truncate_preview(&details.command, 72),
                            true,
                            labels.copy.clone(),
                            cx,
                        )),
                )
                .child(
                    div()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.section_header))
                        .p_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight(700.))
                                .text_color(rgb(palette.text))
                                .child(labels.networking.clone()),
                        )
                        .when_some(container.as_ref(), |this, container| {
                            this.child(docker_detail_line(
                                palette,
                                labels.ports.clone(),
                                if container.ports.trim().is_empty() {
                                    "-".to_string()
                                } else {
                                    docker_ports_value(&container.ports)
                                },
                                if container.ports.trim().is_empty() {
                                    "-".to_string()
                                } else {
                                    truncate_preview(&docker_ports_value(&container.ports), 96)
                                },
                                true,
                                labels.copy.clone(),
                                cx,
                            ))
                        })
                        .child(docker_detail_line(
                            palette,
                            labels.networks.clone(),
                            networks_value.clone(),
                            truncate_preview(&networks_value, 96),
                            true,
                            labels.copy.clone(),
                            cx,
                        ))
                        .child(networks),
                ),
        )
        .child(
            div()
                .rounded_sm()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(rgb(palette.section_header))
                .p_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text))
                        .child(labels.io.clone()),
                )
                .child(docker_detail_line(
                    palette,
                    labels.net_io.clone(),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| stats.net_io.clone())
                        .unwrap_or_else(|| "-".to_string()),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| truncate_preview(&stats.net_io, 96))
                        .unwrap_or_else(|| "-".to_string()),
                    false,
                    labels.copy.clone(),
                    cx,
                ))
                .child(docker_detail_line(
                    palette,
                    labels.block_io.clone(),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| stats.block_io.clone())
                        .unwrap_or_else(|| "-".to_string()),
                    details
                        .stats
                        .as_ref()
                        .map(|stats| truncate_preview(&stats.block_io, 96))
                        .unwrap_or_else(|| "-".to_string()),
                    false,
                    labels.copy.clone(),
                    cx,
                )),
        )
        .child(
            div()
                .rounded_sm()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(rgb(palette.section_header))
                .p_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text_muted))
                        .child(labels.mounts.clone()),
                )
                .child(docker_detail_line(
                    palette,
                    labels.mounts.clone(),
                    mounts_value.clone(),
                    truncate_preview(&mounts_value, 120),
                    true,
                    labels.copy.clone(),
                    cx,
                ))
                .child(mounts),
        );

    modal_dialog_shell(palette, dialog_bg, "docker-details-modal", 620., card).into_any_element()
}

fn docker_metric(
    palette: crate::theme::ThemePalette,
    label: impl Into<SharedString>,
    value: impl Into<gpui::SharedString>,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .rounded_sm()
        .bg(rgb(palette.surface))
        .px_3()
        .py_2()
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .child(label),
        )
        .child(
            div()
                .mt_1()
                .font_family(gpui_code_font_family())
                .text_size(px(11.))
                .text_color(rgb(palette.text))
                .child(value.into()),
        )
}

fn docker_detail_line(
    palette: crate::theme::ThemePalette,
    label: impl Into<SharedString>,
    value: String,
    display_value: String,
    copyable: bool,
    copy_label: impl Into<SharedString>,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::Div {
    let label: SharedString = label.into();
    let copy_label: SharedString = copy_label.into();
    let copy_value = value.clone();
    div()
        .mt_1()
        .flex()
        .items_start()
        .justify_between()
        .gap_2()
        .child(
            div()
                .w(px(72.))
                .flex_none()
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(label.clone()),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .font_family(gpui_code_font_family())
                .text_size(px(11.))
                .line_height(px(15.))
                .text_color(rgb(palette.text))
                .child(display_value),
        )
        .when(copyable && value.trim() != "-", |this| {
            this.child(small_button(
                palette,
                format!("docker-details-copy-{label}"),
                copy_label,
                cx.listener({
                    let label = label.clone();
                    move |panel, _, _, cx| {
                        panel.with_app(cx, |this, cx| {
                            this.copy_docker_text(copy_value.clone(), &label, cx);
                        });
                    }
                }),
            ))
        })
}

fn docker_ports_value(ports: &str) -> String {
    let value = ports
        .split(',')
        .map(str::trim)
        .filter(|port| !port.is_empty())
        .map(|port| port.replace("->", " -> "))
        .collect::<Vec<_>>()
        .join("\n");
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value
    }
}

fn docker_networks_value(details: &DockerContainerDetails) -> String {
    let value = details
        .networks
        .iter()
        .map(|network| {
            if network.ip_address.trim().is_empty() {
                network.name.clone()
            } else {
                format!("{}: {}", network.name, network.ip_address)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value
    }
}

fn docker_mounts_value(details: &DockerContainerDetails) -> String {
    let value = details
        .mounts
        .iter()
        .map(|mount| {
            let access = if mount.rw { "rw" } else { "ro" };
            let mode = if mount.mode.trim().is_empty() {
                access.to_string()
            } else {
                format!("{access},{}", mount.mode)
            };
            format!(
                "{} {} -> {} ({mode})",
                if mount.kind.trim().is_empty() {
                    "mount"
                } else {
                    &mount.kind
                },
                if mount.source.trim().is_empty() {
                    "-"
                } else {
                    &mount.source
                },
                if mount.destination.trim().is_empty() {
                    "-"
                } else {
                    &mount.destination
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value
    }
}
