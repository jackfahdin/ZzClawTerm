use std::collections::HashSet;

use gpui::{Context, FocusHandle, Window};
use nyaterm_core::uuid;
use nyaterm_transport::SessionInfo;

use super::NyaTermApp;
use crate::models::SyncInputGroup;

const GROUP_COLORS: [u32; 8] = [
    0x3b82f6, 0xef4444, 0x22c55e, 0xf59e0b, 0x8b5cf6, 0xec4899, 0x06b6d4, 0xf97316,
];

pub(in crate::features) struct SyncInputFeatureState {
    groups: Vec<SyncInputGroup>,
    open: bool,
    focus: FocusHandle,
    search_draft: String,
    selected_id: Option<String>,
    delete_pending: Option<String>,
    broadcast_to_all: bool,
}

pub(in crate::features) enum SyncSessionPauseResult {
    Paused,
    Resumed,
    NotMember,
    NoGroup,
}

impl SyncInputFeatureState {
    pub(in crate::features) fn new(focus: FocusHandle) -> Self {
        Self {
            groups: Vec::new(),
            open: false,
            focus,
            search_draft: String::new(),
            selected_id: None,
            delete_pending: None,
            broadcast_to_all: false,
        }
    }

    pub(in crate::features) fn open(&mut self) {
        self.open = true;
        self.search_draft.clear();
        self.delete_pending = None;
        if self.selected_id.is_none() {
            self.selected_id = self.groups.first().map(|group| group.id.clone());
        }
    }

    pub(in crate::features) fn groups(&self) -> &[SyncInputGroup] {
        &self.groups
    }

    pub(in crate::features) fn is_open(&self) -> bool {
        self.open
    }

    pub(in crate::features) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(in crate::features) fn search_draft(&self) -> &str {
        &self.search_draft
    }

    pub(in crate::features) fn has_delete_pending(&self) -> bool {
        self.delete_pending.is_some()
    }

    pub(in crate::features) fn pending_delete_group(&self) -> Option<&SyncInputGroup> {
        self.delete_pending
            .as_deref()
            .and_then(|id| self.groups.iter().find(|group| group.id == id))
    }

    pub(in crate::features) fn broadcast_to_all(&self) -> bool {
        self.broadcast_to_all
    }

    pub(in crate::features) fn close(&mut self) {
        self.open = false;
        self.search_draft.clear();
        self.delete_pending = None;
    }

    pub(in crate::features) fn create_group(&mut self) {
        let index = self.groups.len();
        let group = SyncInputGroup {
            id: format!("sync-group-{}", uuid()),
            name: format!("Sync Group {}", index + 1),
            color: self.next_group_color(),
            session_ids: Vec::new(),
            paused_session_ids: Vec::new(),
            enabled: true,
        };
        self.selected_id = Some(group.id.clone());
        self.groups.push(group);
    }

    pub(in crate::features) fn delete_selected(&mut self) -> bool {
        let Some(group_id) = self.selected_id.clone() else {
            return false;
        };
        self.groups.retain(|group| group.id != group_id);
        self.selected_id = self.groups.first().map(|group| group.id.clone());
        true
    }

    pub(in crate::features) fn request_delete_selected(&mut self) -> bool {
        let Some(group_id) = self.selected_id.clone() else {
            return false;
        };
        self.delete_pending = Some(group_id);
        true
    }

    pub(in crate::features) fn confirm_delete(&mut self) -> bool {
        let Some(group_id) = self.delete_pending.take() else {
            return false;
        };
        self.selected_id = Some(group_id);
        self.delete_selected()
    }

    pub(in crate::features) fn cancel_delete(&mut self) {
        self.delete_pending = None;
    }

    pub(in crate::features) fn set_search(&mut self, text: String) {
        self.search_draft = text;
    }

