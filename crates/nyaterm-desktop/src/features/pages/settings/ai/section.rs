use rust_i18n::t;

use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::{
    AiAgentKind, AiPermissionMode, CodexThreadMode, ExternalMcpSessionScope, RiskLevel,
};
use nyaterm_ui::NyaSelectOption;

use crate::features::ai::McpHelperStatus;
use crate::features::mcp::McpHostStatus;
use crate::features::pages::settings::panel::SettingsPanel;
use crate::models::AiInputField;
use crate::theme::ThemePalette;

use super::super::{
    settings_form_row, settings_form_section, settings_input_control, settings_switch,
};

impl SettingsPanel {
    pub(in crate::features) fn ai_input(
        &mut self,
        _id: &'static str,
        label: impl Into<SharedString>,
        value: String,
        field: AiInputField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label: SharedString = label.into();
        let _ = (value, cx);
        self.existing_text_input_field(format!("ai.input.{}", field.input_key()), label, false)
    }

    pub(in crate::features) fn ai_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let selected_risk =
            ai_risk_value(&self.ai.settings_config().agent_smart_auto_execute_max_risk);
        let risk_options = [
            ("low", "ai.riskLow"),
            ("medium", "ai.riskMedium"),
            ("high", "ai.riskHigh"),
            ("critical", "ai.riskCritical"),
        ]
        .into_iter()
        .map(|(value, label)| NyaSelectOption::new(value, t!(label)))
        .collect();
        let settings = self.ai.settings_config().clone();
        let agent_options = vec![
            NyaSelectOption::new("nyaterm", t!("ai.agent.nyaterm")),
            NyaSelectOption::new("codex", t!("ai.agent.codex")),
            NyaSelectOption::new("claude_code", t!("ai.agent.claudeCode")),
        ];
        let permission_options = || {
            vec![
                NyaSelectOption::new("observer", t!("ai.permission.observer")),
                NyaSelectOption::new("confirm", t!("ai.permission.confirm")),
                NyaSelectOption::new("auto", t!("ai.permission.auto")),
                NyaSelectOption::new("full_access", t!("ai.permission.fullAccess")),
            ]
        };
        let any_full_access = [
            &settings.external_agent_permission_mode,
            &settings.codex.permission_mode,
            &settings.claude_code.permission_mode,
            &settings.external_mcp.permission_mode,
        ]
        .into_iter()
        .any(|mode| *mode == AiPermissionMode::FullAccess);

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(settings_form_section(
                palette,
                Some(t!("ai.general")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.enabled"),
                        None,
                        settings_switch(
                            palette,
                            "ai-enabled",
                            self.ai.settings_config().enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_enabled(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.redaction"),
                        None,
                        settings_switch(
                            palette,
                            "ai-redaction-toggle",
                            self.ai.settings_config().redaction_enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_redaction(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.allowSave"),
                        None,
                        settings_switch(
                            palette,
                            "ai-save-command-toggle",
                            self.ai.settings_config().allow_save_command,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_allow_save_command(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.recordHistory"),
                        None,
                        settings_switch(
                            palette,
                            "ai-history-toggle",
                            self.ai.settings_config().record_history,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_record_history(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.requestUserAgent"),
                        Some(SharedString::from(t!("ai.requestUserAgentDesc"))),
                        settings_input_control(
                            300.,
                            self.ai_input(
                                "ai-request-user-agent",
                                t!("ai.requestUserAgent"),
                                self.ai.settings_config().request_user_agent.clone(),
                                AiInputField::RequestUserAgent,
                                cx,
                            ),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.contextLineLimit"),
                        None,
                        self.existing_number_input_box("ai.number.context-line-limit"),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.timeoutMs"),
                        None,
                        self.existing_number_input_box("ai.number.timeout-ms"),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("ai.agentSettings")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.smartAutoExecuteMaxRisk"),
                        Some(SharedString::from(t!("ai.smartAutoExecuteMaxRiskDesc"))),
                        self.select_control(
                            "ai-smart-risk",
                            risk_options,
                            Some(selected_risk.to_string()),
                            false,
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.agentMaxSteps"),
                        None,
                        self.existing_number_input_box("ai.number.agent-steps"),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.agentStepTimeout"),
                        None,
                        self.existing_number_input_box("ai.number.agent-step-timeout-ms"),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.terminalOutputLines"),
                        None,
                        self.existing_number_input_box("ai.number.terminal-output-lines"),
                    ))
                    .child(ai_help_text(palette, t!("ai.agentMaxStepsDesc")))
                    .child(ai_help_text(palette, t!("ai.terminalOutputLinesDesc"))),
            ))
            .when(any_full_access, |this| {
                this.child(ai_warning_text(palette, t!("ai.fullAccess.warning")))
            })
            .child(settings_form_section(
                palette,
                Some(t!("ai.externalAgents")),
                Some(t!("ai.externalAgentsDesc")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.defaultAgent"),
                        None,
                        self.select_control(
                            "ai-default-agent",
                            agent_options,
                            Some(agent_kind_value(&settings.default_agent_kind).to_string()),
                            false,
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.externalPermission"),
                        Some(SharedString::from(t!("ai.permission.desc"))),
                        self.select_control(
                            "ai-external-permission",
                            permission_options(),
                            Some(permission_value(&settings.external_agent_permission_mode).into()),
                            false,
                            cx,
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("ai.codex.title")),
                Some(t!("ai.agent.mcpOnly")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.enabled"),
                        None,
                        settings_switch(
                            palette,
                            "ai-codex-enabled",
                            settings.codex.enabled,
                            cx.listener(|this, _, _, cx| this.toggle_ai_codex_enabled(cx)),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.agent.mcpIntegration"),
                        Some(SharedString::from(t!("ai.agent.mcpOnly"))),
                        settings_switch(
                            palette,
                            "ai-codex-mcp-integration",
                            settings.codex.tool_integration_mode.as_deref() == Some("nyaterm_mcp"),
                            cx.listener(|this, _, _, cx| this.toggle_ai_codex_mcp_integration(cx)),
                        ),
                    ))
                    .child(external_input_row(
                        self,
                        palette,
                        t!("ai.executable"),
                        AiInputField::CodexExecutable,
                        cx,
                    ))
                    .child(external_input_row(
                        self,
                        palette,
                        t!("ai.defaultModel"),
                        AiInputField::CodexDefaultModel,
                        cx,
                    ))
                    .child(external_input_row(
                        self,
                        palette,
                        t!("ai.configDirectory"),
                        AiInputField::CodexConfigDirectory,
                        cx,
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.permission.title"),
                        None,
                        self.select_control(
                            "ai-codex-permission",
                            permission_options(),
                            Some(permission_value(&settings.codex.permission_mode).into()),
                            false,
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.codex.threadMode"),
                        None,
                        self.select_control(
                            "ai-codex-thread-mode",
                            vec![
                                NyaSelectOption::new("persistent", t!("ai.codex.persistent")),
                                NyaSelectOption::new("ephemeral", t!("ai.codex.ephemeral")),
                            ],
                            Some(
                                match settings.codex.thread_mode {
                                    CodexThreadMode::Persistent => "persistent",
                                    CodexThreadMode::Ephemeral => "ephemeral",
                                }
                                .into(),
                            ),
                            false,
                            cx,
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("ai.claude.title")),
                Some(t!("ai.agent.mcpOnly")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.enabled"),
                        None,
                        settings_switch(
                            palette,
                            "ai-claude-enabled",
                            settings.claude_code.enabled,
                            cx.listener(|this, _, _, cx| this.toggle_ai_claude_enabled(cx)),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.agent.mcpIntegration"),
                        Some(SharedString::from(t!("ai.agent.mcpOnly"))),
                        settings_switch(
                            palette,
                            "ai-claude-mcp-integration",
                            settings.claude_code.tool_integration_mode.as_deref()
                                == Some("nyaterm_mcp"),
                            cx.listener(|this, _, _, cx| this.toggle_ai_claude_mcp_integration(cx)),
                        ),
                    ))
                    .child(external_input_row(
                        self,
                        palette,
                        t!("ai.executable"),
                        AiInputField::ClaudeExecutable,
                        cx,
                    ))
                    .child(external_input_row(
                        self,
                        palette,
                        t!("ai.defaultModel"),
                        AiInputField::ClaudeDefaultModel,
                        cx,
                    ))
                    .child(external_input_row(
                        self,
                        palette,
                        t!("ai.configDirectory"),
                        AiInputField::ClaudeConfigDirectory,
                        cx,
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.permission.title"),
                        None,
                        self.select_control(
                            "ai-claude-permission",
                            permission_options(),
                            Some(permission_value(&settings.claude_code.permission_mode).into()),
                            false,
                            cx,
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("ai.mcp.title")),
                Some(t!("ai.mcp.desc")),
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.enabled"),
                        None,
                        settings_switch(
                            palette,
                            "ai-mcp-enabled",
                            settings.external_mcp.enabled,
                            cx.listener(|this, _, _, cx| this.toggle_ai_mcp_enabled(cx)),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.mcp.status"),
                        None,
                        ai_help_text(
                            palette,
                            match self.mcp_host_status(cx) {
                                McpHostStatus::Disabled => t!("ai.mcp.disabled"),
                                McpHostStatus::Running => t!("ai.mcp.running"),
                                McpHostStatus::Unavailable => t!("ai.mcp.unavailable"),
                            },
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.mcp.helperStatus"),
                        None,
                        ai_help_text(
                            palette,
                            match self.mcp_helper_status(cx) {
                                McpHelperStatus::Available => t!("ai.mcp.helperAvailable"),
                                McpHelperStatus::Missing => t!("ai.mcp.helperMissing"),
                            },
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.permission.title"),
                        None,
                        self.select_control(
                            "ai-mcp-permission",
                            permission_options(),
                            Some(permission_value(&settings.external_mcp.permission_mode).into()),
                            false,
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.mcp.sessionScope"),
                        None,
                        self.select_control(
                            "ai-mcp-session-scope",
                            vec![
                                NyaSelectOption::new("current_window", t!("ai.mcp.currentWindow")),
                                NyaSelectOption::new("all_sessions", t!("ai.mcp.allSessions")),
                            ],
                            Some(
                                match settings.external_mcp.session_scope {
                                    ExternalMcpSessionScope::CurrentWindow => "current_window",
                                    ExternalMcpSessionScope::AllSessions => "all_sessions",
                                }
                                .into(),
                            ),
                            false,
                            cx,
                        ),
                    )),
            ))
    }
}

fn ai_help_text(palette: ThemePalette, text: impl Into<SharedString>) -> impl IntoElement {
    let text: SharedString = text.into();
    div()
        .text_size(px(11.))
        .text_color(rgb(palette.text_muted))
        .child(text)
}

fn ai_warning_text(palette: ThemePalette, text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.warning))
        .text_color(rgb(palette.warning))
        .child(text.into())
}

fn external_input_row(
    panel: &mut SettingsPanel,
    palette: ThemePalette,
    label: impl Into<SharedString> + Clone,
    field: AiInputField,
    cx: &mut Context<SettingsPanel>,
) -> impl IntoElement {
    let label = label.into();
    settings_form_row(
        palette,
        label.clone(),
        None,
        settings_input_control(
            300.,
            panel.ai_input("external-agent-input", label, String::new(), field, cx),
        ),
    )
}

fn agent_kind_value(kind: &AiAgentKind) -> &'static str {
    match kind {
        AiAgentKind::Nyaterm => "nyaterm",
        AiAgentKind::Codex => "codex",
        AiAgentKind::ClaudeCode => "claude_code",
    }
}

fn permission_value(mode: &AiPermissionMode) -> &'static str {
    match mode {
        AiPermissionMode::Observer => "observer",
        AiPermissionMode::Confirm => "confirm",
        AiPermissionMode::Auto => "auto",
        AiPermissionMode::FullAccess => "full_access",
    }
}

fn ai_risk_value(risk: &RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}
