use rust_i18n::t;

use std::borrow::Cow;

use gpui::{
    Context, FontWeight, IntoElement, ScrollDelta, ScrollWheelEvent, SharedString, div, prelude::*,
    px, rgb, rgba, svg,
};
use nyaterm_core::truncate_preview;
use nyaterm_transport::{
    CpuCoreUsage, RemoteGpu, RemoteGpuOverview, RemoteGpuProcess, RemoteNpu, RemoteNpuOverview,
    RemoteNpuProcess,
};
use nyaterm_ui::NyaScrollable;

use std::sync::Arc;

use super::panels::{PanelChrome, RemoteMonitorKind, RemoteMonitorPanel};
use crate::features::remote::NetworkHistorySample;
use crate::features::remote::StatsPresentationState;
use crate::features::remote::{
    ACCELERATOR_PROCESS_VIEWPORT_ROWS, GpuPresentationState, NpuPresentationState, max_list_offset,
};
use crate::features::{shell::gpui_code_font_family, view_widgets::stats_progress_bar};
use gpui::Entity;
use nyaterm_ui::{NyaInputState, NyaSearchInput};

use super::process::usage_color;

#[derive(Clone, Copy)]
struct ResourceRowPosition {
    first: bool,
    last: bool,
}

/// The Stats panel, rendered from a snapshot.
///
/// Takes no `NyaTermApp`. GPUI records every entity read during a view's render as a
/// dependency of that view, so a single app read here would re-dirty this panel on every
/// unrelated `app.notify()` -- which is exactly what the snapshot exists to prevent.
fn stats_empty_panel(
    text: impl Into<SharedString>,
    palette: crate::theme::ThemePalette,
) -> impl IntoElement {
    nyaterm_ui::empty_panel_with_icon(text, palette, "icons/resources.svg")
}

fn gpu_empty_panel(
    text: impl Into<SharedString>,
    palette: crate::theme::ThemePalette,
) -> impl IntoElement {
    nyaterm_ui::empty_panel_with_icon(text, palette, "icons/gpu.svg")
}

fn npu_empty_panel(
    text: impl Into<SharedString>,
    palette: crate::theme::ThemePalette,
) -> impl IntoElement {
    nyaterm_ui::empty_panel_with_icon(text, palette, "icons/npu.svg")
}
pub(in crate::features::pages::remote) fn stats_panel(
    chrome: PanelChrome,
    has_session: bool,
    stats_state: StatsPresentationState,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let palette = chrome.palette;
    if !has_session {
        return div()
            .size_full()
            .bg(chrome.transparent_surface)
            .child(stats_empty_panel(
                t!("panel.resourceMonitorNoSession"),
                palette,
            ))
            .into_any_element();
    }
    let Some(stats) = stats_state.data else {
        let message = if stats_state.pending {
            t!("common.loading")
        } else if stats_state.error {
            t!("panel.resourceMonitorError")
        } else {
            t!("common.loading")
        };
        return div()
            .size_full()
            .bg(chrome.transparent_surface)
            .child(stats_empty_panel(message, palette))
            .into_any_element();
    };

    let memory_total = stats.memory.used.saturating_add(stats.memory.available);
    let memory_percent = if memory_total > 0 {
        stats.memory.used as f64 / memory_total as f64 * 100.
    } else {
        0.
    };
    let cpu_usage = stats.cpu.usage;
    let system_label = t!("resourceMonitor.system").to_string();
    let hostname_label = t!("resourceMonitor.hostname").to_string();
    let arch_label = t!("resourceMonitor.arch").to_string();
    let os_label = t!("resourceMonitor.os").to_string();
    let uptime_label = t!("resourceMonitor.uptime").to_string();
    let cpu_label = t!("resourceMonitor.cpu").to_string();
    let cpu_average_label = t!("resourceMonitor.cpuAvgUsage").to_string();
    let load_1_label = t!("resourceMonitor.Load1").to_string();
    let load_5_label = t!("resourceMonitor.Load5").to_string();
    let load_15_label = t!("resourceMonitor.Load15").to_string();
    let memory_label = t!("resourceMonitor.memory").to_string();
    let available_label = t!("resourceMonitor.available").to_string();
    let cached_label = t!("resourceMonitor.cached").to_string();
    let network_label = t!("resourceMonitor.network").to_string();
    let disk_label = t!("resourceMonitor.disk").to_string();

    let mut network_rows = div().flex().flex_col();
    if stats.networks.is_empty() {
        network_rows = network_rows.child(resource_empty_value(palette));
    } else {
        let total = stats.networks.len();
        for (index, network) in stats.networks.iter().enumerate() {
            network_rows = network_rows.child(resource_network_row(
                palette,
                &network.nic,
                network.tx_bytes_per_sec,
                network.rx_bytes_per_sec,
                ResourceRowPosition {
                    first: index == 0,
                    last: index + 1 == total,
                },
            ));
        }
    }

    let mut disk_rows = div().flex().flex_col();
    if stats.disks.is_empty() {
        disk_rows = disk_rows.child(resource_empty_value(palette));
    } else {
        let total = stats.disks.len();
        for (index, disk) in stats.disks.iter().enumerate() {
            disk_rows = disk_rows.child(resource_disk_row(
                palette,
                &disk.mount,
                disk.total,
                disk.available,
                disk.use_percent,
                available_label.clone(),
                ResourceRowPosition {
                    first: index == 0,
                    last: index + 1 == total,
                },
            ));
        }
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(chrome.transparent_surface)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .p(px(10.))
                .flex()
                .flex_col()
                .gap_2()
                    .child(resource_section_card(
                        palette,
                        "icons/conn/server.svg",
                        system_label,
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap_x_3()
                            .gap_y_1()
                            .child(resource_info_cell(
                                palette,
                                hostname_label,
                                if stats.system.hostname.trim().is_empty() {
                                    "remote".to_string()
                                } else {
                                    stats.system.hostname.clone()
                                },
                            ))
                            .child(
                                resource_info_cell(
                                    palette,
                                    arch_label,
                                    stats.system.arch.clone(),
                                )
                                .text_right(),
                            )
                            .child(resource_info_cell(palette, os_label, stats.system.os.clone()))
                            .child(
                                resource_info_cell(
                                    palette,
                                    uptime_label,
                                    format_resource_uptime(stats.system.uptime_sec),
                                )
                                .text_right(),
                            ),
                    ))
                    .child(resource_section_card(
                        palette,
                        "icons/resources.svg",
                        cpu_label.clone(),
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(resource_ring_gauge(
                                        palette,
                                        stats.cpu.usage,
                                        format_resource_percent(cpu_usage),
                                    ))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_baseline()
                                                    .justify_between()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(palette.text_muted))
                                                            .child(cpu_average_label),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_family(
                                                                gpui_code_font_family(),
                                                            )
                                                            .text_size(px(13.))
                                                            .font_weight(FontWeight(700.))
                                                            .text_color(usage_color(
                                                                palette,
                                                                cpu_usage.unwrap_or(0.) / 100.,
                                                            ))
                                                            .child(format_optional_percent_decimal(cpu_usage)),
                                                    ),
                                            )
                                            .child(resource_progress_bar(
                                                palette,
                                                cpu_usage.unwrap_or(0.) / 100.,
                                            ))
                                            .child(
                                                div()
                                                    .text_right()
                                                    .font_family(
                                                        crate::features::shell::gpui_code_font_family(),
                                                    )
                                                    .text_size(px(10.))
                                                    .text_color(rgb(palette.text_dimmed))
                                                    .child(if cpu_usage.is_some() { format!("{}C", stats.cpu.cores) } else { t!("resourceMonitor.sampling").to_string() }),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(3)
                                    .gap_1()
                                    .child(resource_load_badge(
                                        palette,
                                        load_1_label,
                                        stats.load.load1,
                                    ))
                                    .child(resource_load_badge(
                                        palette,
                                        load_5_label,
                                        stats.load.load5,
                                    ))
                                    .child(resource_load_badge(
                                        palette,
                                        load_15_label,
                                        stats.load.load15,
                                    )),
                            )
                            .when(!stats.cpu.per_core.is_empty(), |this| {
                                this.child(cpu_core_summary(
                                    palette,
                                    &stats.cpu.per_core,
                                    stats_state.cpu_expanded,
                                    cpu_label,
                                    cx,
                                ))
                            }),
                    ))
                    .child(resource_section_card(
                        palette,
                        "icons/processes.svg",
                        memory_label,
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(resource_ring_gauge(
                                        palette,
                                        Some(memory_percent),
                                        format!("{memory_percent:.0}%"),
                                    ))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_baseline()
                                                    .justify_between()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(palette.text_muted))
                                                            .child("RAM"),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_family(
                                                                crate::features::shell::gpui_code_font_family(),
                                                            )
                                                            .text_size(px(13.))
                                                            .font_weight(FontWeight(700.))
                                                            .text_color(usage_color(
                                                                palette,
                                                                memory_percent / 100.,
                                                            ))
                                                            .child(format!("{memory_percent:.0}%")),
                                                    ),
                                            )
                                            .child(resource_progress_bar(
                                                palette,
                                                memory_percent / 100.,
                                            ))
                                            .child(
                                                div()
                                                    .font_family(
                                                        crate::features::shell::gpui_code_font_family(),
                                                    )
                                                    .text_size(px(10.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(format!(
                                                        "{} / {}",
                                                        format_resource_bytes(stats.memory.used),
                                                        format_resource_bytes(memory_total)
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_x_3()
                                    .gap_y_1()
                                    .child(resource_metric_chip(
                                        palette,
                                        available_label,
                                        format_resource_bytes(stats.memory.available),
                                    ))
                                    .child(resource_metric_chip(
                                        palette,
                                        cached_label,
                                        format_resource_bytes(stats.memory.cached),
                                    )),
                            ),
                    ))
                .child(resource_section_card(
                    palette,
                    "icons/network.svg",
                    network_label,
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .when(!stats_state.network_history.is_empty(), |this| {
                            this.child(resource_network_history_chart(
                                palette,
                                stats_state.network_history.clone(),
                            ))
                        })
                        .child(network_rows),
                ))
                .child(resource_section_card(palette, "icons/file/storage.svg", disk_label, disk_rows))
                .overflow_y_scrollbar()
                .id(SharedString::from("stats-scroll")),
            )
            .into_any_element()
}

