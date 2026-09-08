use rust_i18n::t;

use gpui::{Context, IntoElement, MouseButton, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;
use nyaterm_ui::NyaScrollable;

use crate::features::NyaTermApp;

use super::helpers::{clamp_menu_position, open_external_url, terminal_ctx_item_with_icon};

impl NyaTermApp {
    pub(in crate::features) fn action_link_menu_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let Some(menu) = self.terminal.menus.action_link_menu.clone() else {
            return div().into_any_element();
        };
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let (menu_x, menu_y) = clamp_menu_position(
            f32::from(menu.x),
            f32::from(menu.y),
            260.,
            360.,
            viewport_w,
            viewport_h,
        );
        let mut items = div()
            .max_h(px(360.))
            .overflow_y_scrollbar()
            .py_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(menu.kind_label.clone()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .font_family(crate::features::shell::gpui_code_font_family())
                            .text_size(px(11.))
                            .text_color(rgb(palette.text))
                            .child(truncate_preview(&menu.value, 42)),
                    ),
            );
        for action in menu.actions {
            let command = action.command.clone();
            let open_url = action.open_url.clone();
            let label = if action.is_default {
                format!("{} (default)", action.label)
            } else {
                action.label.clone()
            };
            items = items.child(terminal_ctx_item_with_icon(
                palette,
                format!("action-link-menu-{}", action.id),
                label,
                Some(crate::features::icons::IconDef::mono(
                    if open_url.is_some() {
                        "icons/conn/connect.svg"
                    } else {
                        "icons/fe/forward.svg"
                    },
                    palette.text_muted,
                )),
                None,
                cx.listener(move |this, _, _, cx| {
                    this.close_action_link_menu(cx);
                    if let Some(url) = open_url.clone() {
                        match open_external_url(&url) {
                            Ok(()) => this.shell.set_status(format!("opened link: {url}")),
                            Err(error) => {
                                this.shell.set_status(format!("open link failed: {error}"))
                            }
                        }
                        cx.notify();
                        return;
                    }
                    if let Some(command) = command.clone() {
                        this.execute_action_link_command(command, cx);
                    }
                }),
            ));
        }
        div()
            .id(SharedString::from("action-link-menu-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.close_action_link_menu(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.close_action_link_menu(cx);
                }),
            )
            .child(
                div()
                    .id(SharedString::from("action-link-menu"))
                    .absolute()
                    .top(px(menu_y))
                    .left(px(menu_x))
                    .w(px(260.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.surface))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(items),
            )
            .into_any_element()
    }

    pub(in crate::features) fn action_link_tooltip_overlay(
        &self,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let Some(tip) = self.terminal.menus.action_link_tooltip.clone() else {
            return div().into_any_element();
        };
        let mod_label = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        let kind_color = match tip.kind_label.as_str() {
            "IPv4" => palette.link,
            "Host:Port" => 0xa78bfa,
            "Archive" => palette.warning,
            "URL" => palette.success,
            _ => palette.text_muted,
        };
        let x = f32::from(tip.x) + 16.0;
        let y = f32::from(tip.y) + 16.0;
        div()
            .id(SharedString::from("action-link-tooltip-overlay"))
            .absolute()
            .occlude()
            .left(px(x.max(8.0)))
            .top(px(y.max(8.0)))
            .max_w(px(340.))
            .rounded_lg()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(self.shell_surface_color(palette.surface_elevated))
            .shadow_lg()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.surface))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .py(px(1.))
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(kind_color))
                            .text_size(px(10.))
                            .text_color(rgb(kind_color))
                            .child(tip.kind_label.to_uppercase()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .font_family(crate::features::shell::gpui_code_font_family())
                            .text_size(px(12.))
                            .text_color(rgb(palette.text))
                            .child(truncate_preview(&tip.value, 42)),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .flex_none()
                                    .child(
                                        div()
                                            .px_1()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(rgb(palette.border))
                                            .bg(rgb(palette.input))
                                            .text_size(px(10.))
                                            .text_color(rgb(palette.text))
                                            .child(mod_label),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(palette.text_muted))
                                            .child("+ click"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child("→"),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(crate::features::shell::gpui_code_font_family())
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text))
                                    .child(truncate_preview(&tip.default_action_preview, 48)),
                            ),
                    )
                    .when(tip.has_more_actions, |this| {
                        this.child(
                            div()
                                .pt_1()
                                .border_t_1()
                                .border_color(rgb(palette.border))
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .child(t!("terminal.actionLinkAltClickHint")),
                        )
                    }),
            )
            .into_any_element()
    }
}
