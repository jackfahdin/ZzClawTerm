use gpui::Context;
use nyaterm_store::StoreDomain;

use crate::features::{NyaTermApp, formatting::compact_id, runtime_jobs::await_blocking_job};

use super::super::super::ai_jobs::{ai_active_profile_drafts, ai_usage_counts};

impl NyaTermApp {
    pub(in crate::features) fn sync_ai_drafts_from_active_profile(&mut self) {
        let (model, base_url) = ai_active_profile_drafts(self.ai.settings_config());
        self.ai.sync_settings_active_profile_drafts(model, base_url);
    }

    pub(in crate::features) fn refresh_ai_session_list(&mut self, cx: &mut Context<Self>) {
        let Some(job_id) = self.begin_ai_history_operation("loading AI history", cx) else {
            return;
        };
        let store = self.store_blocking_client();
        let task = self.blocking_jobs.submit_task("ai-history-list", move |_| {
            store
                .request_fn(StoreDomain::Ai, |store| store.list_ai_sessions())
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if this.ai.finish_history_session_list(job_id, result) {
                    this.defer_ai_panel_snapshot_flush(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::features) fn start_new_ai_chat(&mut self, cx: &mut Context<Self>) {
        self.ai.start_new_chat();
        // The composer keeps its own buffer, so clearing the draft is not enough.
        self.reset_text_input("ai.chat.prompt", "", cx);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn load_ai_session_messages(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(job_id) = self.begin_ai_history_operation("loading AI session", cx) else {
            return;
        };
        let source_session_id = self.ai.chat_session_id().to_string();
        let store = self.store_blocking_client();
        let job_session_id = session_id.clone();
        let task = self
            .blocking_jobs
            .submit_task("ai-history-messages", move |_| {
                store
                    .request_fn(StoreDomain::Ai, move |store| {
                        store.list_ai_messages(&job_session_id)
                    })
                    .map_err(|error| error.to_string())
            });
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                let loaded_status = format!("loaded AI session {}", compact_id(&session_id));
                if this.ai.finish_history_message_load(
                    job_id,
                    &source_session_id,
                    session_id,
                    result,
                    loaded_status,
                ) {
                    this.defer_ai_panel_snapshot_flush(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::features) fn delete_ai_session(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(job_id) = self.begin_ai_history_operation("deleting AI session", cx) else {
            return;
        };
        let store = self.store_blocking_client();
        let job_session_id = session_id.clone();
        let task = self
            .blocking_jobs
            .submit_task("ai-history-delete", move |_| {
                store
                    .request_fn(StoreDomain::Ai, move |store| {
                        store.delete_ai_session(&job_session_id)
                    })
                    .map_err(|error| error.to_string())
            });
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if let Some(succeeded) =
                    this.ai
                        .finish_history_session_delete(job_id, &session_id, result)
                {
                    if succeeded {
                        this.refresh_ai_usage_counts(cx);
                    }
                    this.defer_ai_panel_snapshot_flush(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::features) fn clear_all_ai_history(&mut self, cx: &mut Context<Self>) {
        let Some(job_id) = self.begin_ai_history_operation("clearing AI history", cx) else {
            return;
        };
        let source_session_id = self.ai.chat_session_id().to_string();
        let store = self.store_blocking_client();
        let task = self
            .blocking_jobs
            .submit_task("ai-history-clear", move |_| {
                store
                    .request_fn(StoreDomain::Ai, |store| store.clear_ai_history())
                    .map_err(|error| error.to_string())
            });
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if let Some(succeeded) =
                    this.ai
                        .finish_history_clear(job_id, &source_session_id, result)
                {
                    if succeeded {
                        this.refresh_ai_usage_counts(cx);
                    }
                    this.defer_ai_panel_snapshot_flush(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::features) fn apply_ai_history_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.set_history_query(text);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn refresh_ai_usage_counts(&mut self, cx: &mut Context<Self>) {
        let job_id = self.ai.begin_history_usage_count_job();
        let store = self.store_blocking_client();
        let task = self.blocking_jobs.submit_task("ai-usage-counts", move |_| {
            store
                .request_fn(StoreDomain::Ai, |store| Ok(ai_usage_counts(store)))
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = await_blocking_job(task).await.and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if this.ai.finish_history_usage_counts(job_id, result) {
                    this.defer_ai_panel_snapshot_flush(cx);
                }
            });
        })
        .detach();
    }

    fn begin_ai_history_operation(
        &mut self,
        status: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let job_id = self.ai.begin_history_operation(status);
        self.defer_ai_panel_snapshot_flush(cx);
        job_id
    }
}
