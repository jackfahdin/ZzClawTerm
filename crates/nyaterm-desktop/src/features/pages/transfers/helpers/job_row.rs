use gpui::{
    Context, FontWeight, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div,
    prelude::FluentBuilder as _, px, relative, rgb, svg,
};
use nyaterm_core::truncate_preview;

use crate::features::transfers::format_file_size;
use crate::models::{TransferJobKind, TransferJobRowSnapshot, TransferJobStatus};
use crate::theme::ThemePalette;

use super::{transfer_progress_percent_label, transfer_progress_ratio};
use crate::features::pages::transfers::panel::TransferPanel;

#[derive(Clone)]
pub(in crate::features::pages::transfers) struct TransferJobRowLabels {
    pub(in crate::features::pages::transfers) transferring: String,
    pub(in crate::features::pages::transfers) paused: String,
    pub(in crate::features::pages::transfers) cancelling: String,
    pub(in crate::features::pages::transfers) cancelled: String,
    pub(in crate::features::pages::transfers) completed: String,
    pub(in crate::features::pages::transfers) failed: String,
    pub(in crate::features::pages::transfers) streaming: String,
    pub(in crate::features::pages::transfers) unknown_size: String,
}

pub(in crate::features::pages::transfers) fn transfer_job_row(
    palette: ThemePalette,
    job: TransferJobRowSnapshot,
    directory_progress: Option<String>,
    labels: TransferJobRowLabels,
    _selected_remote_path: Option<String>,
    selected_job_id: Option<String>,
    cx: &mut Context<TransferPanel>,
) -> impl IntoElement {
    let status_color = match job.status {
        TransferJobStatus::Running => rgb(palette.warning),
        TransferJobStatus::Paused => rgb(palette.link),
        TransferJobStatus::Cancelling => rgb(palette.warning),
        TransferJobStatus::Cancelled => rgb(palette.text_muted),
        TransferJobStatus::Completed => rgb(0x34d399),
        TransferJobStatus::Failed => rgb(0xfb7185),
    };
    let job_selected = selected_job_id.as_deref() == Some(job.id.as_str());
    let file_name = transfer_job_file_name(&job);
    let icon_path = transfer_job_icon_path(&job.kind, &job);
    let direction_color = transfer_job_direction_color(&job.kind);
    let detail = transfer_job_detail(&job, directory_progress, &labels);
    let progress_label = if job.status == TransferJobStatus::Running {
        job.progress
            .as_ref()
            .map(|progress| transfer_progress_percent_label(progress, &labels.streaming))
            .unwrap_or_else(|| labels.transferring.clone())
    } else {
        transfer_job_status_label(job.status, &labels).to_string()
    };
    let progress_percent = transfer_job_progress_percent(&job);
    let context_job_id = job.id.clone();

    div()
        .id(SharedString::from(format!("transfer-job-row-{}", job.id)))
        .rounded_sm()
        .bg(if job_selected {
            rgb(palette.hover)
        } else {
            rgb(palette.surface)
        })
        .border_1()
        .border_color(if job_selected {
            rgb(palette.link)
        } else {
            rgb(palette.surface)
        })
        .px_2()
        .py(px(6.))
        .cursor_pointer()
        .on_click({
            let job_id = job.id.clone();
            cx.listener(move |panel, _, window, cx| {
                panel.with_app(cx, |this, cx| {
                    window.focus(this.transfer.queue_focus(), cx);
                    this.select_transfer_job(job_id.clone(), cx);
                })
            })
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |panel, event: &MouseDownEvent, window, cx| {
                panel.with_app(cx, |this, cx| {
                    cx.stop_propagation();
                    this.open_transfer_job_menu(context_job_id.clone(), event, window, cx);
                })
            }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    svg()
                        .size(px(14.))
                        .flex_none()
                        .path(icon_path)
                        .text_color(direction_color),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(12.))
                                .text_color(rgb(palette.text))
                                .child(truncate_preview(&file_name, 48)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .overflow_hidden()
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .child(truncate_preview(&detail, 58)),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w(px(52.))
                        .text_align(gpui::TextAlign::Right)
                        .text_size(px(10.))
                        .font_weight(FontWeight(700.))
                        .text_color(status_color)
                        .child(progress_label),
                ),
        )
        .when_some(progress_percent, |this, percent| {
            this.child(
                div()
                    .mt_1()
                    .h(px(4.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgb(palette.border))
                    .child(
                        div()
                            .h_full()
                            .w(relative(percent))
                            .rounded_full()
                            .bg(direction_color),
                    ),
            )
        })
}

fn transfer_job_file_name(job: &TransferJobRowSnapshot) -> String {
    (!job.display_name.trim().is_empty())
        .then(|| job.display_name.clone())
        .or_else(|| {
            job.summary.as_ref().map(|summary| match &job.kind {
                TransferJobKind::Upload { .. } => local_file_name(&summary.local_path),
                _ => remote_file_name(&summary.remote_path),
            })
        })
        .unwrap_or_else(|| match &job.kind {
            TransferJobKind::Download { remote_path, .. }
            | TransferJobKind::OpenExternal { remote_path, .. }
            | TransferJobKind::LoadEditor { remote_path, .. }
            | TransferJobKind::SaveEditor { remote_path, .. }
            | TransferJobKind::LoadProperties { remote_path }
            | TransferJobKind::UpdateProperties { remote_path, .. }
            | TransferJobKind::AiFileAction { remote_path, .. } => remote_file_name(remote_path),
            TransferJobKind::Upload { local_path, .. } => local_file_name(local_path),
            TransferJobKind::ZmodemUpload { file_name, .. }
            | TransferJobKind::ZmodemDownload { file_name, .. }
            | TransferJobKind::TrzszDownload { file_name, .. }
            | TransferJobKind::TrzszUpload { file_name, .. } => file_name.clone(),
            other => truncate_preview(&format!("{other:?}"), 48),
        })
}

fn transfer_job_detail(
    job: &TransferJobRowSnapshot,
    directory_progress: Option<String>,
    labels: &TransferJobRowLabels,
) -> String {
    let time = format_transfer_row_time(job.created_at_ms);
    let size_detail = job.progress.as_ref().and_then(|progress| {
        if let Some(total) = progress.total_bytes.filter(|total| *total > 0) {
            return Some(format!(
                "{} / {}",
                format_file_size(Some(progress.bytes_transferred)),
                format_file_size(Some(total))
            ));
        }
        (progress.bytes_transferred > 0).then(|| {
            format!(
                "{} / {}",
                format_file_size(Some(progress.bytes_transferred)),
                labels.unknown_size
            )
        })
    });
    let completed_size = job.summary.as_ref().and_then(|summary| {
        (summary.bytes > 0).then(|| {
            format!(
                "{} / {}",
                format_file_size(Some(summary.bytes)),
                format_file_size(Some(summary.bytes))
            )
        })
    });
    let text = directory_progress
        .or(size_detail)
        .or(completed_size)
        .or_else(|| (job.status == TransferJobStatus::Failed).then(|| job.detail.clone()))
        .filter(|value| !value.trim().is_empty());
    match text {
        Some(text) => format!("{time} · {text}"),
        None => time,
    }
}

fn transfer_job_progress_percent(job: &TransferJobRowSnapshot) -> Option<f32> {
    matches!(
        job.status,
        TransferJobStatus::Running | TransferJobStatus::Paused
    )
    .then(|| job.progress.as_ref().and_then(transfer_progress_ratio))
    .flatten()
}

fn transfer_job_status_label(status: TransferJobStatus, labels: &TransferJobRowLabels) -> &str {
    match status {
        TransferJobStatus::Running => &labels.transferring,
        TransferJobStatus::Paused => &labels.paused,
        TransferJobStatus::Cancelling => &labels.cancelling,
        TransferJobStatus::Cancelled => &labels.cancelled,
        TransferJobStatus::Completed => &labels.completed,
        TransferJobStatus::Failed => &labels.failed,
    }
}

fn format_transfer_row_time(created_at_ms: u128) -> String {
    let timestamp = i64::try_from(created_at_ms / 1000).unwrap_or(i64::MAX);
    let Ok(datetime) = time::OffsetDateTime::from_unix_timestamp(timestamp) else {
        return "--:--:--".to_string();
    };
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let format = time::macros::format_description!("[hour]:[minute]:[second]");
    datetime
        .to_offset(offset)
        .format(&format)
        .unwrap_or_else(|_| "--:--:--".to_string())
}

fn transfer_job_icon_path(kind: &TransferJobKind, job: &TransferJobRowSnapshot) -> &'static str {
    if transfer_job_is_directory(job) {
        return "icons/conn/folder.svg";
    }
    match kind {
        TransferJobKind::Upload { .. }
        | TransferJobKind::ZmodemUpload { .. }
        | TransferJobKind::TrzszUpload { .. } => "icons/fe/upload.svg",
        TransferJobKind::SendTo { .. } => "icons/send.svg",
        _ => "icons/fe/download.svg",
    }
}

