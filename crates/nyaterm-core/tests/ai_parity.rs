use nyaterm_core::{
    AiAgentKind, AiApiFormat, AiAuditFile, AiBackendKind, AiHistoryFile, AiPermissionMode,
    AiReasoningEffort, AiSettings, ExternalMcpSessionScope, parse_anthropic_stream_chunk,
    parse_gemini_stream_chunk, parse_openai_compatible_stream_chunk,
};
use serde_json::Value;

const SETTINGS_V3: &str = include_str!("fixtures/ai/settings_v3_tauri.json");
const SETTINGS_V4: &str = include_str!("fixtures/ai/settings_v4_tauri.json");
const SETTINGS_V5: &str = include_str!("fixtures/ai/settings_v5_tauri.json");
const SETTINGS_V6: &str = include_str!("fixtures/ai/settings_v6_tauri.json");
const HISTORY: &str = include_str!("fixtures/ai/history_tauri.json");
const AUDIT: &str = include_str!("fixtures/ai/audit_tauri.json");

#[test]
fn tauri_ai_settings_generations_are_valid_and_loadable() {
    for (expected_version, raw) in [
        (3, SETTINGS_V3),
        (4, SETTINGS_V4),
        (5, SETTINGS_V5),
        (6, SETTINGS_V6),
    ] {
        let value: Value = serde_json::from_str(raw).expect("valid fixture JSON");
        assert_eq!(value["schema_version"], expected_version);
        let settings: AiSettings = serde_json::from_value(value).expect("load Tauri AI settings");
        assert_eq!(settings.schema_version, expected_version);
        assert!(settings.enabled);
    }
}

#[test]
fn tauri_v6_settings_fields_round_trip_through_the_core_contract() {
    let settings: AiSettings = serde_json::from_str(SETTINGS_V6).expect("load v6 settings");

    assert_eq!(settings.default_agent_kind, AiAgentKind::ClaudeCode);
    assert_eq!(
        settings.external_agent_permission_mode,
        AiPermissionMode::Confirm
    );
    assert_eq!(settings.default_reasoning_effort, AiReasoningEffort::High);
    assert_eq!(settings.models[0].backend, AiBackendKind::Genai);
    assert_eq!(
        settings.provider_credentials[0].api_format,
        AiApiFormat::ChatCompletions
    );
    assert!(settings.claude_code.enabled);
    assert_eq!(settings.claude_code.permission_mode, AiPermissionMode::Auto);
    assert!(settings.external_mcp.enabled);
    assert_eq!(
        settings.external_mcp.session_scope,
        ExternalMcpSessionScope::AllSessions
    );

    let serialized = serde_json::to_value(settings).expect("serialize v6 settings");
    assert_eq!(serialized["schema_version"], 6);
    assert_eq!(serialized["default_agent_kind"], "claude_code");
    assert_eq!(serialized["default_reasoning_effort"], "high");
    assert_eq!(
        serialized["provider_credentials"][0]["api_format"],
        "chat_completions"
    );
}

#[test]
fn tauri_history_and_audit_contracts_load_without_secrets() {
    let history: AiHistoryFile = serde_json::from_str(HISTORY).expect("load Tauri history");
    let audit: AiAuditFile = serde_json::from_str(AUDIT).expect("load Tauri audit");

    assert_eq!(history.sessions.len(), 2);
    assert_eq!(history.messages.len(), 4);
    assert_eq!(
        history.messages[1].reasoning_content.as_deref(),
        Some("The user wants free disk space; df -h is read-only.")
    );
    assert_eq!(audit.logs.len(), 3);
    assert!(audit.logs.last().expect("audit entry").blocked);
    assert!(!HISTORY.contains("sk-"));
    assert!(!AUDIT.contains("Authorization: Bearer"));
}

#[test]
fn provider_stream_fixtures_freeze_text_reasoning_tools_and_completion() {
    let openai = parse_openai_compatible_stream_chunk(include_str!(
        "fixtures/ai/stream_openai_compatible.sse"
    ))
    .expect("parse OpenAI-compatible SSE");
    assert!(openai.iter().any(|delta| !delta.text_delta.is_empty()));
    assert!(openai.iter().any(|delta| delta.reasoning_delta.is_some()));
    assert!(openai.iter().any(|delta| delta.done));

    let anthropic = parse_anthropic_stream_chunk(include_str!("fixtures/ai/stream_anthropic.sse"))
        .expect("parse Anthropic SSE");
    assert!(
        anthropic
            .iter()
            .any(|delta| delta.reasoning_delta.is_some())
    );
    assert!(
        anthropic
            .iter()
            .any(|delta| !delta.tool_call_deltas.is_empty())
    );
    assert!(anthropic.iter().any(|delta| delta.done));

    let gemini = parse_gemini_stream_chunk(include_str!("fixtures/ai/stream_gemini.sse"))
        .expect("parse Gemini SSE");
    assert!(gemini.iter().any(|delta| !delta.text_delta.is_empty()));
    assert!(gemini.iter().any(|delta| delta.reasoning_delta.is_some()));
    assert!(
        gemini
            .iter()
            .any(|delta| !delta.tool_call_deltas.is_empty())
    );
}