fn resource_network_history_chart(
    palette: crate::theme::ThemePalette,
    history: Arc<[NetworkHistorySample]>,
) -> gpui::Div {
    let rx_color = rgb(palette.success);
    let tx_color = rgb(palette.primary);
    div()
        .relative()
        .h(px(72.))
        .w_full()
        .rounded_sm()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.surface))
        .child(
            gpui::canvas(
                move |_, _, _| {},
                move |bounds, _, window, _| {
                    let max_rate = history
                        .iter()
                        .flat_map(|sample| [sample.rx_bytes_per_sec, sample.tx_bytes_per_sec])
                        .fold(1.0_f64, f64::max);
                    for (is_rx, color) in [(true, rx_color), (false, tx_color)] {
                        let mut builder = gpui::PathBuilder::stroke(px(1.5));
                        let count = history.len().max(2);
                        for (index, sample) in history.iter().enumerate() {
                            let rate = if is_rx {
                                sample.rx_bytes_per_sec
                            } else {
                                sample.tx_bytes_per_sec
                            };
                            let x = bounds.origin.x
                                + bounds.size.width * (index as f32 / (count - 1) as f32);
                            let y = bounds.origin.y
                                + bounds.size.height
                                    * (1.0 - (rate / max_rate).clamp(0.0, 1.0) as f32);
                            let point = gpui::point(x, y);
                            if index == 0 {
                                builder.move_to(point);
                            } else {
                                builder.line_to(point);
                            }
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, color);
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// The GPU panel, rendered from a snapshot. See `stats_panel` on why there is no app.
pub(in crate::features::pages::remote) fn gpu_panel(
    chrome: PanelChrome,
    has_session: bool,
    gpu_state: GpuPresentationState,
    processes: Arc<[RemoteGpuProcess]>,
    search: Entity<NyaInputState>,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let palette = chrome.palette;
    // Built from the handle the snapshot carries. Reading that entity here is wanted:
    // typing notifies it, which invalidates this panel and nothing else.
    let gpu_search_input = NyaSearchInput::new("remote.gpu.filter", &search).into_any_element();
    if !has_session {
        return div()
            .size_full()
            .bg(chrome.transparent_surface)
            .child(gpu_empty_panel(t!("gpuMonitor.noSession"), palette))
            .into_any_element();
    }
    let content = match gpu_state.data.as_ref() {
        Some(overview) if overview.available && !overview.gpus.is_empty() => rich_gpu_panel(
            palette,
            overview,
            &gpu_state,
            processes,
            gpu_search_input,
            cx,
        ),
        Some(overview) if !overview.available => {
            gpu_empty_panel(t!("gpuMonitor.unavailable"), palette).into_any_element()
        }
        Some(_) => gpu_empty_panel(t!("gpuMonitor.noGpus"), palette).into_any_element(),
        None => {
            let message = if gpu_state.pending || !gpu_state.status.contains("failed") {
                t!("common.loading")
            } else {
                t!("gpuMonitor.error")
            };
            gpu_empty_panel(message, palette).into_any_element()
        }
    };
    div()
        .size_full()
        .overflow_hidden()
        .bg(chrome.transparent_surface)
        .child(content)
        .into_any_element()
}

/// The NPU panel, rendered from a snapshot. See `stats_panel` on why there is no app.
pub(in crate::features::pages::remote) fn npu_panel(
    chrome: PanelChrome,
    has_session: bool,
    npu_state: NpuPresentationState,
    processes: Arc<[RemoteNpuProcess]>,
    search: Entity<NyaInputState>,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let palette = chrome.palette;
    let npu_search_input = NyaSearchInput::new("remote.npu.filter", &search).into_any_element();
    if !has_session {
        return div()
            .size_full()
            .bg(chrome.transparent_surface)
            .child(npu_empty_panel(t!("ascendNpuMonitor.noSession"), palette))
            .into_any_element();
    }
    let content = match npu_state.data.as_ref() {
        Some(overview) if overview.available && !overview.npus.is_empty() => rich_npu_panel(
            palette,
            overview,
            &npu_state,
            processes,
            npu_search_input,
            cx,
        ),
        Some(overview) if !overview.available => {
            npu_empty_panel(t!("ascendNpuMonitor.unavailable"), palette).into_any_element()
        }
        Some(_) => npu_empty_panel(t!("ascendNpuMonitor.noNpus"), palette).into_any_element(),
        None => {
            let message = if npu_state.pending || !npu_state.status.contains("failed") {
                t!("common.loading")
            } else {
                t!("ascendNpuMonitor.error")
            };
            npu_empty_panel(message, palette).into_any_element()
        }
    };
    div()
        .size_full()
        .overflow_hidden()
        .bg(chrome.transparent_surface)
        .child(content)
        .into_any_element()
}

const ACCELERATOR_PROCESS_ROW_HEIGHT_PX: f32 = 56.;
const ACCELERATOR_PROCESS_LIST_MAX_HEIGHT_PX: f32 = 320.;
const ACCELERATOR_PROCESS_OVERSCAN: usize = 6;

#[derive(Clone)]
struct GpuSummary {
    count: usize,
    max_utilization: f64,
    max_temperature: Option<f64>,
    memory_total_mb: u64,
    memory_used_mb: u64,
}

#[derive(Clone)]
struct NpuSummary {
    count: usize,
    max_aicore: Option<f64>,
    max_temperature: Option<f64>,
    memory_total_mb: u64,
    memory_used_mb: u64,
}

fn rich_gpu_panel(
    palette: crate::theme::ThemePalette,
    overview: &RemoteGpuOverview,
    state: &GpuPresentationState,
    processes: Arc<[RemoteGpuProcess]>,
    search_input: gpui::AnyElement,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let summary = build_gpu_summary(overview);
    let normalized_query = state.search_draft.trim().to_ascii_lowercase();
    // Filtered, sorted and offset-clamped by `RemoteOpsFeatureState`, which reconciles
    // them when the overview or the query changes. This pass only reads them.
    let total_processes = processes.len();
    let offset = state.process_list_offset;
    let (visible_processes, pad_top, pad_bottom) = accelerator_visible_window(&processes, offset);
    let list_height = accelerator_process_list_height(total_processes);

    let mut cards = div().flex().flex_col().gap_2();
    for gpu in &overview.gpus {
        let key = gpu_device_key(gpu.index, &gpu.uuid);
        cards = cards.child(gpu_card(
            palette,
            gpu,
            state.expanded_devices.contains(&key),
            key,
            cx,
        ));
    }

    div()
        .id(SharedString::from("gpu-monitor-scroll"))
        .size_full()
        .overflow_scrollbar()
        .p(px(10.))
        .flex()
        .flex_col()
        .gap_2()
        .child(accelerator_summary_grid(
            palette,
            vec![
                (&t!("gpuMonitor.gpus"), summary.count.to_string()),
                (
                    &t!("gpuMonitor.maxUtilization"),
                    format_percent(summary.max_utilization),
                ),
                (
                    &t!("gpuMonitor.memory"),
                    format!(
                        "{} / {}",
                        format_memory_mb(summary.memory_used_mb),
                        format_memory_mb(summary.memory_total_mb)
                    ),
                ),
                (
                    &t!("gpuMonitor.maxTemperature"),
                    format_temperature(summary.max_temperature),
                ),
            ],
        ))
        .child(cards)
        .child(accelerator_process_section(
            palette,
            search_input,
            total_processes,
            AcceleratorProcessWindow {
                rows: visible_processes
                    .into_iter()
                    .map(|process| gpu_process_row(palette, &process))
                    .collect(),
                pad_top,
                pad_bottom,
                list_height,
            },
            if normalized_query.is_empty() {
                t!("gpuMonitor.noProcesses")
            } else {
                t!("gpuMonitor.noMatches")
            },
            cx.listener(move |panel, event: &ScrollWheelEvent, _, cx| {
                handle_accelerator_process_scroll(
                    event,
                    total_processes,
                    offset,
                    RemoteMonitorKind::Gpu,
                    panel,
                    cx,
                );
            }),
        ))
        .into_any_element()
}

fn rich_npu_panel(
    palette: crate::theme::ThemePalette,
    overview: &RemoteNpuOverview,
    state: &NpuPresentationState,
    processes: Arc<[RemoteNpuProcess]>,
    search_input: gpui::AnyElement,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let summary = build_npu_summary(overview);
    let normalized_query = state.search_draft.trim().to_ascii_lowercase();
    // Filtered, sorted and offset-clamped by `RemoteOpsFeatureState`, which reconciles
    // them when the overview or the query changes. This pass only reads them.
    let total_processes = processes.len();
    let offset = state.process_list_offset;
    let (visible_processes, pad_top, pad_bottom) = accelerator_visible_window(&processes, offset);
    let list_height = accelerator_process_list_height(total_processes);

    let mut cards = div().flex().flex_col().gap_2();
    for npu in &overview.npus {
        let key = npu.device_key.clone();
        cards = cards.child(npu_card(
            palette,
            npu,
            state.expanded_devices.contains(&key),
            key,
            cx,
        ));
    }

    div()
        .id(SharedString::from("npu-monitor-scroll"))
        .size_full()
        .overflow_scrollbar()
        .p(px(10.))
        .flex()
        .flex_col()
        .gap_2()
        .child(accelerator_summary_grid(
            palette,
            vec![
                (&t!("ascendNpuMonitor.npus"), summary.count.to_string()),
                (
                    &t!("ascendNpuMonitor.maxAicore"),
                    format_optional_percent(summary.max_aicore),
                ),
                (
                    &t!("ascendNpuMonitor.memory"),
                    format!(
                        "{} / {}",
                        format_memory_mb(summary.memory_used_mb),
                        format_memory_mb(summary.memory_total_mb)
                    ),
                ),
                (
                    &t!("ascendNpuMonitor.maxTemperature"),
                    format_temperature(summary.max_temperature),
                ),
            ],
        ))
        .child(cards)
        .child(accelerator_process_section(
            palette,
            search_input,
            total_processes,
            AcceleratorProcessWindow {
                rows: visible_processes
                    .into_iter()
                    .map(|process| npu_process_row(palette, &process))
                    .collect(),
                pad_top,
                pad_bottom,
                list_height,
            },
            if normalized_query.is_empty() {
                t!("ascendNpuMonitor.noProcesses")
            } else {
                t!("ascendNpuMonitor.noMatches")
            },
            cx.listener(move |panel, event: &ScrollWheelEvent, _, cx| {
                handle_accelerator_process_scroll(
                    event,
                    total_processes,
                    offset,
                    RemoteMonitorKind::Npu,
                    panel,
                    cx,
                );
            }),
        ))
        .into_any_element()
}

fn accelerator_summary_grid(
    palette: crate::theme::ThemePalette,
    items: Vec<(&str, String)>,
) -> gpui::Div {
    let mut grid = div().grid().grid_cols(2).gap_1();
    for (label, value) in items {
        grid = grid.child(
            div()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(rgba((palette.surface_elevated << 8) | 0x33))
                .px_2()
                .py_2()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .mt_1()
                        .min_w_0()
                        .overflow_hidden()
                        .font_family(gpui_code_font_family())
                        .text_size(px(14.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text))
                        .child(truncate_preview(&value, 22)),
                ),
        );
    }
    grid
}

fn gpu_card(
    palette: crate::theme::ThemePalette,
    gpu: &RemoteGpu,
    expanded: bool,
    key: String,
    cx: &mut Context<RemoteMonitorPanel>,
) -> impl IntoElement {
    let gpu_utilization = gpu.utilization_gpu_percent.unwrap_or(0.);
    let memory_percent = percent_from_parts(gpu.memory_used_mb, gpu.memory_total_mb);
    let severity = gpu_utilization.max(memory_percent);
    accelerator_device_card(
        palette,
        severity,
        key.clone(),
        "gpu-card",
        cx.listener(move |panel, _, _, cx| {
            let key = key.clone();
            panel.with_app(cx, move |app, cx| app.toggle_gpu_device_expanded(key, cx));
        }),
        AcceleratorDeviceCardContent {
            header: div()
                .flex()
                .items_start()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(accelerator_badge(
                            palette,
                            format!("GPU #{}", gpu.index),
                            true,
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .font_family(gpui_code_font_family())
                                .font_weight(FontWeight(700.))
                                .text_size(px(13.))
                                .text_color(rgb(palette.text))
                                .child(truncate_preview(
                                    if gpu.name.trim().is_empty() {
                                        t!("gpuMonitor.unknownGpu")
                                    } else {
                                        Cow::Borrowed(gpu.name.as_str())
                                    }
                                    .as_ref(),
                                    28,
                                )),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(accelerator_badge(
                            palette,
                            if gpu.pstate.trim().is_empty() {
                                "-".to_string()
                            } else {
                                gpu.pstate.clone()
                            },
                            false,
                        ))
                        .child(accelerator_chevron(palette, expanded)),
                ),
            metrics: div()
                .mt_2()
                .flex()
                .flex_col()
                .gap_2()
                .child(accelerator_metric_bar(
                    palette,
                    t!("gpuMonitor.gpuUtilization"),
                    gpu_utilization,
                    format_percent(gpu_utilization),
                ))
                .child(accelerator_metric_bar(
                    palette,
                    t!("gpuMonitor.memoryUtilization"),
                    memory_percent,
                    format!(
                        "{} / {}",
                        format_memory_mb(gpu.memory_used_mb),
                        format_memory_mb(gpu.memory_total_mb)
                    ),
                )),
            details: if expanded {
                gpu_details(palette, gpu).into_any_element()
            } else {
                div().into_any_element()
            },
        },
    )
}

fn npu_card(
    palette: crate::theme::ThemePalette,
    npu: &RemoteNpu,
    expanded: bool,
    key: String,
    cx: &mut Context<RemoteMonitorPanel>,
) -> impl IntoElement {
    let aicore_utilization = npu.utilization_aicore_percent.unwrap_or(0.);
    let memory_percent = percent_from_parts(npu.memory_used_mb, npu.memory_total_mb);
    let severity = aicore_utilization.max(memory_percent);
    accelerator_device_card(
        palette,
        severity,
        key.clone(),
        "npu-card",
        cx.listener(move |panel, _, _, cx| {
            let key = key.clone();
            panel.with_app(cx, move |app, cx| app.toggle_npu_device_expanded(key, cx));
        }),
        AcceleratorDeviceCardContent {
            header: div()
                .flex()
                .items_start()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(accelerator_badge(
                            palette,
                            format!("NPU #{}:{}", npu.index, npu.chip_id),
                            true,
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .font_family(gpui_code_font_family())
                                .font_weight(FontWeight(700.))
                                .text_size(px(13.))
                                .text_color(rgb(palette.text))
                                .child(truncate_preview(
                                    if npu.name.trim().is_empty() {
                                        t!("ascendNpuMonitor.unknownNpu")
                                    } else {
                                        Cow::Borrowed(npu.name.as_str())
                                    }
                                    .as_ref(),
                                    28,
                                )),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(accelerator_badge(
                            palette,
                            if npu.health.trim().is_empty() {
                                "-".to_string()
                            } else {
                                npu.health.clone()
                            },
                            false,
                        ))
                        .child(accelerator_chevron(palette, expanded)),
                ),
            metrics: div()
                .mt_2()
                .flex()
                .flex_col()
                .gap_2()
                .child(accelerator_metric_bar(
                    palette,
                    t!("ascendNpuMonitor.aicoreUtilization"),
                    aicore_utilization,
                    format_optional_percent(npu.utilization_aicore_percent),
                ))
                .child(accelerator_metric_bar(
                    palette,
                    t!("ascendNpuMonitor.memoryUtilization"),
                    memory_percent,
                    if npu.memory_total_mb > 0 {
                        format!(
                            "{} / {}",
                            format_memory_mb(npu.memory_used_mb),
                            format_memory_mb(npu.memory_total_mb)
                        )
                    } else {
                        "-".to_string()
                    },
                )),
            details: if expanded {
                npu_details(palette, npu).into_any_element()
            } else {
                div().into_any_element()
            },
        },
    )
}

struct AcceleratorDeviceCardContent {
    header: gpui::Div,
    metrics: gpui::Div,
    details: gpui::AnyElement,
}

fn accelerator_device_card(
    palette: crate::theme::ThemePalette,
    severity_percent: f64,
    key: String,
    id_prefix: &'static str,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    content: AcceleratorDeviceCardContent,
) -> impl IntoElement {
    let AcceleratorDeviceCardContent {
        header,
        metrics,
        details,
    } = content;
    let accent = accelerator_accent_color(palette, severity_percent);
    div()
        .id(SharedString::from(format!("{id_prefix}-{key}")))
        .rounded_md()
        .border_1()
        .border_color(rgb(accent))
        .bg(rgba((palette.surface_elevated << 8) | 0x33))
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(|this| this.bg(rgba(0x38bdf80f)))
        .on_click(on_toggle)
        .child(header)
        .child(metrics)
        .child(details)
}

fn accelerator_badge(
    palette: crate::theme::ThemePalette,
    label: String,
    primary: bool,
) -> gpui::Div {
    div()
        .flex_none()
        .rounded_md()
        .border_1()
        .border_color(if primary {
            rgba((palette.link << 8) | 0x66)
        } else {
            rgb(palette.border)
        })
        .bg(if primary {
            rgba((palette.link << 8) | 0x1f)
        } else {
            rgba((palette.input << 8) | 0x80)
        })
        .px_1()
        .py(px(2.))
        .font_family(gpui_code_font_family())
        .text_size(px(10.))
        .font_weight(FontWeight(700.))
        .text_color(if primary {
            rgb(palette.link)
        } else {
            rgb(palette.text)
        })
        .child(label)
}

fn accelerator_chevron(palette: crate::theme::ThemePalette, expanded: bool) -> gpui::Svg {
    svg()
        .size(px(16.))
        .path(if expanded {
            "icons/chevron-down.svg"
        } else {
            "icons/fe/forward.svg"
        })
        .text_color(rgb(palette.text_muted))
}

fn accelerator_metric_bar(
    palette: crate::theme::ThemePalette,
    label: impl Into<SharedString>,
    value_percent: f64,
    detail: String,
) -> gpui::Div {
    let label: SharedString = label.into();
    let ratio = (value_percent / 100.).clamp(0., 1.);
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .text_size(px(11.))
                .child(
                    div()
                        .text_color(rgb(palette.text_muted))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .font_family(gpui_code_font_family())
                        .text_color(rgb(palette.text_muted))
                        .child(detail),
                ),
        )
        .child(
            div()
                .h(px(6.))
                .w_full()
                .rounded_md()
                .bg(rgb(palette.input))
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(ratio as f32))
                        .bg(rgb(accelerator_accent_color(palette, value_percent))),
                ),
        )
}

fn gpu_details(palette: crate::theme::ThemePalette, gpu: &RemoteGpu) -> gpui::Div {
    accelerator_detail_grid(
        palette,
        vec![
            (
                &t!("gpuMonitor.uuid"),
                if gpu.uuid.trim().is_empty() {
                    "-".to_string()
                } else {
                    gpu.uuid.clone()
                },
            ),
            (
                &t!("gpuMonitor.temperature"),
                format_temperature(gpu.temperature_c),
            ),
            (
                &t!("gpuMonitor.power"),
                format_power(gpu.power_draw_w, gpu.power_limit_w),
            ),
            (
                &t!("gpuMonitor.fan"),
                format_optional_percent(gpu.fan_speed_percent),
            ),
            (
                &t!("gpuMonitor.memoryFree"),
                format_memory_mb(gpu.memory_free_mb),
            ),
        ],
    )
}

fn npu_details(palette: crate::theme::ThemePalette, npu: &RemoteNpu) -> gpui::Div {
    accelerator_detail_grid(
        palette,
        vec![
            (
                &t!("ascendNpuMonitor.device"),
                if npu.device_key.trim().is_empty() {
                    "-".to_string()
                } else {
                    npu.device_key.clone()
                },
            ),
            (
                &t!("ascendNpuMonitor.busId"),
                if npu.bus_id.trim().is_empty() {
                    "-".to_string()
                } else {
                    npu.bus_id.clone()
                },
            ),
            (
                &t!("ascendNpuMonitor.physicalId"),
                npu.physical_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            (
                &t!("ascendNpuMonitor.temperature"),
                format_temperature(npu.temperature_c),
            ),
            (
                &t!("ascendNpuMonitor.power"),
                format_watts(npu.power_draw_w),
            ),
            (
                &t!("ascendNpuMonitor.memoryFree"),
                if npu.memory_total_mb > 0 {
                    format_memory_mb(npu.memory_free_mb)
                } else {
                    "-".to_string()
                },
            ),
        ],
    )
}

fn accelerator_detail_grid(
    palette: crate::theme::ThemePalette,
    items: Vec<(&str, String)>,
) -> gpui::Div {
    let mut grid = div().mt_2().grid().grid_cols(2).gap_1();
    for (label, value) in items {
        grid = grid.child(
            div()
                .rounded_md()
                .bg(rgba((palette.input << 8) | 0x66))
                .px_2()
                .py_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(truncate_preview(label, 18)),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_right()
                        .font_family(gpui_code_font_family())
                        .text_size(px(10.))
                        .text_color(rgb(palette.text))
                        .child(truncate_preview(&value, 28)),
                ),
        );
    }
    grid
}

struct AcceleratorProcessWindow {
    rows: Vec<gpui::Div>,
    pad_top: f32,
    pad_bottom: f32,
    list_height: f32,
}

fn accelerator_process_section(
    palette: crate::theme::ThemePalette,
    search_input: gpui::AnyElement,
    total_processes: usize,
    window: AcceleratorProcessWindow,
    empty_message: impl Into<SharedString>,
    on_scroll: impl Fn(&ScrollWheelEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Div {
    let AcceleratorProcessWindow {
        rows,
        pad_top,
        pad_bottom,
        list_height,
    } = window;
    let empty_message: SharedString = empty_message.into();
    let mut list_rows = div().flex().flex_col();
    if total_processes == 0 {
        list_rows = list_rows.child(
            div()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .p_4()
                .text_center()
                .text_size(px(12.))
                .text_color(rgb(palette.text_muted))
                .child(empty_message.to_string()),
        );
    } else {
        if pad_top > 0. {
            list_rows = list_rows.child(div().h(px(pad_top)).w_full().flex_none());
        }
        for row in rows {
            list_rows = list_rows.child(row);
        }
        if pad_bottom > 0. {
            list_rows = list_rows.child(div().h(px(pad_bottom)).w_full().flex_none());
        }
    }

    div().flex().flex_col().gap_2().child(search_input).child(
        div()
            .h(px(list_height))
            .max_h(px(ACCELERATOR_PROCESS_LIST_MAX_HEIGHT_PX))
            .overflow_hidden()
            .flex()
            .flex_col()
            .on_scroll_wheel(on_scroll)
            .child(list_rows),
    )
}

fn gpu_process_row(palette: crate::theme::ThemePalette, process: &RemoteGpuProcess) -> gpui::Div {
    accelerator_process_row(
        palette,
        if process.process_name.trim().is_empty() {
            "-".to_string()
        } else {
            process.process_name.clone()
        },
        format!(
            "{} {}   {} {}",
            t!("gpuMonitor.pid"),
            process.pid,
            t!("gpuMonitor.gpu"),
            process
                .gpu_index
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format_memory_mb(process.used_memory_mb),
    )
}

fn npu_process_row(palette: crate::theme::ThemePalette, process: &RemoteNpuProcess) -> gpui::Div {
    accelerator_process_row(
        palette,
        if process.process_name.trim().is_empty() {
            "-".to_string()
        } else {
            process.process_name.clone()
        },
        format!(
            "{} {}   {} {}:{}",
            t!("ascendNpuMonitor.pid"),
            process.pid,
            t!("ascendNpuMonitor.npu"),
            process.npu_index,
            process.chip_id
        ),
        format_memory_mb(process.used_memory_mb),
    )
}

fn accelerator_process_row(
    palette: crate::theme::ThemePalette,
    name: String,
    meta: String,
    memory: String,
) -> gpui::Div {
    div()
        .h(px(ACCELERATOR_PROCESS_ROW_HEIGHT_PX))
        .flex_none()
        .pb_1()
        .child(
            div()
                .size_full()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(rgba((palette.surface_elevated << 8) | 0x33))
                .px_2()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .overflow_hidden()
                                .font_weight(FontWeight(700.))
                                .text_size(px(12.))
                                .text_color(rgb(palette.text))
                                .child(truncate_preview(&name, 32)),
                        )
                        .child(
                            div()
                                .mt(px(2.))
                                .font_family(gpui_code_font_family())
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .child(meta),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family(gpui_code_font_family())
                        .text_size(px(12.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text))
                        .child(memory),
                ),
        )
}

fn handle_accelerator_process_scroll(
    event: &ScrollWheelEvent,
    total: usize,
    current: usize,
    kind: RemoteMonitorKind,
    panel: &RemoteMonitorPanel,
    cx: &mut Context<RemoteMonitorPanel>,
) {
    let max_offset = max_list_offset(total, ACCELERATOR_PROCESS_VIEWPORT_ROWS);
    if max_offset == 0 {
        return;
    }
    let delta_rows = match event.delta {
        ScrollDelta::Lines(delta) => delta.y,
        ScrollDelta::Pixels(delta) => f32::from(delta.y) / ACCELERATOR_PROCESS_ROW_HEIGHT_PX,
    };
    let next = (current as f32 - delta_rows)
        .round()
        .clamp(0., max_offset as f32) as usize;
    // The offset write and the snapshot flush happen on the event, not in render.
    let moved = panel.with_app(cx, move |app, cx| {
        let moved = match kind {
            RemoteMonitorKind::Gpu => app.remote_ops.set_gpu_process_offset(next),
            _ => app.remote_ops.set_npu_process_offset(next),
        };
        if moved {
            cx.notify();
        }
        moved
    });
    if moved {
        cx.stop_propagation();
    }
}

fn accelerator_visible_window<T: Clone>(items: &[T], offset: usize) -> (Vec<T>, f32, f32) {
    let total = items.len();
    let window_capacity = ACCELERATOR_PROCESS_VIEWPORT_ROWS + ACCELERATOR_PROCESS_OVERSCAN * 2;
    let start = offset.saturating_sub(ACCELERATOR_PROCESS_OVERSCAN);
    let end = (start + window_capacity).min(total);
    let visible = items.get(start..end).unwrap_or(&[]).to_vec();
    let pad_top = start as f32 * ACCELERATOR_PROCESS_ROW_HEIGHT_PX;
    let pad_bottom = total.saturating_sub(end) as f32 * ACCELERATOR_PROCESS_ROW_HEIGHT_PX;
    (visible, pad_top, pad_bottom)
}

fn accelerator_process_list_height(total: usize) -> f32 {
    (total as f32 * ACCELERATOR_PROCESS_ROW_HEIGHT_PX)
        .min(ACCELERATOR_PROCESS_LIST_MAX_HEIGHT_PX)
        .max(if total == 0 {
            ACCELERATOR_PROCESS_ROW_HEIGHT_PX
        } else {
            0.
        })
}

fn build_gpu_summary(overview: &RemoteGpuOverview) -> GpuSummary {
    let temperatures = overview
        .gpus
        .iter()
        .filter_map(|gpu| gpu.temperature_c)
        .collect::<Vec<_>>();
    GpuSummary {
        count: overview.gpus.len(),
        max_utilization: overview
            .gpus
            .iter()
            .filter_map(|gpu| gpu.utilization_gpu_percent)
            .fold(0., f64::max),
        max_temperature: temperatures.into_iter().reduce(f64::max),
        memory_total_mb: overview.gpus.iter().map(|gpu| gpu.memory_total_mb).sum(),
        memory_used_mb: overview.gpus.iter().map(|gpu| gpu.memory_used_mb).sum(),
    }
}

fn build_npu_summary(overview: &RemoteNpuOverview) -> NpuSummary {
    let temperatures = overview
        .npus
        .iter()
        .filter_map(|npu| npu.temperature_c)
        .collect::<Vec<_>>();
    let aicore_values = overview
        .npus
        .iter()
        .filter_map(|npu| npu.utilization_aicore_percent)
        .collect::<Vec<_>>();
    NpuSummary {
        count: overview.npus.len(),
        max_aicore: aicore_values.into_iter().reduce(f64::max),
        max_temperature: temperatures.into_iter().reduce(f64::max),
        memory_total_mb: overview.npus.iter().map(|npu| npu.memory_total_mb).sum(),
        memory_used_mb: overview.npus.iter().map(|npu| npu.memory_used_mb).sum(),
    }
}

fn gpu_device_key(index: u32, uuid: &str) -> String {
    let uuid = uuid.trim();
    if uuid.is_empty() {
        index.to_string()
    } else {
        uuid.to_string()
    }
}

fn accelerator_accent_color(palette: crate::theme::ThemePalette, value_percent: f64) -> u32 {
    if value_percent >= 90. {
        palette.danger
    } else if value_percent >= 70. {
        palette.warning
    } else {
        palette.success
    }
}

fn percent_from_parts(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.
    } else {
        used as f64 / total as f64 * 100.
    }
}

fn format_percent(value: f64) -> String {
    format!("{:.0}%", value.clamp(0., 100.))
}

fn format_optional_percent(value: Option<f64>) -> String {
    value.map(format_percent).unwrap_or_else(|| "-".to_string())
}

fn format_memory_mb(value: u64) -> String {
    if value >= 1024 {
        let gib = value as f64 / 1024.;
        if gib < 10. {
            format!("{gib:.1} GiB")
        } else {
            format!("{:.0} GiB", gib)
        }
    } else {
        format!("{value} MiB")
    }
}

fn format_temperature(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.0} C", value))
        .unwrap_or_else(|| "-".to_string())
}

fn format_power(draw: Option<f64>, limit: Option<f64>) -> String {
    match (draw, limit) {
        (None, None) => "-".to_string(),
        (None, Some(limit)) => format!("- / {}", format_watts(Some(limit))),
        (Some(draw), None) => format_watts(Some(draw)),
        (Some(draw), Some(limit)) => {
            format!(
                "{} / {}",
                format_watts(Some(draw)),
                format_watts(Some(limit))
            )
        }
    }
}

fn format_watts(value: Option<f64>) -> String {
    value
        .map(|value| {
            if value < 100. {
                format!("{value:.1} W")
            } else {
                format!("{value:.0} W")
            }
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_resource_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024. && unit + 1 < UNITS.len() {
        value /= 1024.;
        unit += 1;
    }
    if value < 10. {
        format!("{value:.2} {}", UNITS[unit])
    } else if value < 100. {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

fn format_resource_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec <= 0. {
        return "0 B/s".to_string();
    }
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = bytes_per_sec;
    let mut unit = 0;
    while value >= 1024. && unit + 1 < UNITS.len() {
        value /= 1024.;
        unit += 1;
    }
    if value < 10. {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

fn format_resource_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    if days == 1 {
        t!("resourceMonitor.day", count = days).to_string()
    } else {
        t!("resourceMonitor.days", count = days).to_string()
    }
}

fn format_resource_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.0}%", value.clamp(0., 100.)))
        .unwrap_or_else(|| "--".to_string())
}

fn format_optional_percent_decimal(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.1}%", value.clamp(0., 100.)))
        .unwrap_or_else(|| "--".to_string())
}
fn resource_section_card(
    palette: crate::theme::ThemePalette,
    icon_path: &'static str,
    title: String,
    child: impl IntoElement,
) -> gpui::Div {
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.bg))
        .px_3()
        .py(px(10.))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(crate::features::view_widgets::mono_icon(
                    icon_path,
                    rgb(palette.text_muted).into(),
                    14.,
                ))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text))
                        .child(title),
                ),
        )
        .child(child)
}

