//! Provider identity, protocol selection and model capabilities, without UI or I/O.

use super::{
    AiApiFormat, AiBackendKind, AiModelConfigItem, AiModelReasoningEffort, AiProviderApiProtocol,
    AiProviderCredential, AiProviderKind, AiReasoningEffort,
};

pub const MODEL_REASONING_EFFORTS: [AiModelReasoningEffort; 8] = [
    AiModelReasoningEffort::None,
    AiModelReasoningEffort::Minimal,
    AiModelReasoningEffort::Low,
    AiModelReasoningEffort::Medium,
    AiModelReasoningEffort::High,
    AiModelReasoningEffort::XHigh,
    AiModelReasoningEffort::Max,
    AiModelReasoningEffort::Ultra,
];
pub const DEFAULT_MODEL_REASONING_EFFORTS: [AiModelReasoningEffort; 5] = [
    AiModelReasoningEffort::None,
    AiModelReasoningEffort::Low,
    AiModelReasoningEffort::Medium,
    AiModelReasoningEffort::High,
    AiModelReasoningEffort::XHigh,
];

pub fn effective_protocol(credential: &AiProviderCredential) -> AiProviderApiProtocol {
    credential
        .api_protocol
        .unwrap_or(match credential.provider_kind {
            AiProviderKind::Anthropic => AiProviderApiProtocol::Anthropic,
            AiProviderKind::Gemini => AiProviderApiProtocol::Gemini,
            AiProviderKind::Ollama
                if !credential
                    .base_url
                    .as_deref()
                    .unwrap_or_default()
                    .trim_end_matches('/')
                    .ends_with("/v1") =>
            {
                AiProviderApiProtocol::Ollama
            }
            _ => AiProviderApiProtocol::OpenaiCompatible,
        })
}

pub fn provider_requires_api_key(credential: &AiProviderCredential) -> bool {
    match effective_protocol(credential) {
        AiProviderApiProtocol::Ollama => false,
        AiProviderApiProtocol::OpenaiCompatible => !matches!(
            credential.provider_kind,
            AiProviderKind::Ollama | AiProviderKind::OpenaiCompatible
        ),
        AiProviderApiProtocol::Anthropic | AiProviderApiProtocol::Gemini => true,
    }
}

pub fn is_builtin_provider(id: &str) -> bool {
    matches!(
        id,
        "openai"
            | "anthropic"
            | "gemini"
            | "deepseek"
            | "groq"
            | "ollama"
            | "xai"
            | "cohere"
            | "mimo"
            | "zai"
    )
}

pub fn provider_label(kind: &AiProviderKind) -> &'static str {
    match kind {
        AiProviderKind::Openai => "OpenAI",
        AiProviderKind::Anthropic => "Anthropic",
        AiProviderKind::Gemini => "Gemini",
        AiProviderKind::Deepseek => "DeepSeek",
        AiProviderKind::Groq => "Groq",
        AiProviderKind::Ollama => "Ollama",
        AiProviderKind::Xai => "xAI",
        AiProviderKind::Cohere => "Cohere",
        AiProviderKind::Mimo => "MiMo",
        AiProviderKind::Zai => "Z.AI",
        AiProviderKind::OpenaiCompatible => "my-provider",
    }
}

pub fn provider_default_url(kind: &AiProviderKind) -> &'static str {
    match kind {
        AiProviderKind::Openai => "https://api.openai.com/v1/",
        AiProviderKind::Anthropic => "https://api.anthropic.com/v1/",
        AiProviderKind::Gemini => "https://generativelanguage.googleapis.com/v1beta/",
        AiProviderKind::Deepseek => "https://api.deepseek.com/v1/",
        AiProviderKind::Groq => "https://api.groq.com/openai/v1/",
        AiProviderKind::Ollama => "http://localhost:11434/",
        AiProviderKind::Xai => "https://api.x.ai/v1/",
        AiProviderKind::Cohere => "https://api.cohere.com/compatibility/v1/",
        AiProviderKind::Mimo => "https://api.xiaomimimo.com/v1/",
        AiProviderKind::Zai => "https://open.bigmodel.cn/api/paas/v4/",
        AiProviderKind::OpenaiCompatible => "",
    }
}

