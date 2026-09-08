use rust_i18n::t;

use std::rc::Rc;

use gpui::{
    AnyElement, AppContext as _, Context, Entity, ParentElement as _, Render, Subscription, Window,
    div,
};
use nyaterm_ui::{NyaConfirmDialog, NyaDialog, NyaDialogFooter, NyaDialogWindowExt};

use crate::features::NyaTermApp;

type FormRenderer =
    dyn Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> AnyElement + 'static;
type FormSubmitHandler =
    dyn Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> bool + 'static;
type AppDialogHandler = dyn Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static;
type FormBusyPredicate = dyn Fn(&NyaTermApp) -> bool + 'static;
type ConfirmHandler =
    dyn Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> bool + 'static;

struct AppDialogContent {
    app: Entity<NyaTermApp>,
    renderer: Rc<FormRenderer>,
    _app_subscription: Subscription,
}

impl AppDialogContent {
    fn new(app: Entity<NyaTermApp>, renderer: Rc<FormRenderer>, cx: &mut Context<Self>) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            renderer,
            _app_subscription: app_subscription,
        }
    }
}

impl Render for AppDialogContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.app
            .update(cx, |app, cx| (self.renderer)(app, window, cx))
    }
}

struct AppDialogFooter {
    app: Entity<NyaTermApp>,
    cancel_label: String,
    action_label: String,
    busy: Rc<FormBusyPredicate>,
    _app_subscription: Subscription,
}

impl AppDialogFooter {
    fn new(
        app: Entity<NyaTermApp>,
        cancel_label: String,
        action_label: String,
        busy: Rc<FormBusyPredicate>,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            cancel_label,
            action_label,
            busy,
            _app_subscription: app_subscription,
        }
    }
}

impl Render for AppDialogFooter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let busy = (self.busy)(self.app.read(cx));
        NyaDialogFooter::new(self.cancel_label.clone(), self.action_label.clone())
            .disabled(busy)
            .loading(busy)
            .into_element()
    }
}

impl NyaTermApp {
    pub(in crate::features) fn open_content_dialog(
        &mut self,
        title: String,
        width: f32,
        render: impl Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> AnyElement + 'static,
        on_close: impl Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.entity();
        let content = cx.new(|cx| AppDialogContent::new(app.clone(), Rc::new(render), cx));
        let on_close = Rc::new(on_close);

        window.open_nya_dialog(cx, move |dialog, _, _| {
            let close_app = app.clone();
            let on_close = on_close.clone();
            dialog
                .title(title.clone())
                .width(width)
                .content(content.clone())
                .on_close(move |_, _, cx| {
                    close_app.update(cx, |app, cx| on_close(app, cx));
                })
        });
    }

