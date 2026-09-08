use rust_i18n::t;

use gpui::{ClickEvent, Context, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::truncate_preview;
use nyaterm_ui::NyaScrollable;

use crate::features::NyaTermApp;

impl NyaTermApp {
    pub(in crate::features) fn command_history_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        // Tauri CommandHistory: a double-click sends the command; a single click only focuses it.
        let history = self.session.active_command_history_snapshot();
        let mut rows = if history.is_empty() {
            div()
                .size_full()
                .min_h_0()
                .child(crate::widgets::empty_panel_with_icon(
                    t!("panel.noCommandsYet"),
                    palette,
                    "icons/history.svg",
                ))
        } else {
            div().flex().flex_col().gap_0().p_2()
        };
        if !history.is_empty() {
            for (index, command) in history.into_iter().enumerate() {
                let run_index = index;
                rows = rows.child(
                    div()
                        .id(SharedString::from(format!("command-history-row-{index}")))
                        .h(px(28.))
                        .px_2()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .hover(|this| this.bg(rgb(palette.hover)))
                        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                            if event.click_count() >= 2 {
                                this.run_history_command(run_index, cx);
                            }
                        }))
                        .child(div().child(crate::features::view_widgets::mono_icon(
                            "icons/fe/forward.svg",
                            rgb(palette.text_dimmed).into(),
                            10.,
                        )))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .font_family(crate::features::shell::gpui_code_font_family())
                                .text_size(px(12.))
                                .text_color(rgb(palette.text))
                                .overflow_hidden()
                                .child(truncate_preview(&command, 120)),
                        ),
                );
            }
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(palette.surface))
            .child(
                div()
                    .id(SharedString::from("command-history-list"))
                    .flex_1()
                    .min_h_0()
                    .overflow_scrollbar()
                    .child(rows),
            )
    }
}