    pub(in crate::features) fn rename_group(&mut self, group_id: &str, text: String) -> bool {
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return false;
        };
        group.name = text;
        true
    }

    pub(in crate::features) fn select(&mut self, group_id: String) -> bool {
        if !self.groups.iter().any(|group| group.id == group_id) {
            return false;
        }
        self.selected_id = Some(group_id);
        true
    }

    pub(in crate::features) fn selected_group(&self) -> Option<&SyncInputGroup> {
        self.selected_id
            .as_deref()
            .and_then(|id| self.groups.iter().find(|group| group.id == id))
    }

    fn selected_group_mut(&mut self) -> Option<&mut SyncInputGroup> {
        let selected_id = self.selected_id.clone()?;
        self.groups.iter_mut().find(|group| group.id == selected_id)
    }

    pub(in crate::features) fn replace_selected_sessions(
        &mut self,
        session_ids: Vec<String>,
    ) -> bool {
        let Some(group) = self.selected_group_mut() else {
            return false;
        };
        group.session_ids = session_ids;
        group.paused_session_ids.clear();
        true
    }

    pub(in crate::features) fn clear_selected_sessions(&mut self) -> bool {
        self.replace_selected_sessions(Vec::new())
    }

    pub(in crate::features) fn add_selected_sessions(&mut self, session_ids: Vec<String>) -> bool {
        let Some(group) = self.selected_group_mut() else {
            return false;
        };
        for session_id in session_ids {
            if !group.session_ids.iter().any(|id| id == &session_id) {
                group.session_ids.push(session_id);
            }
        }
        group.paused_session_ids.clear();
        true
    }

    pub(in crate::features) fn remove_selected_sessions(
        &mut self,
        remove_ids: &HashSet<String>,
    ) -> bool {
        let Some(group) = self.selected_group_mut() else {
            return false;
        };
        group.session_ids.retain(|id| !remove_ids.contains(id));
        group
            .paused_session_ids
            .retain(|id| !remove_ids.contains(id));
        true
    }

    pub(in crate::features) fn toggle_selected_enabled(&mut self) -> Option<bool> {
        let group = self.selected_group_mut()?;
        group.enabled = !group.enabled;
        Some(group.enabled)
    }

    /// Returns `Some(true)` when added and `Some(false)` when removed.
    pub(in crate::features) fn toggle_selected_session(
        &mut self,
        session_id: String,
    ) -> Option<bool> {
        let group = self.selected_group_mut()?;
        if group.session_ids.iter().any(|id| id == &session_id) {
            group.session_ids.retain(|id| id != &session_id);
            group.paused_session_ids.retain(|id| id != &session_id);
            Some(false)
        } else {
            group.session_ids.push(session_id);
            Some(true)
        }
    }

    pub(in crate::features) fn toggle_selected_session_paused(
        &mut self,
        session_id: String,
    ) -> SyncSessionPauseResult {
        let Some(group) = self.selected_group_mut() else {
            return SyncSessionPauseResult::NoGroup;
        };
        if !group.session_ids.iter().any(|id| id == &session_id) {
            return SyncSessionPauseResult::NotMember;
        }
        if group.paused_session_ids.iter().any(|id| id == &session_id) {
            group.paused_session_ids.retain(|id| id != &session_id);
            SyncSessionPauseResult::Resumed
        } else {
            group.paused_session_ids.push(session_id);
            SyncSessionPauseResult::Paused
        }
    }

    pub(in crate::features) fn active_group_for_session(
        &self,
        session_id: &str,
    ) -> Option<&SyncInputGroup> {
        self.groups
            .iter()
            .find(|group| group.enabled && group.session_ids.iter().any(|id| id == session_id))
    }

    pub(in crate::features) fn session_is_paused_in_active_group(&self, session_id: &str) -> bool {
        self.active_group_for_session(session_id)
            .is_some_and(|group| group.paused_session_ids.iter().any(|id| id == session_id))
    }

    pub(in crate::features) fn peer_session_ids(
        &self,
        session_id: &str,
        live_ids: &HashSet<String>,
    ) -> Vec<String> {
        let mut peers = HashSet::new();
        for group in &self.groups {
            if !group.enabled
                || !group.session_ids.iter().any(|id| id == session_id)
                || group.paused_session_ids.iter().any(|id| id == session_id)
            {
                continue;
            }
            for peer_id in &group.session_ids {
                if peer_id != session_id
                    && live_ids.contains(peer_id)
                    && !group.paused_session_ids.iter().any(|id| id == peer_id)
                {
                    peers.insert(peer_id.clone());
                }
            }
        }
        if self.broadcast_to_all {
            for peer_id in live_ids {
                if peer_id != session_id {
                    peers.insert(peer_id.clone());
                }
            }
        }
        let mut peers = peers.into_iter().collect::<Vec<_>>();
        peers.sort();
        peers
    }

    pub(in crate::features) fn may_fan_out_from(&self, session_id: &str) -> bool {
        self.broadcast_to_all
            || self.groups.iter().any(|group| {
                group.enabled
                    && group.session_ids.iter().any(|id| id == session_id)
                    && !group.paused_session_ids.iter().any(|id| id == session_id)
            })
    }

    pub(in crate::features) fn toggle_broadcast_to_all(&mut self) -> bool {
        self.broadcast_to_all = !self.broadcast_to_all;
        self.broadcast_to_all
    }

    pub(in crate::features) fn toggle_active_session_paused(
        &mut self,
        session_id: String,
    ) -> SyncSessionPauseResult {
        let Some(group_id) = self
            .active_group_for_session(&session_id)
            .map(|group| group.id.clone())
        else {
            return SyncSessionPauseResult::NoGroup;
        };
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return SyncSessionPauseResult::NoGroup;
        };
        if group.paused_session_ids.iter().any(|id| id == &session_id) {
            group.paused_session_ids.retain(|id| id != &session_id);
            SyncSessionPauseResult::Resumed
        } else {
            group.paused_session_ids.push(session_id);
            SyncSessionPauseResult::Paused
        }
    }

    pub(in crate::features) fn leave_active_group(&mut self, session_id: &str) -> bool {
        let Some(group_id) = self
            .active_group_for_session(session_id)
            .map(|group| group.id.clone())
        else {
            return false;
        };
        if let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) {
            group.session_ids.retain(|id| id != session_id);
            group.paused_session_ids.retain(|id| id != session_id);
        }
        self.groups.retain(|group| !group.session_ids.is_empty());
        self.repair_selection();
        true
    }

    pub(in crate::features) fn close_active_group(&mut self, session_id: &str) -> bool {
        let Some(group_id) = self
            .active_group_for_session(session_id)
            .map(|group| group.id.clone())
        else {
            return false;
        };
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return false;
        };
        group.enabled = false;
        true
    }

    pub(in crate::features) fn purge_session(&mut self, session_id: &str) {
        for group in &mut self.groups {
            group.session_ids.retain(|id| id != session_id);
            group.paused_session_ids.retain(|id| id != session_id);
        }
        self.groups.retain(|group| !group.session_ids.is_empty());
        self.repair_selection();
    }

    pub(in crate::features) fn replace_session_id(&mut self, old_id: &str, new_id: &str) {
        for group in &mut self.groups {
            for session_id in &mut group.session_ids {
                if session_id == old_id {
                    *session_id = new_id.to_string();
                }
            }
            if group.paused_session_ids.iter().any(|id| id == old_id) {
                group.paused_session_ids.retain(|id| id != old_id);
                if !group.paused_session_ids.iter().any(|id| id == new_id) {
                    group.paused_session_ids.push(new_id.to_string());
                }
            }
        }
    }

    fn repair_selection(&mut self) {
        if self
            .selected_id
            .as_deref()
            .is_some_and(|id| !self.groups.iter().any(|group| group.id == id))
        {
            self.selected_id = self.groups.first().map(|group| group.id.clone());
        }
    }

    fn next_group_color(&self) -> u32 {
        GROUP_COLORS
            .iter()
            .copied()
            .find(|color| self.groups.iter().all(|group| group.color != *color))
            .unwrap_or(GROUP_COLORS[self.groups.len() % GROUP_COLORS.len()])
    }
}

