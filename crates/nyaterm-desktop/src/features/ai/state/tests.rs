use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui::{TestAppContext, px};
use nyaterm_core::{
    AiAction, AiContext, AiMessage, AiMessageRole, AiMode, AiModelConfigItem, AiModelSource,
    AiProviderCredential, AiProviderKind, AiSession, AiSettings,
};

use crate::features::{
    runtime_jobs::AiAgentLoopState, runtime_jobs::AiAgentStepStatus, runtime_jobs::AiChatJobOutput,
    runtime_jobs::AiChatWorkerEvent, runtime_jobs::AiDiscoveryJobResult,
};
use crate::models::{AiMessageMenuState, AiPreparedRequest};

use super::{AiFeatureFocus, AiFeatureInit, AiFeatureState, AiSettingsMutation};

fn state(cx: &TestAppContext) -> AiFeatureState {
    let focus = cx.update(|cx| AiFeatureFocus {
        chat: cx.focus_handle(),
        action: cx.focus_handle(),
        manual_model: cx.focus_handle(),
        credential: cx.focus_handle(),
    });
    AiFeatureState::new(
        AiFeatureInit {
            settings: AiSettings::default(),
            model_draft: "model-a".to_string(),
            base_url_draft: "https://example.invalid".to_string(),
            chat_session_id: "session-a".to_string(),
            session_count: 0,
            message_count: 0,
            audit_count: 0,
        },
        focus,
    )
}

#[test]
fn settings_draft_restore_and_replacement_keep_related_values_together() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.apply_settings_input(crate::models::AiInputField::ApiKey, "secret".to_string());
    let snapshot = state.settings_draft_snapshot();

    state.apply_settings_input(crate::models::AiInputField::Model, "changed".to_string());
    assert!(!state.settings_draft_matches(&snapshot.0, &snapshot.1, &snapshot.2, &snapshot.3));

    state.restore_settings_draft(snapshot.0, snapshot.1, snapshot.2, snapshot.3);
    let restored = state.settings_draft_snapshot();
    assert_eq!(restored.1, "model-a");
    assert_eq!(restored.3.expose_secret(), "secret");

    state.replace_settings_config(AiSettings::default(), true);
    assert!(state.settings_draft_snapshot().3.is_empty());
}

#[test]
fn pending_settings_preserve_masked_secret_until_a_new_draft_exists() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.settings.config.provider_profiles[0].api_key = Some("__SET__".to_string().into());
    state.settings.config.provider_credentials[0].api_key = Some("__SET__".to_string().into());

    let pending = state.pending_settings();
    assert_eq!(
        pending.provider_profiles[0].api_key.as_deref(),
        Some("__SET__")
    );
    assert_eq!(
        pending.provider_credentials[0].api_key.as_deref(),
        Some("__SET__")
    );

    state.apply_settings_input(
        crate::models::AiInputField::ApiKey,
        "replacement".to_string(),
    );
    let pending = state.pending_settings();
    assert_eq!(
        pending.provider_profiles[0].api_key.as_deref(),
        Some("replacement")
    );
    assert_eq!(
        pending.provider_credentials[0].api_key.as_deref(),
        Some("replacement")
    );
}

#[test]
fn external_agent_mcp_integration_toggles_fail_closed_setting() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);

    state.toggle_settings_codex_mcp_integration();
    state.toggle_settings_claude_mcp_integration();
    let disabled = state.pending_settings();
    assert_eq!(disabled.codex.tool_integration_mode, None);
    assert_eq!(disabled.claude_code.tool_integration_mode, None);

    state.toggle_settings_codex_mcp_integration();
    state.toggle_settings_claude_mcp_integration();
    let enabled = state.pending_settings();
    assert_eq!(
        enabled.codex.tool_integration_mode.as_deref(),
        Some("nyaterm_mcp")
    );
    assert_eq!(
        enabled.claude_code.tool_integration_mode.as_deref(),
        Some("nyaterm_mcp")
    );
}

