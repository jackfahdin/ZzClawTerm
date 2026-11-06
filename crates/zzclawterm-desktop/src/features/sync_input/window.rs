use std::rc::Rc;

use gpui::{
    App, AppContext as _, Context, Entity, IntoElement, KeyDownEvent, Render, Subscription, Window,
    div, prelude::*, px, rgb,
};
use rust_i18n::t;
use zzclawterm_ui::{
    ZzClawDialogWindowExt as _, ZzClawWindowHandle, activate_child_window, zzclaw_root,
};

use crate::features::ZzClawTermApp;
use crate::features::view_widgets::{
    ChildWindowChrome, ChildWindowCloseHandler, ChildWindowSpec, child_window_header,
    child_window_options, child_window_root, focus_child_window_shell_if_idle,
};

struct SyncGroupsWindow {
    app: Entity<ZzClawTermApp>,
    chrome: ChildWindowChrome,
    _app_subscription: Subscription,
}

impl SyncGroupsWindow {
    fn new(app: Entity<ZzClawTermApp>, chrome: ChildWindowChrome, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            chrome,
            _app_subscription: subscription,
        }
    }
}

impl Render for SyncGroupsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.app.read(cx).sync_input.window_is_open_or_pending() {
            window.defer(cx, |window, _| window.remove_window());
            return div().into_any_element();
        }
        let (palette, font, font_size, focus) = self.app.read_with(cx, |app, _| {
            (
                app.theme_palette(),
                app.gpui_ui_font().font(),
                app.settings.summary().ui_font_size.clamp(12, 24) as f32,
                app.sync_input.focus().clone(),
            )
        });
        let viewport_width = f32::from(window.viewport_size().width);
        let content = self.app.update(cx, |app, cx| {
            app.sync_groups_window_view(viewport_width, cx)
                .into_any_element()
        });
        let close_app = self.app.clone();
        let on_close: ChildWindowCloseHandler = Rc::new(move |window, cx| {
            close_sync_groups_window(&close_app, window, cx);
            window.remove_window();
        });
        let header_close = on_close.clone();
        let keyboard_app = self.app.clone();
        focus_child_window_shell_if_idle(&focus, window, cx);
        window.set_window_title(&t!("syncGroup.title"));

        child_window_root(&focus, true, on_close)
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .font(font)
            .text_size(px(font_size))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if window.has_active_nya_dialog(cx) {
                    return;
                }
                keyboard_app.update(cx, |app, cx| {
                    if !app.sync_input.focus().is_focused(window) {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "n" | "N" => app.create_sync_group(cx),
                        "delete" => app.request_delete_selected_sync_group(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                });
            })
            .child(child_window_header(
                palette,
                t!("syncGroup.title").to_string(),
                None,
                self.chrome,
                window,
                cx,
                move |_, window, cx| header_close(window, cx),
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

impl ZzClawTermApp {
    pub(in crate::features) fn open_sync_groups(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = self.sync_input.window.handle() {
            activate_child_window(
                &cx.entity(),
                handle,
                |app: &mut ZzClawTermApp| Some(&mut app.sync_input.window),
                cx,
            );
            return;
        }
        if !self.sync_input.request_open() {
            return;
        }
        self.sync_input.restore_focus = window.focused(cx);
        self.forget_text_inputs("sync.groups.search");
        self.forget_text_inputs("sync.group-name.");
        cx.notify();
        let app = cx.entity();
        cx.defer(move |cx| {
            if app.read(cx).sync_input.window.is_pending() {
                // Menu items restore the workspace focus after their handler returns.
                // Capture that target before the modal disables the parent window.
                if let Some(parent) = app.read(cx).shell.main_window() {
                    let _ = parent.update(cx, |_, window, cx| {
                        let focus = window.focused(cx);
                        app.update(cx, |app, _| app.sync_input.restore_focus = focus);
                    });
                }
                open_sync_groups_window(app, cx);
            }
        });
    }
}

fn close_sync_groups_window(app: &Entity<ZzClawTermApp>, window: &Window, cx: &mut App) {
    // A delayed close callback must never clear a replacement window's slot.
    let id = window.window_handle().window_id();
    app.update(cx, |app, cx| {
        if app
            .sync_input
            .window
            .handle()
            .is_some_and(|handle| handle.window_id() == id)
        {
            app.close_sync_groups(cx);
        }
    });
}

fn open_sync_groups_window(app: Entity<ZzClawTermApp>, cx: &mut App) {
    let spec = ChildWindowSpec::modal_editor(t!("syncGroup.title").to_string(), 960., 640.)
        .min_size(760., 480.);
    let chrome = spec.chrome();
    let options = child_window_options(&spec, app.read(cx).shell.main_window(), cx);
    let close_app = app.clone();
    let view_app = app.clone();
    let result: anyhow::Result<ZzClawWindowHandle> = cx.open_window(options, move |window, cx| {
        window.on_window_should_close(cx, move |window, cx| {
            close_sync_groups_window(&close_app, window, cx);
            true
        });
        let focus = view_app.read(cx).sync_input.focus().clone();
        window.focus(&focus, cx);
        let view = cx.new(|cx| SyncGroupsWindow::new(view_app, chrome, cx));
        cx.new(|cx| zzclaw_root(view, window, cx))
    });
    app.update(cx, |app, cx| {
        match result {
            Ok(handle) => {
                app.sync_input.window.finish_open(handle);
                app.shell.set_status("sync groups opened".to_string());
            }
            Err(error) => {
                app.close_sync_groups(cx);
                app.shell
                    .set_status(format!("failed to open sync groups window: {error}"));
            }
        }
        cx.notify();
    });
}

#[cfg(test)]
mod tests;