impl NyaTermApp {
    pub(in crate::features) fn open_sync_groups(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_input.open();
        self.forget_text_inputs("sync.groups.search");
        self.forget_text_inputs("sync.group-name.");
        self.shell.set_status("sync groups opened".to_string());
        window.focus(self.sync_input.focus(), cx);
        cx.notify();
    }

    pub(in crate::features) fn close_sync_groups(&mut self, cx: &mut Context<Self>) {
        self.sync_input.close();
        self.forget_text_inputs("sync.groups.search");
        self.forget_text_inputs("sync.group-name.");
        self.shell.set_status("sync groups closed".to_string());
        cx.notify();
    }

    pub(in crate::features) fn create_sync_group(&mut self, cx: &mut Context<Self>) {
        self.sync_input.create_group();
        self.shell.set_status("sync group created".to_string());
        cx.notify();
    }

    pub(in crate::features) fn request_delete_selected_sync_group(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.sync_input.request_delete_selected() {
            self.shell.set_status("no sync group selected".to_string());
            cx.notify();
            return;
        }
        cx.notify();
    }

    pub(in crate::features) fn cancel_delete_sync_group(&mut self, cx: &mut Context<Self>) {
        self.sync_input.cancel_delete();
        cx.notify();
    }

    pub(in crate::features) fn confirm_delete_sync_group(&mut self, cx: &mut Context<Self>) {
        if !self.sync_input.confirm_delete() {
            return;
        }
        self.shell.set_status("sync group deleted".to_string());
        cx.notify();
    }

