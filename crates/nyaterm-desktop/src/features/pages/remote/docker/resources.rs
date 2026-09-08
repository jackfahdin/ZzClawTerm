use gpui::{
    Context, FontWeight, IntoElement, ScrollDelta, ScrollWheelEvent, SharedString, div, prelude::*,
    px, rgb,
};
use nyaterm_core::truncate_preview;
use nyaterm_transport::{DockerImage, DockerNetwork, DockerVolume};
use nyaterm_ui::NyaScrollable;

use super::super::panels::RemoteMonitorPanel;
use crate::features::remote::DOCKER_RESOURCE_VIEWPORT_ROWS;
use crate::features::{formatting::compact_id, shell::gpui_code_font_family};
use crate::models::{DockerConfirmAction, DockerConfirmState};
use crate::theme::ThemePalette;
use crate::widgets::{empty_panel, svg_icon_button};

use super::DockerLabels;

const DOCKER_RESOURCE_ROW_PX: f32 = 64.;
const DOCKER_RESOURCE_OVERSCAN: usize = 6;

pub(in crate::features::pages::remote) fn docker_images_panel(
    palette: ThemePalette,
    images: &[DockerImage],
    list_offset: usize,
    labels: DockerLabels,
    cx: &mut Context<RemoteMonitorPanel>,
) -> impl IntoElement {
    if images.is_empty() {
        return docker_resource_empty(palette, "Images", labels.no_matches.clone());
    }

    let total = images.len();
    let (window_start, window_end, pad_top, pad_bottom, scroll_offset) =
        docker_resource_window(total, list_offset);
    let mut rows = div().flex().flex_col().gap(px(6.));
    if pad_top > 0. {
        rows = rows.child(div().h(px(pad_top)).w_full().flex_none());
    }
    for image in images.get(window_start..window_end).unwrap_or(&[]) {
        let image_id = image.id.clone();
        let label = docker_image_label(image);
        let row_labels = labels.clone();
        rows = rows.child(
            docker_resource_row(
                palette,
                label.clone(),
                format!(
                    "{} · {} · {}",
                    compact_id(&image.id),
                    image.created_since,
                    image.size
                ),
            )
            .child(svg_icon_button(
                format!("docker-image-remove-{}", compact_id(&image_id)),
                "icons/fe/delete.svg",
                14.,
                palette,
                cx.listener(move |panel, _, window, cx| {
                    panel.with_app(cx, |this, cx| {
                        this.request_docker_confirm(
                            DockerConfirmState {
                                title: row_labels.confirm_action_title.to_string(),
                                detail: row_labels
                                    .confirm_description(&row_labels.remove_image, &label),
                                action: DockerConfirmAction::ImageRemove {
                                    image_id: image_id.clone(),
                                    force: false,
                                },
                            },
                            window,
                            cx,
                        );
                    });
                }),
            )),
        );
    }
    if pad_bottom > 0. {
        rows = rows.child(div().h(px(pad_bottom)).w_full().flex_none());
    }
    docker_resource_panel(palette, "Images", total, rows, scroll_offset, cx)
}

pub(in crate::features::pages::remote) fn docker_volumes_panel(
    palette: ThemePalette,
    volumes: &[DockerVolume],
    list_offset: usize,
    labels: DockerLabels,
    cx: &mut Context<RemoteMonitorPanel>,
) -> impl IntoElement {
    if volumes.is_empty() {
        return docker_resource_empty(palette, "Volumes", labels.no_matches.clone());
    }

    let total = volumes.len();
    let (window_start, window_end, pad_top, pad_bottom, scroll_offset) =
        docker_resource_window(total, list_offset);
    let mut rows = div().flex().flex_col().gap(px(6.));
    if pad_top > 0. {
        rows = rows.child(div().h(px(pad_top)).w_full().flex_none());
    }
    for volume in volumes.get(window_start..window_end).unwrap_or(&[]) {
        let volume_name = volume.name.clone();
        let row_labels = labels.clone();
        rows = rows.child(
            docker_resource_row(
                palette,
                volume.name.clone(),
                row_labels.volume_driver_label(&volume.driver),
            )
            .child(svg_icon_button(
                format!("docker-volume-remove-{volume_name}"),
                "icons/fe/delete.svg",
                14.,
                palette,
                cx.listener(move |panel, _, window, cx| {
                    panel.with_app(cx, |this, cx| {
                        this.request_docker_confirm(
                            DockerConfirmState {
                                title: row_labels.confirm_action_title.to_string(),
                                detail: row_labels
                                    .confirm_description(&row_labels.remove_volume, &volume_name),
                                action: DockerConfirmAction::VolumeRemove {
                                    volume_name: volume_name.clone(),
                                    force: false,
                                },
                            },
                            window,
                            cx,
                        );
                    });
                }),
            )),
        );
    }
    if pad_bottom > 0. {
        rows = rows.child(div().h(px(pad_bottom)).w_full().flex_none());
    }
    docker_resource_panel(palette, "Volumes", total, rows, scroll_offset, cx)
}