fn resource_info_cell(
    palette: crate::theme::ThemePalette,
    label: String,
    value: impl Into<String>,
) -> gpui::Div {
    div()
        .min_w_0()
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(label),
        )
        .child(
            div()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(12.))
                .font_weight(FontWeight(600.))
                .text_color(rgb(palette.text))
                .overflow_hidden()
                .child(truncate_preview(&value.into(), 42)),
        )
}

fn resource_ring_gauge(
    palette: crate::theme::ThemePalette,
    percent: Option<f64>,
    label: String,
) -> gpui::Div {
    let ratio = (percent.unwrap_or(0.) / 100.).clamp(0., 1.);
    let track = rgb(palette.border);
    let accent = usage_color(palette, ratio);
    div()
        .relative()
        .size(px(56.))
        .rounded_full()
        .bg(rgb(palette.surface))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            gpui::canvas(
                move |_, _, _| {},
                move |bounds, _, window, _| {
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    let center = gpui::point(
                        bounds.origin.x + px(width / 2.),
                        bounds.origin.y + px(height / 2.),
                    );
                    let radius = width.min(height) / 2. - 3.;
                    if let Some(path) = resource_ring_path(center, radius, 1.) {
                        window.paint_path(path, track);
                    }
                    if ratio > 0.
                        && let Some(path) = resource_ring_path(center, radius, ratio as f32)
                    {
                        window.paint_path(path, accent);
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(
            div()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(12.))
                .font_weight(FontWeight(700.))
                .text_color(usage_color(palette, ratio))
                .child(label),
        )
}

fn resource_ring_path(
    center: gpui::Point<gpui::Pixels>,
    radius: f32,
    ratio: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    let ratio = ratio.clamp(0., 1.);
    let segments = (64. * ratio).ceil().max(1.) as usize;
    let mut builder = gpui::PathBuilder::stroke(px(5.));
    for index in 0..=segments {
        let progress = index as f32 / segments as f32 * ratio;
        let angle = -std::f32::consts::FRAC_PI_2 + progress * std::f32::consts::TAU;
        let point = gpui::point(
            center.x + px(angle.cos() * radius),
            center.y + px(angle.sin() * radius),
        );
        if index == 0 {
            builder.move_to(point);
        } else {
            builder.line_to(point);
        }
    }
    builder.build().ok()
}

fn resource_load_badge(
    palette: crate::theme::ThemePalette,
    label: String,
    value: f64,
) -> gpui::Div {
    div()
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.input))
        .px_2()
        .py(px(6.))
        .text_center()
        .child(
            div()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(12.))
                .font_weight(FontWeight(700.))
                .text_color(rgb(palette.text))
                .overflow_hidden()
                .child(format!("{value:.2}")),
        )
        .child(
            div()
                .mt(px(2.))
                .text_size(px(9.))
                .text_color(rgb(palette.text_dimmed))
                .overflow_hidden()
                .child(label),
        )
}

