use std::collections::HashMap;

use futures::channel::mpsc::UnboundedReceiver;
use zzclawterm_transport::RecordingStatus;

use crate::models::{
    RecordingHistorySearchKey, RecordingPathPromptKind, RecordingWriteEvent, RecordingWriteHandle,
    RecordingWritePipeline,
};

pub(in crate::features) struct RecordingFeatureState {
    active_count: usize,
    pending_auto_start: Option<(String, String)>,
    pipeline: RecordingWritePipeline,
    search_draft: String,
    busy_actions: HashMap<String, String>,
    rekeyed_sessions: HashMap<String, String>,
    statuses: HashMap<String, RecordingStatus>,
    path_prompt: Option<RecordingPathPromptKind>,
}

impl RecordingFeatureState {
    pub(in crate::features) fn new(memory_limit_bytes: usize) -> Self {
        let pipeline = RecordingWritePipeline::spawn(memory_limit_bytes);
        Self {
            active_count: 0,
            pending_auto_start: None,
            pipeline,
            search_draft: String::new(),
            busy_actions: HashMap::new(),
            rekeyed_sessions: HashMap::new(),
            statuses: HashMap::new(),
            path_prompt: None,
        }
    }

    pub(in crate::features) fn writer(&self) -> RecordingWriteHandle {
        self.pipeline.writer()
    }

    pub(in crate::features) fn shutdown_worker(&mut self) {
        self.pipeline.shutdown();
    }

    pub(in crate::features) fn set_memory_limit(&self, memory_limit_bytes: usize) {
        self.pipeline.set_memory_limit(memory_limit_bytes);
    }

    pub(in crate::features) fn active_count(&self) -> usize {
        self.active_count
    }

    pub(in crate::features) fn is_recording(&self, session_id: &str) -> bool {
        self.statuses.contains_key(session_id)
    }

    pub(in crate::features) fn status(&self, session_id: &str) -> Option<RecordingStatus> {
        self.statuses.get(session_id).cloned()
    }

    pub(in crate::features) fn apply_status(&mut self, status: RecordingStatus) {
        if matches!(
            status.state,
            zzclawterm_transport::RecordingStatusState::Recording
                | zzclawterm_transport::RecordingStatusState::Degraded
                | zzclawterm_transport::RecordingStatusState::Failed
        ) {
            self.statuses.insert(status.session_id.clone(), status);
        } else {
            self.statuses.remove(&status.session_id);
        }
        self.active_count = self.statuses.len();
    }

    pub(in crate::features) fn remove_status(&mut self, session_id: &str) {
        self.statuses.remove(session_id);
        self.active_count = self.statuses.len();
    }

    pub(in crate::features) fn busy_action(&self, session_id: &str) -> Option<&str> {
        self.busy_actions.get(session_id).map(String::as_str)
    }

    pub(in crate::features) fn begin_action(&mut self, session_id: &str, action: &str) -> bool {
        if self.busy_actions.contains_key(session_id) {
            return false;
        }
        self.busy_actions
            .insert(session_id.to_string(), action.to_string());
        true
    }

    pub(in crate::features) fn finish_action(&mut self, session_id: &str) {
        let current_id = self.current_session_id(session_id);
        self.busy_actions.remove(&current_id);
    }

    pub(in crate::features) fn current_session_id(&self, session_id: &str) -> String {
        let mut current = session_id;
        while let Some(next) = self.rekeyed_sessions.get(current) {
            current = next;
        }
        current.to_string()
    }

    pub(in crate::features) fn search_draft(&self) -> &str {
        &self.search_draft
    }

    pub(in crate::features) fn set_search_draft(&mut self, text: String) {
        self.search_draft = text;
    }

    pub(in crate::features) fn clear_search_draft(&mut self) {
        self.search_draft.clear();
    }

    pub(in crate::features) fn begin_path_prompt(&mut self, kind: RecordingPathPromptKind) -> bool {
        if self.path_prompt.is_some() {
            return false;
        }
        self.path_prompt = Some(kind);
        true
    }

    pub(in crate::features) fn finish_path_prompt(&mut self) {
        self.path_prompt = None;
    }

    pub(in crate::features) fn has_pending_auto_start(&self) -> bool {
        self.pending_auto_start.is_some()
    }

