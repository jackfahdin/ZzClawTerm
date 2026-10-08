use super::DesktopController;
use crate::features::update::UpdateCheckKind;
use crate::features::update::UpdateEvent;
use futures::StreamExt as _;
use gpui::Context;
use std::time::Duration;

impl DesktopController {
    pub(super) fn start_update_runtime(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = crate::features::update::install::take_update_cleanup_path() {
            let blocking_jobs = self.update_store.read(cx).blocking_jobs();
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = blocking_jobs.submit_detached("update-cleanup", move |_| {
                    crate::features::update::install::cleanup_update_work_dir(path);
                });
            })
            .detach();
        }

        if let Some(mut rx) = self
            .update_store
            .update(cx, |store, _| store.take_event_receiver())
        {
            let update_store = self.update_store.clone();
            cx.spawn(async move |_, cx| {
                while let Some(event) = rx.next().await {
                    update_store.update(cx, |store, cx| {
                        if store.apply_event(event) {
                            cx.notify();
                        }
                    });
                }
            })
            .detach();
        }

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |controller, cx| {
                if controller.process_quitting {
                    return;
                }
                let should_start = controller
                    .update_store
                    .update(cx, |store, _| store.mark_startup_check_started());
                if should_start {
                    controller.start_update_check(UpdateCheckKind::Silent, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn start_update_check(&mut self, kind: UpdateCheckKind, cx: &mut Context<Self>) {
        let Some((tx, generation, source)) = self.update_store.update(cx, |store, cx| {
            let request = store.begin_check(kind);
            if request.is_some() {
                cx.notify();
            }
            request.map(|(tx, generation)| (tx, generation, store.source()))
        }) else {
            return;
        };
        let rejected_tx = tx.clone();
        let blocking_jobs = self.update_store.read(cx).blocking_jobs();
        if let Err(error) = blocking_jobs.submit_detached("update-check", move |_| {
            let result = crate::http::update::check_native_update(source);
            let _ = tx.unbounded_send(UpdateEvent::Check {
                generation,
                kind,
                result,
            });
        }) {
            let _ = rejected_tx.unbounded_send(UpdateEvent::Check {
                generation,
                kind,
                result: Err(format!("could not start update check: {error}")),
            });
        }
    }
}
