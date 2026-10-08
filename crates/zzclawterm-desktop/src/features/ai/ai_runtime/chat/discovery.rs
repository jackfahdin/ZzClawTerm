use futures::StreamExt as _;
use gpui::Context;

use zzclawterm_core::AiModelDiscovery;

use crate::features::{ZzClawTermApp, runtime_jobs::AiDiscoveryJobResult};

impl ZzClawTermApp {
    /// Deliver AI model-discovery replies as they arrive.
    ///
    /// Started once at window open; before this the runtime tick polled for them.
    pub(in crate::features) fn start_ai_discovery_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut rx) = self.ai.take_discovery_event_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| {
                        this.ai.note_discovery_event_delivered();
                        this.apply_ai_discovery_event(event, cx);
                        this.request_settings_panel_refresh(cx);
                        this.defer_ai_panel_snapshot_flush(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_ai_discovery_event(&mut self, event: AiDiscoveryJobResult, cx: &mut Context<Self>) {
        match event.result {
            Ok(discoveries) if discoveries.is_empty() => {
                self.ai
                    .set_panel_status("AI discovery returned no models".to_string());
            }
            Ok(discoveries) => {
                let count = self.apply_ai_model_discoveries(&event.profile_id, discoveries);
                self.ai
                    .set_panel_status(format!("Discovered {count} AI model(s)"));
                self.settings
                    .update_store_status(self.ai.panel_status().to_string(), true);
                self.persist_ai_settings_now(cx);
            }
            Err(error) => {
                self.ai
                    .set_panel_status(format!("AI model discovery failed: {error}"));
                self.settings
                    .update_store_status(self.ai.panel_status().to_string(), false);
            }
        }
    }

    pub(in crate::features) fn apply_ai_model_discoveries(
        &mut self,
        _profile_id: &str,
        discoveries: Vec<AiModelDiscovery>,
    ) -> usize {
        self.ai.apply_settings_model_discoveries(discoveries)
    }
}