#[test]
fn ai_settings_persistence_ignores_old_completion_and_retries_latest_snapshot() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let first_snapshot = state.settings_config_cloned();
    let (first_generation, _) = state
        .queue_settings_persistence(first_snapshot)
        .expect("first save should start");
    state.toggle_settings_enabled();
    let latest_snapshot = state.settings_config_cloned();
    assert!(
        state
            .queue_settings_persistence(latest_snapshot.clone())
            .is_none()
    );

    let first = state.finish_settings_persistence(first_generation, true);
    assert!(!first.apply_result);
    let (latest_generation, queued) = first.next.expect("latest snapshot should follow");
    assert_eq!(queued.enabled, latest_snapshot.enabled);

    let failed = state.finish_settings_persistence(latest_generation, false);
    assert!(failed.report_result);
    assert!(state.settings_persistence_is_dirty());

    let retry_snapshot = state.settings_config_cloned();
    let (retry_generation, _) = state
        .queue_settings_persistence(retry_snapshot)
        .expect("retry should submit");
    let retried = state.finish_settings_persistence(retry_generation, true);
    assert!(retried.apply_result);
    assert!(!state.settings_persistence_is_dirty());
}

#[test]
fn model_catalog_mutations_keep_default_model_valid() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let first = "openai:model-a".to_string();
    let fallback = "openai:model-b".to_string();
    state.settings.config.models = vec![
        AiModelConfigItem {
            backend: Default::default(),
            id: first.clone(),
            name: "model-a".to_string(),
            provider_kind: Some(AiProviderKind::Openai),
            credential_id: None,
            enabled: true,
            source: AiModelSource::RustGenai,
            last_seen_at: None,
        },
        AiModelConfigItem {
            backend: Default::default(),
            id: fallback.clone(),
            name: "model-b".to_string(),
            provider_kind: Some(AiProviderKind::Openai),
            credential_id: None,
            enabled: true,
            source: AiModelSource::RustGenai,
            last_seen_at: None,
        },
    ];
    state.settings.config.default_model_id = Some(first.clone());

    state.toggle_settings_model_enabled(&first);
    assert_eq!(
        state.settings.config.default_model_id.as_deref(),
        Some(fallback.as_str())
    );
    assert!(state.default_model_is_enabled());

    state
        .settings
        .config
        .provider_credentials
        .push(AiProviderCredential {
            api_format: Default::default(),
            id: "custom".to_string(),
            name: "Custom".to_string(),
            provider_kind: AiProviderKind::OpenaiCompatible,
            base_url: Some("https://example.invalid".to_string()),
            api_key: None,
            enabled: true,
        });
    state.settings.config.default_model_id = None;
    assert_eq!(
        state.add_settings_manual_model("custom", "model-x"),
        AiSettingsMutation::Persist
    );
    let manual_id = state.settings.config.default_model_id.clone().unwrap();
    assert_eq!(
        state.remove_settings_manual_model(&manual_id),
        AiSettingsMutation::Persist
    );
    assert!(state.default_model_is_enabled());
}

#[test]
fn credential_edits_move_secret_drafts_into_both_compatible_records() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    assert!(state.apply_settings_credential_input("openai.api-key", "new-key".to_string()));
    state.commit_settings_credential_edits("openai");

    assert_eq!(
        state.settings.config.provider_credentials[0]
            .api_key
            .as_deref(),
        Some("new-key")
    );
    assert_eq!(
        state.settings.config.provider_profiles[0]
            .api_key
            .as_deref(),
        Some("new-key")
    );
    assert!(
        !state
            .settings
            .credential_secret_drafts
            .contains_key("openai")
    );
}

#[test]
fn credential_catalog_changes_preserve_an_absent_default_model() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.settings.config.default_model_id = None;
    state.settings.config.models.push(AiModelConfigItem {
        backend: Default::default(),
        id: "openai:model-a".to_string(),
        name: "model-a".to_string(),
        provider_kind: Some(AiProviderKind::Openai),
        credential_id: None,
        enabled: true,
        source: AiModelSource::RustGenai,
        last_seen_at: None,
    });

    assert_eq!(
        state.toggle_settings_credential_enabled("openai"),
        AiSettingsMutation::Persist
    );
    assert!(state.settings.config.default_model_id.is_none());

    state
        .settings
        .config
        .provider_credentials
        .push(AiProviderCredential {
            api_format: Default::default(),
            id: "custom".to_string(),
            name: "Custom".to_string(),
            provider_kind: AiProviderKind::OpenaiCompatible,
            base_url: Some("https://example.invalid".to_string()),
            api_key: None,
            enabled: true,
        });
    assert_eq!(
        state.remove_settings_credential("custom"),
        AiSettingsMutation::Persist
    );
    assert!(state.settings.config.default_model_id.is_none());
}

