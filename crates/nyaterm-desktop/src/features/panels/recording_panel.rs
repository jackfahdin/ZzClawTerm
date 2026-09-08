use rust_i18n::t;

use std::sync::Arc;
use std::time::Instant;

use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, KeyDownEvent, SharedString,
    Window, div, prelude::*, px, rgb, rgba, svg, uniform_list,
};
use nyaterm_core::truncate_preview;
use nyaterm_transport::{RecordingMode, SessionInfo};
use nyaterm_ui::{NyaScrollable, NyaSearchInput};

use crate::features::formatting::{session_kind_label, short_id};
use crate::features::perf::record_gpui_perf_sample;
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::RecordingPathPromptKind;

const RECORDING_PANEL_LOGICAL_ROW_HEIGHT_PX: f32 = 52.;

#[derive(Clone)]
struct RecordingSessionPanelRow {
    session: SessionInfo,
    display_name: String,
}

pub(in crate::features) struct RecordingSessionsPanelModel {
    total_count: usize,
    rows: Arc<Vec<RecordingSessionPanelRow>>,
    query_active: bool,
}

impl RecordingSessionsPanelModel {
    pub(in crate::features) fn count_label(&self) -> String {
        session_panel_count_label(self.total_count, self.rows.len(), self.query_active)
    }
}

impl NyaTermApp {
    pub(in crate::features) fn recording_sessions_panel_model(
        &self,
    ) -> RecordingSessionsPanelModel {
        let started_at = Instant::now();
        let sessions = self.sorted_active_sessions();
        let total_count = sessions.len();
        let query = self.recording_session_filter_query();
        let query_active = !query.is_empty();
        let rows = sessions
            .into_iter()
            .filter_map(|session| {
                let display_name = self.session.display_name_by_info(&session);
                recording_session_matches_query(&session, &display_name, &query).then_some(
                    RecordingSessionPanelRow {
                        session,
                        display_name,
                    },
                )
            })
            .collect::<Vec<_>>();
        let model = RecordingSessionsPanelModel {
            total_count,
            rows: Arc::new(rows),
            query_active,
        };
        record_gpui_perf_sample(
            "recording_sessions_model",
            started_at.elapsed(),
            self.gpui_perf_context(model.rows.len(), None),
        );
        model
    }

