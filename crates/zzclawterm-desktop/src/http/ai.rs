mod provider;
mod proxy;
pub(crate) use provider::discover_provider_models_cancellable;
pub use provider::{discover_provider_models, test_model_connection};
use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;
use zzclawterm_core::ai::{AiProviderApiProtocol, provider_settings::effective_protocol};

use zed_reqwest::StatusCode;
use zzclawterm_core::{
    AiChatCompletion, AiChatRequest, AiChatStreamDelta, AiMessage, AiModelError,
    AiProviderCredential, AiProviderKind, AiSettings, AiToolCall, anthropic_messages_url,
    build_anthropic_chat_request_body, build_anthropic_chat_request_body_with_stream,
    build_gemini_chat_request_body, build_openai_compatible_chat_request_body,
    build_openai_compatible_chat_request_body_with_stream, build_openai_responses_request_body,
    effective_ai_request_user_agent, gemini_generate_content_url,
    gemini_stream_generate_content_url, openai_compatible_chat_completions_url,
    openai_responses_url, parse_anthropic_chat_response, parse_anthropic_stream_chunk,
    parse_gemini_chat_response, parse_gemini_stream_chunk, parse_openai_compatible_chat_response,
    parse_openai_compatible_stream_chunk, parse_openai_responses_response,
    parse_openai_responses_stream_chunk, resolve_request_model, uses_responses_api,
    validate_ai_request_contract,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";

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
    let client = proxy::apply_proxy(zed_reqwest::blocking::Client::builder(), &settings.proxy)?
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

    match effective_protocol(credential) {
        AiProviderApiProtocol::Anthropic => complete_anthropic_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
        ),
        AiProviderApiProtocol::Gemini => complete_gemini_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
        ),
        AiProviderApiProtocol::Ollama => provider::complete_ollama(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            None,
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
    let client = proxy::apply_proxy(zed_reqwest::blocking::Client::builder(), &settings.proxy)?
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

    match effective_protocol(credential) {
        AiProviderApiProtocol::Anthropic => stream_anthropic_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            on_delta,
        ),
        AiProviderApiProtocol::Gemini => stream_gemini_chat(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            on_delta,
        ),
        AiProviderApiProtocol::Ollama => provider::complete_ollama(
            &client,
            credential,
            settings,
            request,
            history,
            &resolved_model,
            Some(&mut { on_delta }),
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
    on_delta: impl FnMut(AiChatStreamDelta),
) -> Result<AiChatCompletion, String> {
    let base_url = provider_base_url(
        credential,
        "https://generativelanguage.googleapis.com/v1beta",
    );
    let model_name = zzclawterm_core::genai_model_name(
        &resolved_model.provider_kind,
        &resolved_model.model_name,
    );
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
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
    resolved_model: &zzclawterm_core::ResolvedAiModel,
) -> Result<AiChatCompletion, String> {
    let base_url = provider_base_url(
        credential,
        "https://generativelanguage.googleapis.com/v1beta",
    );
    let model_name = zzclawterm_core::genai_model_name(
        &resolved_model.provider_kind,
        &resolved_model.model_name,
    );
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
    let mut pending_utf8 = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut done = false;
    loop {
        let read = response
            .read(&mut chunk)
            .map_err(|error| format!("AI stream read failed: {error}"))?;
        if read == 0 {
            break;
        }
        buffer.push_str(&decode_stream_utf8(&mut pending_utf8, &chunk[..read]));
        done = drain_ai_stream_buffer(
            &mut buffer,
            &mut raw_text,
            &mut reasoning,
            &mut tool_call_buffers,
            parse_chunk,
            &mut on_delta,
        )?;
        if done {
            break;
        }
    }
    if !done && !pending_utf8.is_empty() {
        buffer.push_str(&String::from_utf8_lossy(&pending_utf8));
    }
    if !done && !buffer.trim().is_empty() {
        let tail = std::mem::take(&mut buffer);
        done = apply_ai_stream_deltas(
            &tail,
            &mut raw_text,
            &mut reasoning,
            &mut tool_call_buffers,
            parse_chunk,
            &mut on_delta,
        )?;
    }
    if !done {
        on_delta(AiChatStreamDelta {
            done: true,
            ..Default::default()
        });
    }

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

fn decode_stream_utf8(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    let mut decoded = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                decoded.push_str(text);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                decoded
                    .push_str(std::str::from_utf8(&pending[..valid]).expect("valid UTF-8 prefix"));
                if let Some(invalid_len) = error.error_len() {
                    decoded.push('\u{fffd}');
                    pending.drain(..valid + invalid_len);
                } else {
                    pending.drain(..valid);
                    break;
                }
            }
        }
    }
    decoded
}

