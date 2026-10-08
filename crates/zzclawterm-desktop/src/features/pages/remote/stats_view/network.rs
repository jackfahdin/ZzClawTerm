use gpui::{Entity, FontWeight, Hsla, IntoElement, div, prelude::*, px, rgb};
use rust_i18n::t;
use zzclawterm_ui::plot::{ZzClawLinePlot, ZzClawLinePoint, ZzClawLineSeries};
use zzclawterm_ui::{NYA_FORM_CONTROL_HEIGHT_PX, ZzClawSelect, ZzClawSelectState};

use super::{format_resource_rate, resource_empty_value, resource_section_card_with_action};
use crate::features::remote::StatsPresentationState;
use crate::features::shell::gpui_code_font_family;
use crate::features::view_widgets::mono_icon;
use crate::theme::ThemePalette;

pub(super) fn resource_network_card(
    palette: ThemePalette,
    state: &StatsPresentationState,
    select: &Entity<ZzClawSelectState>,
) -> gpui::Div {
    let has_interfaces = state
        .data
        .as_ref()
        .is_some_and(|stats| !stats.networks.is_empty());
    let action = has_interfaces.then(|| {
        div()
            .w(px(120.))
            .min_w_0()
            .h(px(NYA_FORM_CONTROL_HEIGHT_PX))
            .child(ZzClawSelect::new(select).appearance(false))
            .into_any_element()
    });
    let mut body = div().flex().flex_col().gap_2();
    if has_interfaces {
        let traffic = state.network_traffic().unwrap_or_default();
        let rx_color: Hsla = rgb(0x3b82f6).into();
        let tx_color: Hsla = rgb(0x22c55e).into();
        body = body
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_2()
                    .child(network_metric(
                        palette,
                        t!("resourceMonitor.download").to_string(),
                        traffic.rx_bytes_per_sec,
                        "icons/arrow-down.svg",
                        rx_color,
                    ))
                    .child(network_metric(
                        palette,
                        t!("resourceMonitor.upload").to_string(),
                        traffic.tx_bytes_per_sec,
                        "icons/fe/up.svg",
                        tx_color,
                    )),
            )
            .child(network_history_chart(palette, state, rx_color, tx_color));
    } else {
        body = body.child(resource_empty_value(palette));
    }
    resource_section_card_with_action(
        palette,
        "icons/network.svg",
        t!("resourceMonitor.network").to_string(),
        action,
        body,
    )
}

fn network_metric(
    palette: ThemePalette,
    label: String,
    value: f64,
    arrow: &'static str,
    color: Hsla,
) -> gpui::Div {
    div()
        .min_w_0()
        .rounded_md()
        .bg(rgb(palette.surface))
        .p_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .child(mono_icon(arrow, color, 11.))
                .child(label),
        )
        .child(
            div()
                .font_family(gpui_code_font_family())
                .text_size(px(13.))
                .font_weight(FontWeight(700.))
                .text_color(color)
                .child(format_resource_rate(value)),
        )
}

fn network_history_chart(
    palette: ThemePalette,
    state: &StatsPresentationState,
    rx_color: Hsla,
    tx_color: Hsla,
) -> gpui::Div {
    let interface = state.selected_network_interface.as_deref();
    let samples = state
        .network_history
        .iter()
        .map(|sample| (sample.sampled_at, sample.traffic(interface)))
        .collect::<Vec<_>>();
    let available = samples
        .iter()
        .filter(|(_, traffic)| traffic.is_some())
        .collect::<Vec<_>>();
    let chart_shell = || {
        div()
            .h(px(84.))
            .w_full()
            .rounded_sm()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
    };
    if available.len() < 2 {
        return chart_shell()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.))
            .text_color(rgb(palette.text_dimmed))
            .child(t!("resourceMonitor.collectingNetworkHistory").to_string());
    }
    let first = available[0].0;
    let last = available[available.len() - 1].0;
    let origin = samples[0].0;
    let series = [(true, rx_color), (false, tx_color)]
        .into_iter()
        .map(|(is_rx, color)| ZzClawLineSeries {
            points: samples
                .iter()
                .map(|(sampled_at, traffic)| ZzClawLinePoint {
                    x: sampled_at.duration_since(origin).as_secs_f64(),
                    value: traffic.as_ref().map(|traffic| {
                        if is_rx {
                            traffic.rx_bytes_per_sec
                        } else {
                            traffic.tx_bytes_per_sec
                        }
                    }),
                })
                .collect::<Vec<_>>()
                .into(),
            color,
        })
        .collect();
    let plot = ZzClawLinePlot::new("stats-network-lines", series, rgb(palette.border).into());
    let maximum = format_resource_rate(plot.max_value());
    let minutes = format!(
        "{:.1}",
        (last.duration_since(first).as_secs_f64() / 60.).max(0.1)
    );
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
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(network_legend("RX", rx_color))
                        .child(network_legend("TX", tx_color)),
                )
                .child(t!("resourceMonitor.maxRate", value = maximum).to_string()),
        )
        .child(chart_shell().child(plot))
        .child(
            div()
                .text_right()
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(t!("resourceMonitor.recentMinutes", count = minutes).to_string()),
        )
}

fn network_legend(label: &'static str, color: Hsla) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().size(px(6.)).rounded_full().bg(color))
        .child(label)
}