    pub(in crate::features) fn recording_panel(
        &mut self,
        model: RecordingSessionsPanelModel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let RecordingSessionsPanelModel {
            total_count,
            rows,
            query_active: _,
        } = model;
        let palette = self.theme_palette();
        let active_session_id = self.session.active_id_owned();
        let no_sessions_label = t!("panel.noActiveSessions").to_string();
        let no_matches_label = t!("activeSessions.noMatches").to_string();
        let search_placeholder = t!("recording.searchPlaceholder").to_string();
        let search_draft = self.recording.search_draft().to_string();
        let search_field = self.text_input(
            "recording.search",
            &search_draft,
            TextInputSetup::placeholder(search_placeholder),
            cx,
        );

        let list_scroll = self.session.recording_list_scroll().clone();
        let session_list: AnyElement = if total_count == 0 {
            div()
                .id(SharedString::from("recording-session-list"))
                .flex_1()
                .min_h_0()
                .child(crate::widgets::empty_panel_with_icon(
                    no_sessions_label,
                    palette,
                    "icons/record.svg",
                ))
                .into_any_element()
        } else if rows.is_empty() {
            div()
                .id(SharedString::from("recording-session-list"))
                .flex_1()
                .min_h_0()
                .child(crate::widgets::empty_panel_with_icon(
                    no_matches_label,
                    palette,
                    "icons/record.svg",
                ))
                .into_any_element()
        } else {
            uniform_list(
                "recording-session-list",
                rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    let mut items = Vec::with_capacity(range.len());
                    for index in range {
                        let Some(row) = rows.get(index).cloned() else {
                            continue;
                        };
                        items.push(
                            div()
                                .h(px(RECORDING_PANEL_LOGICAL_ROW_HEIGHT_PX))
                                .px_2()
                                .pb_1()
                                .flex_none()
                                .child(this.recording_session_row(
                                    row.session,
                                    row.display_name,
                                    active_session_id.as_deref(),
                                    cx,
                                )),
                        );
                    }
                    items
                }),
            )
            .flex_1()
            .min_h_0()
            .track_scroll(&list_scroll)
            .into_any_element()
        };

        // Tauri RecordingPanel: PanelHeader(meta count) + search strip + dense session rows.
        // Shared stack already renders PanelHeader; body is search + list only.
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(self.shell_transparent_color(palette.surface))
            .child(
                div()
                    .h(px(40.))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_transparent_color(palette.section_header))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().relative().flex_1().min_w_0().child(
                        NyaSearchInput::new("recording-session-search", &search_field).on_key_down(
                            cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                if event.keystroke.key == "escape" {
                                    cx.stop_propagation();
                                    this.recording.clear_search_draft();
                                    this.reset_text_input("recording.search", "", cx);
                                    this.shell
                                        .set_status("recording search cleared".to_string());
                                    cx.notify();
                                }
                            }),
                        ),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .py_2()
                    .child(session_list)
                    .vertical_scrollbar(&list_scroll),
            )
    }

    fn recording_session_row(
        &mut self,
        session: SessionInfo,
        display_name: String,
        active_session_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let recording_label = t!("recording.recording").to_string();
        let session_name = display_name;
        let session_id = session.id.clone();
        let start_session_id = session.id.clone();
        let save_session_id = session.id.clone();
        let select_session_id = session.id.clone();
        let start_session_name = session_name.clone();
        let save_session_name = session_name.clone();
        let is_current = active_session_id == Some(session.id.as_str());
        let session_is_recording = self.recording.is_recording(&session.id);
        let busy_action = self.recording.busy_action(&session.id).map(str::to_string);
        let is_busy = busy_action.is_some();
        let kind = session_kind_label(session.kind).to_ascii_uppercase();
        let short = short_id(&session.id).to_string();
        let recording_status = self.recording.status(&session.id);
        let recording_path = recording_status
            .as_ref()
            .and_then(|status| status.file_path.clone());
        let recording_status_text = recording_status.as_ref().map(|status| {
            let mode = match status.mode {
                RecordingMode::Transcript => "transcript",
                RecordingMode::Raw => "raw",
            };
            let path = status
                .file_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("recording.log");
            format!(
                "{mode} · {} · {path}",
                format_recording_bytes(status.written_bytes)
            )
        });

        div()
            .id(SharedString::from(format!(
                "recording-session-row-{session_id}"
            )))
            .h(px(48.))
            .rounded_md()
            .px_2()
            .border_1()
            .border_color(if is_current {
                rgba((palette.primary << 8) | 0x73)
            } else {
                rgba(0x00000000)
            })
            .bg(rgba(0x00000000))
            .when(is_busy, |this| this.opacity(0.72))
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .hover(|this| this.bg(rgb(palette.hover)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_session(select_session_id.clone(), cx);
            }))
            .child(
                div()
                    .size(px(8.))
                    .rounded_full()
                    .flex_none()
                    .bg(if session_is_recording {
                        rgb(0xef4444)
                    } else {
                        rgb(0x22c55e)
                    }),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_xs()
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(palette.text))
                                    .overflow_hidden()
                                    .child(truncate_preview(&session_name, 34)),
                            )
                            .child(
                                div()
                                    .px_1()
                                    .rounded_sm()
                                    .bg(rgb(palette.hover))
                                    .text_size(px(10.))
                                    .font_weight(FontWeight(700.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(kind),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .font_family(crate::features::shell::gpui_code_font_family())
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child(short),
                            )
                            .when(session_is_recording, |this| {
                                this.child(
                                    div()
                                        .px_1()
                                        .rounded_sm()
                                        .bg(rgba((palette.danger << 8) | 0x24))
                                        .text_size(px(10.))
                                        .text_color(rgb(palette.danger))
                                        .child(format!("● {recording_label}")),
                                )
                            })
                            .when_some(recording_status_text.clone(), |this, text| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_size(px(10.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(truncate_preview(&text, 44)),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_0()
                    .when_some(recording_path.clone(), |this, path| {
                        this.child(recording_action_svg_button(
                            palette,
                            format!("recording-session-reveal-{session_id}"),
                            "icons/conn/folder.svg",
                            rgb(palette.text_muted),
                            "Show recording in folder".to_string(),
                            !is_busy,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                cx.reveal_path(&path);
                                this.shell
                                    .set_status("recording folder revealed".to_string());
                            }),
                        ))
                    })
                    .child(recording_action_svg_button(
                        palette,
                        format!("recording-session-toggle-{session_id}"),
                        if session_is_recording {
                            "icons/session/stop.svg"
                        } else {
                            "icons/session/record.svg"
                        },
                        if busy_action.as_deref() == Some("record") {
                            rgb(palette.warning)
                        } else if session_is_recording {
                            rgb(palette.danger)
                        } else {
                            rgb(palette.text_muted)
                        },
                        if session_is_recording {
                            t!("recording.stop").to_string()
                        } else {
                            t!("recording.start").to_string()
                        },
                        !is_busy,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.recording.busy_action(&start_session_id).is_some() {
                                return;
                            }
                            if this.recording.is_recording(&start_session_id) {
                                this.stop_recording_for_session(&start_session_id, cx);
                            } else {
                                this.prompt_recording_path_for_session(
                                    RecordingPathPromptKind::Start,
                                    start_session_id.clone(),
                                    start_session_name.clone(),
                                    cx,
                                );
                            }
                        }),
                    ))
                    .child(recording_action_svg_button(
                        palette,
                        format!("recording-session-save-{session_id}"),
                        "icons/session/save.svg",
                        if busy_action.as_deref() == Some("save") {
                            rgb(palette.warning)
                        } else {
                            rgb(palette.text_muted)
                        },
                        t!("recording.saveTranscript").to_string(),
                        !is_busy,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.recording.busy_action(&save_session_id).is_some() {
                                return;
                            }
                            this.prompt_recording_path_for_session(
                                RecordingPathPromptKind::SaveTranscript,
                                save_session_id.clone(),
                                save_session_name.clone(),
                                cx,
                            );
                        }),
                    )),
            )
    }

    fn recording_session_filter_query(&self) -> String {
        self.recording.search_draft().trim().to_lowercase()
    }
}

