use std::fmt;
use std::sync::{Arc, Mutex, mpsc};

use ironrdp_client::rdp::RdpInputSender;
use ironrdp_cliprdr::backend::{ClipboardMessage, CliprdrBackend};
use ironrdp_cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
    OwnedFormatDataResponse,
};
use ironrdp_cliprdr::{Cliprdr, CliprdrClient};
use ironrdp_core::impl_as_any;
use nyaterm_remote_desktop::{MAX_CLIPBOARD_TEXT_BYTES, RdpControlMessage};

use super::Outbound;

pub(super) struct ClipboardBridge {
    session_id: String,
    output_tx: mpsc::SyncSender<Outbound>,
    state: Mutex<ClipboardState>,
}

#[derive(Default)]
struct ClipboardState {
    local_text: String,
    input: Option<RdpInputSender>,
    generation: u64,
}

impl fmt::Debug for ClipboardBridge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipboardBridge")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl ClipboardBridge {
    pub(super) fn new(session_id: String, output_tx: mpsc::SyncSender<Outbound>) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            output_tx,
            state: Mutex::new(ClipboardState::default()),
        })
    }

    pub(super) fn set_input_sender(&self, sender: RdpInputSender) {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .input = Some(sender);
    }

    pub(super) fn set_local_text(&self, text: String) -> anyhow::Result<()> {
        if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
            anyhow::bail!("clipboard text exceeds the 4 MiB limit");
        }
        let sender = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.local_text = text;
            state.input.clone()
        };
        if let Some(sender) = sender {
            sender
                .send_clipboard(ClipboardMessage::SendInitiateCopy(vec![
                    ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
                ]))
                .map_err(|_| anyhow::anyhow!("IronRDP clipboard input channel closed"))?;
        }
        Ok(())
    }

    pub(super) fn cliprdr_client(self: &Arc<Self>) -> CliprdrClient {
        Cliprdr::new(Box::new(TextClipboardBackend {
            bridge: Arc::clone(self),
        }))
    }

    fn send_clipboard_message(&self, message: ClipboardMessage) {
        let sender = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .input
            .clone();
        if let Some(sender) = sender {
            let _ = sender.send_clipboard(message);
        }
    }

    fn publish_remote_text(&self, text: String) {
        if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
            return;
        }
        let generation = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.generation = state.generation.wrapping_add(1);
            state.generation
        };
        let _ = self
            .output_tx
            .send(Outbound::Control(RdpControlMessage::Clipboard {
                session_id: self.session_id.clone(),
                text,
                generation,
            }));
    }

    fn local_text(&self) -> String {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .local_text
            .clone()
    }
}

#[derive(Debug)]
struct TextClipboardBackend {
    bridge: Arc<ClipboardBridge>,
}

impl_as_any!(TextClipboardBackend);

impl CliprdrBackend for TextClipboardBackend {
    fn temporary_directory(&self) -> &str {
        ".nyaterm-cliprdr"
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {
        self.on_request_format_list();
    }

    fn on_request_format_list(&mut self) {
        if !self.bridge.local_text().is_empty() {
            self.bridge
                .send_clipboard_message(ClipboardMessage::SendInitiateCopy(vec![
                    ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
                ]));
        }
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        _capabilities: ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        if available_formats
            .iter()
            .any(|format| format.id() == ClipboardFormatId::CF_UNICODETEXT)
        {
            self.bridge
                .send_clipboard_message(ClipboardMessage::SendInitiatePaste(
                    ClipboardFormatId::CF_UNICODETEXT,
                ));
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let response = if request.format == ClipboardFormatId::CF_UNICODETEXT {
            OwnedFormatDataResponse::new_unicode_string(&self.bridge.local_text())
        } else {
            OwnedFormatDataResponse::new_error()
        };
        self.bridge
            .send_clipboard_message(ClipboardMessage::SendFormatData(response));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }
        if let Ok(text) = response.to_unicode_string() {
            self.bridge.publish_remote_text(text);
        }
    }

    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _data_id: LockDataId) {}

    fn on_unlock(&mut self, _data_id: LockDataId) {}
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::ClipboardBridge;
    use nyaterm_remote_desktop::MAX_CLIPBOARD_TEXT_BYTES;

    #[test]
    fn headless_clipboard_accepts_text_and_rejects_oversize_payloads() {
        let (output_tx, _output_rx) = mpsc::sync_channel(1);
        let bridge = ClipboardBridge::new("session".to_string(), output_tx);
        bridge.set_local_text("hello".to_string()).unwrap();
        assert_eq!(bridge.local_text(), "hello");
        assert!(
            bridge
                .set_local_text("x".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1))
                .is_err()
        );
        assert_eq!(bridge.local_text(), "hello");
    }
}
