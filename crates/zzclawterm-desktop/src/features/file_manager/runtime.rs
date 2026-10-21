use gpui::{Context, Window, prelude::*};
use rust_i18n::t;
use zzclawterm_transport::{
    FileBrowserService, RemoteFilePath, SftpDuplicatePolicy, SftpDuplicateResolver,
    SftpPathTransferOptions, SftpTransferControl, file_browser_join, file_browser_name,
    file_browser_parent, file_browser_path_is_root,
};

use super::operations::FileManagerCopyPlan;
use super::pane::{PaneEndpoint, PaneId};
use super::state::FileManagerJobTarget;
use super::view::FileManagerNameKind;
use super::workspace::{FileManagerListResult, FileManagerWorkspace};
use crate::features::ZzClawTermApp;
use crate::features::pages::transfers::file_ops::clipboard::{
    ClipboardCopyRequest, run_clipboard_copy,
};
use crate::features::runtime_jobs::await_blocking_job;
use crate::features::text_inputs::TextInputSetup;
use crate::features::transfers::transfer_jobs::TransferProgressEventSender;
use crate::models::{
    TransferJobEvent, TransferJobKind, TransferJobOutput, TransferJobResult, TransferJobState,
    TransferJobStatus,
};

impl ZzClawTermApp {
    pub(in crate::features) fn file_browser_service_for_endpoint(
        &mut self,
        endpoint: &PaneEndpoint,
    ) -> anyhow::Result<FileBrowserService> {
        match endpoint {
            PaneEndpoint::Local => Ok(FileBrowserService::local()),
            PaneEndpoint::Session(id) => self.file_browser_service_for_session(id),
        }
    }