fn recording_session_matches_query(session: &SessionInfo, display_name: &str, query: &str) -> bool {
    query.is_empty()
        || format!(
            "{} {} {} {}",
            display_name,
            session.name,
            session_kind_label(session.kind),
            session.id
        )
        .to_lowercase()
        .contains(query)
}

fn session_panel_count_label(total: usize, visible: usize, query_active: bool) -> String {
    if query_active {
        format!("{visible}/{total}")
    } else {
        total.to_string()
    }
}

fn format_recording_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / 1024. / 1024.)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}

fn recording_action_svg_button(
    palette: crate::theme::ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    color: impl Into<gpui::Hsla>,
    tooltip: impl Into<String>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = color.into();
    let tooltip = tooltip.into();
    div()
        .id(SharedString::from(id.into()))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(color)
        .when(enabled, |this| {
            this.cursor_pointer().hover(|this| {
                this.bg(rgb(palette.surface_elevated))
                    .text_color(rgb(palette.text))
            })
        })
        .when(!enabled, |this| this.opacity(0.4))
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon_path)
                .text_color(color),
        )
        .tooltip(move |window, cx| nyaterm_ui::NyaTooltip::new(tooltip.clone()).build(window, cx))
        .on_click(move |event, window, cx| {
            if enabled {
                on_click(event, window, cx);
            }
        })
}

#[cfg(test)]
mod tests {
    use nyaterm_transport::{SessionInfo, SessionKind};

    use super::{recording_session_matches_query, session_panel_count_label};

    fn session() -> SessionInfo {
        SessionInfo {
            id: "session-42".to_string(),
            name: "Production Recorder".to_string(),
            kind: SessionKind::Ssh,
            working_dir: None,
            cols: 80,
            rows: 24,
        }
    }

    #[test]
    fn recording_session_query_matches_dynamic_name_original_name_kind_and_id() {
        let session = session();

        assert!(recording_session_matches_query(
            &session,
            "Renamed Recorder",
            "renamed"
        ));
        assert!(recording_session_matches_query(
            &session,
            "Renamed Recorder",
            "production"
        ));
        assert!(recording_session_matches_query(
            &session,
            "Renamed Recorder",
            "ssh"
        ));
        assert!(recording_session_matches_query(
            &session,
            "Renamed Recorder",
            "session-42"
        ));
        assert!(!recording_session_matches_query(
            &session,
            "Renamed Recorder",
            "missing"
        ));
    }

    #[test]
    fn recording_session_count_label_tracks_filtered_and_unfiltered_counts() {
        assert_eq!(session_panel_count_label(100, 9, false), "100");
        assert_eq!(session_panel_count_label(100, 9, true), "9/100");
    }
}
