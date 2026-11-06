use crate::features::shell::tab_mouse::TabActionTarget;

use super::{
    PendingSessionStart, SessionFeatureState, SessionStartEndReason, SessionStartTabPlacement,
};

fn matches_placement(
    target: &TabActionTarget,
    placement: Option<SessionStartTabPlacement>,
) -> bool {
    matches!((target, placement),
        (TabActionTarget::Start { sequence, allocated_at }, Some(placement))
            if *sequence == placement.request_sequence && *allocated_at == placement.requested_at)
}

impl SessionFeatureState {
    pub(in crate::features) fn start_request_cancelled(&self, request_id: &str) -> bool {
        self.start
            .failed
            .get(request_id)
            .is_some_and(|failed| failed.reason == SessionStartEndReason::Cancelled)
    }

    pub(in crate::features) fn start_request_target(
        &self,
        request_id: &str,
    ) -> Option<TabActionTarget> {
        let pending = self.start.pending.get(request_id).or_else(|| {
            self.start
                .failed
                .get(request_id)
                .map(|failed| &failed.pending)
        })?;
        let placement = pending.tab_placement?;
        Some(TabActionTarget::Start {
            sequence: placement.request_sequence,
            allocated_at: placement.requested_at,
        })
    }

    pub(in crate::features) fn start_action_request(
        &self,
        target: &TabActionTarget,
    ) -> Option<(&str, &PendingSessionStart, Option<SessionStartEndReason>)> {
        self.start
            .pending
            .iter()
            .filter(|(_, pending)| pending.reconnect_session_id.is_none())
            .find(|(_, pending)| matches_placement(target, pending.tab_placement))
            .map(|(id, pending)| (id.as_str(), pending, None))
            .or_else(|| {
                self.start
                    .failed
                    .iter()
                    .find(|(_, failed)| matches_placement(target, failed.pending.tab_placement))
                    .map(|(id, failed)| (id.as_str(), &failed.pending, Some(failed.reason)))
            })
    }

    pub(in crate::features) fn resolved_tab_action_session(
        &self,
        target: &TabActionTarget,
    ) -> Option<String> {
        match target {
            TabActionTarget::Session(id) => self.has_session(id).then(|| id.clone()),
            TabActionTarget::Start { .. } => self
                .start_tab_placements
                .iter()
                .find(|(id, placement)| {
                    self.has_session(id) && matches_placement(target, Some(**placement))
                })
                .map(|(id, _)| id.clone()),
        }
    }

    pub(in crate::features) fn tab_action_display_name(
        &self,
        target: &TabActionTarget,
    ) -> Option<String> {
        if let Some((_, pending, _)) = self.start_action_request(target) {
            return Some(super::pending_session_start_display_name(pending));
        }
        self.display_name(&self.resolved_tab_action_session(target)?)
    }

    pub(in crate::features) fn rename_tab_target(
        &mut self,
        target: &TabActionTarget,
        name: String,
    ) -> bool {
        if let Some(pending) = self
            .start
            .pending
            .values_mut()
            .chain(
                self.start
                    .failed
                    .values_mut()
                    .map(|failed| &mut failed.pending),
            )
            .find(|pending| matches_placement(target, pending.tab_placement))
        {
            pending.custom_name = Some(name);
            return true;
        }
        if let Some(id) = self.resolved_tab_action_session(target) {
            self.set_custom_name(id, name);
            return true;
        }
        false
    }
}