pub fn model_belongs_to_provider(
    model: &AiModelConfigItem,
    credential: &AiProviderCredential,
) -> bool {
    if model.backend != AiBackendKind::Genai {
        return false;
    }
    if let Some(id) = &model.credential_id {
        return id == &credential.id;
    }
    // Legacy implicit IDs must never be captured by a second account of the same brand.
    is_builtin_provider(&credential.id)
        && model
            .id
            .split_once(':')
            .is_some_and(|(id, _)| id == credential.id)
}

pub fn provider_name_taken(
    name: &str,
    credentials: &[AiProviderCredential],
    exclude: &str,
) -> bool {
    let name = name.trim().to_lowercase();
    !name.is_empty()
        && credentials.iter().any(|credential| {
            credential.enabled
                && credential.id != exclude
                && credential.name.trim().to_lowercase() == name
        })
}

pub fn available_provider_name(
    base: &str,
    credentials: &[AiProviderCredential],
    exclude: &str,
) -> String {
    let mut name = base.to_string();
    let mut suffix = 2;
    while provider_name_taken(&name, credentials, exclude) {
        name = format!("{base}-{suffix}");
        suffix += 1;
    }
    name
}

pub fn protocol_value(credential: &AiProviderCredential) -> &'static str {
    match effective_protocol(credential) {
        AiProviderApiProtocol::OpenaiCompatible
            if credential.api_format == AiApiFormat::Responses =>
        {
            "responses"
        }
        AiProviderApiProtocol::OpenaiCompatible => "chat_completions",
        AiProviderApiProtocol::Anthropic => "anthropic",
        AiProviderApiProtocol::Gemini => "gemini",
        AiProviderApiProtocol::Ollama => "ollama",
    }
}

pub fn set_provider_protocol(credential: &mut AiProviderCredential, value: &str) {
    let previous = effective_protocol(credential);
    let protocol = match value {
        "anthropic" => AiProviderApiProtocol::Anthropic,
        "gemini" => AiProviderApiProtocol::Gemini,
        "ollama" => AiProviderApiProtocol::Ollama,
        _ => AiProviderApiProtocol::OpenaiCompatible,
    };
    if credential.provider_kind == AiProviderKind::Ollama {
        let url = credential
            .base_url
            .as_deref()
            .unwrap_or_default()
            .trim_end_matches('/');
        if previous == AiProviderApiProtocol::Ollama
            && protocol == AiProviderApiProtocol::OpenaiCompatible
            && url == "http://localhost:11434"
        {
            credential.base_url = Some("http://localhost:11434/v1/".into());
        } else if previous == AiProviderApiProtocol::OpenaiCompatible
            && protocol == AiProviderApiProtocol::Ollama
            && url == "http://localhost:11434/v1"
        {
            credential.base_url = Some("http://localhost:11434/".into());
        }
    }
    credential.api_protocol = Some(protocol);
    credential.api_format = if value == "responses" {
        AiApiFormat::Responses
    } else {
        AiApiFormat::ChatCompletions
    };
}

