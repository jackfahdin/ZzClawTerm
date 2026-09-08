# AI parity fixtures

These fixtures freeze the Tauri (`temp/nyaterm-tauri`, read-only reference) to GPUI
(`nyaterm-core`) AI behaviour baseline. They are consumed by
`crates/nyaterm-core/tests/ai_parity.rs`.

## Contents

- `settings_v3_tauri.json` .. `settings_v6_tauri.json` — AI settings documents as the
  Tauri build wrote them at each `schema_version`. They deliberately carry
  Tauri-only fields the GPUI build dropped (`backend`, `api_format`,
  `default_agent_kind`, `default_reasoning_effort`, `codex`, `claude_code`,
  `external_mcp`) to prove the GPUI deserializer loads them without erroring and
  without discarding the still-supported data around them.
- `history_tauri.json` — AI chat history (`AiHistoryFile`) written by the Tauri
  build (camelCase, optional `reasoningContent`, embedded command cards).
- `audit_tauri.json` — AI audit log (`AiAuditFile`) written by the Tauri build.
- `stream_openai_compatible.sse`, `stream_anthropic.sse`, `stream_gemini.sse` —
  representative provider streaming payloads that freeze the SSE chunk parsers.

## Redaction

All secrets are redacted. Every API key is the literal placeholder
`__REDACTED_FIXTURE_SECRET__`; hosts use `.invalid` / `example` domains and
connection ids are `conn-redacted-*`. No real credential, token, host, or user
data is present. Do not replace the placeholders with real secrets.
