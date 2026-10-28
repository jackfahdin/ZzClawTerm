use gpui::{App, ClickEvent, FontWeight, IntoElement, Window, div, prelude::*, px, rgb, svg};
use rust_i18n::t;
use zzclawterm_core::CloudSyncHistoryEntry;
use zzclawterm_ui::{ZzClawButton, ZzClawButtonVariant};

use crate::features::formatting::{
    SyncHistoryTimeFormatter, cloud_sync_history_summary, cloud_sync_history_trigger,
    cloud_sync_status_dot_color, format_cloud_provider, format_duration_ms,
};
use crate::theme::ThemePalette;

pub(in crate::features) fn cloud_sync_history_row(
    palette: ThemePalette,
    entry: CloudSyncHistoryEntry,
    formatter: &SyncHistoryTimeFormatter,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_copy: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let summary = cloud_sync_history_summary(&entry);
    let trigger = cloud_sync_history_trigger(&entry.trigger);
    let duration = format_duration_ms(entry.duration_ms);
    let metadata = duration.as_ref().map_or_else(
        || trigger.clone(),
        |duration| format!("{trigger} · {duration}"),
    );
    let icon = match entry.status.as_str() {
        "success" => "icons/check.svg",
        "conflict" => "icons/warning.svg",
        "failed" => "icons/close.svg",
        _ => "icons/clock.svg",
    };
    let actions = div()
        .flex()
        .flex_wrap()
        .gap_1()
        .child(
            ZzClawButton::new(
                format!("sync-history-toggle-{}", entry.id),
                if expanded {
                    t!("settings.historyHideDetails")
                } else {
                    t!("settings.historyViewDetails")
                },
            )
            .variant(ZzClawButtonVariant::Ghost)
            .compact()
            .small()
            .on_click(on_toggle),
        )
        .when(expanded, |this| {
            this.child(
                ZzClawButton::new(
                    format!("sync-history-copy-{}", entry.id),
                    t!("settings.syncHistory.copyDetails"),
                )
                .variant(ZzClawButtonVariant::Ghost)
                .compact()
                .small()
                .on_click(on_copy),
            )
        });
    let heading = div()
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_size(px(12.))
                .font_weight(FontWeight(600.))
                .text_color(rgb(palette.text))
                .child(summary),
        )
        .child(
            div()
                .flex_none()
                .font_family(crate::features::shell::gpui_code_font_family())
                .text_size(px(10.))
                .text_color(rgb(palette.text_dimmed))
                .child(formatter.clock(entry.timestamp_ms)),
        );
    let content = div()
        .min_w_0()
        .flex_1()
        .flex()
        .flex_col()
        .gap_1()
        .child(heading)
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .child(metadata),
        )
        .when(entry.status == "conflict", |this| {
            this.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("settings.syncHistory.conflictHint")),
            )
        })
        .child(actions)
        .when(expanded, |this| {
            let details = div()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .py_2()
                .text_size(px(10.))
                .text_color(rgb(palette.text_muted))
                .child(detail_field(
                    t!("settings.syncHistory.time"),
                    formatter.full(entry.timestamp_ms),
                ))
                .child(detail_field(
                    t!("settings.syncHistory.provider"),
                    entry
                        .provider
                        .as_deref()
                        .map(format_cloud_provider)
                        .unwrap_or_else(|| "—".to_string()),
                ))
                .child(detail_field(t!("settings.syncHistory.trigger"), trigger))
                .when_some(duration, |this, duration| {
                    this.child(detail_field(t!("settings.syncHistory.duration"), duration))
                })
                .when_some(entry.revision, |this, revision| {
                    this.child(detail_field(t!("settings.historyRevision"), revision))
                })
                .child(detail_field(
                    t!("settings.syncHistory.event"),
                    format!("{} · {} · {}", entry.kind, entry.status, entry.trigger),
                ))
                .when(!entry.message.trim().is_empty(), |this| {
                    this.child(detail_field(
                        t!("settings.syncHistory.diagnostics"),
                        entry.message,
                    ))
                });
            this.child(details)
        });
    div()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(palette.surface_elevated))
        .child(
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(
                    svg()
                        .path(icon)
                        .size(px(14.))
                        .mt(px(2.))
                        .flex_none()
                        .text_color(cloud_sync_status_dot_color(palette, &entry.status)),
                )
                .child(content),
        )
}

fn detail_field(label: impl Into<gpui::SharedString>, value: String) -> impl IntoElement {
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().child(label.into()))
        .child(
            div()
                .whitespace_normal()
                .font_family(crate::features::shell::gpui_code_font_family())
                .child(value),
        )
}