fn resource_metric_chip(
    palette: crate::theme::ThemePalette,
    label: String,
    value: String,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(label),
        )
        .child(
            div()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(11.))
                .text_color(rgb(palette.text_muted))
                .child(value),
        )
}

fn resource_network_row(
    palette: crate::theme::ThemePalette,
    nic: &str,
    tx: f64,
    rx: f64,
    position: ResourceRowPosition,
) -> gpui::Div {
    let ResourceRowPosition { first, last } = position;
    div()
        .when(!first, |this| this.pt_2())
        .when(!last, |this| {
            this.pb_2().border_b_1().border_color(rgb(palette.border))
        })
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(12.))
                .font_weight(FontWeight(600.))
                .text_color(rgb(palette.text))
                .overflow_hidden()
                .child(truncate_preview(nic, 34)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .flex_wrap()
                .child(rate_value(
                    palette,
                    "icons/fe/up.svg",
                    tx,
                    rgb(0x22c55e).into(),
                ))
                .child(rate_value(
                    palette,
                    "icons/arrow-down.svg",
                    rx,
                    rgb(0x3b82f6).into(),
                )),
        )
}

fn resource_disk_row(
    palette: crate::theme::ThemePalette,
    mount: &str,
    total: u64,
    available: u64,
    use_percent: u32,
    available_label: String,
    position: ResourceRowPosition,
) -> gpui::Div {
    let ResourceRowPosition { first, last } = position;
    let ratio = use_percent as f64 / 100.;
    div()
        .when(!first, |this| this.pt_2())
        .when(!last, |this| {
            this.pb_2().border_b_1().border_color(rgb(palette.border))
        })
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .text_size(px(12.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(palette.text))
                        .overflow_hidden()
                        .child(truncate_preview(mount, 42)),
                )
                .child(
                    div()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .text_size(px(12.))
                        .font_weight(FontWeight(700.))
                        .text_color(usage_color(palette, ratio))
                        .child(format!("{use_percent}%")),
                ),
        )
        .child(resource_progress_bar(palette, ratio))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_x_2()
                .gap_y_1()
                .child(
                    div()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(format_resource_bytes(total)),
                )
                .child(resource_metric_chip(
                    palette,
                    available_label,
                    format_resource_bytes(available),
                )),
        )
}

