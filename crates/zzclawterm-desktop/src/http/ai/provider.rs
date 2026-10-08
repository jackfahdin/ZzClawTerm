use serde_json::json;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use zzclawterm_core::ai::provider_settings::{
    ProviderModelCursor, effective_protocol, is_builtin_provider, parse_provider_model_page,
};
use zzclawterm_core::ai::{
    AiBackendKind, AiChatCompletion, AiChatRequest, AiChatStreamDelta, AiMessage, AiModelDiscovery,
    AiProviderApiProtocol, AiProviderCredential, AiReasoningEffort, AiSettings, ResolvedAiModel,
};

pub fn discover_provider_models(
    settings: &AiSettings,
    credential: &AiProviderCredential,
) -> Result<Vec<AiModelDiscovery>, String> {
    discover_provider_models_cancellable(settings, credential, &|| false)
}

pub(crate) fn discover_provider_models_cancellable(
    settings: &AiSettings,
    credential: &AiProviderCredential,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<AiModelDiscovery>, String> {
    if is_cancelled() {
        return Err("AI model discovery cancelled".into());
    }
    let base = credential
        .base_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .ok_or("API base URL is required")?;
    let protocol = effective_protocol(credential);
    let path = if protocol == AiProviderApiProtocol::Ollama {
        "api/tags"
    } else {
        "models"
    };
    let url =
        zzclawterm_core::ai::ollama::ollama_url(base, path).map_err(|error| error.to_string())?;
    let client =
        super::proxy::apply_proxy(zed_reqwest::blocking::Client::builder(), &settings.proxy)?
            .timeout(Duration::from_secs(15))
            .user_agent(zzclawterm_core::effective_ai_request_user_agent(settings))
            .build()
            .map_err(|error| error.to_string())?;
    let mut names = Vec::new();
    let mut cursor: Option<ProviderModelCursor> = None;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100 {
        if is_cancelled() {
            return Err("AI model discovery cancelled".into());
        }
        let mut request = client.get(&url);
        match protocol {
            AiProviderApiProtocol::Anthropic => {
                request = request
                    .header("anthropic-version", "2023-06-01")
                    .query(&[("limit", "1000")]);
            }
            AiProviderApiProtocol::Gemini => {
                request = request.query(&[("pageSize", "1000")]);
            }
            _ => {}
        }
        if let Some(cursor) = &cursor {
            request = request.query(&[cursor.query()]);
        }
        if let Some(key) = credential
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
        {
            request = match protocol {
                AiProviderApiProtocol::Anthropic => request.header("x-api-key", key),
                AiProviderApiProtocol::Gemini => request.header("x-goog-api-key", key),
                _ => request.bearer_auth(key),
            };
        }
        let response = request.send().map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Provider returned {}", response.status()));
        }
        let raw = response
            .text()
            .map_err(|_| "Cannot read provider model response")?;
        if is_cancelled() {
            return Err("AI model discovery cancelled".into());
        }
        let page = parse_provider_model_page(protocol, &raw).map_err(|error| error.to_string())?;
        names.extend(page.names);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
        if !seen.insert(cursor.clone()) {
            return Err("Provider pagination repeated a cursor".into());
        }
    }
    if cursor.is_some() {
        return Err("Provider pagination exceeded 100 pages".into());
    }
    names.sort();
    names.dedup();
    Ok(names
        .into_iter()
        .map(|name| AiModelDiscovery {
            id: if is_builtin_provider(&credential.id) {
                zzclawterm_core::ai_model_id_for_provider(&credential.provider_kind, &name)
            } else {
                zzclawterm_core::ai_model_id_for_credential(&credential.id, &name)
            },
            name,
            source: zzclawterm_core::AiModelSource::RustGenai,
            provider_kind: Some(credential.provider_kind.clone()),
            credential_id: (!is_builtin_provider(&credential.id)).then(|| credential.id.clone()),
        })
        .collect())
}

pub fn complete_ollama(
    client: &zed_reqwest::blocking::Client,
    credential: &AiProviderCredential,
    settings: &AiSettings,
    request: &AiChatRequest,
    history: &[AiMessage],
    model: &ResolvedAiModel,
    mut on_delta: Option<&mut dyn FnMut(AiChatStreamDelta)>,
) -> Result<AiChatCompletion, String> {
    let url = zzclawterm_core::ai::ollama::ollama_url(
        credential
            .base_url
            .as_deref()
            .unwrap_or("http://localhost:11434/"),
        "api/chat",
    )
    .map_err(|error| error.to_string())?;
    let body = zzclawterm_core::ai::ollama::request_body(
        model,
        request,
        settings,
        history,
        on_delta.is_some(),
    );
    let mut http = client.post(url).json(&body);
    if let Some(key) = credential.api_key.as_deref().filter(|key| !key.is_empty()) {
        http = http.bearer_auth(key);
    }
    let response = http.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Ollama returned {}", response.status()));
    }
    if let Some(callback) = on_delta.as_mut() {
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();
        for line in BufReader::new(response).lines() {
            let line = line.map_err(|error| error.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(completion) = zzclawterm_core::ai::ollama::parse_response(&line) {
                tool_calls.extend(completion.tool_calls);
            }
            let mut done = false;
            for delta in zzclawterm_core::ai::ollama::parse_stream(&line)
                .map_err(|error| error.to_string())?
            {
                text.push_str(&delta.text_delta);
                if let Some(value) = &delta.reasoning_delta {
                    reasoning.push_str(value);
                }
                done |= delta.done;
                callback(delta);
            }
            if done {
                break;
            }
        }
        if text.is_empty() && tool_calls.is_empty() {
            return Err("Ollama returned no content".into());
        }
        Ok(AiChatCompletion {
            text,
            reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
            tool_calls,
        })
    } else {
        zzclawterm_core::ai::ollama::parse_response(
            &response.text().map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}

pub fn test_model_connection(settings: &AiSettings, model_id: &str) -> Result<(), String> {
    let mut settings = settings.clone();
    let model = settings
        .models
        .iter_mut()
        .find(|model| model.id == model_id)
        .ok_or("Model no longer exists")?;
    if model.backend != AiBackendKind::Genai {
        return Err("External agent models do not support this test".into());
    }
    model.enabled = true;
    settings.default_reasoning_effort = AiReasoningEffort::Auto;
    settings.timeout_ms = settings.timeout_ms.clamp(1_000, 60_000);
    let mut request: AiChatRequest = serde_json::from_value(json!({
        "action": "explain_output", "mode": "ask", "agentKind": "zzclawterm", "modelId": model_id,
        "userInput": "Reply with OK.", "context": {}, "options": {}
    }))
    .map_err(|error| error.to_string())?;
    request.options.connectivity_test = true;
    super::complete_native_chat(&settings, &request, &[]).map(|_| ())
}
