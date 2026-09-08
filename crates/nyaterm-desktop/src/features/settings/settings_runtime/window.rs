use rust_i18n::t;

use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, IntoElement, Render, Subscription, Window, div,
    prelude::*, px, rgb,
};
use nyaterm_ui::{NyaWindowHandle, activate_child_window, nya_root};

use crate::features::pages::settings::panel::{SettingsPanel, SettingsSurface};
use crate::features::{
    NyaTermApp,
    view_widgets::{
        ChildWindowChrome, ChildWindowCloseHandler, ChildWindowSpec, child_window_header,
        child_window_options, child_window_root, focus_child_window_shell_if_idle,
    },
};

pub(in crate::features) struct SettingsWindow {
    app: Entity<NyaTermApp>,
    settings_panel: Entity<SettingsPanel>,
    shell_focus: FocusHandle,
    chrome: ChildWindowChrome,
    _app_subscription: Subscription,
}

impl SettingsWindow {
    fn new(app: Entity<NyaTermApp>, chrome: ChildWindowChrome, cx: &mut Context<Self>) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        let settings_panel = cx.new(|cx| {
            SettingsPanel::new_for_surface(app.downgrade(), SettingsSurface::NativeWindow, cx)
        });
        app.update(cx, |app, cx| {
            app.register_native_settings_panel(&settings_panel, cx);
        });
        Self {
            app,
            settings_panel,
            shell_focus: cx.focus_handle(),
            chrome,
            _app_subscription: app_subscription,
        }
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.app.read(cx).shell.has_settings_draft() {
            self.app.update(cx, |app, cx| {
                app.clear_native_settings_panel(&self.settings_panel, cx);
                app.shell.clear_settings_window();
                cx.notify();
            });
            window.defer(cx, |window, _| window.remove_window());
            return div().size_full().into_any_element();
        }

        let (palette, font, font_size, title) = self.app.read_with(cx, |app, _| {
            (
                app.theme_palette(),
                app.gpui_ui_font().font(),
                app.settings.summary().ui_font_size.clamp(12, 24) as f32,
                t!("settings.title").to_string(),
            )
        });
        window.set_window_title(&title);
        let content = self
            .settings_panel
            .clone()
            .cached(crate::features::layout::cached_panel_style());
        let close_app = self.app.clone();
        let close_panel = self.settings_panel.clone();
        let on_close: ChildWindowCloseHandler =
            Rc::new(move |window: &mut Window, cx: &mut App| {
                let panel = close_panel.clone();
                let may_close = close_app.update(cx, |app, cx| {
                    app.request_settings_window_close(&panel, window, cx)
                });
                if may_close {
                    window.remove_window();
                }
            });
        let header_close = on_close.clone();
        focus_child_window_shell_if_idle(&self.shell_focus, window, cx);

        // Settings deliberately does not close on `escape`: the draft can hold a
        // long edit and closing discards it.
        child_window_root(&self.shell_focus, false, on_close)
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .font(font)
            .text_size(px(font_size))
            .child(child_window_header(
                palette,
                title,
                Some("icons/settings.svg"),
                self.chrome,
                window,
                move |_, window, cx| header_close(window, cx),
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

impl NyaTermApp {
    /// Close the settings window, asking first when the draft has unsaved edits.
    ///
    /// Returns whether the window may close now. Closing settings *discards*:
    /// `cancel_settings` restores every field from the snapshot taken when the
    /// window opened, so an unconfirmed close silently throws the edits away.
    /// The remote text editor already vetoes its own close this way.
    fn request_settings_window_close(
        &mut self,
        panel: &Entity<SettingsPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_draft_dirty() {
            self.discard_settings_draft_and_window(panel, cx);
            return true;
        }
        let panel = panel.clone();
        self.open_confirm_dialog(
            (
                t!("settings.discardChangesTitle").to_string(),
                t!("settings.discardChangesDesc").to_string(),
                t!("settings.discardChanges").to_string(),
                true,
                move |app: &mut NyaTermApp, window: &mut Window, cx: &mut Context<NyaTermApp>| {
                    app.discard_settings_draft_and_window(&panel, cx);
                    window.remove_window();
                    true
                },
            ),
            window,
            cx,
        );
        false
    }

    /// `cancel_settings` already clears the window slot through
    /// `finish_settings_navigation`, which is also what restores the panel
    /// collapse state the embedded page had before settings opened.
    fn discard_settings_draft_and_window(
        &mut self,
        panel: &Entity<SettingsPanel>,
        cx: &mut Context<Self>,
    ) {
        self.clear_native_settings_panel(panel, cx);
        self.cancel_settings(cx);
    }

    /// Raise the settings window if one is already open.
    ///
    /// Returning `true` is what keeps a second "open settings" from starting a
    /// competing draft: there is one draft, so the request has to land on the
    /// window already holding it.
    pub(in crate::features) fn activate_settings_window(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(handle) = self.shell.settings_window() else {
            return false;
        };
        activate_child_window(
            &cx.entity(),
            handle,
            |app: &mut NyaTermApp| Some(app.shell.settings_window_slot()),
            cx,
        );
        true
    }

    pub(in crate::features) fn open_settings_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.activate_settings_window(cx) {
            return true;
        }
        if !self.shell.begin_settings_window_open() {
            return true;
        }
        cx.notify();
        let app = cx.entity();
        cx.defer(move |cx| {
            let should_open = app.read(cx).shell.settings_window_open_pending();
            if should_open {
                open_settings_window_now_from_app(app, cx);
            }
        });
        true
    }
}

fn open_settings_window_now_from_app(app: Entity<NyaTermApp>, cx: &mut App) {
    if app.read(cx).shell.settings_window().is_some() {
        app.update(cx, |app, cx| {
            app.shell.cancel_settings_window_open();
            app.activate_settings_window(cx);
            cx.notify();
        });
        return;
    }
    if !app.read(cx).shell.has_settings_draft() {
        app.update(cx, |app, cx| {
            app.shell.cancel_settings_window_open();
            cx.notify();
        });
        return;
    }

    let spec = ChildWindowSpec::settings(t!("settings.title").to_string(), 800., 560.)
        .min_size(640., 480.);
    let chrome = spec.chrome();
    let parent = app.read(cx).shell.main_window();
    let options = child_window_options(&spec, parent, cx);
    let close_app = app.clone();
    let view_app = app.clone();
    let result: anyhow::Result<NyaWindowHandle> = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| SettingsWindow::new(view_app, chrome, cx));
        let close_panel = view.read(cx).settings_panel.clone();
        window.on_window_should_close(cx, move |window, cx| {
            let panel = close_panel.clone();
            close_app.update(cx, |app, cx| {
                app.request_settings_window_close(&panel, window, cx)
            })
        });
        cx.new(|cx| nya_root(view, window, cx))
    });

    app.update(cx, |app, cx| match result {
        Ok(handle) => {
            app.shell.complete_settings_window_open(handle);
            cx.notify();
        }
        Err(error) => {
            app.shell.fail_settings_window_open();
            app.shell
                .set_status(format!("failed to open settings window: {error}"));
            cx.notify();
        }
    });
}