    pub(in crate::features) fn navigate_file_manager_pane(
        &mut self,
        manager_id: String,
        pane_id: PaneId,
        path: RemoteFilePath,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let (endpoint, generation, snapshot) = workspace.update(cx, |workspace, _| {
            let pane = workspace.pane_mut(pane_id);
            let snapshot = pane.capture_navigation();
            let generation = pane.begin_load(path.clone());
            (pane.endpoint.clone(), generation, snapshot)
        });
        if matches!(&endpoint, PaneEndpoint::Session(id) if self.session.is_disconnected(id)) {
            workspace.update(cx, |workspace, cx| {
                workspace.pane_mut(pane_id).disconnected = true;
                workspace.pane_mut(pane_id).loading = false;
                workspace.pane_mut(pane_id).status = t!("fileManager.disconnected").to_string();
                cx.notify();
            });
            return;
        }
        let service = match self.file_browser_service_for_endpoint(&endpoint) {
            Ok(service) => service,
            Err(error) => {
                workspace.update(cx, |workspace, cx| {
                    workspace.apply_list_result(FileManagerListResult {
                        pane: pane_id,
                        generation,
                        endpoint,
                        path,
                        snapshot,
                        result: Err(error.to_string()),
                    });
                    cx.notify();
                });
                return;
            }
        };
        let task = self.blocking_jobs.submit_task("file-manager-list", {
            let path = path.clone();
            move |_| {
                service
                    .list_dir_path(&path)
                    .map_err(|error| error.to_string())
            }
        });
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |_, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.apply_list_result(FileManagerListResult {
                        pane: pane_id,
                        generation,
                        endpoint,
                        path,
                        snapshot,
                        result,
                    });
                    cx.notify();
                });
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn navigate_file_manager_home(
        &mut self,
        manager_id: String,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let endpoint = workspace.read(cx).pane(pane_id).endpoint.clone();
        let service = match self.file_browser_service_for_endpoint(&endpoint) {
            Ok(service) => service,
            Err(error) => {
                workspace.update(cx, |workspace, cx| {
                    workspace.pane_mut(pane_id).error = Some(error.to_string());
                    cx.notify();
                });
                return;
            }
        };
        match service.home_dir() {
            Ok(home) => {
                workspace.update(cx, |workspace, _| {
                    let pane = workspace.pane_mut(pane_id);
                    pane.home_dir = home.clone();
                    pane.home_dir_pending = false;
                });
                self.navigate_file_manager_pane(manager_id, pane_id, RemoteFilePath::new(home), cx);
            }
            Err(error) => {
                workspace.update(cx, |workspace, cx| {
                    workspace.pane_mut(pane_id).error = Some(error.to_string());
                    cx.notify();
                });
            }
        }
    }

    pub(in crate::features) fn navigate_file_manager_parent(
        &mut self,
        manager_id: String,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let (endpoint, path) = workspace.read(cx).pane(pane_id).let_parent_nav();
        let service = match self.file_browser_service_for_endpoint(&endpoint) {
            Ok(service) => service,
            Err(_) => return,
        };
        if file_browser_path_is_root(service.kind(), &path.display_path) {
            return;
        }
        let parent = RemoteFilePath::new(file_browser_parent(service.kind(), &path.display_path));
        self.navigate_file_manager_pane(manager_id, pane_id, parent, cx);
    }

    pub(in crate::features) fn refresh_file_manager_pane(
        &mut self,
        manager_id: String,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let path = workspace.read(cx).pane(pane_id).path.clone();
        if path.display_path.is_empty() {
            self.navigate_file_manager_home(manager_id, pane_id, cx);
        } else {
            self.navigate_file_manager_pane(manager_id, pane_id, path, cx);
        }
    }

    pub(in crate::features) fn start_file_manager_copy(
        &mut self,
        manager_id: String,
        cut: bool,
        cx: &mut Context<Self>,
    ) {
        if cut && self.file_manager_same_endpoint_move(&manager_id, cx) {
            self.start_file_manager_same_endpoint_move(manager_id, cx);
            return;
        }
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let (source, target, entries, target_directory) = workspace.read(cx).copy_inputs();
        if entries.is_empty() {
            self.shell
                .set_status(t!("fileManager.selectBeforeCopy").to_string());
            cx.notify();
            return;
        }
        if source
            .session_id()
            .is_some_and(|id| self.session.is_disconnected(id))
            || target
                .session_id()
                .is_some_and(|id| self.session.is_disconnected(id))
        {
            self.shell
                .set_status(t!("fileManager.disconnected").to_string());
            cx.notify();
            return;
        }
        let source_service = match self.file_browser_service_for_endpoint(&source) {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };
        let target_service = match self.file_browser_service_for_endpoint(&target) {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };
        let plan = FileManagerCopyPlan {
            source: source.clone(),
            source_service,
            entries,
            target: target.clone(),
            target_service,
            target_directory: target_directory.clone(),
            cut,
        };
        let duplicate_policy = if cut {
            SftpDuplicatePolicy::Skip
        } else {
            self.transfer.duplicate_policy()
        };
        let duplicate_resolver = (duplicate_policy == SftpDuplicatePolicy::Ask).then(|| {
            self.session.prompt_duplicate_broker() as std::sync::Arc<dyn SftpDuplicateResolver>
        });
        let options = SftpPathTransferOptions::new(
            duplicate_policy,
            duplicate_resolver,
            self.sftp_transfer_options(),
        );
        let mut started = 0;
        for entry in plan.entries.clone() {
            if plan.rejects(&entry) {
                continue;
            }
            let target_path = plan.destination_for(&entry);
            let id = self.transfer.next_transfer_job_id("file-manager-copy");
            let control = SftpTransferControl::new();
            self.transfer.enqueue_transfer_job(TransferJobState {
                id: id.clone(),
                session_id: plan.source.session_id().map(str::to_string),
                kind: TransferJobKind::SendTo {
                    source_path: entry.path.clone(),
                    target_session_id: plan.target.session_id().unwrap_or("local").to_string(),
                    target_path: target_path.clone(),
                    target_parent_path: plan.target_directory.display_path.clone(),
                },
                status: TransferJobStatus::Running,
                detail: format!("{} {}", if cut { "Moving" } else { "Copying" }, entry.path),
                created_at_ms: TransferJobState::now_ms(),
                display_name: String::new(),
                entries: Vec::new(),
                summary: None,
                progress: None,
                control: Some(control.clone()),
                speed: Default::default(),
            });
            self.file_manager.track_job(
                id.clone(),
                FileManagerJobTarget {
                    source: plan.source.clone(),
                    source_parent: file_browser_parent(plan.source_service.kind(), &entry.path),
                    target: plan.target.clone(),
                    target_parent: plan.target_directory.display_path.clone(),
                },
            );
            if plan.cut {
                self.transfer
                    .track_cut_job(id.clone(), 0, entry.path.clone());
            }
            let request = ClipboardCopyRequest {
                source_service: plan.source_service.clone(),
                target_service: plan.target_service.clone(),
                source: entry.remote_path(),
                target_path,
                target_session_id: plan.target.session_id().unwrap_or("local").to_string(),
                cut: plan.cut,
                control,
                options: options.clone(),
            };
            let tx = self.transfer.transfer_event_sender();
            self.submit_bulk_transfer_job("file-manager-copy", id.clone(), tx.clone(), move || {
                let mut progress = TransferProgressEventSender::new(id.clone(), tx.clone());
                let result = run_clipboard_copy(request, move |value| progress.send(value));
                let _ = tx.unbounded_send(TransferJobResult {
                    id,
                    event: TransferJobEvent::Finished(result),
                });
            });
            started += 1;
        }
        self.shell
            .set_status(t!("fileManager.copyJobsStarted", count = started).to_string());
        cx.notify();
    }

    pub(in crate::features) fn start_file_manager_move(
        &mut self,
        manager_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        self.start_file_manager_copy(manager_id, true, cx);
    }

    pub(in crate::features) fn file_manager_same_endpoint_move(
        &mut self,
        manager_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(workspace) = self.file_manager.workspace(manager_id) else {
            return false;
        };
        let (source, target, _, _) = workspace.read(cx).copy_inputs();
        source == target
    }

    fn start_file_manager_same_endpoint_move(
        &mut self,
        manager_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let (source, _, entries, target_directory) = workspace.read(cx).copy_inputs();
        let service = match self.file_browser_service_for_endpoint(&source) {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };
        let mut moved = 0;
        for entry in entries {
            let name = file_browser_name(service.kind(), &entry.path);
            let destination =
                file_browser_join(service.kind(), &target_directory.display_path, &name);
            if destination == entry.path {
                continue;
            }
            if let Err(error) =
                service.rename_remote_paths(&entry.remote_path(), &RemoteFilePath::new(destination))
            {
                self.shell.set_status(error.to_string());
            } else {
                moved += 1;
            }
        }
        self.shell
            .set_status(t!("fileManager.movedItems", count = moved).to_string());
        self.refresh_file_manager_pane(manager_id.clone(), PaneId::Left, cx);
        self.refresh_file_manager_pane(manager_id, PaneId::Right, cx);
    }

    pub(in crate::features) fn apply_file_manager_job_output(
        &mut self,
        job_id: &str,
        output: &TransferJobOutput,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.file_manager.job_target(job_id).cloned() else {
            return;
        };
        if let TransferJobOutput::Sent {
            source_parent_path,
            source_entries,
            entries,
            ..
        } = output
        {
            let workspaces: Vec<_> = self
                .file_manager
                .workspaces()
                .map(|(_, entity)| entity.clone())
                .collect();
            for workspace in workspaces {
                workspace.update(cx, |workspace, cx| {
                    let source_parent = source_parent_path
                        .as_deref()
                        .unwrap_or(target.source_parent.as_str());
                    if let Some(source_entries) = source_entries.as_ref() {
                        workspace.refresh_matching_directory(
                            &target.source,
                            source_parent,
                            source_entries.clone(),
                        );
                    }
                    workspace.refresh_matching_directory(
                        &target.target,
                        &target.target_parent,
                        entries.clone(),
                    );
                    cx.notify();
                });
            }
        }
    }

    pub(in crate::features) fn mark_file_manager_session_disconnected(
        &mut self,
        session_id: &str,
        disconnected: bool,
        cx: &mut Context<Self>,
    ) {
        let workspaces: Vec<_> = self
            .file_manager
            .workspaces()
            .map(|(_, entity)| entity.clone())
            .collect();
        for workspace in workspaces {
            workspace.update(cx, |workspace, cx| {
                workspace.mark_disconnected(session_id, disconnected);
                cx.notify();
            });
        }
        if !disconnected {
            self.refresh_file_manager_session_panes(session_id, cx);
        }
    }

    pub(in crate::features) fn refresh_file_manager_session_panes(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        let workspaces: Vec<_> = self
            .file_manager
            .workspaces()
            .map(|(id, entity)| (id.clone(), entity.clone()))
            .collect();
        for (manager_id, workspace) in workspaces {
            let panes = workspace.read(cx).pane_ids_for_session(session_id);
            for pane in panes {
                self.refresh_file_manager_pane(manager_id.clone(), pane, cx);
            }
        }
    }

    pub(in crate::features) fn confirm_file_manager_delete(
        &mut self,
        manager_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let entries = workspace.read(cx).active_pane().selected_entries();
        if entries.is_empty() {
            return;
        }
        let count = entries.len();
        self.open_confirm_dialog(
            (
                t!("fileExplorer.delete").to_string(),
                t!("fileManager.deleteConfirm", count = count).to_string(),
                t!("fileExplorer.delete").to_string(),
                true,
                move |app, _, cx| {
                    app.run_file_manager_delete(manager_id.clone(), cx);
                    true
                },
            ),
            window,
            cx,
        );
    }

    fn run_file_manager_delete(&mut self, manager_id: String, cx: &mut Context<Self>) {
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return;
        };
        let pane_id = workspace.read(cx).active;
        let (endpoint, entries) = {
            let workspace = workspace.read(cx);
            (
                workspace.pane(pane_id).endpoint.clone(),
                workspace.pane(pane_id).selected_entries(),
            )
        };
        let service = match self.file_browser_service_for_endpoint(&endpoint) {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                return;
            }
        };
        for entry in entries {
            if let Err(error) = service.delete_remote_path(&entry.remote_path()) {
                self.shell.set_status(error.to_string());
            }
        }
        self.refresh_file_manager_pane(manager_id, pane_id, cx);
    }

    pub(in crate::features) fn open_file_manager_name_dialog(
        &mut self,
        manager_id: String,
        pane_id: PaneId,
        kind: FileManagerNameKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.forget_text_inputs("file-manager.name");
        let title = match kind {
            FileManagerNameKind::Folder => t!("fileExplorer.newFolder").to_string(),
            FileManagerNameKind::File => t!("fileExplorer.newFile").to_string(),
            FileManagerNameKind::Rename => t!("fileExplorer.cmRename").to_string(),
        };
        self.open_form_dialog(
            (
                title,
                420.,
                t!("common.confirm").to_string(),
                |app, _, cx| {
                    app.text_input_box(
                        "file-manager.name",
                        "",
                        TextInputSetup::placeholder(t!("fileExplorer.name")),
                        cx,
                    )
                    .into_any_element()
                },
                move |app, _, cx| {
                    let name = app
                        .existing_text_input("file-manager.name")
                        .map(|field| field.read(cx).value(cx))
                        .unwrap_or_default();
                    app.submit_file_manager_name(manager_id.clone(), pane_id, kind, name, cx)
                },
                |app, cx| {
                    app.forget_text_inputs("file-manager.name");
                    cx.notify();
                },
            ),
            window,
            cx,
        );
    }

    fn submit_file_manager_name(
        &mut self,
        manager_id: String,
        pane_id: PaneId,
        kind: FileManagerNameKind,
        name: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = name.trim().to_string();
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            self.shell
                .set_status(t!("fileManager.invalidName").to_string());
            return false;
        }
        let Some(workspace) = self.file_manager.workspace(&manager_id) else {
            return true;
        };
        let (endpoint, directory, selected) = {
            let workspace = workspace.read(cx);
            let pane = workspace.pane(pane_id);
            (
                pane.endpoint.clone(),
                pane.path.clone(),
                pane.selected_entries().into_iter().next(),
            )
        };
        let service = match self.file_browser_service_for_endpoint(&endpoint) {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                return true;
            }
        };
        let target = file_browser_join(service.kind(), &directory.display_path, &name);
        let result = match kind {
            FileManagerNameKind::Folder => service.create_dir_path(&target, Some(0o755)),
            FileManagerNameKind::File => service.create_file_path(&target, Some(0o644)),
            FileManagerNameKind::Rename => {
                let Some(entry) = selected else {
                    return true;
                };
                service.rename_remote_paths(&entry.remote_path(), &RemoteFilePath::new(target))
            }
        };
        if let Err(error) = result {
            self.shell.set_status(error.to_string());
            return false;
        }
        self.forget_text_inputs("file-manager.name");
        self.refresh_file_manager_pane(manager_id, pane_id, cx);
        true
    }

    pub(in crate::features) fn workspace_references_session(
        &self,
        session_id: &str,
        cx: &gpui::App,
    ) -> bool {
        self.transfer.editor_references_session(session_id)
            || self.file_manager.references_session(session_id, cx)
    }
}

impl FileManagerWorkspace {
    pub(in crate::features) fn copy_inputs(
        &self,
    ) -> (
        PaneEndpoint,
        PaneEndpoint,
        Vec<zzclawterm_transport::SftpFileEntry>,
        RemoteFilePath,
    ) {
        let source = self.pane(self.active);
        let target = self.pane(self.active.other());
        (
            source.endpoint.clone(),
            target.endpoint.clone(),
            source.selected_entries(),
            target.path.clone(),
        )
    }
}

impl super::pane::BrowserPaneState {
    fn let_parent_nav(&self) -> (PaneEndpoint, RemoteFilePath) {
        (self.endpoint.clone(), self.path.clone())
    }
}