#[test]
fn action_and_discovery_catalog_updates_stay_on_settings_owner() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.add_settings_action(
        crate::models::AiActionListKind::Terminal,
        "custom-action".to_string(),
    );
    assert!(state.apply_settings_action_input(
        crate::models::AiActionListKind::Terminal,
        "custom-action",
        crate::models::AiActionEditorField::Prompt,
        "explain".to_string(),
    ));
    assert_eq!(
        state.settings_action_value(
            crate::models::AiActionListKind::Terminal,
            "custom-action",
            crate::models::AiActionEditorField::Prompt,
        ),
        "explain"
    );

    let discovery = nyaterm_core::AiModelDiscovery {
        id: "custom:model-y".to_string(),
        name: "model-y".to_string(),
        provider_kind: Some(AiProviderKind::OpenaiCompatible),
        credential_id: Some("custom".to_string()),
        source: AiModelSource::Manual,
    };
    assert_eq!(state.apply_settings_model_discoveries(vec![discovery]), 1);
    assert!(
        state
            .settings
            .config
            .models
            .iter()
            .any(|model| model.id == "custom:model-y")
    );
}

#[test]
fn transient_ai_menus_are_mutually_exclusive() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);

    assert!(!state.transient_menus_are_open());
    assert!(state.toggle_execution_menu());
    assert!(state.transient_menus_are_open());
    assert!(state.toggle_discovery_menu(3));
    assert!(!state.panel_execution_menu_is_open());
    assert!(state.discovery_menu_is_open());

    assert!(state.toggle_history());
    assert!(!state.discovery_menu_is_open());
    state.open_message_menu(AiMessageMenuState {
        message_id: "message".to_string(),
        text: "text".to_string(),
        x: px(1.),
        y: px(2.),
    });
    assert!(!state.history_is_open());
    assert!(state.chat_message_menu().is_some());

    state.close_transient_menus();
    assert!(!state.transient_menus_are_open());
}

#[test]
fn history_and_auto_execution_confirmations_transition_on_the_owner() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.history.sessions.push(AiSession {
        agent_kind: Default::default(),
        scope: Default::default(),
        external_session_id: None,
        backend_metadata: None,
        id: "history".to_string(),
        connection_id: None,
        title: "History".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
    });

    assert!(state.request_history_clear_confirm());
    assert!(state.confirm_history_clear());

    state.request_agent_auto_confirm();
    assert!(state.confirm_agent_auto_execution());
    assert_eq!(state.panel_status(), "Agent execution mode: auto");
}

#[test]
fn history_jobs_reject_overlap_and_ignore_stale_completions() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);

    let first = state.begin_history_operation("first").unwrap();
    assert!(state.begin_history_operation("overlap").is_none());
    assert!(state.history_is_pending());
    assert_eq!(
        state.panel_status(),
        "AI history operation already in progress"
    );
    assert!(state.finish_history_session_list(first, Ok(Vec::new())));

    let second = state.begin_history_operation("second").unwrap();
    assert!(!state.finish_history_session_list(first, Ok(Vec::new())));
    assert!(state.history_is_pending());
    assert!(state.finish_history_session_list(second, Ok(Vec::new())));
    assert!(!state.history_is_pending());
}

#[test]
fn history_usage_counts_ignore_superseded_jobs() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);

    let first = state.begin_history_usage_count_job();
    let second = state.begin_history_usage_count_job();
    assert!(!state.finish_history_usage_counts(first, Ok((1, 2, 3))));
    assert_eq!(
        (
            state.history.session_count,
            state.history.message_count,
            state.history.audit_count,
        ),
        (0, 0, 0)
    );
    assert!(state.finish_history_usage_counts(second, Ok((4, 5, 6))));
    assert_eq!(
        (
            state.history.session_count,
            state.history.message_count,
            state.history.audit_count,
        ),
        (4, 5, 6)
    );
}

