use std::borrow::Cow;

use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, KeyDownEvent, SharedString, Window, div,
    prelude::*, px, rgb,
};
use nyaterm_core::truncate_preview;
use nyaterm_transport::RemoteProcess;

use super::super::panels::RemoteMonitorPanel;
use crate::features::transfers::format_file_size;
use crate::theme::ThemePalette;
use crate::widgets::small_button;

use super::data::{ProcessDisplayMode, process_details_height_px};
use super::resources::usage_color;

#[derive(Clone)]
pub(in crate::features::pages::remote) struct ProcessDetailLabels {
    pub cpu: Cow<'static, str>,
    pub memory: Cow<'static, str>,
    pub rss: Cow<'static, str>,
    pub elapsed: Cow<'static, str>,
    pub copy_command: Cow<'static, str>,
    pub apply_nice: Cow<'static, str>,
}

pub(in crate::features::pages::remote) fn process_details(
    palette: ThemePalette,
    process: &RemoteProcess,
    mode: ProcessDisplayMode,
    labels: ProcessDetailLabels,
    nice_input: Option<gpui::AnyElement>,
    on_copy_command: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<RemoteMonitorPanel>,
) -> gpui::AnyElement {
    let command = if process.command_line.trim().is_empty() {
        process.command.clone()
    } else {
        process.command_line.clone()
    };
    let pid = process.pid;
    let narrow = matches!(
        mode,
        ProcessDisplayMode::Compact | ProcessDisplayMode::Narrow
    );
    let command_height = match mode {
        ProcessDisplayMode::Compact => 80.,
        ProcessDisplayMode::Narrow => 36.,
        _ => 36.,
    };
    let state_color = match process.state.trim().to_ascii_lowercase().as_str() {
        "running" | "r" => rgb(0x34d399),
        "stopped" | "t" => rgb(palette.warning),
        "zombie" | "z" => rgb(palette.danger),
        _ => rgb(palette.text_muted),
    };

    let metrics = div()
        .grid()
        .grid_cols(if narrow { 2 } else { 4 })
        .gap_1()
        .child(process_metric(
            palette,
            labels.cpu.clone(),
            format!("{:.1}%", process.cpu_percent),
            usage_color(palette, process.cpu_percent / 100.),
        ))
        .child(process_metric(
            palette,
            labels.memory.clone(),
            format!("{:.1}%", process.memory_percent),
            usage_color(palette, process.memory_percent / 100.),
        ))
        .child(process_metric(
            palette,
            labels.rss.clone(),
            format_file_size(Some(process.rss_kb.saturating_mul(1024))),
            rgb(0xc084fc).into(),
        ))
        .child(process_metric(
            palette,
            labels.elapsed.clone(),
            process.elapsed.clone(),
            rgb(0x34d399).into(),
        ));

    div()
        .h(px(process_details_height_px(mode)))
        .flex_none()
        .overflow_hidden()
        .border_t_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.bg))
        .px_2()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .h(px(31.))
                .flex_none()
                .flex()
                .items_start()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight(600.))
                                .text_color(rgb(palette.text))
                                .overflow_hidden()
                                .child(truncate_preview(&process.command, 54)),
                        )
                        .child(
                            div()
                                .font_family(crate::features::shell::gpui_code_font_family())
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_dimmed))
                                .overflow_hidden()
                                .child(format!(
                                    "PID {} · PPID {} · {}",
                                    process.pid, process.ppid, process.user
                                )),
                        ),
                )
                .child(
                    div()
                        .h(px(22.))
                        .px_2()
                        .flex_none()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .border_1()
                        .border_color(state_color)
                        .text_size(px(10.))
                        .font_weight(FontWeight(700.))
                        .text_color(state_color)
                        .child(truncate_preview(&process.state, 12)),
                ),
        )
        .child(metrics)
        .child(
            div()
                .h(px(command_height))
                .max_h(px(command_height))
                .flex_none()
                .min_w_0()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x1d4f67))
                .bg(rgb(palette.surface))
                .pl_2()
                .pr_1()
                .py_1()
                .flex()
                .items_start()
                .gap_1()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .max_h(px(command_height - 10.))
                        .overflow_hidden()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .text_size(px(10.))
                        .line_height(px(14.))
                        .text_color(rgb(palette.text_muted))
                        .child(command),
                )
                .child(small_button(
                    palette,
                    format!("process-copy-command-{pid}"),
                    labels.copy_command.clone(),
                    on_copy_command,
                )),
        )
        .child(
            div()
                .h(px(28.))
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .w(px(if mode == ProcessDisplayMode::Compact {
                            140.
                        } else {
                            80.
                        }))
                        .on_key_down(cx.listener(|panel, event: &KeyDownEvent, window, cx| {
                            panel.with_app(cx, |this, cx| {
                                if event.keystroke.key.as_str() == "enter" {
                                    cx.stop_propagation();
                                    this.apply_process_nice_draft(window, cx);
                                }
                            });
                        }))
                        .children(nice_input),
                )
                .child(small_button(
                    palette,
                    format!("process-nice-apply-{pid}"),
                    labels.apply_nice.clone(),
                    cx.listener(move |panel, _, window, cx| {
                        panel.with_app(cx, |this, cx| {
                            this.apply_process_nice_draft(window, cx);
                        });
                    }),
                )),
        )
        .into_any_element()
}

fn process_metric(
    palette: ThemePalette,
    label: impl Into<SharedString>,
    value: String,
    color: gpui::Hsla,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .h(px(39.))
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(color.opacity(0.35))
        .bg(rgb(palette.surface))
        .px_2()
        .py_1()
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(px(9.))
                .text_color(color.opacity(0.75))
                .overflow_hidden()
                .child(label),
        )
        .child(
            div()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(11.))
                .font_weight(FontWeight(700.))
                .text_color(color)
                .overflow_hidden()
                .child(value),
        )
}