pub(in crate::features::pages::remote) fn docker_networks_panel(
    palette: ThemePalette,
    networks: &[DockerNetwork],
    list_offset: usize,
    labels: DockerLabels,
    cx: &mut Context<RemoteMonitorPanel>,
) -> impl IntoElement {
    if networks.is_empty() {
        return docker_resource_empty(palette, "Networks", labels.no_matches.clone());
    }

    let total = networks.len();
    let (window_start, window_end, pad_top, pad_bottom, scroll_offset) =
        docker_resource_window(total, list_offset);
    let mut rows = div().flex().flex_col().gap(px(6.));
    if pad_top > 0. {
        rows = rows.child(div().h(px(pad_top)).w_full().flex_none());
    }
    for network in networks.get(window_start..window_end).unwrap_or(&[]) {
        let network_id = network.id.clone();
        let name = network.name.clone();
        let row_labels = labels.clone();
        rows = rows.child(
            docker_resource_row(
                palette,
                network.name.clone(),
                format!(
                    "{} · {} · {}",
                    compact_id(&network.id),
                    network.driver,
                    network.scope
                ),
            )
            .child(svg_icon_button(
                format!("docker-network-remove-{}", compact_id(&network_id)),
                "icons/fe/delete.svg",
                14.,
                palette,
                cx.listener(move |panel, _, window, cx| {
                    panel.with_app(cx, |this, cx| {
                        this.request_docker_confirm(
                            DockerConfirmState {
                                title: row_labels.confirm_action_title.to_string(),
                                detail: row_labels
                                    .confirm_description(&row_labels.remove_network, &name),
                                action: DockerConfirmAction::NetworkRemove {
                                    network_id: network_id.clone(),
                                },
                            },
                            window,
                            cx,
                        );
                    });
                }),
            )),
        );
    }
    if pad_bottom > 0. {
        rows = rows.child(div().h(px(pad_bottom)).w_full().flex_none());
    }
    docker_resource_panel(palette, "Networks", total, rows, scroll_offset, cx)
}

fn docker_resource_window(total: usize, list_offset: usize) -> (usize, usize, f32, f32, usize) {
    let window_capacity = DOCKER_RESOURCE_VIEWPORT_ROWS + DOCKER_RESOURCE_OVERSCAN * 2;
    let max_offset = total.saturating_sub(DOCKER_RESOURCE_VIEWPORT_ROWS.min(total));
    let scroll_row = list_offset.min(max_offset);
    let window_start = scroll_row.saturating_sub(DOCKER_RESOURCE_OVERSCAN);
    let window_end = (window_start + window_capacity).min(total);
    let pad_top = (window_start as f32) * DOCKER_RESOURCE_ROW_PX;
    let pad_bottom = ((total.saturating_sub(window_end)) as f32) * DOCKER_RESOURCE_ROW_PX;
    (window_start, window_end, pad_top, pad_bottom, scroll_row)
}

fn docker_resource_empty(
    palette: ThemePalette,
    title: &'static str,
    message: impl Into<SharedString>,
) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!(
            "docker-resource-{}",
            title.to_ascii_lowercase()
        )))
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(empty_panel(message, palette))
        .into_any_element()
}

pub(in crate::features::pages::remote) fn docker_resource_panel(
    _palette: ThemePalette,
    title: &'static str,
    count: usize,
    rows: impl IntoElement,
    _scroll_offset: usize,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    // Tauri resource tabs: full-height virtual list + wheel offset.
    let _ = title;
    let total_for_scroll = count;
    div()
        .id(SharedString::from(format!(
            "docker-resource-{}",
            title.to_ascii_lowercase()
        )))
        .size_full()
        .overflow_hidden()
        .flex()
        .flex_col()
        .on_scroll_wheel(cx.listener(move |panel, event: &ScrollWheelEvent, _, cx| {
            panel.with_app(cx, |this, cx| {
                let max_offset = total_for_scroll
                    .saturating_sub(DOCKER_RESOURCE_VIEWPORT_ROWS.min(total_for_scroll));
                if max_offset == 0 {
                    return;
                }
                let delta_rows = match event.delta {
                    ScrollDelta::Lines(delta) => delta.y,
                    ScrollDelta::Pixels(delta) => f32::from(delta.y) / DOCKER_RESOURCE_ROW_PX,
                };
                let current = this.remote_ops.docker_presentation().resource_list_offset;
                let next = (current as f32 - delta_rows)
                    .round()
                    .clamp(0., max_offset as f32) as usize;
                if this.remote_ops.set_docker_resource_offset(next) {
                    cx.stop_propagation();
                    cx.notify();
                }
            });
        }))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .px_2()
                .pb_2()
                .flex()
                .flex_col()
                .child(rows),
        )
        .into_any_element()
}

pub(in crate::features::pages::remote) fn docker_resource_static_panel(
    _palette: ThemePalette,
    title: &'static str,
    _count: usize,
    rows: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!(
            "docker-resource-{}",
            title.to_ascii_lowercase()
        )))
        .size_full()
        .overflow_scrollbar()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(rows)
}

pub(in crate::features::pages::remote) fn docker_resource_row(
    palette: ThemePalette,
    title: String,
    detail: String,
) -> gpui::Div {
    // ~64px Tauri SIMPLE_ROW_HEIGHT-ish dense resource row (slightly tighter chrome).
    div()
        .h(px(58.))
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .px_3()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .hover(|this| this.bg(rgb(palette.hover)))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(palette.text))
                        .overflow_hidden()
                        .child(truncate_preview(&title, 48)),
                )
                .child(
                    div()
                        .font_family(gpui_code_font_family())
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_dimmed))
                        .overflow_hidden()
                        .child(truncate_preview(&detail, 72)),
                ),
        )
}

pub(in crate::features::pages::remote) fn docker_image_label(image: &DockerImage) -> String {
    match (
        image.repository.trim().is_empty(),
        image.tag.trim().is_empty(),
    ) {
        (true, true) => compact_id(&image.id),
        (false, true) => image.repository.clone(),
        (true, false) => image.tag.clone(),
        (false, false) => format!("{}:{}", image.repository, image.tag),
    }
}