fn transfer_job_is_directory(job: &TransferJobRowSnapshot) -> bool {
    job.progress
        .as_ref()
        .is_some_and(|progress| progress.item_count_total.is_some())
}

fn transfer_job_direction_color(kind: &TransferJobKind) -> gpui::Rgba {
    match kind {
        TransferJobKind::Upload { .. }
        | TransferJobKind::ZmodemUpload { .. }
        | TransferJobKind::TrzszUpload { .. } => rgb(0x4ade80),
        _ => rgb(0x60a5fa),
    }
}

fn remote_file_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn local_file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nyaterm_transport::SftpTransferProgress;

    use crate::models::{TransferJobKind, TransferJobRowSnapshot, TransferJobStatus};

    use super::{
        TransferJobRowLabels, format_transfer_row_time, transfer_job_file_name,
        transfer_job_icon_path, transfer_job_progress_percent, transfer_job_status_label,
    };

    fn labels() -> TransferJobRowLabels {
        TransferJobRowLabels {
            transferring: "transferring-i18n".to_string(),
            paused: "paused-i18n".to_string(),
            cancelling: "cancelling-i18n".to_string(),
            cancelled: "cancelled-i18n".to_string(),
            completed: "completed-i18n".to_string(),
            failed: "failed-i18n".to_string(),
            streaming: "streaming-i18n".to_string(),
            unknown_size: "unknown-size-i18n".to_string(),
        }
    }

    fn progress(
        remote_path: &str,
        bytes_transferred: u64,
        total_bytes: Option<u64>,
        item_count_completed: Option<u64>,
        item_count_total: Option<u64>,
    ) -> SftpTransferProgress {
        SftpTransferProgress {
            remote_path: remote_path.to_string(),
            local_path: PathBuf::from("/local/target"),
            bytes_transferred,
            total_bytes,
            item_count_completed,
            item_count_total,
        }
    }

    fn job(status: TransferJobStatus) -> TransferJobRowSnapshot {
        let kind = TransferJobKind::Download {
            remote_path: "/remote/file.bin".to_string(),
            raw_path_token: None,
            local_path: PathBuf::from("/local/file.bin"),
        };
        crate::models::TransferJobState {
            id: "job-1".to_string(),
            session_id: Some("session-a".to_string()),
            display_name: crate::models::TransferJobState::display_name_for_kind(&kind),
            kind,
            status,
            detail: String::new(),
            created_at_ms: 1_785_555_123_000,
            entries: Vec::new(),
            summary: None,
            progress: Some(progress("/remote/file.bin", 25, Some(100), None, None)),
            control: None,
        }
        .row_snapshot()
    }

    #[test]
    fn completed_transfer_keeps_stable_timestamp_text() {
        let created_at_ms = 1_785_555_123_000;

        assert_eq!(
            format_transfer_row_time(created_at_ms),
            format_transfer_row_time(created_at_ms)
        );
    }

    #[test]
    fn progress_bar_is_only_visible_for_running_or_paused_jobs() {
        assert!(transfer_job_progress_percent(&job(TransferJobStatus::Running)).is_some());
        assert!(transfer_job_progress_percent(&job(TransferJobStatus::Paused)).is_some());
        assert!(transfer_job_progress_percent(&job(TransferJobStatus::Completed)).is_none());
        assert!(transfer_job_progress_percent(&job(TransferJobStatus::Failed)).is_none());
        assert!(transfer_job_progress_percent(&job(TransferJobStatus::Cancelled)).is_none());
    }

    #[test]
    fn directory_transfer_title_stays_on_root_item_name() {
        let kind = TransferJobKind::Download {
            remote_path: "/remote/project".to_string(),
            raw_path_token: None,
            local_path: PathBuf::from("/downloads/project"),
        };
        let job = crate::models::TransferJobState {
            id: "job-1".to_string(),
            session_id: Some("session-a".to_string()),
            display_name: crate::models::TransferJobState::display_name_for_kind(&kind),
            kind,
            status: TransferJobStatus::Running,
            detail: String::new(),
            created_at_ms: 1,
            entries: Vec::new(),
            summary: None,
            progress: Some(progress(
                "/remote/project/src/main.rs",
                10,
                Some(100),
                Some(1),
                Some(4),
            )),
            control: None,
        }
        .row_snapshot();

        assert_eq!(transfer_job_file_name(&job), "project");
        assert_eq!(
            transfer_job_icon_path(&job.kind, &job),
            "icons/conn/folder.svg"
        );
    }

    #[test]
    fn status_labels_come_from_i18n_values() {
        let labels = labels();

        assert_eq!(
            transfer_job_status_label(TransferJobStatus::Completed, &labels),
            "completed-i18n"
        );
        assert_eq!(
            transfer_job_status_label(TransferJobStatus::Failed, &labels),
            "failed-i18n"
        );
    }
}
