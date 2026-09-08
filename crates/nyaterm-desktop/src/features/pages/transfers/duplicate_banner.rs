//! The SFTP duplicate-file prompt, shown inside the transfers panel.
//!
//! Its five siblings in `layout/prompts.rs` are all connection and authentication
//! prompts drawn by the app; this one is a file-transfer prompt drawn by the panel,
//! and it kept the odd shape out of that file by moving here.

use rust_i18n::t;

use gpui::{Context, FontWeight, IntoElement, KeyDownEvent, div, prelude::*, px, rgb, rgba};
use nyaterm_transport::SftpDuplicateDecision;

use crate::features::formatting::download_file_name_from_remote_path;
use crate::features::session::SftpDuplicatePromptState;
use crate::features::view_widgets::{bounded_dialog_width, full_window_input_layer};
use crate::widgets::small_button;

use super::panel::{TransferChrome, TransferPanel};

pub(in crate::features::pages::transfers) fn duplicate_prompt_banner(
    chrome: TransferChrome,
    prompt: SftpDuplicatePromptState,
    focus: &gpui::FocusHandle,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    let palette = chrome.palette;
    let panel_focus = focus.clone();
    let overwrite_id = prompt.id.clone();
    let skip_id = prompt.id.clone();
    let escape_id = prompt.id.clone();
    let rename_id = prompt.id.clone();
    let kind = if prompt.request.is_directory {
        t!("fileTransfer.duplicateKindFolder")
    } else {
        t!("fileTransfer.duplicateKindFile")
    };
    let target_name = download_file_name_from_remote_path(&prompt.request.target_path);
    let description = t!(
        "fileTransfer.duplicateDescription",
        kind = kind,
        name = target_name
    );

    full_window_input_layer("duplicate-prompt-overlay")
        .bg(rgba(0x00000080))
        .flex()
        .items_center()
        .justify_center()
        .p_3()
        .track_focus(&panel_focus)
        .on_click(cx.listener(|panel, _, window, cx| {
            panel.with_app(cx, |this, cx| {
                window.focus(this.transfer.panel_focus(), cx);
                cx.notify();
            })
        }))
        .on_key_down(cx.listener(move |panel, event: &KeyDownEvent, _, cx| {
            panel.with_app(cx, |this, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    this.resolve_duplicate_prompt(
                        escape_id.clone(),
                        SftpDuplicateDecision::Skip,
                        cx,
                    );
                }
            })
        }))
        .child(
            div()
                .id("duplicate-prompt-dialog")
                .w(px(bounded_dialog_width(
                    chrome.viewport_width,
                    32.,
                    280.,
                    448.,
                )))
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(chrome.surface)
                .shadow_lg()
                .p_6()
                .flex()
                .flex_col()
                .gap_4()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight(700.))
                        .child(t!("fileTransfer.duplicateTitle")),
                )
                .child(
                    div()
                        .text_xs()
                        .line_height(px(17.))
                        .text_color(rgb(palette.text_muted))
                        .child(description),
                )
                .child(
                    div()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .px_2()
                        .py_1()
                        .font_family(crate::features::shell::gpui_code_font_family())
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_muted))
                        .child(prompt.request.target_path.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap_2()
                        .child(small_button(
                            palette,
                            format!("duplicate-overwrite-{overwrite_id}"),
                            t!("fileTransfer.duplicateOverwrite"),
                            cx.listener(move |panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.resolve_duplicate_prompt(
                                        overwrite_id.clone(),
                                        SftpDuplicateDecision::Overwrite,
                                        cx,
                                    );
                                })
                            }),
                        ))
                        .child(small_button(
                            palette,
                            format!("duplicate-skip-{skip_id}"),
                            t!("fileTransfer.duplicateSkip"),
                            cx.listener(move |panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.resolve_duplicate_prompt(
                                        skip_id.clone(),
                                        SftpDuplicateDecision::Skip,
                                        cx,
                                    );
                                })
                            }),
                        ))
                        .child(small_button(
                            palette,
                            format!("duplicate-rename-{rename_id}"),
                            t!("common.rename"),
                            cx.listener(move |panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.resolve_duplicate_prompt(
                                        rename_id.clone(),
                                        SftpDuplicateDecision::Rename,
                                        cx,
                                    );
                                })
                            }),
                        )),
                ),
        )
}