pub fn model_reasoning_options(model: Option<&AiModelConfigItem>) -> Vec<AiReasoningEffort> {
    let supported = model
        .and_then(|model| model.supported_reasoning_efforts.as_deref())
        .unwrap_or(&DEFAULT_MODEL_REASONING_EFFORTS);
    let mut options = vec![AiReasoningEffort::Auto];
    options.extend(
        MODEL_REASONING_EFFORTS
            .iter()
            .filter(|effort| supported.contains(effort))
            .map(|effort| match effort {
                AiModelReasoningEffort::None => AiReasoningEffort::None,
                AiModelReasoningEffort::Minimal => AiReasoningEffort::Minimal,
                AiModelReasoningEffort::Low => AiReasoningEffort::Low,
                AiModelReasoningEffort::Medium => AiReasoningEffort::Medium,
                AiModelReasoningEffort::High => AiReasoningEffort::High,
                AiModelReasoningEffort::XHigh => AiReasoningEffort::XHigh,
                AiModelReasoningEffort::Max => AiReasoningEffort::Max,
                AiModelReasoningEffort::Ultra => AiReasoningEffort::Ultra,
            }),
    );
    options
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderModelCursor {
    Anthropic(String),
    Gemini(String),
}
impl ProviderModelCursor {
    pub fn query(&self) -> (&str, &str) {
        match self {
            Self::Anthropic(value) => ("after_id", value),
            Self::Gemini(value) => ("pageToken", value),
        }
    }
}

pub struct ProviderModelPage {
    pub names: Vec<String>,
    pub next_cursor: Option<ProviderModelCursor>,
}

pub fn parse_provider_model_page(
    protocol: AiProviderApiProtocol,
    raw: &str,
) -> Result<ProviderModelPage, super::AiModelError> {
    use super::AiModelError;
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| AiModelError::InvalidModelsJson("Invalid provider model response".into()))?;
    let field = if matches!(
        protocol,
        AiProviderApiProtocol::Gemini | AiProviderApiProtocol::Ollama
    ) {
        "models"
    } else {
        "data"
    };
    let entries = value[field]
        .as_array()
        .ok_or_else(|| AiModelError::InvalidModelsJson("Invalid provider model list".into()))?;
    let names = entries
        .iter()
        .filter_map(|entry| {
            let name = entry[if field == "models" { "name" } else { "id" }]
                .as_str()?
                .trim();
            let name = if protocol == AiProviderApiProtocol::Gemini {
                name.strip_prefix("models/").unwrap_or(name)
            } else {
                name
            };
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect();
    let next_cursor = match protocol {
        AiProviderApiProtocol::Gemini => value["nextPageToken"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|value| ProviderModelCursor::Gemini(value.into())),
        AiProviderApiProtocol::Anthropic if value["has_more"] == true => {
            Some(ProviderModelCursor::Anthropic(
                value["last_id"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AiModelError::InvalidModelsJson(
                            "Provider pagination omitted its cursor".into(),
                        )
                    })?
                    .into(),
            ))
        }
        _ => None,
    };
    Ok(ProviderModelPage { names, next_cursor })
}

#[cfg(test)]
mod tests {
    use super::{
        available_provider_name, effective_protocol, model_belongs_to_provider,
        model_reasoning_options, provider_name_taken, set_provider_protocol,
    };
    use crate::ai::{
        AiApiFormat, AiModelConfigItem, AiProviderApiProtocol, AiProviderCredential,
        AiProviderKind, AiReasoningEffort, AiSettings, normalize_ai_settings,
    };
    use serde_json::json;

    fn credential(id: &str, kind: &str) -> AiProviderCredential {
        serde_json::from_value(
            json!({"id":id,"name":"Account","provider_kind":kind,"enabled":true}),
        )
        .unwrap()
    }
    #[test]
    fn same_brand_accounts_never_capture_implicit_builtin_models() {
        let model: AiModelConfigItem = serde_json::from_value(
            json!({"id":"openai:gpt-5","name":"gpt-5","provider_kind":"openai"}),
        )
        .unwrap();
        assert!(model_belongs_to_provider(
            &model,
            &credential("openai", "openai")
        ));
        assert!(!model_belongs_to_provider(
            &model,
            &credential("second-account", "openai")
        ));
    }
    #[test]
    fn provider_names_are_case_insensitive_and_generate_unused_suffixes() {
        let mut one = credential("one", "openai");
        one.name = "OpenAI".into();
        let mut two = credential("two", "openai");
        two.name = "OpenAI-2".into();
        let credentials = vec![one, two];
        assert!(provider_name_taken(" openai ", &credentials, "other"));
        assert!(!provider_name_taken("OPENAI", &credentials, "one"));
        assert_eq!(
            available_provider_name("OpenAI", &credentials, "other"),
            "OpenAI-3"
        );
    }
    #[test]
    fn explicit_protocol_overrides_brand_and_survives_normalization() {
        let mut account = credential("ollama", "ollama");
        account.base_url = Some("http://localhost:11434/".into());
        set_provider_protocol(&mut account, "responses");
        assert_eq!(
            account.base_url.as_deref(),
            Some("http://localhost:11434/v1/")
        );
        assert_eq!(account.api_format, AiApiFormat::Responses);
        let mut settings = AiSettings {
            provider_credentials: vec![account],
            ..AiSettings::default()
        };
        normalize_ai_settings(&mut settings);
        assert_eq!(
            settings.provider_credentials[0].base_url.as_deref(),
            Some("http://localhost:11434/v1/")
        );
        set_provider_protocol(&mut settings.provider_credentials[0], "ollama");
        assert_eq!(
            settings.provider_credentials[0].base_url.as_deref(),
            Some("http://localhost:11434/")
        );
        let mut custom = credential("custom", "anthropic");
        custom.base_url = Some("http://proxy.invalid/custom/v1/".into());
        set_provider_protocol(&mut custom, "chat_completions");
        assert_eq!(
            effective_protocol(&custom),
            AiProviderApiProtocol::OpenaiCompatible
        );
        assert_eq!(custom.provider_kind, AiProviderKind::Anthropic);
        assert_eq!(
            custom.base_url.as_deref(),
            Some("http://proxy.invalid/custom/v1/")
        );
    }
    #[test]
    fn old_and_tauri_capabilities_round_trip_and_always_include_auto() {
        let old: AiModelConfigItem =
            serde_json::from_value(json!({"id":"account:m","name":"m","credential_id":"account"}))
                .unwrap();
        assert_eq!(
            model_reasoning_options(Some(&old)),
            vec![
                AiReasoningEffort::Auto,
                AiReasoningEffort::None,
                AiReasoningEffort::Low,
                AiReasoningEffort::Medium,
                AiReasoningEffort::High,
                AiReasoningEffort::XHigh
            ]
        );
        let new: AiModelConfigItem = serde_json::from_value(json!({"id":"account:m","name":"m","supported_reasoning_efforts":["minimal","max","ultra"]})).unwrap();
        assert_eq!(
            model_reasoning_options(Some(&new)),
            vec![
                AiReasoningEffort::Auto,
                AiReasoningEffort::Minimal,
                AiReasoningEffort::Max,
                AiReasoningEffort::Ultra
            ]
        );
        assert_eq!(
            serde_json::from_value::<AiModelConfigItem>(serde_json::to_value(&new).unwrap())
                .unwrap(),
            new
        );
        let empty: AiModelConfigItem = serde_json::from_value(
            json!({"id":"account:m","name":"m","supported_reasoning_efforts":[]}),
        )
        .unwrap();
        assert_eq!(
            model_reasoning_options(Some(&empty)),
            vec![AiReasoningEffort::Auto]
        );
        let account: AiProviderCredential = serde_json::from_value(json!({"id":"custom","name":"Custom","provider_kind":"openai_compatible","icon_data_url":"data:image/png;base64,fixture","api_protocol":"gemini"})).unwrap();
        assert_eq!(effective_protocol(&account), AiProviderApiProtocol::Gemini);
        assert_eq!(
            serde_json::from_value::<AiProviderCredential>(serde_json::to_value(&account).unwrap())
                .unwrap(),
            account
        );
    }
    #[test]
    fn protocol_authentication_requirements_are_independent_of_brand() {
        let mut account = credential("custom", "anthropic");
        set_provider_protocol(&mut account, "ollama");
        assert!(
            crate::ai::validate_model_credential(&account.provider_kind, Some(&account)).is_ok()
        );
        account.provider_kind = AiProviderKind::Ollama;
        set_provider_protocol(&mut account, "gemini");
        assert!(
            crate::ai::validate_model_credential(&account.provider_kind, Some(&account)).is_err()
        );
        account.api_key = Some("fixture-key".into());
        assert!(
            crate::ai::validate_model_credential(&account.provider_kind, Some(&account)).is_ok()
        );
    }
}
