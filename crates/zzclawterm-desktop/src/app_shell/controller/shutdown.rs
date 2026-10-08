use super::DesktopController;
use crate::features::WorkspaceCloseSnapshot;
use crate::features::shell::tray::show_window as show_tray_window;
use gpui::Context;
use gpui::Task;
use zzclawterm_core::DeviceWindowManifest;
use zzclawterm_core::DeviceWindowState;
use zzclawterm_core::MainWindowState;
use zzclawterm_core::WorkspaceId;
use zzclawterm_store::FlushBarrier;
use zzclawterm_store::StoreDomain;
use zzclawterm_store::StoreSubmitError;
use zzclawterm_store::StoreTask;
use zzclawterm_store::store_request;

impl DesktopController {
    pub fn submit_window_state(
        &mut self,
        workspace_id: WorkspaceId,
        state: MainWindowState,
        generation: u64,
    ) -> Result<StoreTask<()>, StoreSubmitError> {
        if let Some(existing) = self
            .device_windows
            .windows
            .iter_mut()
            .find(|entry| entry.workspace_id == workspace_id)
        {
            existing.window = state;
        } else {
            self.device_windows.windows.push(DeviceWindowState {
                workspace_id,
                window: state,
                extra: Default::default(),
            });
        }
        if !self.device_windows.window_order.contains(&workspace_id) {
            self.device_windows.window_order.push(workspace_id);
        }
        self.device_windows.most_recent_workspace_id = self.most_recent_workspace_id;
        let store = self
            .startup
            .shared_store_runtime()
            .expect("desktop controller requires a store runtime")
            .ui_client();
        let device_windows = self.device_windows.clone();
        let recent = self.most_recent_workspace_id;
        store.try_submit(
            generation,
            store_request(StoreDomain::WindowState, move |store| {
                let mut manifest = store.load_workspace_restore_manifest()?;
                if let Some(recent) = recent {
                    if !manifest
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.id == recent)
                    {
                        manifest
                            .workspaces
                            .push(zzclawterm_core::WorkspaceRestoreState::empty(recent));
                    }
                    manifest.most_recent_workspace_id = Some(recent);
                }
                store.save_restore_manifests_atomically(&manifest, &device_windows)
            }),
        )
    }

    pub fn request_close_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        persistence_task: StoreTask<()>,
        cx: &mut Context<Self>,
    ) -> Result<(), StoreSubmitError> {
        if !self.windows.contains_key(&workspace_id) {
            return Ok(());
        }
        if self.process_quitting || !self.closing_workspaces.is_empty() {
            return Err(StoreSubmitError::ShuttingDown);
        }
        self.closing_workspaces.insert(workspace_id);
        self.tab_drag.cancel_workspace(workspace_id);
        cx.spawn(async move |this, cx| {
            let persistence = persistence_task.await;
            if let Err(error) = persistence.outcome {
                let _ = this.update(cx, |controller, cx| {
                    controller.report_workspace_close_failure(workspace_id, error.to_string(), cx)
                });
                return;
            }

            let barrier = match this.update(cx, |controller, _| {
                controller
                    .startup
                    .shared_store_runtime()
                    .expect("desktop controller requires a store runtime")
                    .ui_client()
                    .try_submit(0, FlushBarrier)
            }) {
                Ok(Ok(task)) => task,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |controller, cx| {
                        controller.report_workspace_close_failure(
                            workspace_id,
                            error.to_string(),
                            cx,
                        )
                    });
                    return;
                }
                Err(_) => return,
            };
            let barrier = barrier.await;
            if let Err(error) = barrier.outcome {
                let _ = this.update(cx, |controller, cx| {
                    controller.report_workspace_close_failure(workspace_id, error.to_string(), cx)
                });
                return;
            }

            let submitted = this.update(cx, |controller, _| {
                controller.submit_closed_workspace_manifests(workspace_id)
            });
            let (task, device_windows, next_recent) = match submitted {
                Ok(Ok(submitted)) => submitted,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |controller, cx| {
                        controller.report_workspace_close_failure(
                            workspace_id,
                            error.to_string(),
                            cx,
                        )
                    });
                    return;
                }
                Err(_) => return,
            };
            let event = task.await;
            if let Err(error) = event.outcome {
                let _ = this.update(cx, |controller, cx| {
                    controller.report_workspace_close_failure(workspace_id, error.to_string(), cx)
                });
                return;
            }
            let _ = this.update(cx, |controller, cx| {
                controller.finish_workspace_close(workspace_id, device_windows, next_recent, cx)
            });
        })
        .detach();
        Ok(())
    }

    pub(super) fn submit_closed_workspace_manifests(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(StoreTask<()>, DeviceWindowManifest, Option<WorkspaceId>), StoreSubmitError> {
        let mut device_windows = self.device_windows.clone();
        device_windows
            .windows
            .retain(|entry| entry.workspace_id != workspace_id);
        device_windows.window_order.retain(|id| *id != workspace_id);
        let next_recent = next_recent_after_close(
            self.most_recent_workspace_id,
            workspace_id,
            &device_windows.window_order,
            |id| self.windows.contains_key(&id),
        );
        device_windows.most_recent_workspace_id = next_recent;
        let store = self
            .startup
            .shared_store_runtime()
            .expect("desktop controller requires a store runtime")
            .ui_client();
        let persisted_device_windows = device_windows.clone();
        let task = store.try_submit(
            0,
            store_request(StoreDomain::Sessions, move |store| {
                let mut manifest = store.load_workspace_restore_manifest()?;
                manifest
                    .workspaces
                    .retain(|workspace| workspace.id != workspace_id);
                manifest.most_recent_workspace_id = next_recent
                    .filter(|id| {
                        manifest
                            .workspaces
                            .iter()
                            .any(|workspace| workspace.id == *id)
                    })
                    .or_else(|| manifest.workspaces.last().map(|workspace| workspace.id));
                store.save_restore_manifests_atomically(&manifest, &persisted_device_windows)
            }),
        )?;
        Ok((task, device_windows, next_recent))
    }

    pub(super) fn finish_workspace_close(
        &mut self,
        workspace_id: WorkspaceId,
        device_windows: DeviceWindowManifest,
        next_recent: Option<WorkspaceId>,
        cx: &mut Context<Self>,
    ) {
        self.tab_drag.close(workspace_id);
        let Some(entry) = self.windows.remove(&workspace_id) else {
            self.closing_workspaces.remove(&workspace_id);
            return;
        };
        let cleanup = entry
            .shell
            .update(cx, |shell, cx| {
                shell.app.as_ref().map(|app| {
                    app.update(cx, |app, cx| {
                        app.shutdown_workspace_sessions();
                        app.shutdown_blocking_jobs(cx)
                    })
                })
            })
            .ok()
            .flatten();
        self.release_settings_owner(workspace_id);
        self.pending_tab_moves.remove(&workspace_id);
        cx.spawn(async move |this, cx| {
            if let Some(cleanup) = cleanup {
                cleanup.await;
            }
            let _ = this.update(cx, |controller, cx| {
                controller.closing_workspaces.remove(&workspace_id);
                controller.close_failed_tab_windows(cx);
                cx.defer(move |cx| {
                    let _ = entry
                        .handle
                        .update(cx, |_, window, _| window.remove_window());
                });
            });
        })
        .detach();
        self.most_recent_workspace_id = next_recent;
        self.device_windows = device_windows;
    }

    pub(super) fn report_workspace_close_failure(
        &mut self,
        workspace_id: WorkspaceId,
        message: String,
        cx: &mut Context<Self>,
    ) {
        self.closing_workspaces.remove(&workspace_id);
        if let Some(entry) = self.windows.get(&workspace_id) {
            let _ = entry.shell.update(cx, |shell, cx| {
                shell.finish_workspace_close_failure(message, cx)
            });
        }
        self.close_failed_tab_windows(cx);
    }

    pub(super) fn defer_workspace_close_failure(
        &self,
        workspace_id: WorkspaceId,
        message: String,
        cx: &mut Context<Self>,
    ) {
        let controller = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = controller.update(cx, |controller, cx| {
                controller.report_workspace_close_failure(workspace_id, message, cx)
            });
        });
    }

    pub fn request_close_unready_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) {
        if self.process_quitting || !self.closing_workspaces.is_empty() {
            return;
        }
        if self.windows.len() == 1 {
            self.process_quitting = true;
            let Some(store_runtime) = self.startup.shared_store_runtime() else {
                self.process_quitting = false;
                self.defer_workspace_close_failure(
                    workspace_id,
                    "The storage runtime is unavailable".to_string(),
                    cx,
                );
                return;
            };
            store_runtime.begin_shutdown();
            let task = store_runtime
                .ui_client()
                .try_submit_shutdown(0, FlushBarrier);
            match task {
                Ok(task) => {
                    cx.spawn(async move |this, cx| {
                        let outcome = task.await.outcome;
                        let _ = this.update(cx, |controller, cx| match outcome {
                            Ok(()) => {
                                controller.quit_after_workspace_shutdown(cx);
                            }
                            Err(error) => {
                                controller.process_quitting = false;
                                store_runtime.resume_after_failed_shutdown();
                                controller.report_workspace_close_failure(
                                    workspace_id,
                                    error.to_string(),
                                    cx,
                                );
                            }
                        });
                    })
                    .detach();
                }
                Err(error) => {
                    self.process_quitting = false;
                    store_runtime.resume_after_failed_shutdown();
                    self.defer_workspace_close_failure(workspace_id, error.to_string(), cx);
                }
            }
            return;
        }
        let Some(store_runtime) = self.startup.shared_store_runtime() else {
            self.defer_workspace_close_failure(
                workspace_id,
                "The storage runtime is unavailable".to_string(),
                cx,
            );
            return;
        };
        let persistence_task = store_runtime.ui_client().try_submit(0, FlushBarrier);
        match persistence_task {
            Ok(task) => {
                if let Err(error) = self.request_close_workspace(workspace_id, task, cx) {
                    self.defer_workspace_close_failure(workspace_id, error.to_string(), cx);
                }
            }
            Err(error) => {
                self.defer_workspace_close_failure(workspace_id, error.to_string(), cx);
            }
        }
    }

    pub fn request_quit(&mut self, cx: &mut Context<Self>) {
        self.tab_drag.cancel();
        if self.process_quitting {
            return;
        }
        let ready = self
            .device_windows
            .window_order
            .iter()
            .rev()
            .copied()
            .filter(|id| {
                self.windows.get(id).is_some_and(|entry| {
                    entry
                        .shell
                        .update(cx, |shell, _| shell.can_coordinate_quit())
                        .unwrap_or(false)
                })
            })
            .collect::<Vec<_>>();
        let workspace_id = ready
            .iter()
            .copied()
            .find(|id| {
                self.windows.get(id).is_some_and(|entry| {
                    entry
                        .shell
                        .update(cx, |shell, cx| {
                            shell
                                .app
                                .as_ref()
                                .is_some_and(|app| app.read(cx).has_live_sessions())
                        })
                        .unwrap_or(false)
                })
            })
            .or_else(|| {
                self.most_recent_workspace_id
                    .filter(|id| ready.contains(id))
            })
            .or_else(|| ready.first().copied());
        let Some(workspace_id) = workspace_id else {
            self.request_quit_without_ready(cx);
            return;
        };
        if let Some(entry) = self.windows.get(&workspace_id) {
            let shell = entry.shell.clone();
            let handle = entry.handle;
            cx.defer(move |cx| {
                let _ = handle.update(cx, move |_, window, cx| {
                    let _ = shell.update(cx, |shell, cx| {
                        show_tray_window(window, cx);
                        shell.request_application_quit(window, cx)
                    });
                });
            });
        }
    }

    pub fn live_session_count_excluding(
        &self,
        excluded_workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) -> usize {
        self.windows
            .iter()
            .filter(|(workspace_id, _)| **workspace_id != excluded_workspace_id)
            .filter_map(|(_, entry)| {
                entry
                    .shell
                    .update(cx, |shell, cx| {
                        shell
                            .app
                            .as_ref()
                            .map(|app| app.read(cx).live_session_count())
                    })
                    .ok()
                    .flatten()
            })
            .sum()
    }

    pub fn request_quit_without_ready(&mut self, cx: &mut Context<Self>) {
        if !self.begin_process_quit() {
            return;
        }
        let Some(store_runtime) = self.startup.shared_store_runtime() else {
            self.quit_after_workspace_shutdown(cx);
            return;
        };
        store_runtime.begin_shutdown();
        let barrier = store_runtime
            .ui_client()
            .try_submit_shutdown(0, FlushBarrier);
        let workspace_id = self.most_recent_workspace_id;
        match barrier {
            Ok(barrier) => {
                cx.spawn(async move |this, cx| {
                    let outcome = barrier.await.outcome;
                    let _ = this.update(cx, |controller, cx| match outcome {
                        Ok(()) => {
                            controller.quit_after_workspace_shutdown(cx);
                        }
                        Err(error) => {
                            store_runtime.resume_after_failed_shutdown();
                            controller.cancel_process_quit();
                            if let Some(id) = workspace_id {
                                controller.report_workspace_close_failure(
                                    id,
                                    error.to_string(),
                                    cx,
                                );
                            }
                        }
                    });
                })
                .detach();
            }
            Err(error) => {
                store_runtime.resume_after_failed_shutdown();
                self.cancel_process_quit();
                if let Some(id) = workspace_id {
                    self.report_workspace_close_failure(id, error.to_string(), cx);
                }
            }
        }
    }

    pub fn prepare_other_workspaces_for_quit(
        &mut self,
        current_workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) -> Result<Vec<StoreTask<()>>, StoreSubmitError> {
        let mut tasks = Vec::new();
        for (workspace_id, entry) in &self.windows {
            if *workspace_id == current_workspace_id {
                continue;
            }
            let task = entry
                .shell
                .update(cx, |shell, cx| shell.submit_process_quit_persistence(cx))
                .map_err(|_| StoreSubmitError::Disconnected)??;
            if let Some(task) = task {
                tasks.push(task);
            }
        }
        Ok(tasks)
    }

    pub(crate) fn submit_process_restore_snapshot(
        &mut self,
        current_workspace_id: WorkspaceId,
        current_snapshot: WorkspaceCloseSnapshot,
        current_state: Option<MainWindowState>,
        cx: &mut Context<Self>,
    ) -> Result<StoreTask<()>, StoreSubmitError> {
        let mut device_windows = self.device_windows.clone();
        let mut workspace_snapshots = vec![current_snapshot];
        for (workspace_id, entry) in &self.windows {
            if *workspace_id != current_workspace_id
                && let Some(app) = entry
                    .shell
                    .update(cx, |shell, _| shell.app.clone())
                    .ok()
                    .flatten()
            {
                workspace_snapshots
                    .push(app.update(cx, |app, _| app.capture_workspace_close_snapshot()));
            }
            let state = if *workspace_id == current_workspace_id {
                current_state.clone()
            } else {
                entry
                    .shell
                    .update(cx, |shell, _| shell.main_window_state.latest_for_shutdown())
                    .ok()
                    .flatten()
            };
            if let Some(state) = state {
                if let Some(existing) = device_windows
                    .windows
                    .iter_mut()
                    .find(|window| window.workspace_id == *workspace_id)
                {
                    existing.window = state;
                } else {
                    device_windows.windows.push(DeviceWindowState {
                        workspace_id: *workspace_id,
                        window: state,
                        extra: Default::default(),
                    });
                }
            }
            if !device_windows.window_order.contains(workspace_id) {
                device_windows.window_order.push(*workspace_id);
            }
        }
        device_windows.most_recent_workspace_id = self.most_recent_workspace_id;
        let recent = self.most_recent_workspace_id;
        let store = self
            .startup
            .shared_store_runtime()
            .expect("desktop controller requires a store runtime")
            .ui_client();
        store.try_submit_shutdown(
            u64::MAX - 1,
            store_request(StoreDomain::Shutdown, move |store| {
                let mut manifest = store.load_workspace_restore_manifest()?;
                for snapshot in workspace_snapshots {
                    let workspace = if let Some(index) = manifest
                        .workspaces
                        .iter()
                        .position(|workspace| workspace.id == snapshot.workspace_id)
                    {
                        &mut manifest.workspaces[index]
                    } else {
                        manifest
                            .workspaces
                            .push(zzclawterm_core::WorkspaceRestoreState::empty(
                                snapshot.workspace_id,
                            ));
                        manifest.workspaces.last_mut().expect("workspace inserted")
                    };
                    snapshot.apply_to(workspace);
                }
                manifest.most_recent_workspace_id = recent.filter(|id| {
                    manifest
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.id == *id)
                });
                store.save_restore_manifests_atomically(&manifest, &device_windows)
            }),
        )
    }

    pub(super) fn quit_after_workspace_shutdown(&mut self, cx: &mut Context<Self>) {
        let mut tasks = self.shutdown_workspaces_except(None, cx);
        tasks.push(self.shutdown_plugins(cx));
        cx.spawn(async move |this, cx| {
            for task in tasks {
                task.await;
            }
            let _ = this.update(cx, |_, cx| cx.quit());
        })
        .detach();
    }

    pub fn shutdown_other_workspaces(
        &mut self,
        excluded_workspace_id: WorkspaceId,
        cx: &mut Context<Self>,
    ) -> Vec<Task<()>> {
        self.shutdown_workspaces_except(Some(excluded_workspace_id), cx)
    }

    pub(super) fn shutdown_workspaces_except(
        &mut self,
        excluded_workspace_id: Option<WorkspaceId>,
        cx: &mut Context<Self>,
    ) -> Vec<Task<()>> {
        let mut tasks = Vec::new();
        for (workspace_id, entry) in &self.windows {
            if excluded_workspace_id == Some(*workspace_id) {
                continue;
            }
            if let Ok(Some(task)) = entry.shell.update(cx, |shell, cx| {
                shell.app.as_ref().map(|app| {
                    app.update(cx, |app, cx| {
                        app.shutdown_workspace_sessions();
                        app.shutdown_blocking_jobs(cx)
                    })
                })
            }) {
                tasks.push(task);
            }
        }
        tasks
    }
}
pub(super) fn next_recent_after_close(
    current: Option<WorkspaceId>,
    closing: WorkspaceId,
    window_order: &[WorkspaceId],
    is_open: impl Fn(WorkspaceId) -> bool,
) -> Option<WorkspaceId> {
    current
        .filter(|id| *id != closing && is_open(*id))
        .or_else(|| {
            window_order
                .iter()
                .rev()
                .copied()
                .find(|id| *id != closing && is_open(*id))
        })
}
