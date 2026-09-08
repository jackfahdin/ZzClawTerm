use rust_i18n::t;

use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, IntoElement, Render, Subscription, Window, div,
    prelude::*, px, rgb,
};
use nyaterm_ui::{NyaWindowHandle, activate_child_window, nya_root};

use crate::features::{
    NyaTermApp,
    view_widgets::{
        ChildWindowChrome, ChildWindowCloseHandler, ChildWindowSpec, child_window_header,
        child_window_options, child_window_root, focus_child_window_shell_if_idle,
    },
};

pub(super) struct TransferExternalSyncWindow {
    app: Entity<NyaTermApp>,
    prompt_id: String,
    shell_focus: FocusHandle,
    chrome: ChildWindowChrome,
    _app_subscription: Subscription,
}

impl TransferExternalSyncWindow {
    fn new(
        app: Entity<NyaTermApp>,
        prompt_id: String,
        chrome: ChildWindowChrome,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            prompt_id,
            shell_focus: cx.focus_handle(),
            chrome,
            _app_subscription: app_subscription,
        }
    }
}

impl Render for TransferExternalSyncWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(prompt) = self
            .app
            .read(cx)
            .transfer
            .external_sync_prompt(&self.prompt_id)
            .cloned()
        else {
            let prompt_id = self.prompt_id.clone();
            self.app.update(cx, |app, cx| {
                app.transfer.clear_external_sync_window_tracking(&prompt_id);
                cx.notify();
            });
            window.defer(cx, |window, _| window.remove_window());
            return div().size_full().into_any_element();
        };

        let (palette, font, font_size, title) = self.app.read_with(cx, |app, _| {
            (
                app.theme_palette(),
                app.gpui_ui_font().font(),
                app.settings.summary().ui_font_size.clamp(12, 24) as f32,
                t!("fileExplorer.fileModified").to_string(),
            )
        });
        window.set_window_title(&title);
        let prompt_id = self.prompt_id.clone();
        let content = self.app.update(cx, |app, cx| {
            app.transfer_external_sync_window_view(prompt_id, prompt, cx)
        });
        let close_app = self.app.clone();
        let close_prompt_id = self.prompt_id.clone();
        let on_close: ChildWindowCloseHandler =
            Rc::new(move |window: &mut Window, cx: &mut App| {
                let prompt_id = close_prompt_id.clone();
                close_app.update(cx, |app, cx| {
                    app.ignore_external_editor_sync_prompt(&prompt_id, cx);
                });
                window.remove_window();
            });
        let header_close = on_close.clone();
        focus_child_window_shell_if_idle(&self.shell_focus, window, cx);

        child_window_root(&self.shell_focus, true, on_close)
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .font(font)
            .text_size(px(font_size))
            .child(child_window_header(
                palette,
                title,
                Some("icons/sync.svg"),
                self.chrome,
                window,
                move |_, window, cx| header_close(window, cx),
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

impl NyaTermApp {
    pub(in crate::features) fn open_transfer_external_sync_window(
        &mut self,
        prompt_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(handle) = self.transfer.external_sync_window(&prompt_id) {
            let slot_prompt_id = prompt_id.clone();
            activate_child_window(
                &cx.entity(),
                handle,
                move |app: &mut NyaTermApp| app.transfer.external_sync_window_slot(&slot_prompt_id),
                cx,
            );
            return true;
        }
        if !self.transfer.begin_external_sync_window_open(&prompt_id) {
            return false;
        }
        cx.notify();
        let app = cx.entity();
        cx.defer(move |cx| {
            let should_open = app
                .read(cx)
                .transfer
                .external_sync_window_open_is_pending(&prompt_id);
            if should_open {
                open_transfer_external_sync_window_now_from_app(app, prompt_id, cx);
            }
        });
        true
    }
}

fn open_transfer_external_sync_window_now_from_app(
    app: Entity<NyaTermApp>,
    prompt_id: String,
    cx: &mut App,
) {
    if let Some(handle) = app.read(cx).transfer.external_sync_window(&prompt_id) {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        app.update(cx, |app, cx| {
            app.transfer
                .finish_external_sync_window_open(prompt_id.clone(), handle);
            cx.notify();
        });
        return;
    }
    if app
        .read(cx)
        .transfer
        .external_sync_prompt(&prompt_id)
        .is_none()
    {
        app.update(cx, |app, cx| {
            app.transfer.clear_external_sync_window_tracking(&prompt_id);
            cx.notify();
        });
        return;
    }

    let title = t!("fileExplorer.fileModified").to_string();
    // Always on top: this fires while the user is in an external editor, and a
    // prompt they cannot see is a prompt that never gets answered. The pre-GPUI
    // implementation set `alwaysOnTop` for exactly these windows and nothing else.
    let spec = ChildWindowSpec::topmost_prompt(title, 440., 240.);
    let chrome = spec.chrome();
    let parent = app.read(cx).shell.main_window();
    let options = child_window_options(&spec, parent, cx);
    let close_app = app.clone();
    let close_prompt_id = prompt_id.clone();
    let view_app = app.clone();
    let view_prompt_id = prompt_id.clone();
    let result: anyhow::Result<NyaWindowHandle> = cx.open_window(options, move |window, cx| {
        window.on_window_should_close(cx, move |_, cx| {
            close_app.update(cx, |app, cx| {
                app.ignore_external_editor_sync_prompt(&close_prompt_id, cx);
            });
            true
        });
        let prompt_focus = view_app.read(cx).transfer.external_sync_focus().clone();
        window.focus(&prompt_focus, cx);
        let view =
            cx.new(|cx| TransferExternalSyncWindow::new(view_app, view_prompt_id, chrome, cx));
        cx.new(|cx| nya_root(view, window, cx))
    });

    app.update(cx, |app, cx| match result {
        Ok(handle) => {
            app.transfer
                .finish_external_sync_window_open(prompt_id.clone(), handle);
            cx.notify();
        }
        Err(error) => {
            app.transfer.clear_external_sync_window_tracking(&prompt_id);
            app.shell
                .set_status(format!("failed to open auto-upload window: {error}"));
            cx.notify();
        }
    });
}