#[test]
fn history_completion_updates_history_and_chat_atomically() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.history.sessions = vec![AiSession {
        id: "session-a".to_string(),
        agent_kind: Default::default(),
        scope: Default::default(),
        connection_id: None,
        title: "Session A".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        external_session_id: None,
        backend_metadata: None,
    }];
    state.chat.messages.push(Arc::new(AiMessage {
        id: "assistant-a".to_string(),
        session_id: "session-a".to_string(),
        role: AiMessageRole::Assistant,
        content: "answer".to_string(),
        created_at: String::new(),
        reasoning_content: None,
        command_cards: Vec::new(),
    }));

    let delete_job = state.begin_history_operation("delete").unwrap();
    assert_eq!(
        state.finish_history_session_delete(delete_job, "session-a", Ok(())),
        Some(true)
    );
    assert!(state.history_sessions().is_empty());
    assert!(state.chat_messages().is_empty());
    assert_ne!(state.chat_session_id(), "session-a");

    state.settings.config.default_mode = AiMode::Agent;
    state.history.sessions.push(AiSession {
        agent_kind: Default::default(),
        scope: Default::default(),
        external_session_id: None,
        backend_metadata: None,
        id: state.chat_session_id().to_string(),
        connection_id: None,
        title: "Current".to_string(),
        created_at: String::new(),
        updated_at: String::new(),
    });
    let source_session_id = state.chat_session_id().to_string();
    let clear_job = state.begin_history_operation("clear").unwrap();
    assert_eq!(
        state.finish_history_clear(clear_job, &source_session_id, Ok(())),
        Some(true)
    );
    assert!(state.history_sessions().is_empty());
    assert!(state.history_query().is_empty());
    assert_eq!(state.chat_response_preview(), "Agent mode ready");
    assert_eq!(state.panel_status(), "AI history cleared");
}

#[test]
fn discovery_job_and_picker_lifecycles_stay_on_the_owner() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);

    let mut rx = state
        .take_discovery_event_receiver()
        .expect("the state holds its receiver until the drain starts");
    let tx = state.begin_discovery_job().unwrap();
    assert!(state.begin_discovery_job().is_none());
    tx.unbounded_send(AiDiscoveryJobResult {
        profile_id: "profile".to_string(),
        result: Ok(Vec::new()),
    })
    .unwrap();
    assert!(rx.try_recv().is_ok());
    state.note_discovery_event_delivered();
    assert!(!state.discovery_is_pending());

    state.toggle_discovery_menu(2);
    state.set_discovery_query("server".to_string());
    state.move_discovery_index(3, 1);
    state.move_discovery_index(3, -1);
    assert_eq!(state.discovery_index(), 0);
    assert!(state.escape_discovery_search(2));
    assert_eq!(state.discovery_index(), 2);
    assert!(state.discovery_query().is_empty());
    assert!(!state.escape_discovery_search(1));
    assert!(!state.discovery_menu_is_open());
}

