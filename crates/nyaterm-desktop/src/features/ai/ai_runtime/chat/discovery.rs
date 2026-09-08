use futures::StreamExt as _;
use gpui::Context;

use crate::http::ai::discover_openai_compatible_models;
use nyaterm_core::AiModelDiscovery;

use crate::features::{NyaTermApp, runtime_jobs::AiDiscoveryJobResult};

impl NyaTermApp {
    pub(in crate::features) fn discover_ai_models(&mut self, cx: &mut Context<Self>) {
        if self.ai.discovery_is_pending() {
            self.ai
                .set_panel_status("AI model discovery already running".to_string());
            self.request_settings_panel_refresh(cx);
            self.defer_ai_panel_snapshot_flush(cx);
            return;
        }

        let (settings, credentials) = self.ai.discovery_settings();
        if credentials.is_empty() {
            self.ai.set_panel_status(
                "AI model discovery requires an enabled custom provider".to_string(),
            );
            self.request_settings_panel_refresh(cx);
            self.defer_ai_panel_snapshot_flush(cx);
            return;
        }

        let Some(tx) = self.ai.begin_discovery_job() else {
            self.request_settings_panel_refresh(cx);
            self.defer_ai_panel_snapshot_flush(cx);
            return;
        };
        self.request_settings_panel_refresh(cx);
        self.defer_ai_panel_snapshot_flush(cx);
        let rejected_tx = tx.clone();
        if let Err(error) = self
            .blocking_jobs
            .submit_detached("ai-model-discovery", move |_| {
                let mut discoveries = Vec::new();
                let mut errors = Vec::new();
                for credential in credentials {
                    match discover_openai_compatible_models(&settings, &credential) {
                        Ok(models) => discoveries.extend(models),
                        Err(error) => errors.push(format!("{}: {error}", credential.name)),
                    }
                }
                let result = if discoveries.is_empty() && !errors.is_empty() {
                    Err(errors.join("; "))
                } else {
                    Ok(discoveries)
                };
                let _ = tx.unbounded_send(AiDiscoveryJobResult {
                    profile_id: String::new(),
                    result,
                });
            })
        {
            let _ = rejected_tx.unbounded_send(AiDiscoveryJobResult {
                profile_id: String::new(),
                result: Err(error.to_string()),
            });
        }
        self.defer_ai_panel_snapshot_flush(cx);
    }

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
