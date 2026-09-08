use rust_i18n::t;

use std::collections::HashSet;

use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb, rgba,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::{NyaScrollable, NyaSearchInput};

use crate::features::formatting::session_kind_label;
use crate::features::view_widgets::{dialog_action_button, full_window_input_layer};
use crate::features::{
    NyaTermApp, text_inputs::ORDINARY_INPUT_SHELL_PADDING_X_PX, text_inputs::TextInputSetup,
    text_inputs::ordinary_input_focus_ring, text_inputs::ordinary_input_shell_border_color,
};
use crate::widgets::{small_button, status_pill};

impl NyaTermApp {
    pub(in crate::features) fn sync_groups_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let dialog_width = (viewport_w - 24.).clamp(280., 900.);
        let dialog_height = (viewport_h - 56.).clamp(280., 500.);
        let groups_width = (dialog_width * 0.23).clamp(160., 208.);
        let selected_group = self.sync_input.selected_group().cloned();
        let selected_group_id = selected_group.as_ref().map(|group| group.id.clone());
        let (group_name_input, group_name_focus, group_name_focused) =
            if let Some(group) = selected_group.as_ref() {
                let field = self.text_input(
                    format!("sync.group-name.{}", group.id),
                    &group.name,
                    TextInputSetup::default(),
                    cx,
                );
                let focus = field.read(cx).focus_handle();
                let focused = field.read(cx).has_focus();
                (Some(field), Some(focus), focused)
            } else {
                (None, None, false)
            };
        let search_draft = self.sync_input.search_draft().to_string();
        let search_input = self.text_input(
            "sync.groups.search",
            &search_draft,
            TextInputSetup::placeholder(t!("syncGroup.searchPlaceholder")),
            cx,
        );
        let pending_delete_name = self
            .sync_input
            .pending_delete_group()
            .map(|group| group.name.clone());
        let pending_delete_message = pending_delete_name
            .as_ref()
            .map(|name| t!("syncGroup.deleteGroupConfirm", name = name));
        let mut group_list = div()
            .id(SharedString::from("sync-groups-list"))
            .flex_1()
            .min_h_0()
            .max_h_full()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_2();
        if self.sync_input.groups().is_empty() {
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
        for group in self.sync_input.groups().to_vec() {
            let selected = selected_group_id.as_deref() == Some(group.id.as_str());
            let session_count = group.session_ids.len();
            group_list = group_list.child(
                div()
                    .id(SharedString::from(format!("sync-group-{}", group.id)))
                    .rounded_sm()
                    .border_1()
                    .border_color(if selected {
                        rgb(0x3b82f6)
                    } else {
                        rgb(palette.border)
                    })
                    .bg(if selected {
                        rgb(palette.hover)
                    } else {
                        rgb(palette.input)
                    })
                    .p_3()
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
                                            .w(px(6.))
                                            .h(px(36.))
                                            .rounded_full()
                                            .bg(rgb(group.color)),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_sm()
                                            .font_weight(FontWeight(800.))
                                            .text_color(rgb(palette.text))
                                            .child(truncate_preview(&group.name, 26)),
                                    ),
                            )
                            .child(div().size(px(8.)).rounded_full().bg(rgb(if group.enabled {
                                palette.success
                            } else {
                                palette.text_muted
                            }))),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(rgb(palette.text_muted))
                            .child(t!("syncGroup.sessionCount", count = session_count)),
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
        let mut session_rows = div()
            .id(SharedString::from("sync-sessions-list"))
            .flex_1()
            .min_h_0()
            .max_h_full()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_2();
        let query = self.sync_input.search_draft().trim().to_ascii_lowercase();
        let all_sessions = self.session.ordered_sessions();
        let has_sessions = !all_sessions.is_empty();
        let sessions = all_sessions
            .into_iter()
            .filter(|session| self.sync_group_session_matches_search(session, &query))
            .collect::<Vec<_>>();
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
            session_rows = session_rows.child(
                div()
                    .id(SharedString::from(format!("sync-session-{session_id}")))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(if in_group {
                        rgb(0x111827)
                    } else {
                        rgb(palette.input)
                    })
                    .p_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight(800.))
                                            .text_color(if paused {
                                                rgb(palette.text_muted)
                                            } else {
                                                rgb(palette.text)
                                            })
                                            .child(truncate_preview(&title, 42)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(palette.text_muted))
                                            .child(format!(
                                                "{}{}",
                                                session_kind_label(session.kind),
                                                if active {
                                                    format!(
                                                        " · {}",
                                                        t!("sessionQuickSwitcher.active")
                                                    )
                                                } else {
                                                    String::new()
                                                }
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(status_pill(
                                        if paused {
                                            t!("syncGroup.paused")
                                        } else if in_group {
                                            t!("syncGroup.activeMembers")
                                        } else {
                                            t!("syncGroup.filterAvailable")
                                        },
                                        if paused {
                                            rgb(0xfacc15)
                                        } else if in_group {
                                            rgb(palette.success)
                                        } else {
                                            rgb(palette.text_muted)
                                        },
                                        if paused {
                                            rgb(0x3a2f14)
                                        } else if in_group {
                                            rgb(palette.hover)
                                        } else {
                                            rgb(palette.border)
                                        },
                                    ))
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
                                                this.toggle_session_in_selected_sync_group(
                                                    session_id.clone(),
                                                    cx,
                                                );
                                            }
                                        }),
                                    ))
                                    .when(in_group, |this| {
                                        this.child(small_button(
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
                                                    this.toggle_session_paused_in_selected_sync_group(
                                                        session_id.clone(),
                                                        cx,
                                                    );
                                                }
                                            }),
                                        ))
                                    }),
                            ),
                    ),
            );
        }

        div()
            .id(SharedString::from("sync-groups-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(self.sync_input.focus())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let mut handled = false;
                if this.sync_input.has_delete_pending() {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            this.cancel_delete_sync_group(cx);
                            handled = true;
                        }
                        "enter" => {
                            this.confirm_delete_sync_group(cx);
                            handled = true;
                        }
                        _ => {}
                    }
                } else {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            this.close_sync_groups(cx);
                            handled = true;
                        }
                        "n" | "N" if this.sync_input.focus().is_focused(window) => {
                            this.create_sync_group(cx);
                            handled = true;
                        }
                        "delete" if this.sync_input.focus().is_focused(window) => {
                            this.request_delete_selected_sync_group(cx);
                            handled = true;
                        }
                        _ => {}
                    }
                }
                if handled {
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .id("sync-groups-backdrop")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(this.sync_input.focus(), cx);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id(SharedString::from("sync-groups-dialog"))
                    .w(px(dialog_width))
                    .max_w_full()
                    .mx_4()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.bg))
                    .shadow_lg()
                    .h(px(dialog_height))
                    .max_h_full()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px_5()
                            .py_4()
                            .border_b_1()
                            .border_color(rgb(palette.border))
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight(800.))
                                            .text_color(rgb(palette.text))
                                            .child(t!("syncGroup.title")),
                                    )
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(small_button(palette,
                                        "sync-group-new",
                                        t!("syncGroup.newGroup"),
                                        cx.listener(|this, _, _, cx| {
                                            this.create_sync_group(cx);
                                        }),
                                    ))
                                    .child(small_button(palette,
                                        "sync-group-close",
                                        t!("common.close"),
                                        cx.listener(|this, _, _, cx| {
                                            this.close_sync_groups(cx);
                                        }),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .px_5()
                            .pt_4()
                            .pb_5()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .gap_0()
                            .child(
                                div()
                                    .w(px(groups_width))
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
                                            .text_xs()
                                            .font_weight(FontWeight(800.))
                                            .text_color(rgb(palette.text_muted))
                                            .child(t!("syncGroup.groups")),
                                    )
                                    .child(group_list),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .pl_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .w(px(6.))
                                                    .h(px(34.))
                                                    .rounded_full()
                                                    .bg(rgb(
                                                        selected_group
                                                            .as_ref()
                                                            .map(|group| group.color)
                                                            .unwrap_or(palette.border),
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .id(SharedString::from("sync-group-name-input"))
                                                    .h(px(34.))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .px(px(ORDINARY_INPUT_SHELL_PADDING_X_PX))
                                                    .flex()
                                                    .items_center()
                                                    .rounded_sm()
                                                    .border_1()
                                                    .border_color(
                                                        ordinary_input_shell_border_color(
                                                            palette,
                                                            group_name_focused,
                                                        ),
                                                    )
                                                    .when(group_name_focused, |this| {
                                                        this.shadow(ordinary_input_focus_ring(
                                                            palette,
                                                        ))
                                                    })
                                                    .bg(rgb(palette.input))
                                                    .text_sm()
                                                    .text_color(rgb(palette.text))
                                                    .cursor_text()
                                                    .when_some(
                                                        group_name_focus,
                                                        |this, focus| {
                                                            this.on_click(move |_, window, cx| {
                                                                window.focus(&focus, cx);
                                                            })
                                                        },
                                                    )
                                                    .children(group_name_input),
                                            ),
                                    )
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
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight(800.))
                                                            .text_color(rgb(palette.text_muted))
                                                            .child(t!("syncGroup.sessions")),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w(px(140.))
                                                            .child(NyaSearchInput::new(
                                                                "sync-group-search-input",
                                                                &search_input,
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(small_button(palette,
                                                        "sync-group-toggle",
                                                        if selected_group
                                                            .as_ref()
                                                            .is_some_and(|group| group.enabled)
                                                        {
                                                            t!("syncGroup.disable")
                                                        } else {
                                                            t!("syncGroup.enable")
                                                        },
                                                        cx.listener(|this, _, _, cx| {
                                                            this.toggle_selected_sync_group_enabled(cx);
                                                        }),
                                                    ))
                                                    .child(small_button(palette,
                                                        "sync-group-delete",
                                                        t!("syncGroup.deleteGroup"),
                                                        cx.listener(|this, _, _, cx| {
                                                            this.request_delete_selected_sync_group(cx);
                                                        }),
                                                    )),
                                            ),
                                    )
                                    .child(session_rows)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_2()
                                            .pt_2()
                                            .border_t_1()
                                            .border_color(rgb(palette.border))
                                            .child(small_button(
                                                palette,
                                                "sync-group-select-all",
                                                t!("syncGroup.selectAll"),
                                                cx.listener(|this, _, _, cx| {
                                                    this.select_all_sync_group_sessions(cx);
                                                }),
                                            ))
                                            .child(small_button(
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
                                                    this.remove_filtered_sync_group_sessions(cx);
                                                }),
                                            ))
                                            .child(small_button(
                                                palette,
                                                "sync-group-same-host",
                                                t!("syncGroup.selectSameHost"),
                                                cx.listener(|this, _, _, cx| {
                                                    this.select_same_host_sync_group_sessions(cx);
                                                }),
                                            ))
                                            .child(small_button(
                                                palette,
                                                "sync-group-clear-all",
                                                t!("syncGroup.deselectAll"),
                                                cx.listener(|this, _, _, cx| {
                                                    this.clear_sync_group_sessions(cx);
                                                }),
                                            )),
                                    ),
                            ),
                    ),
            )
            .when_some(pending_delete_message, |this, message| {
                this.child(
                    full_window_input_layer("sync-group-delete-backdrop")
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba(0x00000099))
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .id(SharedString::from("sync-group-delete-dialog"))
                                .w(px(384.))
                                .max_w_full()
                                .mx_4()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(palette.border))
                                .bg(rgb(palette.surface_elevated))
                                .shadow_lg()
                                .p_6()
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight(800.))
                                        .text_color(rgb(palette.text))
                                        .child(t!("syncGroup.deleteGroup")),
                                )
                                .child(
                                    div()
                                        .mt_2()
                                        .text_xs()
                                        .text_color(rgb(palette.text_muted))
                                        .child(message),
                                )
                                .child(
                                    div()
                                        .mt_4()
                                        .flex()
                                        .justify_end()
                                        .gap_2()
                                        .child(small_button(
                                            palette,
                                            "sync-group-delete-cancel",
                                            t!("common.cancel"),
                                            cx.listener(|this, _, _, cx| {
                                                this.cancel_delete_sync_group(cx);
                                            }),
                                        ))
                                        .child(dialog_action_button(
                                            palette,
                                            "sync-group-delete-confirm",
                                            t!("syncGroup.deleteGroup"),
                                            true,
                                            cx.listener(|this, _, _, cx| {
                                                this.confirm_delete_sync_group(cx);
                                            }),
                                        )),
                                ),
                        ),
                )
            })
    }
}