    pub(in crate::features) fn open_form_dialog<R, S, C>(
        &mut self,
        spec: (String, f32, String, R, S, C),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        R: Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> AnyElement + 'static,
        S: Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> bool + 'static,
        C: Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static,
    {
        let (title, width, action_label, render, on_submit, on_cancel) = spec;
        let cancel_label = t!("common.cancel").to_string();
        let app = cx.entity();
        let content = cx
            .new(|cx| AppDialogContent::new(app.clone(), Rc::new(render) as Rc<FormRenderer>, cx));
        let on_submit = Rc::new(on_submit) as Rc<FormSubmitHandler>;
        let on_cancel = Rc::new(on_cancel) as Rc<AppDialogHandler>;

        window.open_nya_dialog(cx, move |dialog, _, _| {
            let submit_app = app.clone();
            let cancel_app = app.clone();
            let on_submit = on_submit.clone();
            let on_cancel = on_cancel.clone();
            NyaConfirmDialog::new(
                dialog.title(title.clone()).width(width),
                NyaDialogFooter::new(cancel_label.clone(), action_label.clone()),
            )
            .content(content.clone())
            .on_confirm(move |_, window, cx| {
                submit_app.update(cx, |app, cx| on_submit(app, window, cx))
            })
            .on_cancel(move |_, _, cx| {
                cancel_app.update(cx, |app, cx| on_cancel(app, cx));
                true
            })
            .into_dialog()
        });
    }

    pub(in crate::features) fn open_guarded_form_dialog<R, S, C, B>(
        &mut self,
        spec: (String, f32, String, R, S, C, B),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        R: Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> AnyElement + 'static,
        S: Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> bool + 'static,
        C: Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static,
        B: Fn(&NyaTermApp) -> bool + 'static,
    {
        let (title, width, action_label, render, on_submit, on_cancel, busy) = spec;
        let cancel_label = t!("common.cancel").to_string();
        let app = cx.entity();
        let content = cx
            .new(|cx| AppDialogContent::new(app.clone(), Rc::new(render) as Rc<FormRenderer>, cx));
        let busy = Rc::new(busy) as Rc<FormBusyPredicate>;
        let footer = cx.new(|cx| {
            AppDialogFooter::new(
                app.clone(),
                cancel_label.clone(),
                action_label.clone(),
                busy.clone(),
                cx,
            )
        });
        let on_submit = Rc::new(on_submit) as Rc<FormSubmitHandler>;
        let on_cancel = Rc::new(on_cancel) as Rc<AppDialogHandler>;

        window.open_nya_dialog(cx, move |dialog, _, _| {
            let submit_app = app.clone();
            let cancel_app = app.clone();
            let submit_busy = busy.clone();
            let cancel_busy = busy.clone();
            let on_submit = on_submit.clone();
            let on_cancel = on_cancel.clone();
            let button_props = NyaDialogFooter::new(cancel_label.clone(), action_label.clone());
            NyaDialog::confirm_with_footer(
                dialog.title(title.clone()).width(width).close_button(false),
                button_props,
                footer.clone(),
            )
            .content(content.clone())
            .on_ok(move |_, window, cx| {
                submit_app.update(cx, |app, cx| {
                    if submit_busy(app) {
                        false
                    } else {
                        on_submit(app, window, cx)
                    }
                })
            })
            .on_cancel(move |_, _, cx| {
                cancel_app.update(cx, |app, cx| {
                    if cancel_busy(app) {
                        false
                    } else {
                        on_cancel(app, cx);
                        true
                    }
                })
            })
        });
    }

    pub(in crate::features) fn open_confirm_dialog<F>(
        &mut self,
        spec: (String, String, String, bool, F),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        F: Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> bool + 'static,
    {
        let (title, message, action_label, danger, on_confirm) = spec;
        self.open_confirm_dialog_with_cancel(
            (title, message, action_label, danger, on_confirm, |_, _| {}),
            window,
            cx,
        );
    }

    pub(in crate::features) fn open_confirm_dialog_with_cancel<F, C>(
        &mut self,
        spec: (String, String, String, bool, F, C),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        F: Fn(&mut NyaTermApp, &mut Window, &mut Context<NyaTermApp>) -> bool + 'static,
        C: Fn(&mut NyaTermApp, &mut Context<NyaTermApp>) + 'static,
    {
        let (title, message, action_label, danger, on_confirm, on_cancel) = spec;
        let cancel_label = t!("common.cancel").to_string();
        let app = cx.weak_entity();
        let on_confirm = Rc::new(on_confirm) as Rc<ConfirmHandler>;
        let on_cancel = Rc::new(on_cancel) as Rc<AppDialogHandler>;
        window.open_nya_dialog(cx, move |dialog, _, _| {
            let confirm_app = app.clone();
            let cancel_app = app.clone();
            let on_confirm = on_confirm.clone();
            let on_cancel = on_cancel.clone();
            let footer = NyaDialogFooter::new(cancel_label.clone(), action_label.clone());
            let footer = if danger { footer.danger() } else { footer };
            NyaConfirmDialog::new(dialog.title(title.clone()).width(384.), footer)
                .content(div().child(message.clone()))
                .on_confirm(move |_, window, cx| {
                    confirm_app
                        .update(cx, |app, cx| on_confirm(app, window, cx))
                        .unwrap_or(false)
                })
                .on_cancel(move |_, _, cx| {
                    let _ = cancel_app.update(cx, |app, cx| on_cancel(app, cx));
                    true
                })
                .into_dialog()
        });
    }
}
