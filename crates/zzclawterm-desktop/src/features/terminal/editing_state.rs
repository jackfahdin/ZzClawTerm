//! Session-owned shell editing state, independent of suggestion visibility.

use std::collections::HashMap;

use zzclawterm_core::terminal::editing::{EditCapability, ShellEditState};
use zzclawterm_core::terminal::input_tracker::TerminalInputState;
use zzclawterm_terminal::editing::InputMapping;
use zzclawterm_terminal::{ShellInputAnchor, TerminalLineId, TerminalSnapshot};

use crate::models::TerminalSelection;

#[derive(Default)]
pub(super) struct TerminalEditingState {
    active: String,
    pub(super) sessions: HashMap<String, SessionEditingState>,
}

#[derive(Default)]
pub(super) struct SessionEditingState {
    pub(super) command_navigation_interactive: bool,
    pub(super) model: ShellEditState,
    pub(super) capability: EditCapability,
    pub(super) echo_line_editor: bool,
    pub(super) launched_line_editor: bool,
    pub(super) line_editor_prompt: Option<String>,
    pub(super) mapping: Option<InputMapping>,
    pub(super) selection: Option<(TerminalSelection, u64)>,
    pub(super) selection_origin_version: Option<u64>,
    pub(super) geometry: Option<(usize, usize, u64)>,
    pub(super) observed_anchor: Option<Option<ShellInputAnchor>>,
    pub(super) submitted_anchor: Option<ShellInputAnchor>,
    pub(super) confirmation_task: Option<gpui::Task<()>>,
    pub(super) pending_evidence: Option<SnapshotEvidence>,
    pub(super) awaiting_snapshot: bool,
}

#[derive(PartialEq, Eq)]
pub(super) struct SnapshotEvidence {
    cursor: (usize, usize),
    anchor: Option<ShellInputAnchor>,
    rows: Vec<(Option<TerminalLineId>, u64)>,
}

impl SnapshotEvidence {
    pub(super) fn from_snapshot(snapshot: &TerminalSnapshot) -> Self {
        Self {
            cursor: (snapshot.cursor.row, snapshot.cursor.col),
            anchor: snapshot.shell_input_anchor,
            rows: snapshot
                .rows()
                .iter()
                .map(|row| (row.line_id, row.revision))
                .collect(),
        }
    }
}

impl SessionEditingState {
    pub(super) fn allows_editing(&self) -> bool {
        self.capability.allows_editing(self.echo_line_editor)
    }

    fn deactivate(&mut self) {
        self.model.deactivate(self.allows_editing());
        self.invalidate();
    }

    pub(super) fn invalidate(&mut self) {
        self.model.invalidate();
        self.capability = EditCapability::None;
        self.mapping = None;
        self.selection = None;
        self.selection_origin_version = None;
        self.pending_evidence = None;
        self.confirmation_task = None;
    }
}

impl TerminalEditingState {
    pub(super) fn activate(&mut self, session_id: &str) {
        if self.active != session_id {
            if let Some(state) = self.sessions.get_mut(&self.active) {
                state.deactivate();
            }
            self.active = session_id.to_string();
            self.state_mut().invalidate();
        }
    }

    pub(super) fn state(&self) -> Option<&SessionEditingState> {
        self.sessions.get(&self.active)
    }
    pub(super) fn state_mut(&mut self) -> &mut SessionEditingState {
        self.sessions.entry(self.active.clone()).or_default()
    }

    pub(super) fn predicted_input(&self) -> &TerminalInputState {
        static EMPTY: std::sync::OnceLock<TerminalInputState> = std::sync::OnceLock::new();
        self.state().map_or_else(
            || EMPTY.get_or_init(TerminalInputState::new),
            |state| &state.model.input,
        )
    }

    pub(super) fn assist_input(&self) -> &TerminalInputState {
        static UNAVAILABLE: std::sync::OnceLock<TerminalInputState> = std::sync::OnceLock::new();
        self.state()
            .and_then(|state| state.model.assist_input())
            .unwrap_or_else(|| {
                UNAVAILABLE.get_or_init(|| TerminalInputState {
                    desynced: true,
                    desync_reason: Some("unconfirmed_input"),
                    ..TerminalInputState::new()
                })
            })
    }

    /// Compatibility adapter for command suggestion replacement. Invalidates any
    /// old mapping before handing out mutable input, so it cannot authorize edits.
    pub(super) fn input_mut(&mut self) -> &mut TerminalInputState {
        let state = self.state_mut();
        state.invalidate();
        state.model.clear_input();
        &mut state.model.input
    }

    pub(super) fn clear_active(&mut self) {
        let awaiting_snapshot = self.state_mut().awaiting_snapshot;
        *self.state_mut() = SessionEditingState {
            awaiting_snapshot,
            ..SessionEditingState::default()
        };
    }
}

impl super::state::TerminalFeatureState {
    pub(in crate::features) fn activate_shell_editing_session(&mut self, session_id: &str) {
        self.editing.activate(session_id);
    }

    pub(in crate::features) fn invalidate_shell_editing_session(&mut self, session_id: &str) {
        if let Some(state) = self.editing.sessions.get_mut(session_id) {
            state.invalidate();
            state.model.clear_input();
            state.model.input.desynced = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalEditingState;
    use std::time::Instant;
    use zzclawterm_core::terminal::editing::{EditCapability, EditPhase};

    #[test]
    fn switching_sessions_retains_only_confirmed_editable_input() {
        let mut state = TerminalEditingState::default();
        state.activate("one");
        state.state_mut().model.note_input("abc", Instant::now());
        state.state_mut().model.confirm();
        state.state_mut().capability = EditCapability::MarkedShell;
        state.state_mut().model.queued_cursor = Some(1);
        state.activate("two");
        state.state_mut().model.note_input("xyz", Instant::now());
        state.activate("one");
        assert_eq!(state.predicted_input().value, "abc");
        assert_eq!(state.state().unwrap().model.phase, EditPhase::Uncertain);
        assert_eq!(state.state().unwrap().model.queued_cursor, None);
        assert!(state.sessions["two"].model.input.value.is_empty());
        assert!(state.assist_input().desynced);
        state.state_mut().model.confirm();
        assert_eq!(state.assist_input().value, "abc");
    }
}
