use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb, rgba,
};
use nyaterm_ui::NyaScrollable;

use crate::features::NyaTermApp;
use crate::features::view_widgets::dialog_action_button;
use crate::models::MultiLinePasteDraft;
use crate::models::normalize_paste_newlines;
use crate::widgets::small_button;

impl NyaTermApp {
    pub(in crate::features) fn multi_line_paste_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let paste_review = self.terminal.paste_review();
        let paste_focus = paste_review.focus.clone();
        let selection = paste_review.selected_byte_range;
        let cursor = paste_review.cursor;
        let marked_range = paste_review.marked_range;
        let draft = paste_review
            .draft
            .cloned()
            .unwrap_or_else(|| MultiLinePasteDraft::new(String::new()));
        let input_entity = cx.entity();
        let draft_text = draft.text.clone();
        let normalized = normalize_paste_newlines(&draft_text);
        let stats = t!(
            "terminal.multiLinePasteStats",
            lines = normalized.split('\n').count(),
            chars = draft_text.chars().count()
        );
        let can_send = !draft_text.is_empty();
        let preview_height = (viewport_h - 160.).clamp(128., 288.);
        let mut preview = div()
            .id(SharedString::from("multi-line-paste-text"))
            .mt_3()
            .h(px(preview_height))
            .rounded_sm()
            .border_1()
            .border_color(if can_send {
                rgb(palette.border)
            } else {
                rgb(0x7f1d1d)
            })
            .bg(rgb(palette.input))
            .p_3()
            .font_family(crate::features::shell::gpui_code_font_family())
            .text_xs()
            .line_height(px(18.))
            .whitespace_normal()
            .text_color(if can_send {
                rgb(palette.text)
            } else {
                rgb(palette.text_muted)
            });
        if normalized.is_empty() {
            preview = preview.child(t!("terminal.multiLinePasteTextPlaceholder"));
        } else {
            let show_caret = selection.is_empty();
            let cursor = cursor.min(normalized.len());
            let display_text = if show_caret {
                let mut display = normalized.clone();
                display.insert(cursor, '|');
                display
            } else {
                normalized.clone()
            };
            let mut highlights = Vec::new();
            if !selection.is_empty() {
                highlights.push((
                    display_range_after_caret(selection, cursor, show_caret),
                    gpui::HighlightStyle {
                        background_color: Some(rgba(0x2f81f750).into()),
                        ..Default::default()
                    },
                ));
            }
            if let Some(marked_range) = marked_range {
                highlights.push((
                    display_range_after_caret(marked_range, cursor, show_caret),
                    gpui::HighlightStyle {
                        underline: Some(gpui::UnderlineStyle {
                            color: Some(rgb(palette.text).into()),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                ));
            }
            preview =
                preview.child(gpui::StyledText::new(display_text).with_highlights(highlights));
        }

        div()
            .id(SharedString::from("multi-line-paste-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&paste_focus)
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(this.terminal.paste_review().focus, cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                cx.stop_propagation();
                this.handle_multi_line_paste_key_down(event, cx);
            }))
            .child(
                div()
                    .id(SharedString::from("multi-line-paste-dialog"))
                    .w(px((viewport_w - 32.).clamp(280., 576.)))
                    .max_h(px((viewport_h - 24.).max(240.)))
                    .max_w_full()
                    .mx_4()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.bg))
                    .shadow_lg()
                    .p_6()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight(800.))
                            .text_color(rgb(palette.text))
                            .child(t!("terminal.multiLinePasteTitle")),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(rgb(palette.text_muted))
                            .child(stats),
                    )
                    .child(
                        preview
                            .relative()
                            .track_focus(&paste_focus)
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.focus(this.terminal.paste_review().focus, cx);
                                cx.notify();
                            }))
                            .overflow_y_scrollbar()
                            .child(
                                gpui::canvas(
                                    |_bounds, _window, _cx| {},
                                    move |bounds, _state, window, cx| {
                                        let focus = input_entity
                                            .read(cx)
                                            .terminal
                                            .paste_review()
                                            .focus
                                            .clone();
                                        window.handle_input(
                                            &focus,
                                            gpui::ElementInputHandler::new(
                                                bounds,
                                                input_entity.clone(),
                                            ),
                                            cx,
                                        );
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            ),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(small_button(
                                palette,
                                "multi-line-paste-cancel",
                                t!("common.cancel"),
                                cx.listener(|this, _, _, cx| {
                                    this.close_multi_line_paste(cx);
                                }),
                            ))
                            .child(div().when(!can_send, |this| this.opacity(0.45)).child(
                                dialog_action_button(
                                    palette,
                                    "multi-line-paste-direct",
                                    t!("terminal.multiLinePasteDirect"),
                                    false,
                                    cx.listener(|this, _, _, cx| {
                                        this.direct_multi_line_paste(cx);
                                    }),
                                ),
                            ))
                            .child(div().when(!can_send, |this| this.opacity(0.45)).child(
                                small_button(
                                    palette,
                                    "multi-line-paste-line",
                                    t!("terminal.multiLinePasteSendLineByLine"),
                                    cx.listener(|this, _, _, cx| {
                                        this.send_multi_line_paste_by_line(cx);
                                    }),
                                ),
                            )),
                    ),
            )
    }
}

fn display_range_after_caret(
    range: std::ops::Range<usize>,
    cursor: usize,
    show_caret: bool,
) -> std::ops::Range<usize> {
    if !show_caret {
        return range;
    }
    let shift = |offset: usize| offset + usize::from(offset >= cursor);
    shift(range.start)..shift(range.end)
}
