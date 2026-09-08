use rust_i18n::t;

use gpui::{
    Context, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};

use crate::features::{shell::gpui_code_font_family, view_widgets::panel_header_with_actions};
use nyaterm_ui::{NyaScrollable, NyaTooltip};

use super::helpers::{TransferJobRowLabels, queue_action_button, transfer_job_row};
use super::panel::TransferPanel;

/// The transfer queue. Filtering and ordering happened here once, each with a deep
/// copy of every job; both now arrive already done in the snapshot.
pub(in crate::features::pages::transfers) fn transfer_queue_view(
    panel: &TransferPanel,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    let snapshot = panel
        .snapshot()
        .expect("the caller returns early without a snapshot");
    let chrome = snapshot.chrome;
    let palette = chrome.palette;
    let queue = &snapshot.queue;
    let has_running = queue.has_running;
    let has_paused = queue.has_paused;
    let has_active = queue.has_active;
    let has_completed = queue.has_completed;
    let has_stopped = queue.has_stopped;
    let download_path = queue.download_path.clone();
    let selected_remote_path = snapshot.browser.selected_remote_path.clone();
    let selected_job_id = queue.selected_job_id.clone();

    {
        let mut list = div().flex().flex_col();
        if !snapshot.has_session {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .py_6()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(t!("fileExplorer.connectToSession")),
            );
        } else if queue.rows.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .py_6()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(t!("fileTransfer.noTransfers")),
            );
        } else {
            list = list.gap(px(2.)).p_1();
            let row_labels = TransferJobRowLabels {
                transferring: t!("fileTransfer.transferring").to_string(),
                paused: t!("fileTransfer.paused").to_string(),
                cancelling: t!("fileTransfer.cancelling").to_string(),
                cancelled: t!("fileTransfer.cancelled").to_string(),
                completed: t!("fileTransfer.completed").to_string(),
                failed: t!("fileTransfer.failed").to_string(),
                streaming: t!("fileTransfer.streaming").to_string(),
                unknown_size: t!("fileTransfer.unknownSize").to_string(),
            };
            for job in queue.rows.iter().cloned() {
                let directory_progress = job.progress.as_ref().and_then(|progress| {
                    progress
                        .item_count_completed
                        .zip(progress.item_count_total)
                        .map(|(completed, total)| {
                            t!(
                                "fileTransfer.directoryProgress",
                                completed = completed,
                                total = total
                            )
                            .to_string()
                        })
                });
                list = list.child(transfer_job_row(
                    palette,
                    job,
                    directory_progress,
                    row_labels.clone(),
                    selected_remote_path.clone(),
                    selected_job_id.clone(),
                    cx,
                ));
            }
        }
        div()
            .id(SharedString::from("transfer-queue-panel"))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(chrome.transparent_surface)
            .track_focus(&queue.focus)
            .on_click(cx.listener(|panel, _, window, cx| {
                panel.with_app(cx, |this, cx| {
                    window.focus(this.transfer.queue_focus(), cx);
                    cx.notify();
                })
            }))
            .on_key_down(cx.listener(|panel, event: &KeyDownEvent, window, cx| {
                panel.with_app(cx, |this, cx| {
                    this.handle_transfer_queue_key_down(event, window, cx);
                })
            }))
            .child(panel_header_with_actions(
                t!("panel.fileTransfer"),
                "",
                palette,
                chrome.transparent_section_header,
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(queue_action_button(
                            palette,
                            "transfer-pause-all",
                            "icons/transfer/pause.svg",
                            t!("fileTransfer.pauseAll"),
                            has_running,
                            cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.pause_all_transfer_jobs(cx);
                                })
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-resume-all",
                            "icons/transfer/play.svg",
                            t!("fileTransfer.resumeAll"),
                            has_paused,
                            cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.resume_all_transfer_jobs(cx);
                                })
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-cancel-all",
                            "icons/transfer/stop.svg",
                            t!("fileTransfer.cancelAll"),
                            has_active,
                            cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.cancel_all_transfer_jobs(cx);
                                })
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-clear-completed",
                            "icons/transfer/playlist-remove.svg",
                            t!("fileTransfer.clearCompleted"),
                            has_completed,
                            cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.clear_completed_transfer_jobs(cx);
                                })
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-clear-stopped",
                            "icons/transfer/clear-all.svg",
                            t!("fileTransfer.clearAll"),
                            has_stopped,
                            cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.clear_stopped_transfer_jobs(cx);
                                })
                            }),
                        ))
                        .into_any_element(),
                ),
            ))
            .child(
                div()
                    .id(SharedString::from("transfer-queue-scroll"))
                    .flex_1()
                    .min_h_0()
                    .overflow_scrollbar()
                    .child(list),
            )
            .child(
                div()
                    .h(px(26.))
                    .px_2()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .bg(chrome.transparent_surface)
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from("transfer-download-path-footer"))
                            .min_w_0()
                            .flex_1()
                            .font_family(gpui_code_font_family())
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .cursor_pointer()
                            .hover(|this| this.text_color(rgb(palette.text)))
                            .tooltip({
                                let label = t!("fileTransfer.downloadPath").to_string();
                                move |window, cx| NyaTooltip::new(label.clone()).build(window, cx)
                            })
                            .child(download_path)
                            .on_click(cx.listener(|panel, _, _, cx| {
                                panel.with_app(cx, |this, cx| {
                                    this.reveal_transfer_download_dir(cx);
                                })
                            })),
                    ),
            )
    }
}
