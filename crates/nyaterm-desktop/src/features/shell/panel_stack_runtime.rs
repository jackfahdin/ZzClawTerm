use rust_i18n::t;

use std::borrow::Cow;
use std::collections::HashSet;

use gpui::{
    AnyElement, App, ClickEvent, Context, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, deferred, div,
    prelude::FluentBuilder as _, px, rgb, svg,
};
use nyaterm_core::{AgentCommandExecutionMode, truncate_preview};

use crate::features::{
    NyaTermApp, text_inputs::TextInputSetup, view_widgets::panel_header_with_actions,
};
use crate::models::{
    ActivityBarZone, MainMode, NavItem, NetworkTab, PanelOpenMode, PanelSide, RightFocus,
    SecurityAuthTab, SettingsTab,
};
use crate::theme::ThemePalette;
use nyaterm_ui::NyaTooltip;

const EXCLUSIVE_PANEL_IDS: &[&str] = &["aiAssistant"];
const NON_PANEL_IDS: &[&str] = &["settings", "lock", "quickCmdBar", "serialSend"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidePanelStackRenderMode {
    Overlay(NavItem),
    Stack,
}

fn side_panel_stack_render_mode(overlay: Option<NavItem>) -> SidePanelStackRenderMode {
    overlay
        .map(SidePanelStackRenderMode::Overlay)
        .unwrap_or(SidePanelStackRenderMode::Stack)
}

impl NyaTermApp {
    pub(in crate::features) fn is_exclusive_panel_id(id: &str) -> bool {
        EXCLUSIVE_PANEL_IDS.contains(&id)
    }

    pub(in crate::features) fn is_stackable_panel_id(id: &str) -> bool {
        !NON_PANEL_IDS.contains(&id) && !Self::is_exclusive_panel_id(id)
    }

    fn panel_id_visible(&self, id: &str) -> bool {
        let summary = self.settings.summary();
        match NavItem::from_persistence_id(id) {
            Some(NavItem::Notes) => summary.ui_show_notes_panel,
            Some(NavItem::Stats) => summary.ui_show_remote_stats,
            Some(NavItem::GpuMonitor) => summary.ui_show_gpu_monitor,
            Some(NavItem::AscendNpuMonitor) => summary.ui_show_ascend_npu_monitor,
            Some(NavItem::Processes) => summary.ui_show_process_manager,
            Some(NavItem::Docker) => summary.ui_show_docker_manager,
            _ => true,
        }
    }

    pub(in crate::features) fn toggle_panel_multi_open(&mut self, cx: &mut Context<Self>) {
        let next = !self.shell.panels.multi_open;
        self.shell.panels.multi_open = next;
        if !self.shell.panels.open_mode.is_floating() {
            if next {
                if self.shell.panels.left_open.is_empty()
                    && let Some(panel) = self.shell.panels.active_left
                {
                    let id = panel.persistence_id().to_string();
                    if Self::is_stackable_panel_id(&id) {
                        self.shell.panels.left_open.push(id);
                    }
                }
                if self.shell.panels.right_open.is_empty()
                    && let Some(panel) = self.shell.panels.active_right
                {
                    let id = panel.persistence_id().to_string();
                    if Self::is_stackable_panel_id(&id) {
                        self.shell.panels.right_open.push(id);
                    }
                }
            } else {
                if self.shell.panels.active_left.is_none() {
                    self.shell.panels.active_left = self
                        .shell
                        .panels
                        .left_open
                        .first()
                        .and_then(|id| NavItem::from_persistence_id(id));
                }
                if self.shell.panels.active_right.is_none() {
                    self.shell.panels.active_right = self
                        .shell
                        .panels
                        .right_open
                        .first()
                        .and_then(|id| NavItem::from_persistence_id(id));
                }
                self.shell.panels.left_open.clear();
                self.shell.panels.right_open.clear();
            }
        }
        self.shell.set_status(if next {
            "multi-open panels enabled"
        } else {
            "multi-open panels disabled"
        });
        self.persist_ui_layout();
        cx.notify();
    }

    /// Switch between persisted docked and floating presentation. Floating
    /// selections are transient and docked open state is intentionally cleared
    /// on the boundary, matching main's state model.
    pub(in crate::features) fn set_panel_open_mode(
        &mut self,
        mode: PanelOpenMode,
        cx: &mut Context<Self>,
    ) {
        if self.shell.panels.open_mode == mode {
            return;
        }
        self.shell.panels.set_open_mode(mode);
        self.shell.panels.clear_floating();
        if mode.is_floating() {
            self.shell.panels.left_open.clear();
            self.shell.panels.right_open.clear();
            self.shell.panels.active_left = None;
            self.shell.panels.active_right = None;
            self.shell.panels.left_collapsed = true;
            self.shell.panels.right_collapsed = true;
        }
        self.shell.set_status(if mode.is_floating() {
            "floating panels enabled"
        } else {
            "docked panels enabled"
        });
        self.persist_ui_layout();
        cx.notify();
    }

    pub(in crate::features) fn toggle_floating_panel(
        &mut self,
        item: NavItem,
        side: PanelSide,
        cx: &mut Context<Self>,
    ) {
        let next = (self.shell.panels.floating_panel(side) != Some(item)).then_some(item);
        self.shell.panels.set_floating_panel(side, next);
        self.shell.set_status(if next.is_some() {
            format!("{} floating panel opened", item.persistence_id())
        } else {
            format!("{} floating panel closed", item.persistence_id())
        });
        cx.notify();
    }

    pub(in crate::features) fn close_floating_panel(
        &mut self,
        side: PanelSide,
        cx: &mut Context<Self>,
    ) {
        if self.shell.panels.floating_panel(side).is_some() {
            self.shell.panels.set_floating_panel(side, None);
            cx.notify();
        }
    }

    pub(in crate::features) fn close_last_floating_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(side) = self.shell.panels.last_floating_side else {
            return false;
        };
        self.close_floating_panel(side, cx);
        true
    }

    pub(in crate::features) fn side_open_panel_ids(&self, side: PanelSide) -> Vec<String> {
        if !self.shell.panels.multi_open {
            let active = match side {
                PanelSide::Left => self.shell.panels.active_left,
                PanelSide::Right => self.shell.panels.active_right,
            };
            return active
                .map(|item| item.persistence_id().to_string())
                .into_iter()
                .collect();
        }

        let open = match side {
            PanelSide::Left => &self.shell.panels.left_open,
            PanelSide::Right => &self.shell.panels.right_open,
        };
        if open.is_empty() {
            return Vec::new();
        }
        let open_set: HashSet<_> = open.iter().cloned().collect();
        let zones = match side {
            PanelSide::Left => [ActivityBarZone::LeftTop, ActivityBarZone::LeftBottom],
            PanelSide::Right => [ActivityBarZone::RightTop, ActivityBarZone::RightBottom],
        };
        let mut ordered = Vec::new();
        for zone in zones {
            for id in self.shell.chrome.activity_bar_layout.zone(zone) {
                if open_set.contains(id)
                    && Self::is_stackable_panel_id(id)
                    && self.panel_id_visible(id)
                {
                    ordered.push(id.clone());
                }
            }
        }
        ordered
    }

    /// Whether `item`'s panel is on screen right now.
    ///
    /// Mirrors the branches of `side_panel_stack`, which is the thing that decides what
    /// actually gets built: an exclusive overlay takes its whole side; an empty open
    /// list falls back to the current panel and then to the side's first entry; and
    /// otherwise a single-entry list or single-panel mode shows the first id while a
    /// multi-open stack shows all of them.
    ///
    /// Deliberately *not* mirrored: `has_*_activity_items`, the narrow-viewport overlay
    /// modes, and the mobile drawers from `root.rs`. Those can also stop a side being
    /// laid out, but the predicate this replaced consulted only `current_*_panel`, so
    /// including them would be a separate behaviour change rather than a faithful move.
    pub(in crate::features) fn panel_is_rendered(&self, item: NavItem) -> bool {
        let Some(side) = self.panel_side_for_item(item) else {
            return false;
        };
        if self.shell.panels.open_mode.is_floating() {
            return self.shell.panels.floating_panel(side) == Some(item);
        }
        let side_open = match side {
            PanelSide::Left => self.left_side_open(),
            PanelSide::Right => self.right_side_open(),
        };
        if !side_open {
            return false;
        }
        // `side_open_panel_ids` does not check collapse, so the test above is what
        // stops a collapsed side reporting its remembered active panel as visible.
        if let Some(overlay) = self.side_overlay_panel(side) {
            return overlay == item;
        }
        let open_ids = self.side_open_panel_ids(side);
        if open_ids.is_empty() {
            let fallback = match side {
                PanelSide::Left => self.current_left_panel(),
                PanelSide::Right => self.current_right_panel(),
            }
            .or_else(|| {
                self.shell
                    .chrome
                    .activity_bar_layout
                    .first_panel_on_side(side)
            });
            return fallback == Some(item);
        }
        if !self.shell.panels.multi_open || open_ids.len() == 1 {
            return open_ids
                .first()
                .and_then(|id| NavItem::from_persistence_id(id))
                == Some(item);
        }
        open_ids.iter().any(|id| id == item.persistence_id())
    }

    pub(in crate::features) fn side_overlay_panel(&self, side: PanelSide) -> Option<NavItem> {
        if !self.shell.panels.multi_open {
            return None;
        }
        let active = match side {
            PanelSide::Left => self.shell.panels.active_left,
            PanelSide::Right => self.shell.panels.active_right,
        }?;
        let id = active.persistence_id();
        Self::is_exclusive_panel_id(id).then_some(active)
    }

    pub(in crate::features) fn panel_stack_weight(&self, panel_id: &str) -> f32 {
        self.shell.panels.stack_weight(panel_id)
    }

    pub(in crate::features) fn panel_side_for_item(&self, item: NavItem) -> Option<PanelSide> {
        self.shell
            .chrome
            .activity_bar_layout
            .side_for_entry(item.persistence_id())
            .or_else(|| item.is_left_panel().then_some(PanelSide::Left))
            .or_else(|| item.is_right_panel().then_some(PanelSide::Right))
    }

    pub(in crate::features) fn open_or_toggle_panel(
        &mut self,
        item: NavItem,
        cx: &mut Context<Self>,
    ) {
        if item == NavItem::Settings || item.opens_settings() {
            self.open_page(NavItem::Settings, cx);
            return;
        }
        if self.shell.panel_is_floating()
            && let Some(side) = self.panel_side_for_item(item)
        {
            self.toggle_floating_panel(item, side, cx);
            return;
        }
        if !self.shell.panels.multi_open {
            self.open_panel(item, cx);
            return;
        }

        let id = item.persistence_id().to_string();
        let Some(side) = self.panel_side_for_item(item) else {
            self.open_panel(item, cx);
            return;
        };

        self.shell.navigation.main_mode = MainMode::Workspace;
        self.shell.navigation.selected_nav = item;
        if item == NavItem::Recording {
            self.shell.panels.right_focus = RightFocus::Recording;
        } else {
            self.shell.panels.right_focus = RightFocus::Default;
        }

        if Self::is_exclusive_panel_id(&id) {
            let active = match side {
                PanelSide::Left => self.shell.panels.active_left,
                PanelSide::Right => self.shell.panels.active_right,
            };
            if active == Some(item) {
                // Dismiss exclusive overlay to stack.
                let fallback = self
                    .side_open_panel_ids(side)
                    .into_iter()
                    .find_map(|open_id| NavItem::from_persistence_id(&open_id));
                match side {
                    PanelSide::Left => {
                        self.shell.panels.active_left = fallback;
                        self.shell.panels.left_collapsed = fallback.is_none();
                    }
                    PanelSide::Right => {
                        self.shell.panels.active_right = fallback;
                        self.shell.panels.right_collapsed = fallback.is_none();
                    }
                }
                self.shell.set_status(format!("{} closed", item.label()));
            } else {
                match side {
                    PanelSide::Left => {
                        self.shell.panels.active_left = Some(item);
                        self.shell.panels.left_collapsed = false;
                    }
                    PanelSide::Right => {
                        self.shell.panels.active_right = Some(item);
                        self.shell.panels.right_collapsed = false;
                    }
                }
                self.shell.set_status(format!("{} opened", item.label()));
            }
            self.persist_ui_layout();
            cx.notify();
            return;
        }

        let open_list = match side {
            PanelSide::Left => &mut self.shell.panels.left_open,
            PanelSide::Right => &mut self.shell.panels.right_open,
        };
        let is_open = open_list.iter().any(|value| value == &id);
        let active = match side {
            PanelSide::Left => self.shell.panels.active_left,
            PanelSide::Right => self.shell.panels.active_right,
        };

        // If exclusive overlay is showing and stacked panel already open, reveal stack.
        if is_open
            && active
                .map(|item| Self::is_exclusive_panel_id(item.persistence_id()))
                .unwrap_or(false)
        {
            match side {
                PanelSide::Left => {
                    self.shell.panels.active_left = Some(item);
                    self.shell.panels.left_collapsed = false;
                }
                PanelSide::Right => {
                    self.shell.panels.active_right = Some(item);
                    self.shell.panels.right_collapsed = false;
                }
            }
            self.shell.set_status(format!("{} focused", item.label()));
            self.persist_ui_layout();
            cx.notify();
            return;
        }

        if is_open {
            open_list.retain(|value| value != &id);
            let next_active = if open_list.is_empty() {
                None
            } else if active
                .map(|item| item.persistence_id() == id)
                .unwrap_or(false)
            {
                open_list
                    .first()
                    .and_then(|value| NavItem::from_persistence_id(value))
            } else {
                active.filter(|item| open_list.iter().any(|value| value == item.persistence_id()))
            };
            match side {
                PanelSide::Left => {
                    self.shell.panels.active_left = next_active;
                    self.shell.panels.left_collapsed =
                        next_active.is_none() && self.shell.panels.left_open.is_empty();
                }
                PanelSide::Right => {
                    self.shell.panels.active_right = next_active;
                    self.shell.panels.right_collapsed =
                        next_active.is_none() && self.shell.panels.right_open.is_empty();
                }
            }
            self.shell.set_status(format!("{} closed", item.label()));
        } else {
            open_list.push(id);
            match side {
                PanelSide::Left => {
                    self.shell.panels.active_left = Some(item);
                    self.shell.panels.left_collapsed = false;
                }
                PanelSide::Right => {
                    self.shell.panels.active_right = Some(item);
                    self.shell.panels.right_collapsed = false;
                }
            }
            self.shell.set_status(format!("{} opened", item.label()));
        }
        self.persist_ui_layout();
        cx.notify();
        // Revealing or hiding the browser changes the panel's cwd-poll demand.
        self.flush_transfer_panel_snapshot(cx);
    }

    pub(in crate::features) fn ensure_panel_in_stack(&mut self, item: NavItem) {
        self.shell.navigation.main_mode = MainMode::Workspace;
        self.shell.navigation.selected_nav = item;
        if !self.shell.panels.multi_open {
            self.ensure_panel_open(item);
            return;
        }
        let id = item.persistence_id().to_string();
        match self.panel_side_for_item(item) {
            Some(PanelSide::Left) => {
                self.shell.panels.left_collapsed = false;
                if Self::is_exclusive_panel_id(&id) {
                    self.shell.panels.active_left = Some(item);
                } else {
                    if !self.shell.panels.left_open.iter().any(|value| value == &id) {
                        self.shell.panels.left_open.push(id);
                    }
                    self.shell.panels.active_left = Some(item);
                }
            }
            Some(PanelSide::Right) => {
                self.shell.panels.right_collapsed = false;
                self.shell.panels.right_focus = if item == NavItem::Recording {
                    RightFocus::Recording
                } else {
                    RightFocus::Default
                };
                if Self::is_exclusive_panel_id(&id) {
                    self.shell.panels.active_right = Some(item);
                } else {
                    if !self
                        .shell
                        .panels
                        .right_open
                        .iter()
                        .any(|value| value == &id)
                    {
                        self.shell.panels.right_open.push(id);
                    }
                    self.shell.panels.active_right = Some(item);
                }
            }
            None => {}
        }
    }

    pub(in crate::features) fn start_panel_stack_resize(
        &mut self,
        side: PanelSide,
        above_id: String,
        below_id: String,
        event: &MouseDownEvent,
        container_height: f32,
        cx: &mut Context<Self>,
    ) {
        self.shell.panels.start_stack_resize(
            side,
            above_id,
            below_id,
            event.position.y,
            container_height,
        );
        self.shell.set_status("resizing panel stack".to_string());
        cx.notify();
    }

    pub(in crate::features) fn update_panel_stack_resize(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.shell.panels.update_stack_resize(event.position.y) {
            cx.notify();
        }
    }

    pub(in crate::features) fn finish_panel_stack_resize(&mut self, cx: &mut Context<Self>) {
        if self.shell.panels.finish_stack_resize() {
            self.persist_ui_layout();
            self.shell.set_status("panel stack sizes saved".to_string());
            cx.notify();
        }
    }

    pub(in crate::features) fn apply_panel_stack_from_settings(&mut self) {
        let mode =
            crate::models::PanelOpenMode::from_setting(&self.settings.summary().ui_panel_open_mode);
        self.shell.panels.set_open_mode(mode);
        self.shell.panels.multi_open = self.settings.summary().ui_panel_multi_open;
        self.shell.panels.clear_floating();
        self.shell.panels.left_open = self.settings.summary().ui_left_open_panels.clone();
        self.shell.panels.right_open = self.settings.summary().ui_right_open_panels.clone();
        self.shell.panels.stack_sizes = self
            .settings
            .summary()
            .ui_panel_stack_sizes
            .iter()
            .filter(|(_, value)| **value > 0)
            .map(|(key, value)| (key.clone(), (*value as f32) / 1000.))
            .collect();
        if mode.is_floating() {
            self.shell.panels.left_open.clear();
            self.shell.panels.right_open.clear();
            self.shell.panels.active_left = None;
            self.shell.panels.active_right = None;
            self.shell.panels.left_collapsed = true;
            self.shell.panels.right_collapsed = true;
        } else if self.shell.panels.multi_open {
            if self.shell.panels.left_open.is_empty()
                && let Some(panel) = self.shell.panels.active_left
            {
                let id = panel.persistence_id().to_string();
                if Self::is_stackable_panel_id(&id) {
                    self.shell.panels.left_open.push(id);
                }
            }
            if self.shell.panels.right_open.is_empty()
                && let Some(panel) = self.shell.panels.active_right
            {
                let id = panel.persistence_id().to_string();
                if Self::is_stackable_panel_id(&id) {
                    self.shell.panels.right_open.push(id);
                }
            }
        }
    }

    pub(in crate::features) fn floating_side_panel(
        &mut self,
        side: PanelSide,
        panel: NavItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.single_side_panel(side, panel, window, cx)
    }

    pub(in crate::features) fn side_panel_stack(
        &mut self,
        side: PanelSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use gpui::relative;

        if let SidePanelStackRenderMode::Overlay(overlay) =
            side_panel_stack_render_mode(self.side_overlay_panel(side))
        {
            return div()
                .relative()
                .size_full()
                .overflow_hidden()
                .child(self.single_side_panel(side, overlay, window, cx));
        }

        let open_ids = self.side_open_panel_ids(side);
        if open_ids.is_empty() {
            let fallback = match side {
                PanelSide::Left => self.current_left_panel(),
                PanelSide::Right => self.current_right_panel(),
            }
            .or_else(|| {
                self.shell
                    .chrome
                    .activity_bar_layout
                    .first_panel_on_side(side)
            })
            .unwrap_or(NavItem::Workspace);
            self.single_side_panel(side, fallback, window, cx)
        } else if open_ids.len() == 1 || !self.shell.panels.multi_open {
            let panel = open_ids
                .first()
                .and_then(|id| NavItem::from_persistence_id(id))
                .or_else(|| {
                    self.shell
                        .chrome
                        .activity_bar_layout
                        .first_panel_on_side(side)
                })
                .unwrap_or(NavItem::Workspace);
            self.single_side_panel(side, panel, window, cx)
        } else {
            let weights: Vec<f32> = open_ids
                .iter()
                .map(|id| self.panel_stack_weight(id))
                .collect();
            let total: f32 = weights.iter().sum::<f32>().max(0.001);
            let count = open_ids.len();
            let mut stack = div().size_full().flex().flex_col().min_h_0();
            for (index, panel_id) in open_ids.iter().enumerate() {
                let panel = NavItem::from_persistence_id(panel_id).unwrap_or(NavItem::Transfers);
                let basis = weights[index] / total;
                let title = panel
                    .i18n_key()
                    .map(|key| t!(key))
                    .unwrap_or_else(|| Cow::Borrowed(panel.panel_title()));
                let actions = self.side_panel_header_actions(panel, cx);
                let palette = self.theme_palette();
                let (meta, body) = self.side_panel_content(side, panel, window, cx);
                stack = stack.child(
                    div()
                        .flex_shrink(1.)
                        .flex_basis(relative(basis))
                        .min_h(px(48.))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(panel_header_with_actions(
                            title,
                            meta,
                            palette,
                            self.shell_transparent_color(palette.section_header),
                            actions,
                        ))
                        .child(div().flex_1().min_h_0().overflow_hidden().child(body)),
                );
                if index + 1 < count {
                    let above = panel_id.clone();
                    let below = open_ids[index + 1].clone();
                    stack = stack.child(self.panel_stack_resize_handle(side, above, below, cx));
                }
            }
            stack
        }
    }

    fn single_side_panel(
        &mut self,
        side: PanelSide,
        panel: NavItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let title = panel
            .i18n_key()
            .map(|key| t!(key))
            .unwrap_or_else(|| Cow::Borrowed(panel.panel_title()));
        let actions = self.side_panel_header_actions(panel, cx);
        let palette = self.theme_palette();
        let (meta, body) = self.side_panel_content(side, panel, window, cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(panel_header_with_actions(
                title,
                meta,
                palette,
                self.shell_transparent_color(palette.section_header),
                actions,
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(body))
    }

    fn side_panel_content(
        &mut self,
        side: PanelSide,
        panel: NavItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement) {
        match panel {
            NavItem::ActiveSessions => {
                let model = self.active_sessions_panel_model();
                let meta = SharedString::from(model.count_label());
                let body = self.active_sessions_panel(model, cx).into_any_element();
                (meta, body)
            }
            NavItem::Recording => {
                let model = self.recording_sessions_panel_model();
                let meta = SharedString::from(model.count_label());
                let body = self.recording_panel(model, cx).into_any_element();
                (meta, body)
            }
            _ => {
                let meta = self.side_panel_meta(side, panel);
                let body = match side {
                    PanelSide::Left => self.left_panel_body(panel, window, cx),
                    PanelSide::Right => self.right_panel_body(panel, window, cx),
                };
                (meta, body)
            }
        }
    }

    /// Tauri PanelHeader meta/actions: Connections shows total count; AI shows model name.
    fn side_panel_meta(&self, _side: PanelSide, panel: NavItem) -> SharedString {
        match panel {
            NavItem::Connections => SharedString::from(""),
            NavItem::AiAssistant => SharedString::from(self.ai_header_presentation().model_label),
            NavItem::ActiveSessions => SharedString::from(""),
            // Tauri NetworkPanel header shows active tab profile count.
            NavItem::Tunnels => {
                let count = match self.connection_state.network_active_tab() {
                    NetworkTab::Tunnels => self.tunnel_state.tunnels().len(),
                    NetworkTab::Proxies => self.tunnel_state.proxies().len(),
                };
                SharedString::from(count.to_string())
            }
            NavItem::Transfers => {
                if self.transfer.browser_view().entries.is_empty() {
                    SharedString::from("")
                } else {
                    SharedString::from(self.transfer.browser_view().entries.len().to_string())
                }
            }
            NavItem::Processes => self
                .session
                .active_ssh_config()
                .and_then(|_| self.remote_ops.loaded_process_count())
                .map(|count| SharedString::from(count.to_string()))
                .unwrap_or_else(|| SharedString::from("")),
            NavItem::GpuMonitor => self
                .remote_ops
                .gpu_presentation()
                .data
                .filter(|overview| overview.available)
                .map(|overview| {
                    SharedString::from(format!(
                        "{} {} · {} {}",
                        t!("gpuMonitor.driver"),
                        panel_meta_version_or_dash(&overview.driver_version),
                        t!("gpuMonitor.cuda"),
                        panel_meta_version_or_dash(&overview.cuda_version)
                    ))
                })
                .unwrap_or_else(|| SharedString::from("")),
            NavItem::AscendNpuMonitor => self
                .remote_ops
                .npu_presentation()
                .data
                .filter(|overview| overview.available)
                .map(|overview| {
                    SharedString::from(format!(
                        "{} {} · {} {}",
                        t!("ascendNpuMonitor.driver"),
                        panel_meta_version_or_dash(&overview.driver_version),
                        t!("ascendNpuMonitor.cann"),
                        panel_meta_version_or_dash(&overview.cann_version)
                    ))
                })
                .unwrap_or_else(|| SharedString::from("")),
            NavItem::Docker => {
                if self.session.active_ssh_config().is_none() {
                    return SharedString::from("");
                }
                let Some(version) = self.remote_ops.docker_engine_version() else {
                    return SharedString::from("");
                };
                SharedString::from(format!("Engine {}", truncate_preview(&version, 24)))
            }
            // Tauri SecurityAuthPanel header actions show active-tab count.
            NavItem::SecurityAuth => {
                let count = match self.security.auth_tab() {
                    SecurityAuthTab::Keys => self.security.ssh_keys().len(),
                    SecurityAuthTab::Passwords => self.security.passwords().len(),
                    SecurityAuthTab::Credentials => self.security.credentials().len(),
                    SecurityAuthTab::Otp => self.security.otp_entries().len(),
                };
                SharedString::from(count.to_string())
            }
            NavItem::Recording => SharedString::from(""),
            NavItem::SyncBackupHistory => SharedString::from(""),
            _ => SharedString::from(""),
        }
    }

    fn side_panel_header_actions(
        &mut self,
        panel: NavItem,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match panel {
            NavItem::Connections if !self.connection_state.connections().is_empty() => Some(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(self.theme_palette().text_dimmed))
                    .child(self.connection_state.connections().len().to_string())
                    .into_any_element(),
            ),
            NavItem::AiAssistant => {
                let palette = self.theme_palette();
                let ai_running = self.ai.chat_or_agent_is_running();
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        // Header triggers are part of their open popover. Keep
                        // their mouse-down away from the root outside-click
                        // handler so a second click can toggle the menu closed.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(header_svg_icon_button(
                            palette,
                            "ai-header-execution-mode-toggle",
                            match self.ai.settings_config().agent_command_execution_mode {
                                AgentCommandExecutionMode::Auto => "icons/ai/exec-auto.svg",
                                AgentCommandExecutionMode::Smart => "icons/ai/exec-smart.svg",
                                AgentCommandExecutionMode::ConfirmEach => {
                                    "icons/ai/exec-confirm.svg"
                                }
                            },
                            t!("ai.agentCommandExecutionMode"),
                            !ai_running,
                            cx.listener(|this, _, _, cx| {
                                this.ai.toggle_execution_menu();
                                this.defer_ai_panel_snapshot_flush(cx);
                            }),
                        ))
                        .child(header_svg_icon_button(
                            palette,
                            "ai-header-history-toggle",
                            "icons/ai/history.svg",
                            t!("ai.history"),
                            true,
                            cx.listener(|this, _, window, cx| {
                                if this.ai.toggle_history() {
                                    this.refresh_ai_session_list(cx);
                                    let query = this.ai.history_query().to_string();
                                    this.reset_text_input("ai.history-search", &query, cx);
                                    let field = this.text_input(
                                        "ai.history-search",
                                        &query,
                                        TextInputSetup::placeholder("Search history..."),
                                        cx,
                                    );
                                    window.focus(&field.read(cx).focus_handle(), cx);
                                } else {
                                    this.forget_text_inputs("ai.history-search");
                                }
                                this.defer_ai_panel_snapshot_flush(cx);
                            }),
                        ))
                        .child(header_svg_icon_button(
                            palette,
                            "ai-header-open-settings",
                            "icons/ai/settings.svg",
                            t!("ai.settings"),
                            true,
                            cx.listener(|this, _, _, cx| {
                                this.ai.close_transient_menus();
                                this.shell.navigation.settings.active_tab = SettingsTab::AiGeneral;
                                this.open_page(NavItem::Settings, cx);
                            }),
                        ))
                        .child(header_svg_icon_button(
                            palette,
                            "ai-header-new-chat",
                            "icons/ai/new.svg",
                            t!("ai.newChat"),
                            !ai_running,
                            cx.listener(|this, _, _, cx| {
                                this.start_new_ai_chat(cx);
                            }),
                        ))
                        .into_any_element(),
                )
            }
            NavItem::Stats => {
                let palette = self.theme_palette();
                let manual_refreshing = self.remote_ops.stats_manual_refreshing();
                let can_refresh = self.session.active_ssh_config().is_some()
                    && !self.remote_ops.stats_is_pending();
                Some(
                    header_svg_icon_button(
                        palette,
                        "stats-header-refresh",
                        if manual_refreshing {
                            "icons/conn/spinner-arc.svg"
                        } else {
                            "icons/fe/refresh.svg"
                        },
                        t!("resourceMonitor.refresh"),
                        can_refresh,
                        cx.listener(|this, _, _window, cx| {
                            this.refresh_stats(cx);
                            this.defer_remote_panel_snapshot_flush(cx);
                        }),
                    )
                    .into_any_element(),
                )
            }
            NavItem::GpuMonitor => {
                let palette = self.theme_palette();
                let can_refresh =
                    self.session.active_ssh_config().is_some() && !self.remote_ops.gpu_is_pending();
                Some(
                    header_svg_icon_button(
                        palette,
                        "gpu-header-refresh",
                        "icons/fe/refresh.svg",
                        t!("resourceMonitor.refresh"),
                        can_refresh,
                        cx.listener(|this, _, _window, cx| {
                            this.refresh_gpu(cx);
                            this.defer_remote_panel_snapshot_flush(cx);
                        }),
                    )
                    .into_any_element(),
                )
            }
            NavItem::AscendNpuMonitor => {
                let palette = self.theme_palette();
                let can_refresh =
                    self.session.active_ssh_config().is_some() && !self.remote_ops.npu_is_pending();
                Some(
                    header_svg_icon_button(
                        palette,
                        "npu-header-refresh",
                        "icons/fe/refresh.svg",
                        t!("resourceMonitor.refresh"),
                        can_refresh,
                        cx.listener(|this, _, _window, cx| {
                            this.refresh_npu(cx);
                            this.defer_remote_panel_snapshot_flush(cx);
                        }),
                    )
                    .into_any_element(),
                )
            }
            NavItem::Processes => {
                let palette = self.theme_palette();
                let can_refresh = self.session.active_ssh_config().is_some()
                    && !self.remote_ops.process_is_pending();
                Some(
                    header_svg_icon_button(
                        palette,
                        "process-header-refresh",
                        "icons/fe/refresh.svg",
                        t!("common.refresh"),
                        can_refresh,
                        cx.listener(|this, _, _window, cx| {
                            this.refresh_processes(cx);
                            this.defer_remote_panel_snapshot_flush(cx);
                        }),
                    )
                    .into_any_element(),
                )
            }
            NavItem::Docker => {
                let palette = self.theme_palette();
                let can_refresh = self.session.active_ssh_config().is_some()
                    && !self.remote_ops.docker_is_pending();
                let can_prune = can_refresh && self.remote_ops.docker_can_prune();
                let more_label = t!("dockerManager.moreActions").to_string();
                let prune_label = t!("dockerManager.prune");
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(header_svg_icon_button(
                            palette,
                            "docker-header-refresh",
                            "icons/fe/refresh.svg",
                            t!("common.refresh"),
                            can_refresh,
                            cx.listener(|this, _, _window, cx| {
                                this.refresh_docker(cx);
                                this.defer_remote_panel_snapshot_flush(cx);
                            }),
                        ))
                        .child(
                            div()
                                .relative()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(header_svg_icon_button(
                                    palette,
                                    "docker-header-more",
                                    "icons/session/more.svg",
                                    more_label,
                                    can_prune,
                                    cx.listener(|this, _, _, cx| {
                                        this.remote_ops.toggle_docker_header_menu();
                                        this.defer_remote_panel_snapshot_flush(cx);
                                        cx.notify();
                                    }),
                                ))
                                .when(self.remote_ops.docker_header_menu_open(), |this| {
                                    this.child(
                                        div()
                                            .id("docker-header-more-menu")
                                            .absolute()
                                            .top(px(30.))
                                            .right_0()
                                            .w(px(160.))
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(palette.border))
                                            .bg(self.shell_surface_color(palette.surface))
                                            .shadow_lg()
                                            .py_1()
                                            .child(
                                                div()
                                                    .id("docker-header-prune")
                                                    .h(px(30.))
                                                    .px_3()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(palette.danger))
                                                    .cursor_pointer()
                                                    .hover(|this| this.bg(rgb(palette.hover)))
                                                    .child(
                                                        svg()
                                                            .size(px(14.))
                                                            .flex_none()
                                                            .path("icons/fe/delete.svg")
                                                            .text_color(rgb(palette.danger)),
                                                    )
                                                    .child(prune_label)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.remote_ops.close_docker_menus();
                                                            this.prune_docker_system(window, cx);
                                                            this.defer_remote_panel_snapshot_flush(
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            ),
                                    )
                                }),
                        )
                        .into_any_element(),
                )
            }
            NavItem::SyncBackupHistory => {
                let palette = self.theme_palette();
                Some(
                    header_svg_icon_button(
                        palette,
                        "sync-history-header-refresh",
                        "icons/fe/refresh.svg",
                        t!("resourceMonitor.refresh"),
                        true,
                        cx.listener(|this, _, _, cx| {
                            this.queue_cloud_sync_history_refresh(None, cx);
                            this.shell
                                .set_status("refreshing cloud sync history".to_string());
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
                )
            }
            _ => None,
        }
    }

    fn panel_stack_resize_handle(
        &self,
        side: PanelSide,
        above_id: String,
        below_id: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let above = above_id.clone();
        let below = below_id.clone();
        let id = SharedString::from(format!(
            "panel-stack-resize-{}-{}-{}",
            match side {
                PanelSide::Left => "left",
                PanelSide::Right => "right",
            },
            above_id,
            below_id
        ));
        let hover_id = id.clone();
        let drag_id = id.clone();
        deferred(
            crate::features::view_widgets::horizontal_resize_handle_visual(
                palette,
                self.shell
                    .panels
                    .stack_resize
                    .as_ref()
                    .is_some_and(|resize| {
                        resize.side == side
                            && resize.above_id == above_id
                            && resize.below_id == below_id
                    }),
                self.shell.resize_handle_is_highlighted(&id),
            )
            .id(id.clone())
            .cursor_row_resize()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.update_resize_handle_hover(hover_id.clone(), *hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.activate_resize_handle_immediately(drag_id.clone(), cx);
                    let open = this.side_open_panel_ids(side);
                    let total_weight: f32 = open.iter().map(|id| this.panel_stack_weight(id)).sum();
                    let container_height = 480.0_f32.max(total_weight * 120.);
                    this.start_panel_stack_resize(
                        side,
                        above.clone(),
                        below.clone(),
                        event,
                        container_height,
                        cx,
                    );
                }),
            ),
        )
    }
}

