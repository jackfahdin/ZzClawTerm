//! Provider settings, model catalog, credential and custom-action state transitions.

use std::collections::{HashMap, HashSet};

use gpui::FocusHandle;
use nyaterm_core::{
    AgentCommandExecutionMode, AiAgentKind, AiCustomActionConfig, AiMode, AiModelConfigItem,
    AiModelDiscovery, AiModelSource, AiPermissionMode, AiProviderCredential, AiProviderKind,
    AiSettings, CodexThreadMode, ExternalMcpSessionScope, RiskLevel, ai_model_id_for_credential,
    ai_model_id_for_provider, merge_model_discoveries, now_rfc3339,
};

use crate::models::{AiActionEditorField, AiActionListKind};

use super::{AiFeatureState, AiFullAccessSetting, AiSettingsMutation, AiSettingsState};

fn is_builtin_ai_provider_id(id: &str) -> bool {
    matches!(
        id,
        "openai"
            | "anthropic"
            | "gemini"
            | "deepseek"
            | "ollama"
            | "xai"
            | "cohere"
            | "mimo"
            | "zai"
            | "groq"
    )
}

fn seed_builtin_ai_models_for_provider(settings: &mut AiSettings, provider_kind: &AiProviderKind) {
    let names: &[&str] = match provider_kind {
        AiProviderKind::Openai => &[
            "gpt-4o-mini",
            "gpt-4o",
            "gpt-4.1",
            "gpt-4.1-mini",
            "o3-mini",
            "o4-mini",
        ],
        AiProviderKind::Anthropic => &[
            "claude-3-haiku-20240307",
            "claude-3-5-sonnet-20241022",
            "claude-sonnet-4-20250514",
        ],
        AiProviderKind::Gemini => &["gemini-2.0-flash", "gemini-1.5-pro"],
        AiProviderKind::Deepseek => &["deepseek-chat", "deepseek-reasoner"],
        AiProviderKind::Ollama => &["llama3", "llama3.1", "qwen2.5"],
        AiProviderKind::Xai => &["grok-3", "grok-2"],
        AiProviderKind::Cohere => &["command-a-03-2025", "command-r-plus"],
        AiProviderKind::Mimo => &["mimo-v2.5-pro"],
        AiProviderKind::Zai => &["glm-4", "glm-4-flash"],
        AiProviderKind::Groq => &["llama-3.3-70b-versatile"],
        AiProviderKind::OpenaiCompatible => &[],
    };
    let existing: HashSet<String> = settings
        .models
        .iter()
        .map(|model| model.id.clone())
        .collect();
    for name in names {
        let model_id = ai_model_id_for_provider(provider_kind, name);
        if existing.contains(&model_id) {
            continue;
        }
        settings.models.push(AiModelConfigItem {
            backend: Default::default(),
            id: model_id,
            name: (*name).to_string(),
            provider_kind: Some(provider_kind.clone()),
            credential_id: None,
            enabled: false,
            source: AiModelSource::RustGenai,
            last_seen_at: None,
        });
    }
}

impl AiFeatureState {
    pub(in crate::features) fn settings_config(&self) -> &AiSettings {
        &self.settings.config
    }

    pub(in crate::features) fn settings_config_cloned(&self) -> AiSettings {
        self.settings.config.clone()
    }

    pub(in crate::features) fn set_settings_default_agent(&mut self, kind: AiAgentKind) {
        self.settings.config.default_agent_kind = kind;
    }

    pub(in crate::features) fn request_settings_permission_mode(
        &mut self,
        setting: AiFullAccessSetting,
        mode: AiPermissionMode,
    ) -> bool {
        if mode == AiPermissionMode::FullAccess {
            self.settings.pending_full_access = Some(setting);
            return false;
        }
        self.settings.pending_full_access = None;
        self.apply_settings_permission_mode(setting, mode);
        true
    }

    pub(in crate::features) fn pending_full_access_setting(&self) -> Option<AiFullAccessSetting> {
        self.settings.pending_full_access
    }

    pub(in crate::features) fn cancel_settings_full_access(&mut self) {
        self.settings.pending_full_access = None;
    }

    pub(in crate::features) fn confirm_settings_full_access(
        &mut self,
    ) -> Option<AiFullAccessSetting> {
        let setting = self.settings.pending_full_access.take()?;
        self.apply_settings_permission_mode(setting, AiPermissionMode::FullAccess);
        Some(setting)
    }

