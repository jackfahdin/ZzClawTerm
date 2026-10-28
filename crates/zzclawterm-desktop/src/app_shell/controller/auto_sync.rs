use super::DesktopController;
use crate::features::AutoSyncResult;
use crate::features::AutoSyncTrigger;
use crate::features::run_auto_sync;
use crate::features::sync::operation::{AutoSyncOperationControls, await_cloud_sync_job};
use futures::StreamExt as _;
use gpui::Context;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use zzclawterm_core::WorkspaceId;

mod coordinator;
pub(super) use coordinator::AutoSyncCoordinator;
use coordinator::AutoSyncJob;

impl DesktopController {
    pub(super) fn start_auto_sync_runtime(&mut self, cx: &mut Context<Self>) {
        if let Some(mut events) = self
            .startup
            .shared_store_runtime()
            .and_then(|store| store.take_sync_mutation_events())
        {
            cx.spawn(async move |this, cx| {
                while let Some(event) = events.next().await {
                    if this
                        .update(cx, |controller, _| {
                            controller.auto_sync.record_sync_mutation(event.generation)
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |controller, cx| controller.tick_auto_sync(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn tick_auto_sync(&mut self, cx: &mut Context<Self>) {
        if self.process_quitting || self.auto_sync.running() {
            return;
        }
        let Some(runtime_store) = self.startup.shared_store_runtime() else {
            return;
        };
        if let Some(at) =
            crate::features::sync::io_runtime::CloudSyncIoRuntime::shared().last_check()
        {
            self.auto_sync.observed_remote_check(at);
        }
        let now = Instant::now();
        let generation = runtime_store.sync_mutation_generation();
        let windows = &self.windows;
        let Some(AutoSyncJob {
            trigger,
            change_elapsed,
            pull_safe,
        }) = self.auto_sync.next_job(now, generation, || {
            windows_allow_auto_sync_pull(windows, cx)
        })
        else {
            return;
        };
        let store = runtime_store.blocking_client();
        let runtime = self.runtime.clone();
        let scheduler = self.update_store.read(cx).blocking_jobs();
        let (guard_request, guard_receiver) =
            futures::channel::oneshot::channel::<std::sync::mpsc::SyncSender<bool>>();
        let guard_request = Mutex::new(Some(guard_request));
        let expired = Arc::new(AtomicBool::new(false));
        let task_expired = expired.clone();
        let controls = AutoSyncOperationControls::default();
        let worker_controls = controls.clone();
        let task = scheduler.submit_task("cloud-sync-auto", move |_| {
            let before_apply = || {
                if task_expired.load(Ordering::Acquire) {
                    return Err(zzclawterm_core::CloudSyncError::AutoPullDeferred);
                }
                let (reply, receiver) = std::sync::mpsc::sync_channel(1);
                guard_request
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .ok_or(zzclawterm_core::CloudSyncError::AutoPullDeferred)?
                    .send(reply)
                    .map_err(|_| zzclawterm_core::CloudSyncError::AutoPullDeferred)?;
                if receiver
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap_or(false)
                    && !task_expired.load(Ordering::Acquire)
                {
                    Ok(())
                } else {
                    Err(zzclawterm_core::CloudSyncError::AutoPullDeferred)
                }
            };
            run_auto_sync(
                &store,
                &runtime,
                trigger,
                change_elapsed,
                pull_safe,
                &before_apply,
                &worker_controls,
            )
        });
        let guard_expired = expired.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(reply) = guard_receiver.await {
                let safe = !guard_expired.load(Ordering::Acquire)
                    && this
                        .update(cx, |controller, cx| {
                            !controller.process_quitting && controller.auto_sync_pull_safe(cx)
                        })
                        .unwrap_or(false);
                let _ = reply.send(safe);
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = await_cloud_sync_job(
                task,
                cx.background_executor().timer(Duration::from_secs(300)),
                controls.sync,
                |phase| {
                    expired.store(true, Ordering::Release);
                    controls.cleanup.request_cancel();
                    let _ = this.update(cx, |controller, cx| {
                        for entry in controller.windows.values() {
                            let _ = entry.shell.update(cx, |shell, cx| {
                                if let Some(app) = shell.app.as_ref() {
                                    app.update(cx, |app, cx| {
                                        app.show_cloud_sync_timeout(phase, cx)
                                    });
                                }
                            });
                        }
                    });
                },
            )
            .await;
            let _ = this.update(cx, |controller, cx| {
                controller.complete_auto_sync(trigger, result, cx);
            });
        })
        .detach();
    }

    pub(super) fn auto_sync_pull_safe(&self, cx: &gpui::App) -> bool {
        windows_allow_auto_sync_pull(&self.windows, cx)
    }

    pub(super) fn complete_auto_sync(
        &mut self,
        trigger: AutoSyncTrigger,
        result: Result<AutoSyncResult, zzclawterm_core::CloudSyncError>,
        cx: &mut Context<Self>,
    ) {
        let generation = self
            .startup
            .shared_store_runtime()
            .map(|store| store.sync_mutation_generation());
        let history = self.auto_sync.history_entry(trigger, &result);
        if let Some(mut history) = history {
            let log_dir = self.runtime.log_dir().to_path_buf();
            let store = self
                .startup
                .shared_store_runtime()
                .map(|s| s.blocking_client());
            let job = self.update_store.read(cx).blocking_jobs().submit_task(
                "cloud-sync-auto-history",
                move |_| {
                    if history.provider.is_none()
                        && let Some(store) = store
                    {
                        history.provider = store
                            .request_fn(zzclawterm_store::StoreDomain::CloudSync, |s| {
                                s.load_cloud_sync_settings()
                            })
                            .ok()
                            .map(|s| s.provider);
                    }
                    zzclawterm_core::append_cloud_sync_history(log_dir, &history)
                },
            );
            cx.spawn(async move |this, cx| {
                if !match job {
                    Ok(task) => matches!(task.await, Ok(Ok(()))),
                    Err(_) => false,
                } {
                    tracing::warn!("automatic cloud synchronization history could not be saved");
                }
                let _ = this.update(cx, |controller, cx| {
                    controller.auto_sync.complete(trigger, &result, generation);
                    controller.notify_auto_sync_windows(&result, true, cx);
                });
            })
            .detach();
        } else {
            self.auto_sync.complete(trigger, &result, generation);
            self.notify_auto_sync_windows(&result, false, cx);
        }
    }

    fn notify_auto_sync_windows(
        &self,
        result: &Result<AutoSyncResult, zzclawterm_core::CloudSyncError>,
        refresh_history: bool,
        cx: &mut Context<Self>,
    ) {
        for entry in self.windows.values() {
            let _ = entry.shell.update(cx, |shell, cx| {
                if let Some(app) = shell.app.as_ref() {
                    app.update(cx, |app, cx| {
                        app.queue_cloud_sync_credentials_refresh(cx);
                        if refresh_history {
                            app.queue_cloud_sync_history_refresh(None, cx);
                        }
                        app.apply_auto_cloud_sync_result(result, cx);
                    });
                }
            });
        }
    }

    pub(crate) fn cloud_sync_pull_blocked_excluding(
        &self,
        excluded: WorkspaceId,
        cx: &gpui::App,
    ) -> bool {
        self.windows
            .iter()
            .filter(|(id, _)| **id != excluded)
            .any(|(_, entry)| {
                entry
                    .shell
                    .upgrade()
                    .and_then(|shell| shell.read(cx).app.clone())
                    .is_none_or(|app| app.read(cx).cloud_sync_session_restore_blocked())
            })
    }

    pub(crate) fn cloud_sync_job_running_excluding(
        &self,
        excluded: WorkspaceId,
        cx: &gpui::App,
    ) -> bool {
        self.auto_sync.running()
            || self
                .windows
                .iter()
                .filter(|(id, _)| **id != excluded)
                .any(|(_, entry)| {
                    entry
                        .shell
                        .upgrade()
                        .and_then(|shell| shell.read(cx).app.clone())
                        .is_some_and(|app| app.read(cx).cloud_sync_job_running())
                })
    }
}

fn windows_allow_auto_sync_pull(
    windows: &HashMap<WorkspaceId, super::WorkspaceWindow>,
    cx: &gpui::App,
) -> bool {
    windows.values().all(|entry| {
        entry
            .shell
            .upgrade()
            .and_then(|shell| shell.read(cx).app.clone())
            .is_some_and(|app| !app.read(cx).auto_cloud_sync_pull_blocked())
    })
}
