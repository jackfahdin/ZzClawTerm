use crate::features::NyaTermApp;

impl NyaTermApp {
    pub(in crate::features) fn sync_session_event_bridge_config(&self) {
        self.session.configure_event_bridge(
            self.settings.summary().interaction_default_encoding.clone(),
            self.terminal_scrollback_line_limit(),
        );
    }

    pub(in crate::features) fn sync_session_event_bridge_routing(&self) {
        let mut session_ids = self
            .session
            .session_ids()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if let Some(active_session_id) = self.session.active_id_owned()
            && !session_ids.contains(&active_session_id)
        {
            session_ids.push(active_session_id);
        }
        for session_id in session_ids {
            self.sync_session_event_bridge_session_policy(&session_id);
        }
    }

    pub(in crate::features) fn sync_session_event_bridge_policy(&self) {
        self.sync_session_event_bridge_config();
        self.sync_session_event_bridge_routing();
    }

    pub(in crate::features) fn sync_session_event_bridge_session_policy(&self, session_id: &str) {
        if self.session_has_active_ai_capture(session_id)
            || !self.session_sideband_detectors_idle(session_id)
        {
            self.session.route_session_events_to_ui(session_id);
        } else {
            self.session.resume_session_direct_output(session_id);
        }
    }
}
