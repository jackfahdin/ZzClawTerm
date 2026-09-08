use gpui::{
    Anchor, Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, relative, rgb,
    rgba, svg,
};
use nyaterm_core::truncate_preview;
use nyaterm_transport::{SessionInfo, SessionKind};
use nyaterm_ui::{NyaHoverCard, NyaPopover, NyaPopoverAlign, NyaPopoverPlacement};
use std::collections::HashMap;
use std::time::Instant;

use super::super::super::NyaTermApp;
use super::PaneBorderEdges;
use crate::features::formatting::{session_kind_label, short_id};
use crate::features::perf::record_gpui_perf_sample;
use crate::features::shell::{NewSessionMenuAnchor, SessionTabDragPayload, SessionTabDragPreview};
use crate::models::{
    TabDockEdge, TabDockZone, TerminalWindowNode, WorkspacePaneNode, WorkspaceSplitDirection,
};
use crate::theme::ThemePalette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalWindowTabRenderMeta {
    number: usize,
    kind: SessionKind,
}

#[derive(Default)]
pub(super) struct TerminalWindowRenderIndex {
    tabs: HashMap<String, TerminalWindowTabRenderMeta>,
}

impl TerminalWindowRenderIndex {
    fn from_sessions(sessions: impl IntoIterator<Item = SessionInfo>) -> Self {
        let tabs = sessions
            .into_iter()
            .enumerate()
            .map(|(index, session)| {
                (
                    session.id,
                    TerminalWindowTabRenderMeta {
                        number: index + 1,
                        kind: session.kind,
                    },
                )
            })
            .collect();
        Self { tabs }
    }

    fn tab(&self, session_id: &str) -> Option<TerminalWindowTabRenderMeta> {
        self.tabs.get(session_id).copied()
    }
}

impl NyaTermApp {
    fn terminal_window_render_index(&self) -> TerminalWindowRenderIndex {
        TerminalWindowRenderIndex::from_sessions(self.ordered_tab_sessions())
    }

    pub(super) fn render_terminal_window_tree(
        &mut self,
        node: TerminalWindowNode,
        border_edges: PaneBorderEdges,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let started_at = Instant::now();
        let index = self.terminal_window_render_index();
        let output = self.render_terminal_window_node(node, border_edges, &index, cx);
        record_gpui_perf_sample(
            "terminal_window_chrome",
            started_at.elapsed(),
            self.gpui_perf_context(index.tabs.len(), None),
        );
        output
    }

