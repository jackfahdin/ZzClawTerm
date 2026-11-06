use rust_i18n::t;

use std::collections::HashSet;

use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb, rgba};
use zzclawterm_core::truncate_preview;
use zzclawterm_ui::{
    ZzClawButton, ZzClawButtonVariant, ZzClawInputShell, ZzClawScrollable, ZzClawSearchInput,
    mode_button,
};

use crate::features::formatting::session_kind_label;
use crate::features::sync_input::SyncGroupSessionFilter;
use crate::features::{ZzClawTermApp, text_inputs::TextInputSetup};
use crate::widgets::{small_button, status_pill};

impl ZzClawTermApp {
    pub(in crate::features) fn sync_groups_window_view(
        &mut self,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let groups_width = (viewport_width * 0.25).clamp(176., 224.);
        let selected_group = self.sync_input.selected_group().cloned();
        let selected_group_id = selected_group.as_ref().map(|group| group.id.clone());
        let selected_group_color = selected_group
            .as_ref()
            .map(|group| group.color)
            .unwrap_or(0x3b82f6);
        let group_name_input = selected_group.as_ref().map(|group| {
            self.text_input(
                format!("sync.group-name.{}", group.id),
                &group.name,
                TextInputSetup::placeholder(t!("syncGroup.groupName")),
                cx,
            )
        });
        let search_draft = self.sync_input.search_draft().to_string();
        let search_input = self.text_input(
            "sync.groups.search",
            &search_draft,
            TextInputSetup::placeholder(t!("syncGroup.searchPlaceholder")),
            cx,
        );
        let session_filter = self.sync_input.session_filter();
        let groups_vec = self.sync_input.groups().to_vec();
        let groups_count = groups_vec.len();
        let mut group_list = div()
            .id(SharedString::from("sync-groups-list"))
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_1p5();
        if groups_vec.is_empty() {
            group_list = group_list.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.input))
                    .p_3()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("syncGroup.noGroups")),
            );
        }
        for group in groups_vec {
            let selected = selected_group_id.as_deref() == Some(group.id.as_str());
            let session_count = group.session_ids.len();
            let paused_count = group.paused_session_ids.len();
            let display_name = if group.name.trim().is_empty() {
                t!("syncGroup.unnamedGroup").to_string()
            } else {
                group.name.clone()
            };
            let mut subtitle = t!("syncGroup.sessionCount", count = session_count).to_string();
            if paused_count > 0 {
                subtitle.push_str(&format!(
                    " · {}",
                    t!("syncGroup.pausedCount", count = paused_count)
                ));
            }
            if !group.enabled {
                subtitle.push_str(&format!(" · {}", t!("syncGroup.groupDisabled")));
            }
            group_list = group_list.child(
                div()
                    .id(SharedString::from(format!("sync-group-{}", group.id)))
                    .when(selected, |this| {
                        this.debug_selector(|| "sync-group-selected-row".to_string())
                    })
                    .flex_none()
                    .rounded_sm()
                    .border_1()
                    .border_color(if selected {
                        rgb(group.color)
                    } else {
                        rgb(palette.border)
                    })
                    .bg(if selected {
                        rgb(palette.hover)
                    } else {
                        rgb(palette.input)
                    })
                    .px_2p5()
                    .py_2()
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0x151b24)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .w(px(5.))
                                            .h(px(28.))
                                            .rounded_full()
                                            .bg(rgb(group.color)),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight(700.))
                                                    .text_color(rgb(palette.text))
                                                    .child(truncate_preview(&display_name, 22)),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(subtitle),
                                            ),
                                    ),
                            )
                            .child(div().size(px(8.)).rounded_full().bg(rgb(if group.enabled {
                                palette.success
                            } else {
                                palette.text_muted
                            }))),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_sync_group(group.id.clone(), cx);
                    })),
            );
        }

        let selected_members = selected_group
            .as_ref()
            .map(|group| group.session_ids.iter().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let selected_paused = selected_group
            .as_ref()
            .map(|group| {
                group
                    .paused_session_ids
                    .iter()
                    .cloned()
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let has_selected_ssh_host = selected_members
            .iter()
            .any(|id| self.session.ssh_host(id).is_some());
        let mut session_rows = div()
            .id(SharedString::from("sync-sessions-list"))
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_1p5();
        let query = self.sync_input.search_draft().trim().to_ascii_lowercase();
        let all_sessions = self.session.ordered_sessions();
        let total_sessions = all_sessions.len();
        let has_sessions = !all_sessions.is_empty();
        let sessions = all_sessions
            .into_iter()
            .filter(|session| {
                if !self.sync_group_session_matches_search(session, &query) {
                    return false;
                }
                match session_filter {
                    SyncGroupSessionFilter::All => true,
                    SyncGroupSessionFilter::Selected => selected_members.contains(&session.id),
                    SyncGroupSessionFilter::Available => !selected_members.contains(&session.id),
                }
            })
            .collect::<Vec<_>>();
        let shown_sessions = sessions.len();
        if sessions.is_empty() {
            session_rows = session_rows.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.input))
                    .p_3()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!(if has_sessions {
                        "syncGroup.noSessionMatches"
                    } else {
                        "syncGroup.noSessions"
                    })),
            );
        }
        for session in sessions {
            let session_id = session.id.clone();
            let in_group = selected_members.contains(&session_id);
            let paused = selected_paused.contains(&session_id);
            let active = self.session.active_id() == Some(session_id.as_str());
            let title = self.session.display_name_by_info(&session);
            let other_groups = self.sync_input.other_group_names_for_session(&session_id);
            let mut meta_line = session_kind_label(session.kind).to_string();
            if active {
                meta_line.push_str(&format!(" · {}", t!("sessionQuickSwitcher.active")));
            }
            if !other_groups.is_empty() {
                meta_line.push_str(&format!(
                    " · {}",
                    t!(
                        "syncGroup.alsoInGroups",
                        names = truncate_preview(&other_groups.join(", "), 28)
                    )
                ));
            }
            let row_selector = format!("sync-session-{session_id}");
            session_rows = session_rows.child(
                div()
                    .id(SharedString::from(format!("sync-session-{session_id}")))
                    .debug_selector(move || row_selector.clone())
                    .flex_none()
                    .rounded_sm()
                    .border_1()
                    .border_color(if in_group {
                        rgba((selected_group_color << 8) | 0x88)
                    } else {
                        rgb(palette.border)
                    })
                    .bg(if in_group {
                        rgb(0x111827)
                    } else {
                        rgb(palette.input)
                    })
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0x151b24)))
                    .on_click(cx.listener({
                        let session_id = session_id.clone();
                        move |this, _, _, cx| {
                            this.toggle_session_in_selected_sync_group(session_id.clone(), cx);
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .gap_2p5()
                                    .child(
                                        div()
                                            .size(px(16.))
                                            .flex_none()
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(if in_group {
                                                rgb(selected_group_color)
                                            } else {
                                                rgb(palette.border)
                                            })
                                            .bg(if in_group {
                                                rgb(selected_group_color)
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_xs()
                                            .font_weight(FontWeight(800.))
                                            .text_color(rgb(0xffffff))
                                            .when(in_group, |this| this.child("✓")),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight(700.))
                                                    .text_color(if paused {
                                                        rgb(palette.text_muted)
                                                    } else {
                                                        rgb(palette.text)
                                                    })
                                                    .child(truncate_preview(&title, 46)),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(meta_line),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .when(in_group, |this| {
                                        this.child(status_pill(
                                            if paused {
                                                t!("syncGroup.paused")
                                            } else {
                                                t!("syncGroup.broadcastActive")
                                            },
                                            if paused {
                                                rgb(0xfacc15)
                                            } else {
                                                rgb(palette.success)
                                            },
                                            if paused {
                                                rgb(0x3a2f14)
                                            } else {
                                                rgb(palette.hover)
                                            },
                                        ))
                                        .child(small_button(
                                            palette,
                                            format!("sync-session-pause-{session_id}"),
                                            if paused {
                                                t!("syncGroup.resumeSync")
                                            } else {
                                                t!("syncGroup.pauseSync")
                                            },
                                            cx.listener({
                                                let session_id = session_id.clone();
                                                move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.toggle_session_paused_in_selected_sync_group(
                                                        session_id.clone(),
                                                        cx,
                                                    );
                                                }
                                            }),
                                        ))
                                    })
                                    .child(small_button(
                                        palette,
                                        format!("sync-session-toggle-{session_id}"),
                                        if in_group {
                                            t!("common.remove")
                                        } else {
                                            t!("common.add")
                                        },
                                        cx.listener({
                                            let session_id = session_id.clone();
                                            move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.toggle_session_in_selected_sync_group(
                                                    session_id.clone(),
                                                    cx,
                                                );
                                            }
                                        }),
                                    )),
                            ),
                    ),
            );
        }

        let group_header_bar =
            selected_group.as_ref().map(|group| {
                div()
                    .id("sync-group-header")
                    .debug_selector(|| "sync-group-header".to_string())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id(SharedString::from("sync-group-color"))
                            .size(px(34.))
                            .flex_none()
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .bg(rgb(palette.input))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|this| this.bg(rgb(palette.hover)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cycle_selected_sync_group_color(cx);
                            }))
                            .child(div().size(px(16.)).rounded_full().bg(rgb(group.color))),
                    )
                    .child(
                        div()
                            .h(px(34.))
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .children(group_name_input.as_ref().map(|field| {
                                ZzClawInputShell::new("sync-group-name-input", field)
                            })),
                    )
                    .child(
                        ZzClawButton::new(
                            "sync-group-toggle",
                            if group.enabled {
                                t!("syncGroup.groupEnabled")
                            } else {
                                t!("syncGroup.groupDisabled")
                            },
                        )
                        .variant(ZzClawButtonVariant::Secondary)
                        .selected(group.enabled)
                        .small()
                        .compact()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_selected_sync_group_enabled(cx);
                        })),
                    )
                    .child(
                        ZzClawButton::new("sync-group-delete", t!("syncGroup.deleteGroup"))
                            .variant(ZzClawButtonVariant::Danger)
                            .small()
                            .compact()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_delete_selected_sync_group(window, cx);
                            })),
                    )
            });

        div()
            .id("sync-groups-content")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .px_5()
                    .pt_3()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("syncGroup.description")),
            )
            .child(
                div()
                    .px_5()
                    .pt_3p5()
                    .pb_4()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_0()
                    .child(
                        div()
                            .w(px(groups_width))
                            .debug_selector(|| "sync-groups-column".to_string())
                            .flex_none()
                            .min_h_0()
                            .pr_3()
                            .border_r_1()
                            .border_color(rgb(palette.border))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .debug_selector(|| "sync-groups-heading".to_string())
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight(800.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(t!("syncGroup.groups")),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(format!("({groups_count})")),
                                            ),
                                    )
                                    .child(
                                        ZzClawButton::new(
                                            "sync-group-new",
                                            t!("syncGroup.newGroup"),
                                        )
                                        .variant(ZzClawButtonVariant::Secondary)
                                        .small()
                                        .compact()
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.create_sync_group(cx);
                                            }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "sync-groups-viewport".to_string())
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(group_list),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            // Only the list clips its content; input focus rings paint
                            // outside their borders into the surrounding panel padding.
                            .pl_4()
                            .flex()
                            .flex_col()
                            .gap_2p5()
                            .children(group_header_bar)
                            .child(
                                div()
                                    .debug_selector(|| "sync-sessions-search".to_string())
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(div().flex_1().min_w(px(180.)).child(
                                        ZzClawSearchInput::new(
                                            "sync-group-search-input",
                                            &search_input,
                                        ),
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(mode_button(
                                                "sync-group-filter-all",
                                                t!("syncGroup.filterAll"),
                                                session_filter == SyncGroupSessionFilter::All,
                                                palette,
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_sync_groups_session_filter(
                                                        SyncGroupSessionFilter::All,
                                                        cx,
                                                    );
                                                }),
                                            ))
                                            .child(mode_button(
                                                "sync-group-filter-selected",
                                                format!(
                                                    "{} ({})",
                                                    t!("syncGroup.filterSelected"),
                                                    selected_members.len()
                                                ),
                                                session_filter == SyncGroupSessionFilter::Selected,
                                                palette,
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_sync_groups_session_filter(
                                                        SyncGroupSessionFilter::Selected,
                                                        cx,
                                                    );
                                                }),
                                            ))
                                            .child(mode_button(
                                                "sync-group-filter-available",
                                                t!("syncGroup.filterAvailable"),
                                                session_filter == SyncGroupSessionFilter::Available,
                                                palette,
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_sync_groups_session_filter(
                                                        SyncGroupSessionFilter::Available,
                                                        cx,
                                                    );
                                                }),
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "sync-sessions-viewport".to_string())
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(session_rows),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "sync-sessions-footer".to_string())
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .flex_wrap()
                                    .gap_2()
                                    .pt_2()
                                    .border_t_1()
                                    .border_color(rgb(palette.border))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_1p5()
                                            .child(small_button(
                                                palette,
                                                "sync-group-select-all",
                                                t!("syncGroup.selectAll"),
                                                cx.listener(|this, _, _, cx| {
                                                    this.select_all_sync_group_sessions(cx);
                                                }),
                                            ))
                                            .child(
                                                ZzClawButton::new(
                                                    "sync-group-same-host",
                                                    t!("syncGroup.selectSameHost"),
                                                )
                                                .variant(ZzClawButtonVariant::Secondary)
                                                .small()
                                                .compact()
                                                .disabled(!has_selected_ssh_host)
                                                .tooltip(t!("syncGroup.selectSameHostTooltip"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.select_same_host_sync_group_sessions(cx);
                                                })),
                                            )
                                            .when(!query.is_empty(), |this| {
                                                this.child(small_button(
                                                    palette,
                                                    "sync-group-add-filtered",
                                                    t!("syncGroup.addFiltered"),
                                                    cx.listener(|this, _, _, cx| {
                                                        this.add_filtered_sync_group_sessions(cx);
                                                    }),
                                                ))
                                                .child(small_button(
                                                    palette,
                                                    "sync-group-remove-filtered",
                                                    t!("syncGroup.removeFiltered"),
                                                    cx.listener(|this, _, _, cx| {
                                                        this.remove_filtered_sync_group_sessions(
                                                            cx,
                                                        );
                                                    }),
                                                ))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(
                                                        if !query.is_empty()
                                                            || session_filter
                                                                != SyncGroupSessionFilter::All
                                                        {
                                                            format!(
                                                                "{} · {}",
                                                                t!(
                                                                    "syncGroup.filteredCount",
                                                                    shown = shown_sessions,
                                                                    total = total_sessions
                                                                ),
                                                                t!(
                                                                    "syncGroup.selectedCount",
                                                                    selected =
                                                                        selected_members.len(),
                                                                    total = total_sessions
                                                                )
                                                            )
                                                        } else {
                                                            t!(
                                                                "syncGroup.selectedCount",
                                                                selected = selected_members.len(),
                                                                total = total_sessions
                                                            )
                                                            .to_string()
                                                        },
                                                    ),
                                            )
                                            .child(
                                                ZzClawButton::new(
                                                    "sync-group-clear-all",
                                                    t!("syncGroup.deselectAll"),
                                                )
                                                .variant(ZzClawButtonVariant::Secondary)
                                                .small()
                                                .compact()
                                                .disabled(selected_members.is_empty())
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.clear_sync_group_sessions(cx);
                                                })),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}