fn panel_meta_version_or_dash(value: &str) -> String {
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        truncate_preview(value, 24)
    }
}

fn header_svg_icon_button(
    palette: ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: impl Into<String>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tooltip = tooltip.into();
    div()
        .id(SharedString::from(id.into()))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(rgb(if enabled {
            palette.text_muted
        } else {
            palette.text_dimmed
        }))
        .when(enabled, |this| {
            this.cursor_pointer().hover(|this| {
                this.bg(rgb(palette.surface_elevated))
                    .text_color(rgb(palette.text))
            })
        })
        .when(!enabled, |this| this.opacity(0.45))
        .tooltip(move |window, cx| NyaTooltip::new(tooltip.clone()).build(window, cx))
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon_path)
                .text_color(rgb(if enabled {
                    palette.text_muted
                } else {
                    palette.text_dimmed
                })),
        )
        .on_click(move |event, window, cx| {
            if enabled {
                on_click(event, window, cx);
            }
        })
}

#[cfg(test)]
mod tests {
    use crate::models::NavItem;

    use super::{SidePanelStackRenderMode, side_panel_stack_render_mode};

    #[test]
    fn exclusive_panel_overlay_takes_precedence_over_stack_construction() {
        assert_eq!(
            side_panel_stack_render_mode(Some(NavItem::AiAssistant)),
            SidePanelStackRenderMode::Overlay(NavItem::AiAssistant)
        );
        assert_eq!(
            side_panel_stack_render_mode(None),
            SidePanelStackRenderMode::Stack
        );
    }
}
