//! Session lifecycle, prompts, recording and file-transfer session runtimes.

use std::collections::HashMap;
use std::thread::JoinHandle;

use nyaterm_transport::{RemoteFileService, SshMultiplexHandle};

mod auth_runtime;
mod prompt_runtime;
mod recording_runtime;
mod session_dialog_runtime;
mod session_lifecycle;
mod session_order;
mod session_runtime;
mod session_state;
mod startup_restore_runtime;
mod state;
mod temporary_ssh_link;
mod trzsz_runtime;
mod zmodem_runtime;

#[derive(Default)]
struct SessionProtocolRuntimeState {
    zmodem: HashMap<String, zmodem_runtime::ZmodemSessionState>,
    trzsz: HashMap<String, trzsz_runtime::TrzszSessionState>,
    remote_files: HashMap<String, RemoteFileService>,
    multiplex_handles: HashMap<String, SshMultiplexHandle>,
    multiplex_disconnect_workers: Vec<JoinHandle<()>>,
}

impl SessionProtocolRuntimeState {
    fn shutdown_workers(&mut self) {
        for state in self.zmodem.values_mut() {
            state.stop_worker();
        }
        for state in self.trzsz.values_mut() {
            state.stop_workers();
        }
        for (_, handle) in std::mem::take(&mut self.multiplex_handles) {
            self.spawn_multiplex_disconnect(handle);
        }
        for worker in self.multiplex_disconnect_workers.drain(..) {
            if worker.join().is_err() {
                tracing::warn!("SSH multiplex disconnect worker panicked during shutdown");
            }
        }
    }

    fn spawn_multiplex_disconnect(&mut self, handle: SshMultiplexHandle) {
        let mut still_running = Vec::new();
        for worker in self.multiplex_disconnect_workers.drain(..) {
            if worker.is_finished() {
                if worker.join().is_err() {
                    tracing::warn!("SSH multiplex disconnect worker panicked");
                }
            } else {
                still_running.push(worker);
            }
        }
        self.multiplex_disconnect_workers = still_running;
        match std::thread::Builder::new()
            .name("nyaterm-ssh-multiplex-disconnect".to_string())
            .spawn(move || {
                if let Err(error) = handle.disconnect() {
                    tracing::warn!(error = %error, "failed to disconnect SSH multiplex handle");
                }
            }) {
            Ok(worker) => self.multiplex_disconnect_workers.push(worker),
            Err(error) => {
                tracing::warn!(error = %error, "failed to spawn SSH multiplex disconnect worker");
            }
        }
    }
}

impl Drop for SessionProtocolRuntimeState {
    fn drop(&mut self) {
        self.shutdown_workers();
    }
}

pub(in crate::features) use auth_runtime::{
    AgentPromptBroker, AgentPromptRequest, AgentPromptState, CredentialPromptBroker,
    CredentialPromptRequest, CredentialPromptState, HostKeyPromptBroker, HostKeyPromptChoice,
    HostKeyPromptIssue, HostKeyPromptRequest, KeyboardInteractivePromptState,
    NativeHostKeyVerifier, NativeOtpProvider, SftpDuplicatePromptState, unix_seconds_now,
};
pub(in crate::features) use prompt_runtime::{
    credential_prompt_id, credential_prompt_target, credential_text_input_id,
    keyboard_interactive_prompt_id, keyboard_interactive_prompt_target,
    keyboard_interactive_text_input_id, sftp_duplicate_prompt_id, uuid_like_prompt_id,
};
pub(in crate::features) use state::{
    PendingSessionStart, SavedConnectionStartOptions, SessionFeatureFocus, SessionFeatureState,
    SessionStartEventRequest, SessionStartTabPlacement,
};
