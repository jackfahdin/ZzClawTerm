use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, MouseButton, MouseDownEvent, div, prelude::*,
    px, rgb,
};
use rust_i18n::t;
use zzclawterm_ui::{ZzClawButton, ZzClawButtonVariant, ZzClawScrollable, ZzClawTooltip};

use crate::features::file_manager::workspace::{FileManagerQueueFilter, FileManagerWorkspace};
use crate::features::pages::transfers::format_browser_file_size;
use crate::models::{TransferJobKind, TransferJobStatus};
use crate::theme::ThemePalette;

impl FileManagerWorkspace {
    pub(super) fn queue_view(&self, palette: ThemePalette, cx: &mut Context<Self>) -> AnyElement {
        let jobs = self.read_app(cx, |app| {
            app.transfer
                .transfer_jobs()
                .iter()
                .filter_map(|job| {
                    let context = app.transfer.file_operation(&job.id).cloned();
                    (context.is_some() || self.show_all_jobs && job.is_user_transfer())
                        .then(|| (job.clone(), context))
                })
                .collect::<Vec<_>>()
        });
        let active = jobs
            .iter()
            .filter(|(job, _)| {
                matches!(
                    job.status,
                    TransferJobStatus::Queued
                        | TransferJobStatus::Running
                        | TransferJobStatus::Paused
                        | TransferJobStatus::Cancelling
                )
            })
            .count();
        let failed = jobs
            .iter()
            .filter(|(job, _)| job.status == TransferJobStatus::Failed)
            .count();
        let mut header = div()
            .h(px(30.))
            .min_w_0()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .border_t_1()
            .border_color(rgb(palette.border))
            .child(
                ZzClawButton::new(
                    "fm-queue-toggle",
                    format!(
                        "{} · {active} {} · {failed} {}",
                        t!("panel.fileTransfer"),
                        t!("fileManager.filterActive"),
                        t!("fileManager.filterFailed")
                    ),
                )
                .small()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.queue_expanded = !this.queue_expanded;
                    this.save_layout(window, cx);
                    cx.notify();
                })),
            );
        if self.queue_expanded {
            for (filter, key) in [
                (FileManagerQueueFilter::All, "fileManager.filterAll"),
                (FileManagerQueueFilter::Active, "fileManager.filterActive"),
                (
                    FileManagerQueueFilter::Completed,
                    "fileManager.filterCompleted",
                ),
                (FileManagerQueueFilter::Failed, "fileManager.filterFailed"),
            ] {
                header = header.child(
                    ZzClawButton::new(format!("fm-filter-{filter:?}"), t!(key).to_string())
                        .small()
                        .variant(if self.queue_filter == filter {
                            ZzClawButtonVariant::Primary
                        } else {
                            ZzClawButtonVariant::Ghost
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.queue_filter = filter;
                            cx.notify();
                        })),
                );
            }
            header = header.child(
                ZzClawButton::new("fm-all-jobs", t!("fileManager.allJobs").to_string())
                    .small()
                    .variant(if self.show_all_jobs {
                        ZzClawButtonVariant::Primary
                    } else {
                        ZzClawButtonVariant::Ghost
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_all_jobs = !this.show_all_jobs;
                        cx.notify();
                    })),
            );
        }
        let mut surface = div()
            .flex_none()
            .min_w_0()
            .flex()
            .flex_col()
            .child(header.overflow_x_scrollbar());
        if self.queue_expanded {
            surface = surface.child(
                div()
                    .id("fm-queue-resize")
                    .h(px(4.))
                    .w_full()
                    .cursor(CursorStyle::ResizeUpDown)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.queue_resize =
                                Some((f32::from(event.position.y), this.queue_height));
                            cx.stop_propagation();
                        }),
                    ),
            );
            let mut list = div()
                .id("fm-queue-rows")
                .h(px(self.queue_height))
                .min_w_0()
                .overflow_y_scrollbar()
                .px_2()
                .flex()
                .flex_col()
                .gap_1();
            let mut count = 0;
            for (job, context) in jobs.into_iter().filter(|(job, _)| match self.queue_filter {
                FileManagerQueueFilter::All => true,
                FileManagerQueueFilter::Active => matches!(
                    job.status,
                    TransferJobStatus::Queued
                        | TransferJobStatus::Running
                        | TransferJobStatus::Paused
                        | TransferJobStatus::Cancelling
                ),
                FileManagerQueueFilter::Completed => job.status == TransferJobStatus::Completed,
                FileManagerQueueFilter::Failed => matches!(
                    job.status,
                    TransferJobStatus::Failed | TransferJobStatus::Cancelled
                ),
            }) {
                count += 1;
                let (source_path, target_path) = match &job.kind {
                    TransferJobKind::SendTo {
                        source_path,
                        target_path,
                        ..
                    } => (source_path.clone(), target_path.clone()),
                    _ => (String::new(), String::new()),
                };
                let labels = context
                    .as_ref()
                    .map(|context| {
                        format!(
                            "{} · {} → {}",
                            t!(context.action_key()),
                            context.source_label,
                            context.target_label
                        )
                    })
                    .unwrap_or_default();
                let tooltip = format!("{labels}\n{source_path}\n{target_path}\n{}", job.detail);
                let progress = job
                    .progress
                    .as_ref()
                    .map(|progress| {
                        let transferred =
                            format_browser_file_size(Some(progress.bytes_transferred));
                        if let Some(total) = progress.total_bytes {
                            format!(
                                "{transferred} / {} · {:.0}%",
                                format_browser_file_size(Some(total)),
                                progress.bytes_transferred.min(total) as f64 / total.max(1) as f64
                                    * 100.
                            )
                        } else {
                            format!("{transferred} · …")
                        }
                    })
                    .unwrap_or_else(|| {
                        if job.status == TransferJobStatus::Completed {
                            format!(
                                "{} · 100%",
                                format_browser_file_size(
                                    job.summary.as_ref().map(|summary| summary.bytes)
                                )
                            )
                        } else {
                            "…".into()
                        }
                    });
                let speed = if job.status == TransferJobStatus::Running {
                    format!(
                        "{}/s",
                        format_browser_file_size(Some(job.speed.bytes_per_second().max(0.) as u64))
                    )
                } else {
                    String::new()
                };
                let name = if job.display_name.is_empty() {
                    job.detail.clone()
                } else {
                    job.display_name.clone()
                };
                let mut actions = div().flex().items_center().gap_1();
                let can_pause = context.as_ref().is_none_or(|context| context.can_pause());
                if can_pause
                    && matches!(
                        job.status,
                        TransferJobStatus::Queued
                            | TransferJobStatus::Running
                            | TransferJobStatus::Paused
                    )
                {
                    let id = job.id.clone();
                    let paused = job.status == TransferJobStatus::Paused;
                    actions = actions.child(
                        ZzClawButton::new(
                            format!("fm-pause-{id}"),
                            t!(if paused {
                                "fileTransfer.resume"
                            } else {
                                "fileTransfer.pause"
                            })
                            .to_string(),
                        )
                        .small()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let id = id.clone();
                                this.dispatch_app(window, cx, move |app, _, cx| {
                                    if paused {
                                        app.resume_transfer_job(&id, cx);
                                    } else {
                                        app.pause_transfer_job(&id, cx);
                                    }
                                });
                            },
                        )),
                    );
                }
                if matches!(
                    job.status,
                    TransferJobStatus::Queued
                        | TransferJobStatus::Running
                        | TransferJobStatus::Paused
                ) {
                    let id = job.id.clone();
                    actions = actions.child(
                        ZzClawButton::new(
                            format!("fm-cancel-{id}"),
                            t!("fileTransfer.cancel").to_string(),
                        )
                        .small()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let id = id.clone();
                                this.dispatch_app(window, cx, move |app, _, cx| {
                                    app.cancel_transfer_job(&id, cx)
                                });
                            },
                        )),
                    );
                }
                if matches!(
                    job.status,
                    TransferJobStatus::Failed | TransferJobStatus::Cancelled
                ) && context.is_some()
                {
                    let id = job.id.clone();
                    actions = actions.child(
                        ZzClawButton::new(
                            format!("fm-retry-{id}"),
                            t!("fileTransfer.retry").to_string(),
                        )
                        .small()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let id = id.clone();
                                this.dispatch_app(window, cx, move |app, window, cx| {
                                    app.retry_transfer_job(id, window, cx)
                                });
                            },
                        )),
                    );
                }
                list = list.child(
                    div()
                        .id(gpui::SharedString::from(format!("fm-job-{}", job.id)))
                        .h(px(54.))
                        .flex_none()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .tooltip(move |window, cx| {
                            ZzClawTooltip::new(tooltip.clone()).build(window, cx)
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .text_xs()
                                .child(div().truncate().child(name))
                                .child(
                                    div()
                                        .truncate()
                                        .text_color(rgb(palette.text_muted))
                                        .child(format!("{labels} · {progress} · {speed}")),
                                ),
                        )
                        .child(
                            div()
                                .max_w(px(240.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(if job.status == TransferJobStatus::Failed {
                                    palette.warning
                                } else {
                                    palette.text_muted
                                }))
                                .child(job.detail),
                        )
                        .child(actions),
                );
            }
            if count == 0 {
                list = list.child(
                    div()
                        .py_3()
                        .text_xs()
                        .child(t!("fileTransfer.noTransfers").to_string()),
                );
            }
            surface = surface.child(list);
        }
        surface.into_any_element()
    }
}