    fn apply_settings_permission_mode(
        &mut self,
        setting: AiFullAccessSetting,
        mode: AiPermissionMode,
    ) {
        match setting {
            AiFullAccessSetting::ExternalAgent => {
                self.settings.config.external_agent_permission_mode = mode
            }
            AiFullAccessSetting::Codex => self.settings.config.codex.permission_mode = mode,
            AiFullAccessSetting::ClaudeCode => {
                self.settings.config.claude_code.permission_mode = mode
            }
            AiFullAccessSetting::McpHost => {
                self.settings.config.external_mcp.permission_mode = mode
            }
        }
    }

    pub(in crate::features) fn toggle_settings_codex_enabled(&mut self) {
        self.settings.config.codex.enabled = !self.settings.config.codex.enabled;
    }

    pub(in crate::features) fn toggle_settings_codex_mcp_integration(&mut self) {
        toggle_mcp_integration(&mut self.settings.config.codex.tool_integration_mode);
    }

    pub(in crate::features) fn toggle_settings_claude_enabled(&mut self) {
        self.settings.config.claude_code.enabled = !self.settings.config.claude_code.enabled;
    }

    pub(in crate::features) fn toggle_settings_claude_mcp_integration(&mut self) {
        toggle_mcp_integration(&mut self.settings.config.claude_code.tool_integration_mode);
    }

    pub(in crate::features) fn toggle_settings_mcp_enabled(&mut self) {
        self.settings.config.external_mcp.enabled = !self.settings.config.external_mcp.enabled;
    }

    pub(in crate::features) fn set_settings_codex_thread_mode(&mut self, mode: CodexThreadMode) {
        self.settings.config.codex.thread_mode = mode;
    }

    pub(in crate::features) fn set_settings_mcp_scope(&mut self, scope: ExternalMcpSessionScope) {
        self.settings.config.external_mcp.session_scope = scope;
    }

    pub(in crate::features) fn settings_enabled(&self) -> bool {
        self.settings.config.enabled
    }

    pub(in crate::features) fn settings_max_agent_steps(&self) -> u16 {
        self.settings.config.max_agent_steps.unwrap_or(10).max(1)
    }

    pub(in crate::features) fn settings_context_line_limit(&self) -> usize {
        self.settings.config.context_line_limit as usize
    }

    pub(in crate::features) fn sync_settings_active_profile_drafts(
        &mut self,
        model: String,
        base_url: String,
    ) {
        self.settings.model_draft = model;
        self.settings.base_url_draft = base_url;
        self.settings.secret_draft.expose_secret_mut().clear();
    }