    fn render_terminal_window_node(
        &mut self,
        node: TerminalWindowNode,
        border_edges: PaneBorderEdges,
        render_index: &TerminalWindowRenderIndex,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = self.theme_palette();
        match node {
            TerminalWindowNode::Leaf {
                id,
                tab_ids,
                active_tab_id,
            } => {
                let active = active_tab_id
                    .clone()
                    .or_else(|| tab_ids.first().cloned())
                    .unwrap_or_default();
                let drop_zone = self.terminal.terminal_window_drop_for_leaf(&id);
                let mut strip = div()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .bg(self.shell_surface_color(palette.surface));
                for tab_id in &tab_ids {
                    let is_active_tab = active.as_str() == tab_id.as_str()
                        || self
                            .session
                            .active_id()
                            .is_some_and(|id| self.tab_root_for_session(id) == *tab_id);
                    let tab_meta = render_index.tab(tab_id);
                    let tab_number = tab_meta.map(|meta| meta.number).unwrap_or(0);
                    let title = self
                        .session
                        .display_name(tab_id)
                        .unwrap_or_else(|| short_id(tab_id).to_string());
                    let leaf_id = id.clone();
                    let select_id = tab_id.clone();
                    let close_id = tab_id.clone();
                    let actions_id = tab_id.clone();
                    let drop_before_id = tab_id.clone();
                    let (kind_label, kind_icon) = tab_meta
                        .map(|meta| {
                            (
                                session_kind_label(meta.kind),
                                multi_leaf_session_kind_icon(meta.kind),
                            )
                        })
                        .unwrap_or(("Session", "icons/conn/terminal.svg"));
                    let custom_color = self.session.tab_color(tab_id);
                    let leaf_ids = self
                        .shell
                        .workspace_pane_root(tab_id)
                        .map(|root| root.session_ids())
                        .unwrap_or_else(|| vec![tab_id.clone()]);
                    let is_disconnected =
                        leaf_ids.iter().any(|id| self.session.is_disconnected(id));
                    let has_unread = leaf_ids
                        .iter()
                        .any(|id| self.terminal.session_has_unread(id));
                    let sync_group = self.sync_input.active_group_for_session(tab_id);
                    let sync_paused = self.sync_input.session_is_paused_in_active_group(tab_id);
                    let show_sync_indicator =
                        self.sync_input.broadcast_to_all() || sync_group.is_some();
                    let sync_indicator_color =
                        sync_group.map(|group| group.color).unwrap_or(palette.link);
                    let accent_color = if is_active_tab {
                        custom_color.unwrap_or(palette.primary)
                    } else if let Some(custom_color) = custom_color {
                        custom_color
                    } else if is_disconnected {
                        palette.danger
                    } else if has_unread {
                        palette.warning
                    } else {
                        palette.text_dimmed
                    };
                    let accent = rgb(accent_color);
                    let bg = if let Some(custom_color) = custom_color {
                        rgba((custom_color << 8) | if is_active_tab { 0x24 } else { 0x14 })
                    } else if is_active_tab {
                        self.shell_surface_color(palette.bg)
                    } else {
                        rgba(0x00000000)
                    };
                    let drag_payload = SessionTabDragPayload {
                        session_id: tab_id.clone(),
                        order_index: tab_number.saturating_sub(1),
                        display_name: title.clone(),
                        kind_label,
                        kind_icon,
                        preview_background: palette.surface,
                        preview_border: palette.border,
                        preview_text: palette.text,
                        preview_text_muted: palette.text_muted,
                        preview_accent: accent_color,
                    };
                    let tab_title = truncate_preview(&title, 18);
                    let hover_card = self.session_tab_hover_card(tab_id, palette);
                    let tab = div()
                        .id(SharedString::from(format!("tw-tab-{leaf_id}-{select_id}")))
                        // HoverCard inserts trigger-host elements, so an
                        // inherited percentage height would shrink to the
                        // tab's text content instead of filling this strip.
                        .h(px(36.))
                        .min_w(px(118.))
                        .max_w(px(236.))
                        .pl_3()
                        .pr_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .relative()
                        .cursor_pointer()
                        .cursor_move()
                        .bg(bg)
                        .when(is_disconnected, |this| this.opacity(0.78))
                        .border_r_1()
                        .border_color(rgb(palette.border))
                        .when(!is_active_tab, |this| this.border_b_1())
                        .when(is_active_tab, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .h(px(2.))
                                    .bg(accent),
                            )
                        })
                        .when(custom_color.is_some(), |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left_0()
                                    .w(px(3.))
                                    .bg(accent),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.activate_terminal_window_tab(
                                leaf_id.clone(),
                                select_id.clone(),
                                cx,
                            );
                            window.focus(this.terminal.input_focus(), cx);
                        }))
                        .on_mouse_down(
                            gpui::MouseButton::Middle,
                            cx.listener({
                                let actions_id = actions_id.clone();
                                move |this, event, window, cx| {
                                    this.handle_session_tab_mouse_down(
                                        actions_id.clone(),
                                        event,
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, event, window, cx| {
                                this.handle_session_tab_mouse_down(
                                    actions_id.clone(),
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_drag(drag_payload, |payload, position, _, cx| {
                            cx.new(|_| SessionTabDragPreview::new(payload.clone(), position))
                        })
                        .on_drop(cx.listener(
                            move |this, payload: &SessionTabDragPayload, _, cx| {
                                this.place_tab_before_in_terminal_windows(
                                    payload.session_id.clone(),
                                    drop_before_id.clone(),
                                    cx,
                                );
                            },
                        ))
                        .child(
                            div()
                                .size(px(14.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(svg().size(px(14.)).path(kind_icon).text_color(accent)),
                        )
                        .when(tab_number > 0, |this| {
                            this.child(
                                div()
                                    .min_w(px(12.))
                                    .text_size(px(12.))
                                    .font_weight(FontWeight(700.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child(format!("{tab_number}")),
                            )
                        })
                        .child(
                            div()
                                .min_w_0()
                                .max_w(px(160.))
                                .text_xs()
                                .font_weight(FontWeight(if is_active_tab { 700. } else { 500. }))
                                .text_color(if is_disconnected {
                                    rgb(palette.text_dimmed)
                                } else if is_active_tab {
                                    rgb(palette.text)
                                } else {
                                    rgb(palette.text_muted)
                                })
                                .child(tab_title),
                        )
                        .when(show_sync_indicator, |this| {
                            this.child(
                                div()
                                    .size(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .opacity(if sync_paused { 0.4 } else { 1. })
                                    .child(
                                        svg()
                                            .size(px(10.))
                                            .path("icons/sync.svg")
                                            .text_color(rgb(sync_indicator_color)),
                                    ),
                            )
                        })
                        .when(has_unread && !is_active_tab, |this| {
                            this.child(div().size(px(7.)).rounded_full().bg(rgb(palette.success)))
                        })
                        .child(
                            div()
                                .id(SharedString::from(format!("tw-tab-close-{id}-{close_id}")))
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .hover(|this| {
                                    this.bg(rgb(palette.border)).text_color(rgb(palette.danger))
                                })
                                .child(
                                    svg()
                                        .size(px(13.))
                                        .path("icons/window/close.svg")
                                        .text_color(rgb(palette.text_muted)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.close_session(close_id.clone(), cx);
                                })),
                        );
                    strip = strip.child(
                        NyaHoverCard::new(
                            format!("terminal-window-tab-hover-card-{id}-{tab_id}"),
                            tab,
                            hover_card,
                        )
                        .anchor(Anchor::TopLeft),
                    );
                }
                let menu_anchor = NewSessionMenuAnchor::TerminalLeaf(id.clone());
                let menu_open = self.shell.new_session_menu_is_open_at(&menu_anchor);
                let menu_has_submenu = self.shell.new_session_all_sessions_is_open();
                let menu_suffix = format!("leaf-{id}");
                let trigger = div()
                    .id(SharedString::from(format!("tw-leaf-add-{id}")))
                    .h_full()
                    .w(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_color(rgb(palette.border))
                    .bg(if menu_open {
                        self.shell_surface_color(palette.hover)
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(rgb(palette.text_muted))
                    .hover(|this| this.bg(rgb(palette.hover)).text_color(rgb(palette.text)))
                    .cursor_pointer()
                    .child(
                        svg()
                            .size(px(16.))
                            .path("icons/conn/add.svg")
                            .text_color(rgb(palette.text_muted)),
                    )
                    .tooltip(|window, cx| {
                        nyaterm_ui::NyaTooltip::new(rust_i18n::t!("terminal.newSession"))
                            .build(window, cx)
                    });
                let popover_anchor = menu_anchor.clone();
                let popover = NyaPopover::new(
                    SharedString::from(format!("tw-leaf-new-session-popover-{id}")),
                    trigger,
                    self.render_new_session_menu(&menu_suffix, cx),
                )
                .placement(NyaPopoverPlacement::Bottom)
                .align(NyaPopoverAlign::End)
                .offset(px(4.))
                .appearance(false)
                .overlay_closable(!menu_has_submenu)
                .open(menu_open)
                .on_open_change(cx.listener(move |this, open, _, cx| {
                    if *open {
                        if let NewSessionMenuAnchor::TerminalLeaf(leaf_id) = &popover_anchor {
                            this.shell.set_focused_terminal_leaf(Some(leaf_id.clone()));
                        }
                        this.open_new_session_menu(popover_anchor.clone(), cx);
                    } else if this.shell.new_session_menu_is_open_at(&popover_anchor) {
                        this.close_new_session_menu(cx);
                    }
                }));
                strip = strip.child(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(popover),
                );
                strip = strip.child(
                    div()
                        .h_full()
                        .flex_1()
                        .border_b_1()
                        .border_color(rgb(palette.border)),
                );
                let canvas = if active.is_empty() {
                    div().flex_1().into_any_element()
                } else {
                    self.workspace_session_content(active.clone(), cx)
                };
                let drop_leaf_id_move = id.clone();
                let drop_leaf_id_drop = id.clone();
                let content = div()
                    .id(SharedString::from(format!("tw-leaf-content-{id}")))
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .relative()
                    .can_drop(|drag, _, _| drag.is::<SessionTabDragPayload>())
                    .on_drag_move(cx.listener(
                        move |this, event: &gpui::DragMoveEvent<SessionTabDragPayload>, _, cx| {
                            let _ = event.drag(cx);
                            let bounds = event.bounds;
                            let pos = event.event.position;
                            let width = f32::from(bounds.size.width).max(1.0);
                            let height = f32::from(bounds.size.height).max(1.0);
                            let local_x = (f32::from(pos.x - bounds.origin.x)).clamp(0.0, width);
                            let local_y = (f32::from(pos.y - bounds.origin.y)).clamp(0.0, height);
                            let zone = TabDockZone::detect(local_x, local_y, width, height);
                            this.set_terminal_window_drop(drop_leaf_id_move.clone(), zone, cx);
                        },
                    ))
                    .on_drop(
                        cx.listener(move |this, payload: &SessionTabDragPayload, _, cx| {
                            let zone = this
                                .terminal
                                .terminal_window_drop_for_leaf(&drop_leaf_id_drop)
                                .unwrap_or(TabDockZone::Center);
                            this.dock_tab_on_terminal_window_leaf(
                                payload.session_id.clone(),
                                drop_leaf_id_drop.clone(),
                                zone,
                                cx,
                            );
                        }),
                    )
                    .child(canvas)
                    .when_some(drop_zone, |this, zone| {
                        this.child(self.tab_dock_drop_overlay(zone, palette))
                    });
                div()
                    .id(SharedString::from(format!("tw-leaf-{id}")))
                    .size_full()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .border_t(if border_edges.top { px(1.) } else { px(0.) })
                    .border_r(if border_edges.right { px(1.) } else { px(0.) })
                    .border_b(if border_edges.bottom { px(1.) } else { px(0.) })
                    .border_l(if border_edges.left { px(1.) } else { px(0.) })
                    .border_color(rgb(palette.border))
                    .overflow_hidden()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener({
                            let leaf_id = id.clone();
                            move |this, _, _, cx| {
                                this.shell.set_focused_terminal_leaf(Some(leaf_id.clone()));
                                cx.notify();
                            }
                        }),
                    )
                    .child(strip)
                    .child(content)
                    .into_any_element()
            }
            TerminalWindowNode::Split {
                id,
                direction,
                ratio_percent,
                first,
                second,
            } => {
                let (first_edges, second_edges) = border_edges.split(direction);
                let first_el =
                    self.render_terminal_window_node(*first, first_edges, render_index, cx);
                let second_el =
                    self.render_terminal_window_node(*second, second_edges, render_index, cx);
                let primary_basis =
                    relative(WorkspacePaneNode::primary_weight(ratio_percent) / 100.);
                let secondary_basis =
                    relative(WorkspacePaneNode::secondary_weight(ratio_percent) / 100.);
                let divider = self.workspace_split_resize_handle(id.clone(), direction, cx);
                match direction {
                    WorkspaceSplitDirection::Horizontal => div()
                        .id(SharedString::from(format!("tw-split-{id}")))
                        .size_full()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex_none()
                                .flex_basis(primary_basis)
                                .min_h(px(80.))
                                .overflow_hidden()
                                .child(first_el),
                        )
                        .child(divider)
                        .child(
                            div()
                                .flex_none()
                                .flex_basis(secondary_basis)
                                .min_h(px(80.))
                                .overflow_hidden()
                                .child(second_el),
                        )
                        .into_any_element(),
                    WorkspaceSplitDirection::Vertical => div()
                        .id(SharedString::from(format!("tw-split-{id}")))
                        .size_full()
                        .min_h_0()
                        .min_w_0()
                        .flex()
                        .child(
                            div()
                                .flex_none()
                                .flex_basis(primary_basis)
                                .min_w(px(120.))
                                .overflow_hidden()
                                .child(first_el),
                        )
                        .child(divider)
                        .child(
                            div()
                                .flex_none()
                                .flex_basis(secondary_basis)
                                .min_w(px(120.))
                                .overflow_hidden()
                                .child(second_el),
                        )
                        .into_any_element(),
                }
            }
        }
    }

    fn tab_dock_drop_overlay(&self, zone: TabDockZone, palette: ThemePalette) -> impl IntoElement {
        let label = match zone {
            TabDockZone::Center => "Merge into window",
            TabDockZone::Edge(TabDockEdge::Left) => "Split left",
            TabDockZone::Edge(TabDockEdge::Right) => "Split right",
            TabDockZone::Edge(TabDockEdge::Top) => "Split top",
            TabDockZone::Edge(TabDockEdge::Bottom) => "Split bottom",
        };
        let accent = rgb(palette.link);
        let mut zone_box = div()
            .absolute()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_2()
            .border_color(accent)
            .bg(rgba((palette.link << 8) | 0x28));
        zone_box = match zone {
            TabDockZone::Center => zone_box.inset_2(),
            TabDockZone::Edge(TabDockEdge::Left) => {
                zone_box.top_2().bottom_2().left_2().w(relative(0.38))
            }
            TabDockZone::Edge(TabDockEdge::Right) => {
                zone_box.top_2().bottom_2().right_2().w(relative(0.38))
            }
            TabDockZone::Edge(TabDockEdge::Top) => {
                zone_box.left_2().right_2().top_2().h(relative(0.38))
            }
            TabDockZone::Edge(TabDockEdge::Bottom) => {
                zone_box.left_2().right_2().bottom_2().h(relative(0.38))
            }
        };
        div().absolute().inset_0().child(
            zone_box.child(
                div()
                    .rounded_sm()
                    .border_1()
                    .border_color(accent)
                    .bg(self.shell_surface_color(palette.surface))
                    .px_3()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.text))
                    .child(label),
            ),
        )
    }
}

fn multi_leaf_session_kind_icon(kind: nyaterm_transport::SessionKind) -> &'static str {
    match kind {
        nyaterm_transport::SessionKind::Ssh => "icons/conn/server.svg",
        nyaterm_transport::SessionKind::Telnet | nyaterm_transport::SessionKind::RawTcp => {
            "icons/conn/telnet.svg"
        }
        nyaterm_transport::SessionKind::Serial => "icons/conn/serial.svg",
        nyaterm_transport::SessionKind::LocalPty => "icons/conn/terminal.svg",
        nyaterm_transport::SessionKind::Rdp => "icons/conn/server.svg",
        nyaterm_transport::SessionKind::Vnc => "icons/conn/server.svg",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use nyaterm_transport::{SessionInfo, SessionKind};

    use super::TerminalWindowRenderIndex;

    struct CountedSessions {
        sessions: std::vec::IntoIter<SessionInfo>,
        visits: Rc<Cell<usize>>,
    }

    impl Iterator for CountedSessions {
        type Item = SessionInfo;

        fn next(&mut self) -> Option<Self::Item> {
            let next = self.sessions.next();
            if next.is_some() {
                self.visits.set(self.visits.get() + 1);
            }
            next
        }
    }

    fn session(id: &str, kind: SessionKind) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            working_dir: None,
            cols: 80,
            rows: 24,
        }
    }

    #[test]
    fn terminal_window_render_index_preserves_number_and_kind_in_one_pass() {
        let visits = Rc::new(Cell::new(0));
        let sessions = CountedSessions {
            sessions: vec![
                session("alpha", SessionKind::Ssh),
                session("beta", SessionKind::LocalPty),
                session("gamma", SessionKind::Serial),
            ]
            .into_iter(),
            visits: visits.clone(),
        };

        let index = TerminalWindowRenderIndex::from_sessions(sessions);

        assert_eq!(index.tab("alpha").map(|meta| meta.number), Some(1));
        assert_eq!(index.tab("beta").map(|meta| meta.number), Some(2));
        assert_eq!(
            index.tab("gamma").map(|meta| meta.kind),
            Some(SessionKind::Serial)
        );
        assert_eq!(index.tab("missing"), None);
        assert_eq!(visits.get(), 3);
    }
}