fn rate_value(
    palette: crate::theme::ThemePalette,
    arrow: &'static str,
    value: f64,
    color: gpui::Hsla,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .font_family(crate::features::shell::gpui_code_font_family())
        .text_size(px(11.))
        .text_color(rgb(palette.text_muted))
        .child(crate::features::view_widgets::mono_icon(arrow, color, 11.))
        .child(format_resource_rate(value))
}

fn resource_empty_value(palette: crate::theme::ThemePalette) -> gpui::Div {
    div()
        .py_2()
        .text_size(px(12.))
        .text_color(rgb(palette.text_dimmed))
        .child("-")
}

fn resource_progress_bar(palette: crate::theme::ThemePalette, ratio: f64) -> impl IntoElement {
    stats_progress_bar(palette, ratio)
}

fn cpu_core_summary(
    palette: crate::theme::ThemePalette,
    per_core: &[CpuCoreUsage],
    expanded: bool,
    cpu_label: String,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::Div {
    let summary = format!("{} {cpu_label}", per_core.len());

    let mut rows = div().flex().flex_col().gap_1().child(
        div()
            .id(SharedString::from("stats-cpu-cores-toggle"))
            .flex()
            .items_center()
            .gap_1()
            .text_size(px(11.))
            .text_color(rgb(palette.text_muted))
            .cursor_pointer()
            .hover(|this| this.bg(rgb(palette.input)))
            // The panel hops to the app on the event, never during render, and
            // `with_app` flushes the snapshot the mutation invalidated.
            .on_click(cx.listener(|panel, _, _, cx| {
                panel.with_app(cx, |app, cx| app.toggle_stats_cpu_expanded(cx));
            }))
            .child(
                svg()
                    .size(px(13.))
                    .path(if expanded {
                        "icons/chevron-up.svg"
                    } else {
                        "icons/chevron-down.svg"
                    })
                    .text_color(rgb(palette.text_muted)),
            )
            .child(summary),
    );

    if expanded {
        let mut core_rows = div().flex().flex_col().gap_1().pt_1();
        for core in per_core {
            core_rows = core_rows.child(cpu_core_row(palette, core.id, core.usage));
        }
        rows = rows.child(core_rows);
    }

    rows
}

fn cpu_core_row(palette: crate::theme::ThemePalette, index: u32, usage: f64) -> gpui::Div {
    let ratio = (usage / 100.).clamp(0., 1.);
    div()
        .h(px(22.))
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .w(px(24.))
                .text_right()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .child(format!("cpu{index}")),
        )
        .child(
            div()
                .size(px(6.))
                .rounded_full()
                .bg(usage_color(palette, ratio)),
        )
        .child(div().flex_1().child(resource_progress_bar(palette, ratio)))
        .child(
            div()
                .w(px(44.))
                .text_right()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .child(format!("{usage:.1}%")),
        )
}