#[test]
fn chat_start_stream_and_finish_are_reduced_by_the_owner() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.set_chat_prompt_draft("deploy".to_string());

    let launch = state.begin_chat_request("deploy".to_string(), AiMode::Agent, None);

    assert!(state.chat_is_pending());
    assert_eq!(state.chat_messages().len(), 2);
    assert!(state.chat_prompt_draft().is_empty());
    assert_eq!(state.agent_steps().len(), 1);
    assert_eq!(state.agent_steps()[0].status, AiAgentStepStatus::Planning);
    let mut rx = state
        .take_chat_event_receiver()
        .expect("the state holds its receiver until the drain starts");
    launch
        .tx
        .unbounded_send(AiChatWorkerEvent::Delta {
            job_id: launch.job_id,
            session_id: launch.session_id.clone(),
            text_delta: "working".to_string(),
            reasoning_delta: Some("reason".to_string()),
        })
        .unwrap();
    assert!(state.chat_event_is_wanted());
    let event = rx.try_recv().expect("the delta should be queued");
    let AiChatWorkerEvent::Delta {
        job_id,
        text_delta,
        reasoning_delta,
        ..
    } = event
    else {
        panic!("expected stream delta");
    };
    assert!(state.apply_chat_delta(job_id, &text_delta, reasoning_delta.as_deref()));
    assert_eq!(
        state.chat_response_preview(),
        "Running AI Agent step...working"
    );
    assert_eq!(
        state.chat_messages()[1].reasoning_content.as_deref(),
        Some("reason")
    );

    let effect = state
        .finish_chat_job(
            launch.job_id,
            launch.session_id,
            Ok(AiChatJobOutput {
                mode: AiMode::Agent,
                text: "done".to_string(),
                reasoning: Some("final reason".to_string()),
                command_cards: Vec::new(),
                auto_execute_first: false,
                approval_note: None,
            }),
        )
        .unwrap();
    assert!(effect.succeeded);
    assert!(effect.clear_prompt_input);
    assert!(!state.chat_is_pending());
    assert_eq!(state.chat_response_preview(), "done");
    assert_eq!(state.agent_steps()[0].title, "Final Answer");
    assert!(state.agent_loop_snapshot().is_none());
}

#[test]
fn streaming_snapshot_does_not_share_mutable_message_arc() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let launch = state.begin_chat_request("inspect".to_string(), AiMode::Ask, None);
    assert!(state.apply_chat_delta(launch.job_id, "hello", None));

    let snapshot_messages = state.chat_snapshot_messages();
    let state_messages = state.chat_messages();
    assert_eq!(state_messages.len(), 2);
    assert_eq!(snapshot_messages.len(), 2);
    assert!(
        Arc::ptr_eq(&state_messages[0], &snapshot_messages[0]),
        "completed user messages should stay shared with the snapshot"
    );
    assert!(
        !Arc::ptr_eq(&state_messages[1], &snapshot_messages[1]),
        "the active streaming assistant message should be copied for snapshots"
    );
    assert_eq!(snapshot_messages[1].content, "hello");

    assert!(state.apply_chat_delta(launch.job_id, " world", None));
    assert_eq!(state.chat_messages()[1].content, "hello world");
    assert_eq!(
        snapshot_messages[1].content, "hello",
        "old snapshots should remain immutable after later streaming deltas"
    );
}

#[test]
fn chat_cancel_invalidates_the_job_and_clears_agent_lifecycle() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let launch = state.begin_chat_request("inspect".to_string(), AiMode::Agent, None);

    state.cancel_chat_and_agent();

    assert!(launch.cancel.load(Ordering::Relaxed));
    assert!(!state.chat_is_pending());
    assert_eq!(state.chat_response_preview(), "AI request cancelled");
    assert_eq!(state.agent_steps()[0].status, AiAgentStepStatus::Cancelled);
    assert!(
        state
            .finish_chat_job(launch.job_id, launch.session_id, Err("late".to_string()),)
            .is_none()
    );
}

#[test]
fn mention_selection_and_navigation_are_atomic_owner_transitions() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.set_chat_prompt_draft("run @server".to_string());
    assert!(state.chat_mention_is_open());
    assert_eq!(state.chat_mention_query(), "server");

    state.move_chat_mention_index(3, -1);
    assert_eq!(state.chat_mention_index(), 2);
    state.hide_chat_mention();
    assert_eq!(state.chat_mention_index(), 2);
    state.set_chat_prompt_draft("run @server".to_string());
    state.select_chat_mention("session-a".to_string(), "Server A".to_string());

    assert_eq!(state.chat_prompt_draft(), "run ");
    assert_eq!(state.chat_target_session_ids(), &["session-a".to_string()]);
    assert!(!state.chat_mention_is_open());
    assert_eq!(state.panel_status(), "AI target session selected: Server A");

    state.remove_chat_target_session("session-a");
    assert!(state.chat_target_session_ids().is_empty());
    assert_eq!(state.panel_status(), "AI target sessions cleared");
}

