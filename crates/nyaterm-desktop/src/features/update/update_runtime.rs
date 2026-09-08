use rust_i18n::t;

use futures::StreamExt as _;
use gpui::{Context, IntoElement, Window};
use nyaterm_ui::NyaDialogWindowExt as _;

use crate::features::NyaTermApp;
use crate::http::update::check_native_update;

use super::state::UpdateJobResult;

impl NyaTermApp {
    pub(in crate::features) fn open_update_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_nya_dialog(cx) {
            cx.notify();
            return;
        }
        self.open_content_dialog(
            t!("updater.checking").to_string(),
            560.,
            |app, _, cx| app.update_dialog_content(cx).into_any_element(),
            |_, _| {},
            window,
            cx,
        );
        self.start_update_check(cx);
    }

    pub(in crate::features) fn close_update_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_nya_dialog(cx);
        cx.notify();
    }

    pub(in crate::features) fn start_update_check(&mut self, cx: &mut Context<Self>) {
        let Some(tx) = self.update.begin_check() else {
            cx.notify();
            return;
        };
        let rejected_tx = tx.clone();
        if let Err(error) = self
            .blocking_jobs
            .submit_detached("update-check", move |_| {
                let result = check_native_update();
                let _ = tx.unbounded_send(UpdateJobResult::new(result));
            })
        {
            let _ = rejected_tx.unbounded_send(UpdateJobResult::new(Err(error.to_string())));
        }
        cx.notify();
    }

    /// Deliver update-check results as they arrive.
    ///
    /// Started once at window open. Before this the runtime tick polled
    /// `rx.try_recv`, which meant a result waited for the next tick and forced
    /// `runtime_quiet_tick_allowed` to carry an `update` term to keep that wait
    /// short.
    pub(in crate::features) fn start_update_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut rx) = self.update.take_event_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| {
                        if this.update.apply_event(event) {
                            this.shell.set_status(this.update.status().to_string());
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
