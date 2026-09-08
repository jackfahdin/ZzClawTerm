use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

use nyaterm_core::{
    AiChatCompletion, AiChatRequest, AiChatStreamDelta, AiMessage, AiModelDiscovery, AiModelError,
    AiProviderCredential, AiProviderKind, AiSettings, AiToolCall, anthropic_messages_url,
    build_anthropic_chat_request_body, build_anthropic_chat_request_body_with_stream,
    build_gemini_chat_request_body, build_openai_compatible_chat_request_body,
    build_openai_compatible_chat_request_body_with_stream, build_openai_responses_request_body,
    effective_ai_request_user_agent, gemini_generate_content_url,
    gemini_stream_generate_content_url, openai_compatible_chat_completions_url,
    openai_compatible_models_url, openai_responses_url, parse_anthropic_chat_response,
    parse_anthropic_stream_chunk, parse_gemini_chat_response, parse_gemini_stream_chunk,
    parse_openai_compatible_chat_response, parse_openai_compatible_models_response,
    parse_openai_compatible_stream_chunk, parse_openai_responses_response,
    parse_openai_responses_stream_chunk, resolve_request_model, uses_responses_api,
    validate_ai_request_contract,
};
use zed_reqwest::StatusCode;

const AI_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub fn discover_openai_compatible_models(
    settings: &AiSettings,
    credential: &AiProviderCredential,
) -> Result<Vec<AiModelDiscovery>, String> {
    let base_url = credential
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "AI model discovery requires a Base URL".to_string())?;
    let url = openai_compatible_models_url(base_url).map_err(|error| error.to_string())?;
    let client = zed_reqwest::blocking::Client::builder()
        .timeout(AI_DISCOVERY_TIMEOUT)
        .user_agent(effective_ai_request_user_agent(settings))
        .build()
        .map_err(map_discovery_error)?;

    let mut request = client.get(url);
    if let Some(api_key) = credential
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().map_err(map_discovery_error)?;
    let status = response.status();
    let body = response.text().map_err(map_discovery_error)?;
    if !status.is_success() {
        return Err(format!(
            "models endpoint returned {}: {}",
            status_label(status),
            body.trim()
        ));
    }

    parse_openai_compatible_models_response(&body, credential).map_err(|error| error.to_string())
}

pub fn complete_native_chat(
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
) -> Result<AiChatCompletion, String> {
    validate_ai_request_contract(request, settings).map_err(|error| error.to_string())?;
    let resolved_model =
        resolve_request_model(settings, request).map_err(|error| error.to_string())?;
    let credential = resolved_model
        .credential
        .as_ref()
        .ok_or_else(|| "AI chat requires a provider credential".to_string())?;
    let client = zed_reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(settings.timeout_ms))
        .user_agent(effective_ai_request_user_agent(settings))
        .build()
        .map_err(map_ai_http_error)?;

    if uses_responses_api(&resolved_model) {
        return complete_openai_responses_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
        );
    }

    match resolved_model.provider_kind {
        AiProviderKind::Anthropic => complete_anthropic_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
        ),
        AiProviderKind::Gemini => complete_gemini_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
        ),
        _ => complete_openai_compatible_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
        ),
    }
}

pub fn stream_native_chat(
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    validate_ai_request_contract(request, settings).map_err(|error| error.to_string())?;
    let resolved_model =
        resolve_request_model(settings, request).map_err(|error| error.to_string())?;
    let credential = resolved_model
        .credential
        .as_ref()
        .ok_or_else(|| "AI chat requires a provider credential".to_string())?;
    let client = zed_reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(settings.timeout_ms))
        .user_agent(effective_ai_request_user_agent(settings))
        .build()
        .map_err(map_ai_http_error)?;

    if uses_responses_api(&resolved_model) {
        return stream_openai_responses_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            on_delta,
        );
    }

    match resolved_model.provider_kind {
        AiProviderKind::Anthropic => stream_anthropic_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            on_delta,
        ),
        AiProviderKind::Gemini => stream_gemini_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            on_delta,
        ),
        _ => stream_openai_compatible_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            on_delta,
        ),
    }
}

