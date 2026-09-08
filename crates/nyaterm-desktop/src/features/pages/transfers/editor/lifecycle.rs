use gpui::{Context, Window};
use nyaterm_transport::{RemoteFilePath, RemoteTextGeneration, RemoteTextRevision};

use crate::features::{
    NyaTermApp, transfers::TransferEditorCloseAfterSave, transfers::TransferEditorCloseOutcome,
    transfers::TransferEditorDiscardOutcome,
};
use crate::models::{
    TransferJobEvent, TransferJobKind, TransferJobOutput, TransferJobResult, TransferJobState,
    TransferJobStatus,
};

use super::super::NATIVE_EDITOR_MAX_BYTES;

impl NyaTermApp {
    pub(in crate::features) fn activate_transfer_editor_tab(
        &mut self,
        tab_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.transfer.activate_editor_tab(tab_id) {
            cx.notify();
        }
    }

    pub(in crate::features) fn close_transfer_editor_tab(
        &mut self,
        tab_id: &str,
        cx: &mut Context<Self>,
    ) {
        match self.transfer.request_editor_tab_close(tab_id) {
            TransferEditorCloseOutcome::Missing => return,
            TransferEditorCloseOutcome::ConfirmationRequired => {
                self.shell
                    .set_status("remote editor tab has unsaved changes".to_string());
            }
            TransferEditorCloseOutcome::Closed => {
                self.shell
                    .set_status("remote editor tab closed".to_string());
            }
        }
        cx.notify();
    }

    pub(in crate::features) fn close_transfer_editor(&mut self, cx: &mut Context<Self>) {
        match self.transfer.request_editor_close() {
            TransferEditorCloseOutcome::Missing => return,
            TransferEditorCloseOutcome::ConfirmationRequired => {
                self.shell
                    .set_status("remote editor has unsaved changes".to_string());
            }
            TransferEditorCloseOutcome::Closed => {
                self.shell.set_status("remote editor closed".to_string());
            }
        }
        cx.notify();
    }

    pub(in crate::features) fn discard_transfer_editor(&mut self, cx: &mut Context<Self>) {
        match self.transfer.discard_editor() {
            TransferEditorDiscardOutcome::Missing => return,
            TransferEditorDiscardOutcome::TabDiscarded => {
                self.shell
                    .set_status("remote editor tab discarded".to_string());
            }
            TransferEditorDiscardOutcome::WorkspaceDiscarded => {
                self.shell.set_status("remote editor discarded".to_string());
            }
        }
        cx.notify();
    }