#[cfg(test)]
mod tests {
    use nyaterm_transport::{RemoteGpu, RemoteGpuOverview, RemoteNpu, RemoteNpuOverview};

    use super::{
        build_gpu_summary, build_npu_summary, format_memory_mb, format_resource_bytes,
        format_resource_percent, format_resource_rate, format_temperature,
    };

    fn gpu(index: u32, used: u64, total: u64, temp: Option<f64>, util: Option<f64>) -> RemoteGpu {
        RemoteGpu {
            index,
            uuid: format!("gpu-{index}"),
            name: format!("GPU {index}"),
            temperature_c: temp,
            utilization_gpu_percent: util,
            utilization_memory_percent: None,
            memory_total_mb: total,
            memory_used_mb: used,
            memory_free_mb: total.saturating_sub(used),
            power_draw_w: None,
            power_limit_w: None,
            fan_speed_percent: None,
            pstate: String::new(),
        }
    }

    fn npu(
        index: u32,
        chip_id: u32,
        used: u64,
        total: u64,
        temp: Option<f64>,
        util: Option<f64>,
    ) -> RemoteNpu {
        RemoteNpu {
            index,
            chip_id,
            physical_id: None,
            device_key: format!("npu-{index}-{chip_id}"),
            name: format!("NPU {index}:{chip_id}"),
            health: String::new(),
            bus_id: String::new(),
            temperature_c: temp,
            utilization_aicore_percent: util,
            utilization_memory_percent: None,
            memory_total_mb: total,
            memory_used_mb: used,
            memory_free_mb: total.saturating_sub(used),
            memory_kind: "HBM".to_string(),
            hbm_total_mb: None,
            hbm_used_mb: None,
            power_draw_w: None,
        }
    }