fn complete_openai_responses_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
) -> Result<AiChatCompletion, String> {
    let url = openai_responses_url(resolved_model).map_err(|error| error.to_string())?;
    let body =
        build_openai_responses_request_body(resolved_model, request, settings, history, false);
    let mut http_request = client.post(url).json(&body);
    if let Some(api_key) = credential
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        http_request = http_request.bearer_auth(api_key);
    }
    let response = http_request.send().map_err(map_ai_http_error)?;
    let status = response.status();
    let response_body = response.text().map_err(map_ai_http_error)?;
    if !status.is_success() {
        return Err(format!(
            "Responses API endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }
    parse_openai_responses_response(&response_body).map_err(|error| error.to_string())
}

fn stream_openai_responses_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
    on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    let url = openai_responses_url(resolved_model).map_err(|error| error.to_string())?;
    let body =
        build_openai_responses_request_body(resolved_model, request, settings, history, true);
    let mut http_request = client.post(url).json(&body);
    if let Some(api_key) = credential
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        http_request = http_request.bearer_auth(api_key);
    }
    let response = http_request.send().map_err(map_ai_http_error)?;
    let status = response.status();
    if !status.is_success() {
        let response_body = response.text().map_err(map_ai_http_error)?;
        return Err(format!(
            "Responses API stream endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }
    read_sse_chat_stream(response, parse_openai_responses_stream_chunk, on_delta)
}

fn complete_openai_compatible_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
) -> Result<AiChatCompletion, String> {
    let base_url = openai_compatible_chat_base_url(credential)?;
    let url =
        openai_compatible_chat_completions_url(base_url).map_err(|error| error.to_string())?;
    let body =
        build_openai_compatible_chat_request_body(resolved_model, request, settings, history);

    let mut http_request = client.post(url).json(&body);
    if let Some(api_key) = credential
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        http_request = http_request.bearer_auth(api_key);
    }

    let response = http_request.send().map_err(map_ai_http_error)?;
    let status = response.status();
    let response_body = response.text().map_err(map_ai_http_error)?;
    if !status.is_success() {
        return Err(format!(
            "chat completions endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }

    parse_openai_compatible_chat_response(&response_body).map_err(|error| error.to_string())
}

fn stream_openai_compatible_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
    on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    let base_url = openai_compatible_chat_base_url(credential)?;
    let url =
        openai_compatible_chat_completions_url(base_url).map_err(|error| error.to_string())?;
    let body = build_openai_compatible_chat_request_body_with_stream(
        resolved_model,
        request,
        settings,
        history,
        true,
    );

    let mut http_request = client.post(url).json(&body);
    if let Some(api_key) = credential
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        http_request = http_request.bearer_auth(api_key);
    }

    let response = http_request.send().map_err(map_ai_http_error)?;
    let status = response.status();
    if !status.is_success() {
        let response_body = response.text().map_err(map_ai_http_error)?;
        return Err(format!(
            "chat completions stream endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }

    read_sse_chat_stream(response, parse_openai_compatible_stream_chunk, on_delta)
}

fn stream_anthropic_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
    on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    let base_url = provider_base_url(credential, "https://api.anthropic.com/v1");
    let url = anthropic_messages_url(base_url).map_err(|error| error.to_string())?;
    let body = build_anthropic_chat_request_body_with_stream(
        resolved_model,
        request,
        settings,
        history,
        true,
    );
    let api_key = api_key(credential)?;
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .map_err(map_ai_http_error)?;
    let status = response.status();
    if !status.is_success() {
        let response_body = response.text().map_err(map_ai_http_error)?;
        return Err(format!(
            "Anthropic messages stream endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }

    read_sse_chat_stream(response, parse_anthropic_stream_chunk, on_delta)
}

fn stream_gemini_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
    on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    let base_url = provider_base_url(
        credential,
        "https://generativelanguage.googleapis.com/v1beta",
    );
    let model_name =
        nyaterm_core::genai_model_name(&resolved_model.provider_kind, &resolved_model.model_name);
    let url = gemini_stream_generate_content_url(base_url, &model_name)
        .map_err(|error| error.to_string())?;
    let body = build_gemini_chat_request_body(request, settings, history);
    let api_key = api_key(credential)?;
    let response = client
        .post(url)
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .map_err(map_ai_http_error)?;
    let status = response.status();
    if !status.is_success() {
        let response_body = response.text().map_err(map_ai_http_error)?;
        return Err(format!(
            "Gemini streamGenerateContent endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }

    read_sse_chat_stream(response, parse_gemini_stream_chunk, on_delta)
}

fn complete_anthropic_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
) -> Result<AiChatCompletion, String> {
    let base_url = provider_base_url(credential, "https://api.anthropic.com/v1");
    let url = anthropic_messages_url(base_url).map_err(|error| error.to_string())?;
    let body = build_anthropic_chat_request_body(resolved_model, request, settings, history);
    let api_key = api_key(credential)?;
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .map_err(map_ai_http_error)?;
    let status = response.status();
    let response_body = response.text().map_err(map_ai_http_error)?;
    if !status.is_success() {
        return Err(format!(
            "Anthropic messages endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }

    parse_anthropic_chat_response(&response_body).map_err(|error| error.to_string())
}

fn complete_gemini_chat(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    resolved_model: &nyaterm_core::ResolvedAiModel,
) -> Result<AiChatCompletion, String> {
    let base_url = provider_base_url(
        credential,
        "https://generativelanguage.googleapis.com/v1beta",
    );
    let model_name =
        nyaterm_core::genai_model_name(&resolved_model.provider_kind, &resolved_model.model_name);
    let url =
        gemini_generate_content_url(base_url, &model_name).map_err(|error| error.to_string())?;
    let body = build_gemini_chat_request_body(request, settings, history);
    let api_key = api_key(credential)?;
    let response = client
        .post(url)
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .map_err(map_ai_http_error)?;
    let status = response.status();
    let response_body = response.text().map_err(map_ai_http_error)?;
    if !status.is_success() {
        return Err(format!(
            "Gemini generateContent endpoint returned {}: {}",
            status_label(status),
            response_body.trim()
        ));
    }

    parse_gemini_chat_response(&response_body).map_err(|error| error.to_string())
}

fn read_sse_chat_stream(
    mut response: zed_reqwest::blocking::Response,
    parse_chunk: fn(&str) -> Result<Vec<AiChatStreamDelta>, AiModelError>,
    mut on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    let mut raw_text = String::new();
    let mut reasoning = String::new();
    let mut tool_call_buffers = BTreeMap::new();
    let mut buffer = String::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = response
            .read(&mut chunk)
            .map_err(|error| format!("AI stream read failed: {error}"))?;
        if read == 0 {
            break;
        }
        buffer.push_str(&String::from_utf8_lossy(&chunk[..read]));
        drain_ai_stream_buffer(
            &mut buffer,
            &mut raw_text,
            &mut reasoning,
            &mut tool_call_buffers,
            parse_chunk,
            &mut on_delta,
        )?;
    }
    if !buffer.trim().is_empty() {
        let tail = std::mem::take(&mut buffer);
        apply_ai_stream_deltas(
            &tail,
            &mut raw_text,
            &mut reasoning,
            &mut tool_call_buffers,
            parse_chunk,
            &mut on_delta,
        )?;
    }
    on_delta(AiChatStreamDelta {
        done: true,
        ..Default::default()
    });

    Ok(AiChatCompletion {
        text: raw_text,
        reasoning_content: if reasoning.trim().is_empty() {
            None
        } else {
            Some(reasoning)
        },
        tool_calls: finalize_stream_tool_calls(tool_call_buffers)?,
    })
}

fn drain_ai_stream_buffer(
    buffer: &mut String,
    raw_text: &mut String,
    reasoning: &mut String,
    tool_call_buffers: &mut BTreeMap<usize, StreamToolCallBuffer>,
    parse_chunk: fn(&str) -> Result<Vec<AiChatStreamDelta>, AiModelError>,
    on_delta: &mut impl FnMut(AiChatStreamDelta),
) -> Result<(), String> {
    while let Some((index, delimiter_len)) = find_sse_event_boundary(buffer) {
        let event = buffer[..index + delimiter_len].to_string();
        buffer.drain(..index + delimiter_len);
        apply_ai_stream_deltas(
            &event,
            raw_text,
            reasoning,
            tool_call_buffers,
            parse_chunk,
            on_delta,
        )?;
    }
    Ok(())
}

fn find_sse_event_boundary(buffer: &str) -> Option<(usize, usize)> {
    [("\n\n", 2), ("\r\n\r\n", 4)]
        .into_iter()
        .filter_map(|(delimiter, delimiter_len)| {
            buffer.find(delimiter).map(|index| (index, delimiter_len))
        })
        .min_by_key(|(index, _)| *index)
}

fn apply_ai_stream_deltas(
    chunk: &str,
    raw_text: &mut String,
    reasoning: &mut String,
    tool_call_buffers: &mut BTreeMap<usize, StreamToolCallBuffer>,
    parse_chunk: fn(&str) -> Result<Vec<AiChatStreamDelta>, AiModelError>,
    on_delta: &mut impl FnMut(AiChatStreamDelta),
) -> Result<(), String> {
    for delta in parse_chunk(chunk).map_err(|error| error.to_string())? {
        if !delta.text_delta.is_empty() {
            raw_text.push_str(&delta.text_delta);
        }
        if let Some(reasoning_delta) = delta.reasoning_delta.as_deref() {
            reasoning.push_str(reasoning_delta);
        }
        for tool_delta in &delta.tool_call_deltas {
            tool_call_buffers
                .entry(tool_delta.index)
                .or_default()
                .apply_delta(tool_delta);
        }
        on_delta(delta);
    }
    Ok(())
}

#[derive(Debug, Default)]
struct StreamToolCallBuffer {
    id: Option<String>,
    name: String,
    arguments: String,
}

impl StreamToolCallBuffer {
    fn apply_delta(&mut self, delta: &nyaterm_core::AiToolCallDelta) {
        if let Some(id_delta) = delta.id_delta.as_deref() {
            if self.id.is_none() {
                self.id = Some(id_delta.to_string());
            } else if let Some(id) = self.id.as_mut()
                && !id.ends_with(id_delta)
            {
                id.push_str(id_delta);
            }
        }
        if let Some(name_delta) = delta.name_delta.as_deref() {
            self.name.push_str(name_delta);
        }
        self.arguments.push_str(&delta.arguments_delta);
    }
}

fn finalize_stream_tool_calls(
    tool_call_buffers: BTreeMap<usize, StreamToolCallBuffer>,
) -> Result<Vec<AiToolCall>, String> {
    tool_call_buffers
        .into_values()
        .filter(|buffer| !buffer.name.trim().is_empty())
        .map(|buffer| {
            let arguments = if buffer.arguments.trim().is_empty() {
                serde_json::Value::Object(Default::default())
            } else {
                serde_json::from_str(&buffer.arguments).map_err(|error| {
                    format!(
                        "AI stream tool call '{}' arguments are invalid JSON: {error}",
                        buffer.name
                    )
                })?
            };
            Ok(AiToolCall {
                id: buffer.id,
                name: buffer.name.trim().to_string(),
                arguments,
            })
        })
        .collect()
}

fn openai_compatible_chat_base_url(credential: &AiProviderCredential) -> Result<&str, String> {
    if let Some(base_url) = credential
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(base_url);
    }

    match credential.provider_kind {
        AiProviderKind::Openai => Ok("https://api.openai.com/v1"),
        AiProviderKind::Deepseek => Ok("https://api.deepseek.com/v1"),
        AiProviderKind::Ollama => Ok("http://localhost:11434/v1"),
        AiProviderKind::Xai => Ok("https://api.x.ai/v1"),
        AiProviderKind::Cohere => Ok("https://api.cohere.com/compatibility/v1"),
        AiProviderKind::Mimo => Ok("https://api.xiaomimimo.com/v1"),
        AiProviderKind::Zai => Ok("https://open.bigmodel.cn/api/paas/v4"),
        AiProviderKind::OpenaiCompatible | AiProviderKind::Groq => {
            Err("OpenAI-compatible AI chat requires a Base URL".to_string())
        }
        AiProviderKind::Anthropic | AiProviderKind::Gemini => Err(format!(
            "{:?} cannot use the OpenAI-compatible chat adapter",
            credential.provider_kind
        )),
    }
}

fn provider_base_url<'a>(credential: &'a AiProviderCredential, default: &'static str) -> &'a str {
    credential
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
}

fn api_key(credential: &AiProviderCredential) -> Result<&str, String> {
    credential
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("AI credential '{}' is missing an API key", credential.name))
}

fn map_discovery_error(error: zed_reqwest::Error) -> String {
    if error.is_timeout() {
        format!("AI model discovery timed out: {error}")
    } else {
        format!("AI model discovery request failed: {error}")
    }
}

fn map_ai_http_error(error: zed_reqwest::Error) -> String {
    if error.is_timeout() {
        format!("AI request timed out: {error}")
    } else {
        format!("AI request failed: {error}")
    }
}

fn status_label(status: StatusCode) -> String {
    status
        .canonical_reason()
        .map(|reason| format!("{status} {reason}"))
        .unwrap_or_else(|| status.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use nyaterm_core::{
        AiApiFormat, AiBackendKind, AiMode, AiModelConfigItem, AiModelSource, AiProviderCredential,
        AiProviderKind, AiReasoningEffort, AiSettings,
    };

    use super::{complete_native_chat, stream_native_chat};

    fn responses_settings(base_url: String) -> AiSettings {
        let credential = AiProviderCredential {
            id: "mock-responses".to_string(),
            name: "Mock Responses".to_string(),
            provider_kind: AiProviderKind::OpenaiCompatible,
            api_format: AiApiFormat::Responses,
            base_url: Some(base_url),
            api_key: Some("mock-fixture-key".to_string().into()),
            enabled: true,
        };
        let model = AiModelConfigItem {
            id: "mock-responses:gpt-test".to_string(),
            name: "gpt-test".to_string(),
            backend: AiBackendKind::Genai,
            provider_kind: Some(AiProviderKind::OpenaiCompatible),
            credential_id: Some(credential.id.clone()),
            enabled: true,
            source: AiModelSource::Manual,
            last_seen_at: None,
        };
        AiSettings {
            default_model_id: Some(model.id.clone()),
            models: vec![model],
            provider_credentials: vec![credential],
            default_reasoning_effort: AiReasoningEffort::High,
            ..AiSettings::default()
        }
    }

    fn request() -> nyaterm_core::AiChatRequest {
        nyaterm_core::AiChatRequest {
            owner_scope: Default::default(),
            targets: Vec::new(),
            target_contexts: Vec::new(),
            agent_kind: Default::default(),
            permission_mode: Default::default(),
            default_target_session_id: None,
            existing_external_session_id: None,
            attachments: Vec::new(),
            stream_id: Some("stream-mock".to_string()),
            session_id: Some("session-mock".to_string()),
            connection_id: None,
            terminal_session_id: None,
            mode: AiMode::Ask,
            model_id: Some("mock-responses:gpt-test".to_string()),
            model_name: None,
            action: nyaterm_core::AiAction::GenerateCommand,
            user_input: "show disk usage".to_string(),
            context: Default::default(),
            options: Default::default(),
        }
    }

    fn mock_server(
        response_body: String,
        content_type: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let request = read_http_request(&mut stream);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            request
        });
        (format!("http://{address}/v1"), handle)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 2048];
        let mut expected_len = None;
        loop {
            let count = stream.read(&mut chunk).expect("read request");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let header_end = header_end + 4;
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or_default();
                expected_len = Some(header_end + content_length);
            }
            if expected_len.is_some_and(|expected| bytes.len() >= expected) {
                break;
            }
        }
        String::from_utf8(bytes).expect("UTF-8 HTTP request")
    }

    #[test]
    fn complete_native_chat_routes_responses_format_to_responses_endpoint() {
        let response = r#"{"output":[{"type":"reasoning","summary":[{"text":"read only"}]},{"type":"message","content":[{"type":"output_text","text":"Use df -h"}]}]}"#;
        let (base_url, server) = mock_server(response.to_string(), "application/json");
        let completion = complete_native_chat(&responses_settings(base_url), &request(), &[])
            .expect("Responses completion");
        let raw_request = server.join().expect("mock server");

        assert_eq!(completion.text, "Use df -h");
        assert_eq!(completion.reasoning_content.as_deref(), Some("read only"));
        assert!(raw_request.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(raw_request.contains("\"store\":false"));
        assert!(raw_request.contains("\"reasoning\":{\"effort\":\"high\"}"));
    }

    #[test]
    fn stream_native_chat_folds_responses_sse_and_function_arguments() {
        let response = concat!(
            "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call-1\",\"name\":\"execute_command\",\"arguments\":\"\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":1,\"delta\":\"{\\\"command\\\":\\\"df -h\\\"}\"}\n\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"inspect\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n"
        );
        let (base_url, server) = mock_server(response.to_string(), "text/event-stream");
        let mut deltas = Vec::new();
        let completion =
            stream_native_chat(&responses_settings(base_url), &request(), &[], |delta| {
                deltas.push(delta)
            })
            .expect("Responses stream");
        let raw_request = server.join().expect("mock server");

        assert_eq!(completion.text, "done");
        assert_eq!(completion.reasoning_content.as_deref(), Some("inspect"));
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.tool_calls[0].name, "execute_command");
        assert_eq!(completion.tool_calls[0].arguments["command"], "df -h");
        assert!(deltas.iter().any(|delta| delta.done));
        assert!(raw_request.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(raw_request.contains("\"stream\":true"));
    }
}
