use gpui::{Context, IntoElement, KeyDownEvent, MouseButton, div, prelude::*, px, rgb, rgba};
use rust_i18n::t;
use zzclawterm_ui::ZzClawScrollable;

use crate::features::ZzClawTermApp;
use crate::features::shell::tab_mouse::TabActionTarget;
use crate::features::view_widgets::tab_menu_item_enabled;

use super::{TAB_ACTIONS_MENU_WIDTH, clamp_tab_actions_position};

impl ZzClawTermApp {
    pub(super) fn start_tab_actions_menu(
        &mut self,
        target: TabActionTarget,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let snapshot = self.start_tab_action_snapshot(&target);
        if snapshot.is_none()
            || snapshot.as_ref() != self.session.dialog_tab_actions_start_snapshot()
        {
            self.close_tab_actions(cx);
            return div().into_any_element();
        }
        let palette = self.theme_palette();
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let height = 232_f32.min((viewport_h - 16.).max(0.));
        let (x, y) = self
            .session
            .dialog_tab_actions_anchor()
            .unwrap_or((8., 74.));
        let (x, y) = clamp_tab_actions_position(
            x,
            y,
            TAB_ACTIONS_MENU_WIDTH,
            height,
            viewport_w,
            viewport_h,
        );
        let mut items = div()
            .max_h(px(height))
            .overflow_y_scrollbar()
            .p_1()
            .flex()
            .flex_col();
        for (action, id, label) in [
            ("rename_tab", "tab-ctx-rename", t!("tabCtx.rename")),
            ("copy_tab_name", "tab-ctx-copy-name", t!("tabCtx.copyName")),
            ("copy_server_ip", "tab-ctx-copy-ip", t!("tabCtx.copyIp")),
            (
                "duplicate_session",
                "tab-ctx-duplicate",
                t!("tabCtx.duplicate"),
            ),
            (
                "multiplex_ssh",
                "tab-ctx-multiplex",
                t!("tabCtx.multiplexSsh"),
            ),
            (
                "reconnect_session",
                "tab-ctx-reconnect",
                t!("tabCtx.reconnect"),
            ),
            (
                "disconnect_session",
                "tab-ctx-disconnect",
                t!("tabCtx.disconnect"),
            ),
            ("close_tab", "tab-ctx-close", t!("tabCtx.close")),
        ] {
            let enabled = self.tab_target_action_enabled(&target, action);
            let target = target.clone();
            let snapshot = snapshot.clone();
            items = items.child(div().debug_selector(move || id.to_string()).child(
                tab_menu_item_enabled(
                    palette,
                    id,
                    label,
                    enabled,
                    cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        let current = app.start_tab_action_snapshot(&target);
                        app.close_tab_actions(cx);
                        if current.is_some() && current == snapshot {
                            app.run_tab_target_action(target.clone(), action, window, cx);
                        }
                    }),
                ),
            ));
        }
        div()
            .id("start-tab-actions-overlay")
            .debug_selector(|| "start-tab-actions-overlay".into())
            .absolute()
            .inset_0()
            .bg(rgba(0))
            .track_focus(self.session.dialog_tab_actions_focus())
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.close_tab_actions(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.close_tab_actions(cx);
                }),
            )
            .on_click(cx.listener(|app, _, _, cx| app.close_tab_actions(cx)))
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
                cx.stop_propagation();
                if event.keystroke.key == "escape" {
                    app.close_tab_actions(cx);
                }
            }))
            .child(
                div()
                    .id("start-tab-actions-menu")
                    .debug_selector(|| "start-tab-actions-menu".into())
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(TAB_ACTIONS_MENU_WIDTH))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.surface))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(items),
            )
            .into_any_element()
    }
}