#[test]
fn background_completion_distinguishes_foreign_and_matched_stale_jobs() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let launch = state.begin_chat_job();
    let now = Instant::now();
    let loop_state = AiAgentLoopState {
        available_targets: Vec::new(),
        default_target_session_id: None,
        ai_session_id: "session-a".to_string(),
        terminal_session_id: "terminal-a".to_string(),
        task_prompt: "inspect".to_string(),
        command: "pwd".to_string(),
        marker_id: None,
        background_job_id: Some(launch.job_id),
        step_index: 0,
        max_steps: 3,
        output_start_len: 0,
        started_at: now,
        min_wait_until: now,
        timeout_at: now + Duration::from_secs(1),
        last_seen_len: 0,
        stable_since: now,
    };

    assert!(matches!(
        state.finish_agent_background(
            launch.job_id.wrapping_add(1),
            loop_state.clone(),
            Err("foreign".to_string()),
            |_| String::new(),
        ),
        super::AiAgentBackgroundEffect::Ignored
    ));
    assert!(matches!(
        state.finish_agent_background(launch.job_id, loop_state, Err("stale".to_string()), |_| {
            String::new()
        },),
        super::AiAgentBackgroundEffect::MatchedStale
    ));
}

#[test]
fn agent_step_limit_and_observation_poll_stay_on_the_owner() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    assert!(state.begin_agent_step(1).is_err());

    let now = Instant::now();
    state.set_agent_loop(AiAgentLoopState {
        available_targets: Vec::new(),
        default_target_session_id: None,
        ai_session_id: "session-a".to_string(),
        terminal_session_id: "terminal-a".to_string(),
        task_prompt: "inspect".to_string(),
        command: "pwd".to_string(),
        marker_id: None,
        background_job_id: None,
        step_index: 0,
        max_steps: 3,
        output_start_len: 4,
        started_at: now - Duration::from_secs(2),
        min_wait_until: now - Duration::from_secs(1),
        timeout_at: now + Duration::from_secs(10),
        last_seen_len: 8,
        stable_since: now - Duration::from_secs(1),
    });

    let poll = state.poll_agent_observation(now, 8, Duration::from_millis(100));
    assert!(matches!(poll, super::AiAgentObservationPoll::Target(_)));
    assert!(state.agent_loop_snapshot().is_none());
}

#[test]
fn external_request_preparation_sets_request_status_focus_and_closes_menus() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    state.toggle_history();
    let request = AiPreparedRequest {
        action: AiAction::CustomFileAction,
        context: AiContext::default(),
        source_label: "remote file".to_string(),
    };

    state.prepare_external_request(request.clone(), "ready", "loaded", true);

    assert_eq!(state.chat_prepared_request(), Some(&request));
    assert_eq!(state.chat_response_preview(), "ready");
    assert_eq!(state.panel_status(), "loaded");
    assert!(state.chat_focus_is_pending());
    assert!(!state.history_is_open());
}

#[test]
fn detected_error_throttle_and_picker_indices_are_owned_transitions() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let now = Instant::now();

    assert!(state.note_detected_error("session".to_string(), "first".to_string(), now,));
    assert!(!state.note_detected_error(
        "session".to_string(),
        "second".to_string(),
        now + Duration::from_secs(29),
    ));
    assert!(state.note_detected_error(
        "session".to_string(),
        "third".to_string(),
        now + Duration::from_secs(30),
    ));

    state.set_discovery_index(9);
    state.set_chat_mention_index(7);
    assert_eq!(state.clamp_discovery_index(3), 2);
    assert_eq!(state.clamp_chat_mention_index(2), 1);
    assert_eq!(state.clamp_discovery_index(0), 0);
    assert_eq!(state.clamp_chat_mention_index(0), 0);
}

#[test]
fn panel_status_and_error_banner_change_only_through_owner_operations() {
    let cx = TestAppContext::single();
    let mut state = state(&cx);
    let now = Instant::now();

    state.set_panel_status("completed");
    assert_eq!(state.panel_status(), "completed");
    state.set_panel_status("replacement");
    assert_eq!(state.panel_status(), "replacement");

    assert!(state.note_detected_error("session".to_string(), "failure".to_string(), now,));
    assert!(state.panel_detected_error().is_some());
    state.clear_detected_error();
    assert!(state.panel_detected_error().is_none());
    assert_eq!(state.panel_status(), "terminal error detected");
}