fn drain_ai_stream_buffer(
    buffer: &mut String,
    raw_text: &mut String,
    reasoning: &mut String,
    tool_call_buffers: &mut BTreeMap<usize, StreamToolCallBuffer>,
    parse_chunk: fn(&str) -> Result<Vec<AiChatStreamDelta>, AiModelError>,
    on_delta: &mut impl FnMut(AiChatStreamDelta),
) -> Result<bool, String> {
    while let Some((index, delimiter_len)) = find_sse_event_boundary(buffer) {
        let event = buffer[..index + delimiter_len].to_string();
        buffer.drain(..index + delimiter_len);
        if apply_ai_stream_deltas(
            &event,
            raw_text,
            reasoning,
            tool_call_buffers,
            parse_chunk,
            on_delta,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
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
) -> Result<bool, String> {
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
        let done = delta.done;
        on_delta(delta);
        if done {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug, Default)]
struct StreamToolCallBuffer {
    thought_signature: Option<String>,
    id: Option<String>,
    name: String,
    arguments: String,
}

impl StreamToolCallBuffer {
    fn apply_delta(&mut self, delta: &zzclawterm_core::AiToolCallDelta) {
        if let Some(signature) = &delta.thought_signature {
            self.thought_signature = Some(signature.clone());
        }
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
                thought_signature: buffer.thought_signature,
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

    if credential.api_protocol == Some(AiProviderApiProtocol::OpenaiCompatible) {
        return Err("API base URL is required".into());
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
    use std::collections::BTreeMap;
    use std::io::{Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use zzclawterm_core::ai::AiProviderApiProtocol;

    use zzclawterm_core::{
        AiApiFormat, AiBackendKind, AiMode, AiModelConfigItem, AiModelSource, AiProviderCredential,
        AiProviderKind, AiReasoningEffort, AiSettings,
    };

    use super::{
        complete_native_chat, decode_stream_utf8, drain_ai_stream_buffer, stream_native_chat,
    };

    #[test]
    fn streaming_utf8_decoder_preserves_split_multibyte_characters() {
        let mut pending = Vec::new();
        let bytes = "A한繁B".as_bytes();
        assert_eq!(decode_stream_utf8(&mut pending, &bytes[..2]), "A");
        assert_eq!(decode_stream_utf8(&mut pending, &bytes[2..5]), "한");
        assert_eq!(decode_stream_utf8(&mut pending, &bytes[5..]), "繁B");
        assert!(pending.is_empty());
    }

    #[test]
    fn completed_stream_event_stops_before_later_payloads() {
        let mut buffer = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"first\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"late\"}\n\n",
        )
        .to_string();
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tools = BTreeMap::new();
        let mut events = Vec::new();
        assert!(
            drain_ai_stream_buffer(
                &mut buffer,
                &mut text,
                &mut reasoning,
                &mut tools,
                zzclawterm_core::parse_openai_responses_stream_chunk,
                &mut |event| events.push(event),
            )
            .unwrap()
        );
        assert_eq!(text, "first");
        assert_eq!(events.iter().filter(|event| event.done).count(), 1);
    }

    fn responses_settings(base_url: String) -> AiSettings {
        let credential = AiProviderCredential {
            icon_data_url: None,
            api_protocol: None,
            id: "mock-responses".to_string(),
            name: "Mock Responses".to_string(),
            provider_kind: AiProviderKind::OpenaiCompatible,
            api_format: AiApiFormat::Responses,
            base_url: Some(base_url),
            api_key: Some("mock-fixture-key".to_string().into()),
            enabled: true,
        };
        let model = AiModelConfigItem {
            supported_reasoning_efforts: None,
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

    fn request() -> zzclawterm_core::AiChatRequest {
        zzclawterm_core::AiChatRequest {
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
            action: zzclawterm_core::AiAction::GenerateCommand,
            user_input: "show disk usage".to_string(),
            context: Default::default(),
            options: Default::default(),
        }
    }

    fn mock_server(
        response_body: String,
        content_type: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        mock_server_status(response_body, content_type, "200 OK")
    }
    fn mock_server_status(
        response_body: String,
        content_type: &'static str,
        status: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let request = read_http_request(&mut stream);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
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
    #[test]
    fn protocol_override_drives_discovery_chat_stream_and_authentication() {
        use super::discover_provider_models;
        use zzclawterm_core::ai::AiProviderApiProtocol;
        for (protocol, discovery, response, stream, path, auth) in [
            (
                AiProviderApiProtocol::OpenaiCompatible,
                r#"{"data":[{"id":"gpt-test"}]}"#,
                r#"{"choices":[{"message":{"content":"OK"}}]}"#,
                "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"}}]}\n\ndata: [DONE]\n\n",
                "/v1/chat/completions",
                "authorization: Bearer mock-fixture-key",
            ),
            (
                AiProviderApiProtocol::Anthropic,
                r#"{"data":[{"id":"gpt-test"}],"has_more":false}"#,
                r#"{"content":[{"type":"text","text":"OK"}]}"#,
                "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"OK\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
                "/v1/messages",
                "x-api-key: mock-fixture-key",
            ),
            (
                AiProviderApiProtocol::Gemini,
                r#"{"models":[{"name":"models/gpt-test"}]}"#,
                r#"{"candidates":[{"content":{"parts":[{"text":"OK"}]}}]}"#,
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"OK\"}]},\"finishReason\":\"STOP\"}]}\n\n",
                "/v1/models/gpt-test:",
                "x-goog-api-key: mock-fixture-key",
            ),
            (
                AiProviderApiProtocol::Ollama,
                r#"{"models":[{"name":"gpt-test"}]}"#,
                r#"{"message":{"content":"OK"},"done":true}"#,
                "{\"message\":{\"content\":\"OK\"},\"done\":false}\n{\"done\":true}\n",
                "/v1/api/chat",
                "authorization: Bearer mock-fixture-key",
            ),
        ] {
            let (base, server) = mock_server(discovery.into(), "application/json");
            let mut settings = responses_settings(base);
            // Intentionally use a brand that does not match any tested protocol.
            settings.provider_credentials[0].provider_kind = AiProviderKind::Deepseek;
            settings.provider_credentials[0].api_protocol = Some(protocol);
            settings.provider_credentials[0].api_format = AiApiFormat::ChatCompletions;
            settings.models[0].provider_kind = Some(AiProviderKind::Deepseek);
            let models =
                discover_provider_models(&settings, &settings.provider_credentials[0]).unwrap();
            assert_eq!(models[0].credential_id.as_deref(), Some("mock-responses"));
            let raw = server.join().unwrap();
            assert!(raw.contains(auth), "{protocol:?}: {raw}");
            assert!(
                raw.starts_with(if protocol == AiProviderApiProtocol::Ollama {
                    "GET /v1/api/tags"
                } else {
                    "GET /v1/models"
                })
            );
            let (base, server) = mock_server(response.into(), "application/json");
            settings.provider_credentials[0].base_url = Some(base);
            assert_eq!(
                complete_native_chat(&settings, &request(), &[])
                    .unwrap()
                    .text,
                "OK"
            );
            let raw = server.join().unwrap();
            assert!(
                raw.starts_with(&format!("POST {path}")),
                "{protocol:?}: {raw}"
            );
            assert!(raw.contains(auth));
            let (base, server) = mock_server(stream.into(), "text/event-stream");
            settings.provider_credentials[0].base_url = Some(base);
            let mut deltas = Vec::new();
            assert_eq!(
                stream_native_chat(&settings, &request(), &[], |delta| deltas.push(delta))
                    .unwrap()
                    .text,
                "OK"
            );
            let raw = server.join().unwrap();
            assert!(raw.starts_with(&format!("POST {path}")));
            assert!(
                deltas.iter().any(|delta| delta.done),
                "{protocol:?} missing done"
            );
        }
    }

    #[test]
    fn model_connection_test_uses_fixed_prompt_auto_reasoning_and_bounded_output() {
        let (base, server) = mock_server(
            r#"{"choices":[{"message":{"content":"OK"}}]}"#.into(),
            "application/json",
        );
        let mut settings = responses_settings(base);
        settings.provider_credentials[0].api_format = AiApiFormat::ChatCompletions;
        settings.models[0].enabled = false;
        super::test_model_connection(&settings, "mock-responses:gpt-test").unwrap();
        assert!(!settings.models[0].enabled);
        let raw = server.join().unwrap();
        let body: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["max_tokens"], 64);
        assert_eq!(
            body["messages"][1]["content"],
            "Confirm that this model can respond."
        );
        assert!(body.get("tools").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn model_discovery_rejects_invalid_response_and_accepts_empty_results() {
        for (response, succeeds) in [(r#"{"data":[]}"#, true), ("{}", false), ("not-json", false)] {
            let (base, server) = mock_server(response.into(), "application/json");
            let settings = responses_settings(base);
            let result =
                super::discover_provider_models(&settings, &settings.provider_credentials[0]);
            assert_eq!(result.is_ok(), succeeds);
            if succeeds {
                assert!(result.unwrap().is_empty());
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn cancelled_discovery_does_not_open_a_provider_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let settings = responses_settings(format!("http://{}/v1", listener.local_addr().unwrap()));
        let error = super::discover_provider_models_cancellable(
            &settings,
            &settings.provider_credentials[0],
            &|| true,
        )
        .unwrap_err();
        assert!(error.contains("cancelled"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn discovery_cancelled_during_a_response_does_not_request_the_next_page() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut settings =
            responses_settings(format!("http://{}/v1", listener.local_addr().unwrap()));
        settings.provider_credentials[0].api_protocol = Some(AiProviderApiProtocol::Anthropic);
        let cancelled = Arc::new(AtomicBool::new(false));
        let server_cancelled = Arc::clone(&cancelled);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut stream);
            server_cancelled.store(true, Ordering::Release);
            let body = r#"{"data":[{"id":"fixture-model"}],"has_more":true,"last_id":"cursor"}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            listener
        });
        let error = super::discover_provider_models_cancellable(
            &settings,
            &settings.provider_credentials[0],
            &|| cancelled.load(Ordering::Acquire),
        )
        .unwrap_err();
        assert!(error.contains("cancelled"));
        let listener = server.join().unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn discovery_follows_protocol_pagination_and_rejects_repeated_cursors() {
        for protocol in [
            AiProviderApiProtocol::Anthropic,
            AiProviderApiProtocol::Gemini,
        ] {
            for repeat in [false, true] {
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let base = format!("http://{}/v1", listener.local_addr().unwrap());
                let server = thread::spawn(move || {
                    let mut requests = Vec::new();
                    for page in 0..2 {
                        let (mut stream, _) = listener.accept().unwrap();
                        requests.push(read_http_request(&mut stream));
                        let more = page == 0 || repeat;
                        let body = if protocol == AiProviderApiProtocol::Anthropic {
                            serde_json::json!({"data":[{"id":format!("model-{page}")}],"has_more":more,"last_id":"cursor"})
                        } else { serde_json::json!({"models":[{"name":format!("models/model-{page}")}],"nextPageToken":if more { "cursor" } else { "" }}) }.to_string();
                        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                    }
                    requests
                });
                let mut settings = responses_settings(base);
                settings.provider_credentials[0].api_protocol = Some(protocol);
                let result =
                    super::discover_provider_models(&settings, &settings.provider_credentials[0]);
                assert_eq!(result.is_err(), repeat);
                if !repeat {
                    assert_eq!(result.unwrap().len(), 2);
                }
                let requests = server.join().unwrap();
                assert!(
                    requests[1].contains(if protocol == AiProviderApiProtocol::Gemini {
                        "pageToken=cursor"
                    } else {
                        "after_id=cursor"
                    })
                );
            }
        }
    }
    #[test]
    fn discovery_authentication_errors_do_not_expose_response_body() {
        let (base, server) = mock_server_status(
            "fixture-sensitive-error-body".into(),
            "application/json",
            "401 Unauthorized",
        );
        let settings = responses_settings(base);
        let error = super::discover_provider_models(&settings, &settings.provider_credentials[0])
            .unwrap_err();
        assert!(error.contains("401"));
        assert!(!error.contains("fixture-sensitive"));
        server.join().unwrap();
    }
    #[test]
    fn chat_timeout_is_enforced_by_the_http_adapter() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            std::thread::sleep(std::time::Duration::from_millis(250));
            request
        });
        let mut settings = responses_settings(base);
        settings.timeout_ms = 100;
        assert!(
            complete_native_chat(&settings, &request(), &[])
                .unwrap_err()
                .to_lowercase()
                .contains("timed out")
        );
        server.join().unwrap();
    }
}