    pub(in crate::features) fn pending_settings(&self) -> AiSettings {
        let mut next = self.settings.config.clone();
        let active_id = next.active_profile_id.clone();
        let mut active_kind = None;
        let mut active_name = active_id.clone();
        let mut active_base_url = (!self.settings.base_url_draft.trim().is_empty())
            .then(|| self.settings.base_url_draft.trim().to_string());
        let active_model = self.settings.model_draft.trim().to_string();

        if let Some(profile) = next
            .provider_profiles
            .iter_mut()
            .find(|profile| profile.id == active_id)
        {
            profile.enabled = true;
            if !active_model.is_empty() {
                profile.model = active_model.clone();
            }
            profile.base_url = active_base_url.clone();
            if !self.settings.secret_draft.is_empty() {
                profile.api_key = Some(self.settings.secret_draft.clone());
            }
            active_kind = Some(profile.provider_kind.clone());
            active_name = profile.name.clone();
            active_base_url = profile.base_url.clone();
        }

        if let Some(kind) = active_kind.clone() {
            let credential = AiProviderCredential {
                api_format: Default::default(),
                id: active_id.clone(),
                name: active_name,
                provider_kind: kind.clone(),
                base_url: active_base_url.clone(),
                api_key: if self.settings.secret_draft.is_empty() {
                    next.provider_credentials
                        .iter()
                        .find(|credential| credential.id == active_id)
                        .and_then(|credential| credential.api_key.clone())
                } else {
                    Some(self.settings.secret_draft.clone())
                },
                enabled: true,
            };
            if let Some(existing) = next
                .provider_credentials
                .iter_mut()
                .find(|credential| credential.id == active_id)
            {
                *existing = credential;
            } else {
                next.provider_credentials.push(credential);
            }

            if !active_model.is_empty() {
                let model_id = if active_base_url
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                    || kind == AiProviderKind::OpenaiCompatible
                {
                    ai_model_id_for_credential(&active_id, &active_model)
                } else {
                    ai_model_id_for_provider(&kind, &active_model)
                };
                let model_index = next
                    .models
                    .iter()
                    .position(|model| model.credential_id.as_deref() == Some(active_id.as_str()))
                    .or_else(|| next.models.iter().position(|model| model.id == model_id));
                if let Some(model_index) = model_index {
                    let model = &mut next.models[model_index];
                    model.id = model_id.clone();
                    model.name = active_model.clone();
                    model.provider_kind = Some(kind);
                    model.credential_id = active_base_url
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                        .then(|| active_id.clone());
                    model.enabled = true;
                } else {
                    next.models.push(AiModelConfigItem {
                        backend: Default::default(),
                        id: model_id.clone(),
                        name: active_model,
                        provider_kind: Some(kind),
                        credential_id: active_base_url
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty())
                            .then(|| active_id.clone()),
                        enabled: true,
                        source: AiModelSource::Manual,
                        last_seen_at: None,
                    });
                }
                next.default_model_id = Some(model_id);
            }
        }
        next
    }

    pub(in crate::features) fn settings_draft_snapshot(
        &self,
    ) -> (AiSettings, String, String, nyaterm_core::SecretString) {
        (
            self.settings.config.clone(),
            self.settings.model_draft.clone(),
            self.settings.base_url_draft.clone(),
            self.settings.secret_draft.clone(),
        )
    }

    pub(in crate::features) fn settings_draft_matches(
        &self,
        config: &AiSettings,
        model: &str,
        base_url: &str,
        secret: &nyaterm_core::SecretString,
    ) -> bool {
        &self.settings.config == config
            && self.settings.model_draft == model
            && self.settings.base_url_draft == base_url
            && self.settings.secret_draft == *secret
    }

    pub(in crate::features) fn replace_settings_config(
        &mut self,
        config: AiSettings,
        clear_secret_draft: bool,
    ) {
        self.settings.config = config;
        if clear_secret_draft {
            self.settings.secret_draft.expose_secret_mut().clear();
        }
    }

    pub(in crate::features) fn restore_settings_draft(
        &mut self,
        config: AiSettings,
        model: String,
        base_url: String,
        secret: nyaterm_core::SecretString,
    ) {
        self.settings.config = config;
        self.settings.model_draft = model;
        self.settings.base_url_draft = base_url;
        self.settings.secret_draft = secret;
    }

    pub(in crate::features) fn close_settings_editors(&mut self) {
        self.settings.action_edit = None;
        self.settings.manual_model_edit_group = None;
    }

    pub(in crate::features) fn settings_model_query(&self) -> &str {
        &self.settings.model_query
    }

    pub(in crate::features) fn clear_settings_model_query(&mut self) {
        self.settings.model_query.clear();
    }

    pub(in crate::features) fn settings_model_collapsed_groups(&self) -> &HashSet<String> {
        &self.settings.model_collapsed_groups
    }

    pub(in crate::features) fn settings_manual_model_drafts(&self) -> &HashMap<String, String> {
        &self.settings.manual_model_drafts
    }

    pub(in crate::features) fn settings_credential_secret_drafts(
        &self,
    ) -> &HashMap<String, String> {
        &self.settings.credential_secret_drafts
    }

    pub(in crate::features) fn settings_action_focus(&self) -> &FocusHandle {
        &self.settings.action_focus
    }

    pub(in crate::features) fn accept_saved_settings(&mut self, saved: AiSettings) {
        self.settings.config = saved;
    }

    pub(in crate::features) fn queue_settings_persistence(
        &mut self,
        snapshot: AiSettings,
    ) -> Option<(u64, AiSettings)> {
        self.settings.persistence_generation =
            self.settings.persistence_generation.saturating_add(1);
        self.settings.persistence_dirty = true;
        if self.settings.persistence_in_flight.is_some() {
            self.settings.persistence_pending = Some(snapshot);
            None
        } else {
            self.settings.persistence_in_flight = Some(self.settings.persistence_generation);
            Some((self.settings.persistence_generation, snapshot))
        }
    }

    pub(in crate::features) fn finish_settings_persistence(
        &mut self,
        generation: u64,
        succeeded: bool,
    ) -> super::AiSettingsPersistenceCompletion {
        if self.settings.persistence_in_flight != Some(generation) {
            return super::AiSettingsPersistenceCompletion {
                apply_result: false,
                report_result: false,
                next: None,
            };
        }
        self.settings.persistence_in_flight = None;
        let next = self.settings.persistence_pending.take().map(|snapshot| {
            let generation = self.settings.persistence_generation;
            self.settings.persistence_in_flight = Some(generation);
            (generation, snapshot)
        });
        let report_result = generation == self.settings.persistence_generation && next.is_none();
        let apply_result = succeeded && report_result;
        if apply_result {
            self.settings.persistence_dirty = false;
        }
        super::AiSettingsPersistenceCompletion {
            apply_result,
            report_result,
            next,
        }
    }

    pub(in crate::features) fn settings_persistence_is_dirty(&self) -> bool {
        self.settings.persistence_dirty
    }

    pub(in crate::features) fn toggle_settings_enabled(&mut self) {
        self.settings.config.enabled = !self.settings.config.enabled;
        self.panel.status = if self.settings.config.enabled {
            "AI enabled"
        } else {
            "AI disabled"
        }
        .to_string();
    }

    pub(in crate::features) fn set_settings_mode(&mut self, mode: AiMode) {
        self.settings.config.default_mode = mode;
        self.panel.status = "AI mode updated".to_string();
    }

    pub(in crate::features) fn set_settings_command_mode(
        &mut self,
        mode: AgentCommandExecutionMode,
    ) {
        self.settings.config.agent_command_execution_mode = mode;
        self.panel.status = "Agent command policy updated".to_string();
    }

    pub(in crate::features) fn toggle_settings_background_execution(&mut self) {
        self.settings.config.agent_background_execution_enabled =
            !self.settings.config.agent_background_execution_enabled;
        self.panel.status = if self.settings.config.agent_background_execution_enabled {
            "Agent background execution enabled"
        } else {
            "Agent background execution disabled"
        }
        .to_string();
    }

    pub(in crate::features) fn toggle_settings_redaction(&mut self) {
        self.settings.config.redaction_enabled = !self.settings.config.redaction_enabled;
        self.panel.status = "AI redaction updated".to_string();
    }

    pub(in crate::features) fn toggle_settings_allow_save_command(&mut self) {
        self.settings.config.allow_save_command = !self.settings.config.allow_save_command;
        self.panel.status = "AI command saving updated".to_string();
    }

    pub(in crate::features) fn toggle_settings_record_history(&mut self) {
        self.settings.config.record_history = !self.settings.config.record_history;
        self.panel.status = "AI history recording updated".to_string();
    }

    pub(in crate::features) fn set_settings_context_line_limit(&mut self, value: u32) {
        self.settings.config.context_line_limit = value.clamp(50, 500);
        self.panel.status = "AI context line limit updated".to_string();
    }

    pub(in crate::features) fn set_settings_timeout_ms(&mut self, value: u64) {
        self.settings.config.timeout_ms = value.clamp(5_000, 300_000);
        self.panel.status = "AI timeout updated".to_string();
    }

    pub(in crate::features) fn set_settings_agent_steps(&mut self, value: u16) {
        self.settings.config.max_agent_steps = Some(value.clamp(1, 50));
        self.panel.status = "AI Agent max steps updated".to_string();
    }

    pub(in crate::features) fn set_settings_agent_step_timeout_ms(&mut self, value: u64) {
        self.settings.config.agent_step_timeout_ms = Some(value.clamp(5_000, 120_000));
        self.panel.status = "AI Agent step timeout updated".to_string();
    }

    pub(in crate::features) fn set_settings_terminal_output_lines(&mut self, value: u16) {
        self.settings.config.terminal_output_lines = value.clamp(0, 100);
        self.panel.status = "AI terminal output lines updated".to_string();
    }

    pub(in crate::features) fn set_settings_file_size_mb(&mut self, value: u64) {
        let mb = 1024 * 1024;
        self.settings.config.max_ai_file_size_bytes = value.clamp(1, 256) * mb;
        self.panel.status = "AI file size limit updated".to_string();
    }

    pub(in crate::features) fn set_settings_smart_auto_execute_max_risk(
        &mut self,
        risk: RiskLevel,
    ) {
        self.settings.config.agent_smart_auto_execute_max_risk = risk;
        self.panel.status = "AI smart auto-execute risk updated".to_string();
    }

    fn select_first_enabled_model(&mut self) {
        self.settings.config.default_model_id = self
            .settings
            .config
            .models
            .iter()
            .find(|model| model.enabled)
            .map(|model| model.id.clone());
    }

    pub(super) fn default_model_is_enabled(&self) -> bool {
        self.settings
            .config
            .default_model_id
            .as_ref()
            .is_some_and(|id| {
                self.settings
                    .config
                    .models
                    .iter()
                    .any(|model| model.enabled && model.id == *id)
            })
    }

    fn configured_default_model_is_disabled(&self) -> bool {
        self.settings.config.default_model_id.is_some() && !self.default_model_is_enabled()
    }

    pub(in crate::features) fn toggle_settings_model_enabled(&mut self, model_id: &str) {
        if let Some(model) = self
            .settings
            .config
            .models
            .iter_mut()
            .find(|model| model.id == model_id)
        {
            model.enabled = !model.enabled;
            self.panel.status = "AI model list updated".to_string();
        }
        if !self.default_model_is_enabled() {
            self.select_first_enabled_model();
        }
    }

    pub(in crate::features) fn set_settings_model_query(&mut self, text: String) {
        self.settings.model_query = text;
    }

    pub(in crate::features) fn set_settings_default_model(&mut self, model_id: &str) {
        if let Some(model) = self
            .settings
            .config
            .models
            .iter_mut()
            .find(|model| model.id == model_id)
        {
            model.enabled = true;
            self.settings.config.default_model_id = Some(model.id.clone());
            self.panel.status = "AI default model updated".to_string();
        }
    }

    pub(in crate::features) fn remove_settings_manual_model(
        &mut self,
        model_id: &str,
    ) -> AiSettingsMutation {
        let Some(model) = self
            .settings
            .config
            .models
            .iter()
            .find(|model| model.id == model_id)
            .cloned()
        else {
            return AiSettingsMutation::Ignored;
        };
        if model.source != AiModelSource::Manual {
            self.panel.status = "Only manual models can be deleted".to_string();
            return AiSettingsMutation::Notify;
        }
        self.settings
            .config
            .models
            .retain(|item| item.id != model_id);
        if self.settings.config.default_model_id.as_deref() == Some(model_id) {
            self.select_first_enabled_model();
        }
        self.panel.status = format!("Deleted manual model {}", model.name);
        AiSettingsMutation::Persist
    }

    pub(in crate::features) fn add_settings_manual_model(
        &mut self,
        credential_id: &str,
        name: &str,
    ) -> AiSettingsMutation {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.panel.status = "Manual model name is required".to_string();
            return AiSettingsMutation::Notify;
        }
        let Some(credential) = self
            .settings
            .config
            .provider_credentials
            .iter()
            .find(|credential| credential.id == credential_id)
            .cloned()
        else {
            self.panel.status = "Credential not found".to_string();
            return AiSettingsMutation::Notify;
        };
        let builtin = is_builtin_ai_provider_id(&credential.id);
        let model_id = if builtin {
            ai_model_id_for_provider(&credential.provider_kind, &name)
        } else {
            ai_model_id_for_credential(&credential.id, &name)
        };
        if let Some(existing) = self
            .settings
            .config
            .models
            .iter_mut()
            .find(|model| model.id == model_id)
        {
            existing.enabled = true;
            existing.name = name.clone();
            existing.provider_kind = Some(credential.provider_kind.clone());
            existing.credential_id = (!builtin).then(|| credential.id.clone());
            self.settings.config.default_model_id = Some(model_id);
            self.panel.status = format!("Enabled model {name}");
            return AiSettingsMutation::Persist;
        }
        self.settings.config.models.insert(
            0,
            AiModelConfigItem {
                backend: Default::default(),
                id: model_id.clone(),
                name: name.clone(),
                provider_kind: Some(credential.provider_kind),
                credential_id: (!builtin).then_some(credential.id),
                enabled: true,
                source: AiModelSource::Manual,
                last_seen_at: None,
            },
        );
        if !self.default_model_is_enabled() {
            self.settings.config.default_model_id = Some(model_id);
        }
        self.panel.status = format!("Added manual model {name}");
        AiSettingsMutation::Persist
    }

    pub(in crate::features) fn toggle_settings_model_group(&mut self, group_key: String) {
        if !self.settings.model_collapsed_groups.remove(&group_key) {
            self.settings.model_collapsed_groups.insert(group_key);
        }
    }

    pub(in crate::features) fn begin_settings_manual_model_edit(&mut self, group_key: &str) {
        self.settings.manual_model_edit_group = Some(group_key.to_string());
    }

    pub(in crate::features) fn cancel_settings_manual_model_edit(&mut self) -> FocusHandle {
        self.settings.manual_model_edit_group = None;
        self.settings.manual_model_focus.clone()
    }

    pub(in crate::features) fn settings_manual_model_submission(
        &self,
        group_key: &str,
    ) -> Option<(String, String)> {
        let credential_id = self
            .settings
            .config
            .provider_credentials
            .iter()
            .find(|credential| credential.id == group_key)?
            .id
            .clone();
        let draft = self
            .settings
            .manual_model_drafts
            .get(group_key)
            .cloned()
            .unwrap_or_default();
        Some((credential_id, draft))
    }

    pub(in crate::features) fn apply_settings_manual_model_input(
        &mut self,
        group_key: &str,
        text: String,
    ) -> bool {
        if !self
            .settings
            .config
            .provider_credentials
            .iter()
            .any(|credential| credential.id == group_key)
        {
            return false;
        }
        self.settings
            .manual_model_drafts
            .insert(group_key.to_string(), text);
        self.settings.manual_model_edit_group = Some(group_key.to_string());
        true
    }

    pub(in crate::features) fn settings_manual_model_draft(&self, group_key: &str) -> String {
        self.settings
            .manual_model_drafts
            .get(group_key)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::features) fn focus_settings_manual_model_edit(&mut self, group_key: String) {
        self.settings.manual_model_edit_group = Some(group_key);
    }

    pub(in crate::features) fn clear_settings_manual_model_draft(&mut self, group_key: &str) {
        self.settings
            .manual_model_drafts
            .insert(group_key.to_string(), String::new());
    }

    pub(in crate::features) fn toggle_settings_credential_enabled(
        &mut self,
        credential_id: &str,
    ) -> AiSettingsMutation {
        let Some(index) = self
            .settings
            .config
            .provider_credentials
            .iter()
            .position(|credential| credential.id == credential_id)
        else {
            return AiSettingsMutation::Ignored;
        };
        let enabled = !self.settings.config.provider_credentials[index].enabled;
        let name = self.settings.config.provider_credentials[index]
            .name
            .clone();
        let provider_kind = self.settings.config.provider_credentials[index]
            .provider_kind
            .clone();
        self.settings.config.provider_credentials[index].enabled = enabled;
        if let Some(profile) = self
            .settings
            .config
            .provider_profiles
            .iter_mut()
            .find(|profile| profile.id == credential_id)
        {
            profile.enabled = enabled;
        }
        if is_builtin_ai_provider_id(credential_id) {
            if enabled {
                seed_builtin_ai_models_for_provider(&mut self.settings.config, &provider_kind);
            } else {
                self.settings.config.models.retain(|model| {
                    model.provider_kind.as_ref() != Some(&provider_kind)
                        || model.credential_id.is_some()
                });
                if self.configured_default_model_is_disabled() {
                    self.select_first_enabled_model();
                }
            }
        }
        self.panel.status = format!(
            "AI credential {name} {}",
            if enabled { "enabled" } else { "disabled" }
        );
        AiSettingsMutation::Persist
    }

    pub(in crate::features) fn apply_settings_credential_input(
        &mut self,
        rest: &str,
        text: String,
    ) -> bool {
        let Some((credential_id, field)) = rest.rsplit_once('.') else {
            return false;
        };
        match field {
            "api-key" => {
                self.settings
                    .credential_secret_drafts
                    .insert(credential_id.to_string(), text);
            }
            "name" | "base-url" => {
                let Some(credential) = self
                    .settings
                    .config
                    .provider_credentials
                    .iter_mut()
                    .find(|credential| credential.id == credential_id)
                else {
                    return false;
                };
                if field == "name" {
                    credential.name = text;
                } else {
                    credential.base_url = (!text.trim().is_empty()).then_some(text);
                }
            }
            _ => return false,
        }
        self.panel.status = "AI credential edited".to_string();
        true
    }

    pub(in crate::features) fn commit_settings_credential_edits(&mut self, credential_id: &str) {
        let secret_draft = self
            .settings
            .credential_secret_drafts
            .get(credential_id)
            .cloned()
            .unwrap_or_default();
        if let Some(credential) = self
            .settings
            .config
            .provider_credentials
            .iter_mut()
            .find(|credential| credential.id == credential_id)
        {
            if !secret_draft.is_empty() {
                credential.api_key = Some(secret_draft.clone().into());
            }
            let name = credential.name.clone();
            let base_url = credential.base_url.clone();
            let api_key = credential.api_key.clone();
            let enabled = credential.enabled;
            if let Some(profile) = self
                .settings
                .config
                .provider_profiles
                .iter_mut()
                .find(|profile| profile.id == credential_id)
            {
                profile.name = name;
                profile.base_url = base_url;
                if !secret_draft.is_empty() {
                    profile.api_key = Some(secret_draft.into());
                } else if api_key.is_some() {
                    // Preserve the stored masked/encrypted key through merge_masked.
                }
                profile.enabled = enabled;
            }
        }
        self.settings.credential_secret_drafts.remove(credential_id);
        self.panel.status = "AI credential saved".to_string();
    }

    pub(in crate::features) fn add_settings_credential(&mut self, id: String) -> FocusHandle {
        self.settings.config.provider_credentials.insert(
            0,
            AiProviderCredential {
                api_format: Default::default(),
                id: id.clone(),
                name: String::new(),
                provider_kind: AiProviderKind::OpenaiCompatible,
                base_url: Some(String::new()),
                api_key: None,
                enabled: true,
            },
        );
        self.panel.status = "AI credential added".to_string();
        self.settings.credential_focus.clone()
    }

    pub(in crate::features) fn remove_settings_credential(
        &mut self,
        credential_id: &str,
    ) -> AiSettingsMutation {
        if is_builtin_ai_provider_id(credential_id) {
            self.panel.status = "Built-in AI credentials cannot be deleted".to_string();
            return AiSettingsMutation::Notify;
        }
        self.settings
            .config
            .provider_credentials
            .retain(|credential| credential.id != credential_id);
        self.settings
            .config
            .models
            .retain(|model| model.credential_id.as_deref() != Some(credential_id));
        if self.configured_default_model_is_disabled() {
            self.select_first_enabled_model();
        }
        self.settings.credential_secret_drafts.remove(credential_id);
        self.panel.status = "AI credential removed".to_string();
        AiSettingsMutation::Persist
    }

    pub(in crate::features) fn settings_action_value(
        &self,
        kind: AiActionListKind,
        action_id: &str,
        field: AiActionEditorField,
    ) -> String {
        self.settings
            .action(kind, action_id)
            .map(|action| match field {
                AiActionEditorField::Name => action.name.clone(),
                AiActionEditorField::Prompt => action.prompt.clone(),
            })
            .unwrap_or_default()
    }

    pub(in crate::features) fn focus_settings_action(
        &mut self,
        kind: AiActionListKind,
        action_id: String,
        field: AiActionEditorField,
    ) {
        self.settings.action_edit = Some((kind, action_id, field));
    }

    pub(in crate::features) fn toggle_settings_action_enabled(
        &mut self,
        kind: AiActionListKind,
        action_id: &str,
    ) -> bool {
        let Some(action) = self.settings.action_mut(kind, action_id) else {
            return false;
        };
        action.enabled = !action.enabled;
        self.panel.status = "AI action toggled".to_string();
        true
    }

    pub(in crate::features) fn add_settings_action(&mut self, kind: AiActionListKind, id: String) {
        self.settings.actions_mut(kind).push(AiCustomActionConfig {
            id: id.clone(),
            name: "Custom AI action".to_string(),
            prompt: String::new(),
            enabled: true,
        });
        self.settings.action_edit = Some((kind, id, AiActionEditorField::Name));
        self.panel.status = "AI action added".to_string();
    }

    pub(in crate::features) fn remove_settings_action(
        &mut self,
        kind: AiActionListKind,
        action_id: &str,
    ) {
        self.settings
            .actions_mut(kind)
            .retain(|action| action.id != action_id);
        if self
            .settings
            .action_edit
            .as_ref()
            .is_some_and(|(edit_kind, id, _)| *edit_kind == kind && id == action_id)
        {
            self.settings.action_edit = None;
        }
        self.panel.status = "AI action removed".to_string();
    }

    pub(in crate::features) fn settings_action_edit(
        &self,
    ) -> Option<(AiActionListKind, String, AiActionEditorField)> {
        self.settings.action_edit.clone()
    }

    pub(in crate::features) fn cancel_settings_action_edit(&mut self) -> FocusHandle {
        self.settings.action_edit = None;
        self.settings.action_focus.clone()
    }

    pub(in crate::features) fn apply_settings_action_input(
        &mut self,
        kind: AiActionListKind,
        action_id: &str,
        field: AiActionEditorField,
        text: String,
    ) -> bool {
        let Some(action) = self.settings.action_mut(kind, action_id) else {
            return false;
        };
        match field {
            AiActionEditorField::Name => action.name = text,
            AiActionEditorField::Prompt => action.prompt = text,
        }
        self.settings.action_edit = Some((kind, action_id.to_string(), field));
        true
    }

    pub(in crate::features) fn discovery_settings(
        &self,
    ) -> (AiSettings, Vec<AiProviderCredential>) {
        let credentials = self
            .settings
            .config
            .provider_credentials
            .iter()
            .filter(|credential| {
                credential.enabled
                    && credential.provider_kind == AiProviderKind::OpenaiCompatible
                    && credential
                        .base_url
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
            })
            .cloned()
            .collect();
        (self.settings.config.clone(), credentials)
    }

    pub(in crate::features) fn apply_settings_model_discoveries(
        &mut self,
        discoveries: Vec<AiModelDiscovery>,
    ) -> usize {
        let discoveries = merge_model_discoveries(discoveries);
        let last_seen_at = Some(now_rfc3339());
        for discovery in &discoveries {
            if let Some(model) = self
                .settings
                .config
                .models
                .iter_mut()
                .find(|model| model.id == discovery.id)
            {
                model.name = discovery.name.clone();
                model.provider_kind = discovery.provider_kind.clone();
                model.credential_id = discovery.credential_id.clone();
                model.source = discovery.source.clone();
                model.last_seen_at = last_seen_at.clone();
            } else {
                self.settings.config.models.push(AiModelConfigItem {
                    backend: Default::default(),
                    id: discovery.id.clone(),
                    name: discovery.name.clone(),
                    provider_kind: discovery.provider_kind.clone(),
                    credential_id: discovery.credential_id.clone(),
                    enabled: false,
                    source: discovery.source.clone(),
                    last_seen_at: last_seen_at.clone(),
                });
            }
        }
        discoveries.len()
    }
}

