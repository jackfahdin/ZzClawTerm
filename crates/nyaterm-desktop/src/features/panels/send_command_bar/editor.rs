use rust_i18n::t;

use std::borrow::Cow;

use super::state::SendCommandBarViewState;
use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, ScrollDelta, ScrollWheelEvent, SharedString,
    div, prelude::*, px, rgb, svg,
};

use super::super::{send_command_hex_byte_count, send_command_hex_guide_rows};
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::send_command::{SendCommandDataType, format_send_command_hex_display};
use nyaterm_ui::{NyaScrollable, NyaTooltip};

impl NyaTermApp {
    pub(super) fn send_command_bar_editor(
        &mut self,
        state: &SendCommandBarViewState,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = state.palette;
        let input_hint = state.input_hint.clone();
        let validation_error = state.validation_error;
        let preview = state.preview.clone();
        let is_sending = state.is_sending;
        let progress_label = state.progress_label.clone();
        let progress_ratio = state.progress_ratio;
        let send = &state.send;
        let target_available = !self.send_command_target_session_ids().is_empty();
        let has_payload = if send.data_type == SendCommandDataType::Hex {
            send_command_hex_byte_count(&send.draft).is_some_and(|count| count > 0)
        } else {
            !send.draft.is_empty()
        };
        let send_disabled = !is_sending && (validation_error || !has_payload || !target_available);
        // Built before the panel, which reads `self` throughout: creating the
        // box needs it mutably. Wrapped, like Tauri's textarea — Enter is a
        // newline and Ctrl/Cmd+Enter is what sends.
        let send_input = self
            .text_input_box(
                "send-command.draft",
                &send.draft,
                TextInputSetup::multi_line(input_hint),
                cx,
            )
            .into_any_element();
        div()
            .relative()
            .flex_1()
            .min_h(px(72.))
            .flex()
            .gap(px(6.))
            .pr(px(40.))
            .pb(px(40.))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .min_h(px(72.))
                    .when(send.data_type == SendCommandDataType::Hex, |this| {
                        this.flex_none().flex_basis(gpui::relative(1.0 / 1.85))
                    })
                    .when(send.data_type == SendCommandDataType::Hex, |this| {
                        // Tauri overlays dashed 4-byte guides per line above the hex textarea.
                        // Scroll-sync: wheel adjusts hexScroll.{top,left} approximation.
                        let guide_rows = send_command_hex_guide_rows(&send.draft);
                        const HEX_LINE_PX: f32 = 15.;
                        const HEX_CHAR_PX: f32 = 7.2;
                        const VIEWPORT_LINES: f32 = 5.;
                        const VIEWPORT_CHARS: f32 = 48.;
                        let display = format_send_command_hex_display(&send.draft);
                        let lines: Vec<&str> = display.lines().collect();
                        let line_count = lines.len().max(1) as f32;
                        let max_line_chars = lines
                            .iter()
                            .map(|line| line.chars().count())
                            .max()
                            .unwrap_or(0) as f32;
                        let max_scroll_y = ((line_count - VIEWPORT_LINES).max(0.)) * HEX_LINE_PX;
                        let max_scroll_x =
                            ((max_line_chars - VIEWPORT_CHARS).max(0.)) * HEX_CHAR_PX;
                        let scroll_y = send.hex_scroll_y.clamp(0., max_scroll_y);
                        let scroll_x = send.hex_scroll_x.clamp(0., max_scroll_x);
                        this.on_scroll_wheel(cx.listener(
                            move |this, event: &ScrollWheelEvent, _, cx| {
                                let (delta_x, delta_y) = match event.delta {
                                    ScrollDelta::Lines(delta) => {
                                        (delta.x * HEX_CHAR_PX * 4., delta.y * HEX_LINE_PX)
                                    }
                                    ScrollDelta::Pixels(delta) => {
                                        (f32::from(delta.x), f32::from(delta.y))
                                    }
                                };
                                // Match GPUI / DOM: scroll offsets move opposite wheel delta.
                                if this.send_command.scroll_hex_by(
                                    delta_x,
                                    delta_y,
                                    max_scroll_x,
                                    max_scroll_y,
                                ) {
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            },
                        ))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .h(px(22.))
                                .px_2()
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_b_1()
                                .border_color(rgb(palette.surface_elevated))
                                .bg(self.shell_surface_color(palette.bg))
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight(600.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(t!("serialSend.hexEditor")),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(if validation_error {
                                            rgb(palette.danger)
                                        } else {
                                            rgb(palette.text_dimmed)
                                        })
                                        .child(if validation_error {
                                            t!("serialSend.hexError")
                                        } else {
                                            Cow::Borrowed("")
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .top(px(22.))
                                .px_2()
                                .py_1()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .relative()
                                        .top(px(-scroll_y))
                                        .left(px(-scroll_x))
                                        .flex()
                                        .flex_col()
                                        .children(guide_rows.into_iter().map(|marks| {
                                            div()
                                                .h(px(HEX_LINE_PX))
                                                .relative()
                                                .w_full()
                                                .flex_none()
                                                .children(marks.into_iter().map(|mark| {
                                                    div()
                                                        .absolute()
                                                        .top(px(0.))
                                                        .left(px(mark as f32 * 7.2))
                                                        .h(px(HEX_LINE_PX + 2.))
                                                        .w(px(2.))
                                                        .bg(rgb(0x1f6feb))
                                                        .opacity(0.55)
                                                }))
                                        })),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(crate::features::shell::gpui_code_font_family())
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                if this.handle_send_command_key_down(event, cx) {
                                    cx.stop_propagation();
                                }
                            }))
                            .child(send_input),
                    ),
            )
            .when(send.data_type == SendCommandDataType::Hex, |this| {
                let byte_count = send_command_hex_byte_count(&send.draft);
                this.child(
                    div()
                        .flex_none()
                        .flex_basis(gpui::relative(0.85 / 1.85))
                        .min_w(px(140.))
                        .min_h(px(72.))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(self.shell_surface_color(palette.bg))
                        .px_2()
                        .py_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight(600.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(t!("serialSend.hexPreview")),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(match byte_count {
                                            Some(n) => t!("serialSend.hexByteCount", count = n),
                                            None => t!("serialSend.hexByteCount", count = "0"),
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from("bottom-command-hex-preview-scroll"))
                                .min_h_0()
                                .flex_1()
                                .overflow_scrollbar()
                                .font_family(crate::features::shell::gpui_code_font_family())
                                .text_size(px(11.))
                                .line_height(px(15.))
                                .text_color(if validation_error {
                                    rgb(palette.danger)
                                } else {
                                    rgb(palette.text)
                                })
                                .child(if preview.trim().is_empty() {
                                    "·".to_string()
                                } else {
                                    preview.clone()
                                }),
                        ),
                )
            })
            .when(is_sending, |this| {
                this.child(send_command_progress_popover(
                    palette,
                    progress_label,
                    progress_ratio,
                ))
            })
            .child(send_command_floating_action_button(
                palette,
                is_sending,
                send_disabled,
                t!("serialSend.send"),
                t!("serialSend.stop"),
                cx.listener(|this, _, _, cx| {
                    if this.send_command.is_sending() {
                        this.stop_send_command(cx);
                    } else {
                        this.send_bottom_command(false, cx);
                    }
                }),
            ))
            .into_any_element()
    }
}

fn send_command_progress_popover(
    palette: crate::theme::ThemePalette,
    progress_label: String,
    progress_ratio: f32,
) -> impl IntoElement {
    div()
        .absolute()
        .top(px(8.))
        .left(px(8.))
        .right(px(44.))
        .rounded_md()
        .border_1()
        .border_color(rgb(0x1f6feb))
        .bg(rgb(palette.bg))
        .px_2()
        .py_1()
        .shadow_lg()
        .child(
            div()
                .mb_1()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .text_size(px(10.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(palette.text))
                        .child(progress_label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(format!("{:.0}%", progress_ratio * 100.0)),
                ),
        )
        .child(
            div()
                .h(px(6.))
                .w_full()
                .rounded_full()
                .bg(rgb(palette.surface_elevated))
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(progress_ratio.clamp(0.0, 1.0)))
                        .rounded_full()
                        .bg(rgb(0x1f6feb)),
                ),
        )
}

fn send_command_floating_action_button(
    palette: crate::theme::ThemePalette,
    is_sending: bool,
    disabled: bool,
    send_label: impl Into<SharedString>,
    stop_label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let send_label: SharedString = send_label.into();
    let stop_label: SharedString = stop_label.into();
    let tooltip = if is_sending { stop_label } else { send_label };
    div()
        .id(SharedString::from("bottom-command-floating-send"))
        .absolute()
        .right(px(8.))
        .bottom(px(8.))
        .size(px(28.))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .shadow_lg()
        .bg(if is_sending {
            rgb(palette.danger)
        } else {
            rgb(palette.link)
        })
        .text_color(rgb(0xffffff))
        .opacity(if disabled { 0.45 } else { 1.0 })
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(if is_sending {
                    "icons/session/stop.svg"
                } else {
                    "icons/send.svg"
                })
                .text_color(rgb(0xffffff)),
        )
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| {
                    this.bg(rgb(if is_sending {
                        palette.danger
                    } else {
                        palette.success
                    }))
                })
                .on_click(on_click)
        })
}