    #[test]
    fn accelerator_summaries_and_formatters_cover_empty_optional_values() {
        let gpu_summary = build_gpu_summary(&RemoteGpuOverview {
            available: true,
            gpus: vec![
                gpu(0, 1536, 4096, Some(42.), Some(70.)),
                gpu(1, 512, 2048, None, None),
            ],
            ..Default::default()
        });
        assert_eq!(gpu_summary.count, 2);
        assert_eq!(gpu_summary.max_utilization, 70.);
        assert_eq!(gpu_summary.max_temperature, Some(42.));
        assert_eq!(gpu_summary.memory_used_mb, 2048);
        assert_eq!(gpu_summary.memory_total_mb, 6144);

        let npu_summary = build_npu_summary(&RemoteNpuOverview {
            available: true,
            npus: vec![
                npu(0, 0, 1024, 2048, Some(39.), Some(12.)),
                npu(1, 0, 2048, 4096, Some(55.), None),
            ],
            ..Default::default()
        });
        assert_eq!(npu_summary.count, 2);
        assert_eq!(npu_summary.max_aicore, Some(12.));
        assert_eq!(npu_summary.max_temperature, Some(55.));
        assert_eq!(npu_summary.memory_used_mb, 3072);
        assert_eq!(npu_summary.memory_total_mb, 6144);

        assert_eq!(format_memory_mb(512), "512 MiB");
        assert_eq!(format_memory_mb(1536), "1.5 GiB");
        assert_eq!(format_temperature(None), "-");
    }

    #[test]
    fn resource_formatters_match_tauri_precision_and_warmup_placeholders() {
        assert_eq!(format_resource_bytes(0), "0 B");
        assert_eq!(format_resource_bytes(95_596_000_000), "89.0 GB");
        assert_eq!(format_resource_bytes(135_291_469_824), "126 GB");
        assert_eq!(format_resource_rate(0.), "0 B/s");
        assert_eq!(format_resource_rate(4.6 * 1024.), "4.6 KB/s");
        assert_eq!(format_resource_percent(None), "--");
        assert_eq!(format_resource_percent(Some(23.2)), "23%");
    }
}