fn toggle_mcp_integration(mode: &mut Option<String>) {
    if mode.as_deref() == Some("nyaterm_mcp") {
        *mode = None;
    } else {
        *mode = Some("nyaterm_mcp".to_string());
    }
}

impl AiSettingsState {
    fn actions(&self, kind: AiActionListKind) -> &[AiCustomActionConfig] {
        match kind {
            AiActionListKind::Terminal => &self.config.terminal_ai_actions,
            AiActionListKind::File => &self.config.file_ai_actions,
        }
    }

    fn actions_mut(&mut self, kind: AiActionListKind) -> &mut Vec<AiCustomActionConfig> {
        match kind {
            AiActionListKind::Terminal => &mut self.config.terminal_ai_actions,
            AiActionListKind::File => &mut self.config.file_ai_actions,
        }
    }

    fn action(&self, kind: AiActionListKind, action_id: &str) -> Option<&AiCustomActionConfig> {
        self.actions(kind)
            .iter()
            .find(|action| action.id == action_id)
    }

    fn action_mut(
        &mut self,
        kind: AiActionListKind,
        action_id: &str,
    ) -> Option<&mut AiCustomActionConfig> {
        self.actions_mut(kind)
            .iter_mut()
            .find(|action| action.id == action_id)
    }
}