    pub(in crate::features) fn apply_sync_groups_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.sync_input.set_search(text);
        cx.notify();
    }

    pub(in crate::features) fn apply_sync_group_name(
        &mut self,
        group_id: &str,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if self.sync_input.rename_group(group_id, text) {
            cx.notify();
        }
    }

    pub(in crate::features) fn select_all_sync_group_sessions(&mut self, cx: &mut Context<Self>) {
        let session_ids = self
            .session
            .ordered_sessions()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        if self.sync_input.replace_selected_sessions(session_ids) {
            cx.notify();
        }
    }

    pub(in crate::features) fn clear_sync_group_sessions(&mut self, cx: &mut Context<Self>) {
        if self.sync_input.clear_selected_sessions() {
            cx.notify();
        }
    }

    pub(in crate::features) fn add_filtered_sync_group_sessions(&mut self, cx: &mut Context<Self>) {
        let query = self.sync_input.search_draft().trim().to_ascii_lowercase();
        let session_ids = self
            .session
            .ordered_sessions()
            .into_iter()
            .filter(|session| self.sync_group_session_matches_search(session, &query))
            .map(|session| session.id)
            .collect::<Vec<_>>();
        if self.sync_input.add_selected_sessions(session_ids) {
            cx.notify();
        }
    }

    pub(in crate::features) fn remove_filtered_sync_group_sessions(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let query = self.sync_input.search_draft().trim().to_ascii_lowercase();
        let remove_ids = self
            .session
            .ordered_sessions()
            .into_iter()
            .filter(|session| self.sync_group_session_matches_search(session, &query))
            .map(|session| session.id)
            .collect::<HashSet<_>>();
        if self.sync_input.remove_selected_sessions(&remove_ids) {
            cx.notify();
        }
    }

    pub(in crate::features) fn select_same_host_sync_group_sessions(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let selected_ids = self
            .sync_input
            .selected_group()
            .map(|group| group.session_ids.clone())
            .unwrap_or_default();
        let selected_hosts = selected_ids
            .iter()
            .filter_map(|id| self.session.ssh_host(id))
            .collect::<HashSet<_>>();
        if selected_hosts.is_empty() {
            return;
        }
        let matching_ids = self
            .session
            .ordered_sessions()
            .into_iter()
            .filter(|session| {
                self.session
                    .ssh_host(&session.id)
                    .is_some_and(|host| selected_hosts.contains(&host))
            })
            .map(|session| session.id)
            .collect::<Vec<_>>();
        if self.sync_input.replace_selected_sessions(matching_ids) {
            cx.notify();
        }
    }

    pub(in crate::features) fn sync_group_session_matches_search(
        &self,
        session: &SessionInfo,
        query: &str,
    ) -> bool {
        if query.is_empty() {
            return true;
        }
        let endpoint = self.session.endpoint(&session.id).unwrap_or_default();
        let host = self.session.ssh_host(&session.id).unwrap_or_default();
        format!(
            "{} {:?} {} {} {}",
            self.session.display_name_by_info(session),
            session.kind,
            endpoint,
            host,
            session.id
        )
        .to_ascii_lowercase()
        .contains(query)
    }

    pub(in crate::features) fn select_sync_group(
        &mut self,
        group_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.sync_input.select(group_id) {
            self.shell.set_status("sync group selected".to_string());
            cx.notify();
        }
    }

    pub(in crate::features) fn toggle_selected_sync_group_enabled(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(enabled) = self.sync_input.toggle_selected_enabled() else {
            self.shell.set_status("no sync group selected".to_string());
            cx.notify();
            return;
        };
        self.shell.set_status(if enabled {
            "sync group enabled".to_string()
        } else {
            "sync group disabled".to_string()
        });
        cx.notify();
    }

    pub(in crate::features) fn toggle_session_in_selected_sync_group(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(added) = self.sync_input.toggle_selected_session(session_id) else {
            self.shell
                .set_status("create or select a sync group first".to_string());
            cx.notify();
            return;
        };
        if added {
            self.shell
                .set_status("session added to sync group".to_string());
        } else {
            self.shell
                .set_status("session removed from sync group".to_string());
        }
        cx.notify();
    }

    pub(in crate::features) fn toggle_session_paused_in_selected_sync_group(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        self.shell.set_status(
            match self.sync_input.toggle_selected_session_paused(session_id) {
                SyncSessionPauseResult::Paused => "session sync paused",
                SyncSessionPauseResult::Resumed => "session sync resumed",
                SyncSessionPauseResult::NotMember => "session is not in the selected sync group",
                SyncSessionPauseResult::NoGroup => "create or select a sync group first",
            }
            .to_string(),
        );
        cx.notify();
    }

    pub(in crate::features) fn sync_peer_session_ids(&self, session_id: &str) -> Vec<String> {
        if !self.sync_input.may_fan_out_from(session_id) {
            return Vec::new();
        }
        let live_ids = self
            .session
            .metadata_entries()
            .filter(|(_, metadata)| !metadata.disconnected)
            .map(|(session_id, _)| session_id.to_string())
            .collect::<HashSet<_>>();
        self.sync_input.peer_session_ids(session_id, &live_ids)
    }

    pub(in crate::features) fn toggle_broadcast_to_all(&mut self, cx: &mut Context<Self>) {
        self.shell
            .set_status(if self.sync_input.toggle_broadcast_to_all() {
                "broadcast to all sessions enabled".to_string()
            } else {
                "broadcast to all sessions disabled".to_string()
            });
        cx.notify();
    }

    /// Pause/resume the current session inside its active enabled sync group.
    pub(in crate::features) fn toggle_session_paused_in_active_sync_group(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        self.shell.set_status(
            match self.sync_input.toggle_active_session_paused(session_id) {
                SyncSessionPauseResult::Paused => "session sync paused",
                SyncSessionPauseResult::Resumed => "session sync resumed",
                SyncSessionPauseResult::NoGroup | SyncSessionPauseResult::NotMember => {
                    "session is not in an active sync group"
                }
            }
            .to_string(),
        );
        cx.notify();
    }

    /// Remove session from its active enabled sync group (Tauri Leave).
    pub(in crate::features) fn leave_active_sync_group(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.sync_input.leave_active_group(&session_id) {
            self.shell
                .set_status("session is not in an active sync group".to_string());
            cx.notify();
            return;
        }
        self.shell.set_status("left sync group".to_string());
        cx.notify();
    }

    /// Disable the active enabled sync group without deleting it (Tauri Close Group).
    pub(in crate::features) fn close_active_sync_group_for_session(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.sync_input.close_active_group(&session_id) {
            self.shell
                .set_status("session is not in an active sync group".to_string());
            cx.notify();
            return;
        }
        self.shell.set_status("sync group closed".to_string());
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use gpui::TestAppContext;

    use super::SyncInputFeatureState;
    use crate::models::SyncInputGroup;

    fn state() -> SyncInputFeatureState {
        let cx = TestAppContext::single();
        let focus = cx.update(|cx| cx.focus_handle());
        SyncInputFeatureState::new(focus)
    }

    fn group(id: &str, name: &str) -> SyncInputGroup {
        SyncInputGroup {
            id: id.to_string(),
            name: name.to_string(),
            color: 0,
            session_ids: Vec::new(),
            paused_session_ids: Vec::new(),
            enabled: true,
        }
    }

    #[test]
    fn group_name_input_updates_only_its_addressed_group() {
        let mut state = state();
        state.groups = vec![group("one", "One"), group("two", "Two")];

        assert!(state.rename_group("two", "Renamed".to_string()));
        assert_eq!(state.groups()[0].name, "One");
        assert_eq!(state.groups()[1].name, "Renamed");
        assert!(!state.rename_group("missing", "Ignored".to_string()));
    }

    #[test]
    fn deleting_and_purging_groups_repairs_the_selected_group() {
        let mut state = state();
        let mut first = group("one", "One");
        first.session_ids = vec!["session-a".to_string()];
        let mut second = group("two", "Two");
        second.session_ids = vec!["session-b".to_string()];
        state.groups = vec![first, second];
        state.selected_id = Some("one".to_string());

        state.purge_session("session-a");
        assert_eq!(state.groups().len(), 1);
        assert_eq!(
            state.selected_group().map(|group| group.id.as_str()),
            Some("two")
        );

        assert!(state.request_delete_selected());
        assert!(state.confirm_delete());
        assert!(state.groups().is_empty());
        assert!(state.selected_group().is_none());
    }

    #[test]
    fn replacing_session_id_updates_membership_and_pause_state_together() {
        let mut state = state();
        let mut sync_group = group("one", "One");
        sync_group.session_ids = vec!["old".to_string(), "peer".to_string()];
        sync_group.paused_session_ids = vec!["old".to_string()];
        state.groups.push(sync_group);

        state.replace_session_id("old", "new");

        assert_eq!(state.groups()[0].session_ids, ["new", "peer"]);
        assert_eq!(state.groups()[0].paused_session_ids, ["new"]);
    }

    #[test]
    fn peer_selection_honors_pauses_and_broadcast_override() {
        let mut state = state();
        let mut sync_group = group("one", "One");
        sync_group.session_ids = vec![
            "primary".to_string(),
            "peer-a".to_string(),
            "peer-b".to_string(),
        ];
        sync_group.paused_session_ids = vec!["peer-b".to_string()];
        state.groups.push(sync_group);
        let live_ids = ["primary", "peer-a", "peer-b", "outside"]
            .into_iter()
            .map(str::to_string)
            .collect::<HashSet<_>>();

        assert_eq!(
            state.peer_session_ids("primary", &live_ids),
            vec!["peer-a".to_string()]
        );

        assert!(state.toggle_broadcast_to_all());
        assert_eq!(
            state.peer_session_ids("primary", &live_ids),
            vec![
                "outside".to_string(),
                "peer-a".to_string(),
                "peer-b".to_string()
            ]
        );
    }

    #[test]
    fn fan_out_fast_path_requires_an_active_unpaused_source_or_broadcast() {
        let mut state = state();
        assert!(!state.may_fan_out_from("primary"));

        let mut sync_group = group("one", "One");
        sync_group.session_ids = vec!["primary".to_string(), "peer".to_string()];
        sync_group.enabled = false;
        state.groups.push(sync_group);
        assert!(!state.may_fan_out_from("primary"));

        state.groups[0].enabled = true;
        assert!(state.may_fan_out_from("primary"));
        state.groups[0].paused_session_ids = vec!["primary".to_string()];
        assert!(!state.may_fan_out_from("primary"));

        assert!(state.toggle_broadcast_to_all());
        assert!(state.may_fan_out_from("primary"));
        assert!(state.may_fan_out_from("outside"));
    }
}