    pub(in crate::features) fn schedule_auto_start(
        &mut self,
        session_id: String,
        session_name: String,
    ) {
        self.pending_auto_start = Some((session_id, session_name));
    }

    pub(in crate::features) fn take_pending_auto_start(&mut self) -> Option<(String, String)> {
        self.pending_auto_start.take()
    }

    pub(in crate::features) fn cleanup_writer_session(&self, session_id: &str) {
        self.pipeline.cleanup_session(session_id.to_string());
    }

    pub(in crate::features) fn write_output(
        &self,
        session_id: impl Into<String>,
        text: impl Into<String>,
    ) {
        self.pipeline.write_output(session_id, text);
    }

    pub(in crate::features) fn write_input(
        &self,
        session_id: impl Into<String>,
        data: impl Into<Vec<u8>>,
    ) {
        self.pipeline.write_input(session_id, data);
    }

    pub(in crate::features) fn resync_input_line(
        &self,
        session_id: impl Into<String>,
        line: String,
    ) {
        self.pipeline.resync_input_line(session_id, line);
    }

    pub(in crate::features) fn write_raw_input(
        &self,
        session_id: impl Into<String>,
        data: impl Into<Vec<u8>>,
    ) {
        self.pipeline.write_raw_input(session_id, data);
    }

    pub(in crate::features) fn request_history_search(&self, key: RecordingHistorySearchKey) {
        self.pipeline.request_history_search(key);
    }

    pub(in crate::features) fn take_event_receiver(
        &mut self,
    ) -> Option<UnboundedReceiver<RecordingWriteEvent>> {
        self.pipeline.take_event_receiver()
    }

    pub(in crate::features) fn cleanup_session(&mut self, session_id: &str) {
        self.busy_actions.remove(session_id);
        self.statuses.remove(session_id);
        self.active_count = self.statuses.len();
        self.pipeline.cleanup_session(session_id.to_string());
    }

    pub(in crate::features) fn disconnect_session(&self, session_id: &str) {
        self.pipeline.disconnect_session(session_id.to_string());
    }

    pub(in crate::features) fn rekey_session(&mut self, old_id: &str, new_id: &str) {
        if old_id == new_id {
            return;
        }
        if let Some((session_id, _)) = self.pending_auto_start.as_mut()
            && session_id == old_id
        {
            *session_id = new_id.to_string();
        }
        if let Some(action) = self.busy_actions.remove(old_id) {
            self.busy_actions.insert(new_id.to_string(), action);
        }
        self.rekeyed_sessions
            .insert(old_id.to_string(), new_id.to_string());
        self.pipeline
            .rekey_session(old_id.to_string(), new_id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::RecordingFeatureState;

    #[test]
    fn recording_state_owns_runtime_and_session_cleanup_state() {
        let mut recording = RecordingFeatureState::new(1024);
        assert!(recording.begin_action("session-1", "record"));
        assert!(!recording.begin_action("session-1", "save"));
        recording.schedule_auto_start("session-1".to_string(), "local shell".to_string());
        assert!(recording.begin_path_prompt(crate::models::RecordingPathPromptKind::Start));
        assert!(!recording.begin_path_prompt(crate::models::RecordingPathPromptKind::Start));

        let _writer = recording.writer();
        recording.cleanup_session("session-1");

        assert_eq!(recording.active_count(), 0);
        assert!(recording.busy_action("session-1").is_none());
        assert_eq!(
            recording
                .take_pending_auto_start()
                .as_ref()
                .map(|value| value.0.as_str()),
            Some("session-1")
        );
        recording.finish_path_prompt();
        assert!(recording.begin_path_prompt(crate::models::RecordingPathPromptKind::Start));
    }

    #[test]
    fn reconnect_moves_pending_auto_start_and_busy_action_once() {
        let mut recording = RecordingFeatureState::new(1024);
        recording.schedule_auto_start("s1".to_string(), "shell".to_string());
        assert!(recording.begin_action("s1", "record"));
        recording.rekey_session("s1", "s2");
        assert_eq!(recording.busy_action("s2"), Some("record"));
        assert!(recording.busy_action("s1").is_none());
        assert_eq!(
            recording.take_pending_auto_start(),
            Some(("s2".into(), "shell".into()))
        );
        assert!(recording.take_pending_auto_start().is_none());
    }
}
