use rust_i18n::t;

use gpui::{Context, FontWeight, IntoElement, div, prelude::*, px, relative, rgb};
use zzclawterm_core::RuntimeMode;
use zzclawterm_ui::{ZzClawMarkdown, ZzClawScrollable};

use crate::features::ZzClawTermApp;
use crate::features::update::UpdatePhase;
use crate::features::view_widgets::dialog_action_button;
use crate::widgets::small_button;

const RELEASES_URL: &str = "https://github.com/jackfahdin/ZzClawTerm/releases";

fn format_download_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.;
    const MIB: f64 = KIB * 1024.;
    const GIB: f64 = MIB * 1024.;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

impl ZzClawTermApp {
    pub(in crate::features) fn update_dialog_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let (update_info, phase) = {
            let update = self.update.read(cx);
            (update.info().cloned(), update.phase().clone())
        };
        let checking = matches!(phase, UpdatePhase::Checking);
        let available = update_info.as_ref().is_some_and(|info| info.available);
        let downloading = matches!(phase, UpdatePhase::Downloading { .. });
        let ready = matches!(phase, UpdatePhase::Ready);
        let applying = matches!(phase, UpdatePhase::Applying);
        let check_failed = matches!(
            phase,
            UpdatePhase::Failed {
                download: false,
                ..
            }
        );
        let download_failed = matches!(phase, UpdatePhase::Failed { download: true, .. });
        let failed_message = match &phase {
            UpdatePhase::Failed { message, .. } => Some(message.clone()),
            _ => None,
        };
        let portable = self.runtime.mode() == RuntimeMode::Portable;
        let can_install = crate::features::update::download::supports_native_install(portable);
        let (_, viewport_h) = self.shell.viewport_size();
        let release_url = update_info
            .as_ref()
            .and_then(|info| info.html_url.clone())
            .unwrap_or_else(|| RELEASES_URL.to_string());
        let title = match &phase {
            UpdatePhase::Checking => t!("updater.checking"),
            UpdatePhase::Downloading { .. } => t!("updater.downloading"),
            UpdatePhase::Ready => t!("updater.readyToRestart"),
            UpdatePhase::Applying => t!("updater.installing"),
            UpdatePhase::Failed { .. } => t!("updater.updateFailed"),
            UpdatePhase::Available => t!("updater.newVersionAvailable"),
            UpdatePhase::Idle | UpdatePhase::UpToDate => t!("updater.noUpdate"),
        };

        div()
            .id("update-dialog-content")
            .flex()
            .flex_col()
            .gap_4()
            .max_h(px((viewport_h - 32.).max(220.)))
            .overflow_y_scrollbar()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text))
                            .child(title),
                    )
                    .when_some(update_info.as_ref(), |this, info| {
                        if info.available {
                            this.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(rgb(palette.text_muted))
                                    .child(format!(
                                        "{}: v{}",
                                        t!("updater.currentVersion"),
                                        info.current_version
                                    ))
                                    .child(format!(
                                        "{}: v{}",
                                        t!("updater.newVersion"),
                                        info.latest_version
                                    ))
                                    .when_some(info.release_date.clone(), |this, date| {
                                        this.child(format!(
                                            "{}: {}",
                                            t!("updater.releaseDate"),
                                            date
                                        ))
                                    }),
                            )
                        } else {
                            this.child(div().text_xs().text_color(rgb(palette.text_muted)).child(
                                format!(
                                    "{}: v{}",
                                    t!("updater.currentVersion"),
                                    info.current_version
                                ),
                            ))
                        }
                    })
                    .when_some(failed_message, |this, error| {
                        this.child(
                            div()
                                .text_xs()
                                .line_height(px(18.))
                                .text_color(rgb(palette.danger))
                                .child(error),
                        )
                    }),
            )
            .when_some(
                available
                    .then(|| update_info.as_ref()?.release_notes.clone())
                    .flatten(),
                |this, notes| {
                    this.child(
                        div()
                            .id("update-release-notes")
                            .max_h(px((viewport_h * 0.42).clamp(120., 320.)))
                            .overflow_y_scrollbar()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .p_3()
                            .child(
                                div()
                                    .mb_2()
                                    .text_xs()
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(t!("updater.releaseNotes")),
                            )
                            .child(ZzClawMarkdown::new("update-release-notes-markdown", notes)),
                    )
                },
            )
            .when(can_install && available, |this| {
                this.child(match &phase {
                    UpdatePhase::Downloading { received, total } => {
                        let ratio = total
                            .filter(|total| *total > 0)
                            .map(|total| *received as f32 / total as f32)
                            .unwrap_or(0.)
                            .clamp(0., 1.);
                        let percent = (ratio * 100.).round() as u32;
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_xs()
                                    .text_color(rgb(palette.text_muted))
                                    .child(format!(
                                        "{} / {}",
                                        format_download_bytes(*received),
                                        total
                                            .map(format_download_bytes)
                                            .unwrap_or_else(|| "...".to_string())
                                    ))
                                    .child(format!("{percent}%")),
                            )
                            .child(
                                div()
                                    .h(px(8.))
                                    .w_full()
                                    .rounded_sm()
                                    .bg(rgb(palette.hover))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(ratio))
                                            .rounded_sm()
                                            .bg(rgb(palette.primary)),
                                    ),
                            )
                            .child(
                                div().flex().justify_end().child(
                                    zzclawterm_ui::ZzClawButton::new(
                                        "update-cancel-download",
                                        t!("common.cancel"),
                                    )
                                    .on_click(cx.listener(
                                        |app, _, _, cx| app.cancel_native_update_download(cx),
                                    )),
                                ),
                            )
                            .into_any_element()
                    }
                    UpdatePhase::Available | UpdatePhase::Failed { download: true, .. } => div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .when(download_failed, |this| {
                            this.child(t!("updater.downloadFailed"))
                        })
                        .child(
                            zzclawterm_ui::ZzClawButton::new(
                                "update-download",
                                t!("updater.downloadUpdate"),
                            )
                            .on_click(
                                cx.listener(|app, _, _, cx| app.start_native_update_download(cx)),
                            ),
                        )
                        .into_any_element(),
                    UpdatePhase::Ready => div()
                        .text_xs()
                        .text_color(rgb(palette.text_muted))
                        .child(t!("updater.readyToRestart"))
                        .into_any_element(),
                    _ => div().into_any_element(),
                })
            })
            .when(!checking && !downloading && !applying, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(small_button(
                            palette,
                            "update-close",
                            if ready {
                                t!("updater.later")
                            } else {
                                t!("common.close")
                            },
                            cx.listener(|this, _, window, cx| {
                                this.close_update_dialog(window, cx);
                            }),
                        ))
                        .when(check_failed, |this| {
                            this.child(dialog_action_button(
                                palette,
                                "update-retry",
                                t!("updater.retry"),
                                false,
                                cx.listener(|this, _, _, cx| {
                                    this.start_update_check(cx);
                                }),
                            ))
                        })
                        .when(available, |this| {
                            this.child(dialog_action_button(
                                palette,
                                "update-open-releases",
                                t!("updater.openReleases"),
                                false,
                                cx.listener(move |this, _, _, cx| {
                                    this.open_external_url_for_ui(&release_url, cx);
                                }),
                            ))
                        })
                        .when(ready, |this| {
                            this.child(dialog_action_button(
                                palette,
                                "update-install",
                                t!("updater.installAndRestart"),
                                true,
                                cx.listener(|app, _, window, cx| {
                                    app.request_native_update_install(window, cx)
                                }),
                            ))
                        }),
                )
            })
    }
}