    pub(in crate::features) fn cancel_transfer_editor_close_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.transfer.cancel_editor_close() {
            cx.notify();
            return;
        }
        self.shell
            .set_status("remote editor close cancelled".to_string());
        cx.notify();
    }

    pub(in crate::features) fn cancel_transfer_editor_reload_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.transfer.cancel_editor_reload() {
            cx.notify();
            return;
        }
        self.shell
            .set_status("remote editor reload cancelled".to_string());
        cx.notify();
    }

    pub(in crate::features) fn cancel_transfer_editor_conflict(&mut self, cx: &mut Context<Self>) {
        if !self.transfer.cancel_editor_conflict() {
            cx.notify();
            return;
        }
        self.shell
            .set_status("remote editor conflict dismissed".to_string());
        cx.notify();
    }

    pub(in crate::features) fn start_sftp_editor_load_job(
        &mut self,
        session_id: Option<String>,
        tab_id: String,
        remote_file_path: RemoteFilePath,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remote_path = remote_file_path.display_path.clone();
        let Some(generation) = self
            .transfer
            .editor_tab_snapshot(&tab_id)
            .map(|tab| tab.generation)
        else {
            return;
        };
        let service = match self.transfer_file_browser_service(session_id.as_deref()) {
            Ok(service) => service,
            Err(error) => {
                let error = error.to_string();
                self.transfer
                    .fail_editor_load_tab(&tab_id, generation, error.clone());
                self.shell.set_status(error);
                cx.notify();
                return;
            }
        };
        let id = self.transfer.next_transfer_job_id("sftp-open-text");
        self.transfer.enqueue_transfer_job(TransferJobState {
            id: id.clone(),
            session_id,
            kind: TransferJobKind::LoadEditor {
                remote_path: remote_path.clone(),
                tab_id: tab_id.clone(),
                generation,
            },
            status: TransferJobStatus::Running,
            detail: format!("Opening {remote_path}"),
            created_at_ms: TransferJobState::now_ms(),
            display_name: String::new(),
            entries: Vec::new(),
            summary: None,
            progress: None,
            control: None,
        });
        let transfer_tx = self.transfer.transfer_event_sender();
        self.submit_transfer_blocking_job(
            "sftp-editor-load",
            id.clone(),
            transfer_tx.clone(),
            move || {
                let result = service
                    .read_text_document_path(&remote_file_path, NATIVE_EDITOR_MAX_BYTES)
                    .map(|file| TransferJobOutput::EditorLoaded {
                        tab_id,
                        remote_path,
                        generation,
                        file,
                    })
                    .map_err(|error| error.to_string());
                let _ = transfer_tx.unbounded_send(TransferJobResult {
                    id,
                    event: TransferJobEvent::Finished(result),
                });
            },
        );
        cx.notify();
    }

    pub(in crate::features) fn save_transfer_editor(
        &mut self,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.transfer.active_editor_tab().map(|tab| tab.id.clone());
        let Some(tab_id) = tab_id else {
            self.shell
                .set_status("no remote editor is active".to_string());
            cx.notify();
            return;
        };
        self.save_transfer_editor_tab(&tab_id, force, window, cx);
    }

    pub(in crate::features) fn save_all_transfer_editor_tabs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_ids = self.transfer.dirty_editor_tab_ids();
        for tab_id in tab_ids {
            self.save_transfer_editor_tab(&tab_id, false, window, cx);
        }
    }

    fn save_transfer_editor_tab(
        &mut self,
        tab_id: &str,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.transfer.editor_tab_snapshot(tab_id) else {
            return;
        };
        if snapshot.loading || snapshot.saving {
            return;
        }
        if !self.transfer.begin_editor_tab_save(tab_id) {
            return;
        }
        let remote_file_path = snapshot.remote_file_path();
        self.start_sftp_editor_save_job(
            snapshot.session_id,
            snapshot.id,
            remote_file_path,
            snapshot.content,
            snapshot.revision,
            snapshot.generation,
            force,
            window,
            cx,
        );
    }

    pub(in crate::features) fn save_transfer_editor_and_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.transfer.prepare_editor_close_after_save() {
            TransferEditorCloseAfterSave::Ready(tab_id) => {
                self.save_transfer_editor_tab(&tab_id, false, window, cx);
            }
            TransferEditorCloseAfterSave::All => {
                self.save_all_transfer_editor_tabs(window, cx);
            }
            TransferEditorCloseAfterSave::Missing => {
                self.shell
                    .set_status("no remote editor is active".to_string());
                cx.notify();
            }
            TransferEditorCloseAfterSave::Loading | TransferEditorCloseAfterSave::Saving => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::features) fn start_sftp_editor_save_job(
        &mut self,
        session_id: Option<String>,
        tab_id: String,
        remote_file_path: RemoteFilePath,
        content: String,
        expected_revision: Option<RemoteTextRevision>,
        generation: RemoteTextGeneration,
        force: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remote_path = remote_file_path.display_path.clone();
        let service = match self.transfer_file_browser_service(session_id.as_deref()) {
            Ok(service) => service,
            Err(error) => {
                self.transfer
                    .fail_editor_operation_tab(&tab_id, generation, error.to_string());
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };
        let id = self.transfer.next_transfer_job_id("sftp-save-text");
        self.transfer.enqueue_transfer_job(TransferJobState {
            id: id.clone(),
            session_id,
            kind: TransferJobKind::SaveEditor {
                remote_path: remote_path.clone(),
                tab_id: tab_id.clone(),
                generation,
            },
            status: TransferJobStatus::Running,
            detail: format!("Saving {remote_path}"),
            created_at_ms: TransferJobState::now_ms(),
            display_name: String::new(),
            entries: Vec::new(),
            summary: None,
            progress: None,
            control: None,
        });
        let transfer_tx = self.transfer.transfer_event_sender();
        self.submit_transfer_blocking_job(
            "sftp-editor-save",
            id.clone(),
            transfer_tx.clone(),
            move || {
                let result = service
                    .write_text_document_path(
                        &remote_file_path,
                        &content,
                        expected_revision.as_ref(),
                        force,
                    )
                    .map(|result| TransferJobOutput::EditorSaved {
                        tab_id,
                        remote_path,
                        generation,
                        result,
                    })
                    .map_err(|error| error.to_string());
                let _ = transfer_tx.unbounded_send(TransferJobResult {
                    id,
                    event: TransferJobEvent::Finished(result),
                });
            },
        );
        cx.notify();
    }
}
