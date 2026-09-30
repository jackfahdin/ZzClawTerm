use futures::StreamExt as _;
use gpui::{ClipboardItem, Context};

use crate::features::ZzClawTermApp;
use crate::features::ai::agent_management::AgentCommand;

impl ZzClawTermApp {
    pub(in crate::features) fn refresh_ai_agents(&mut self, cx: &mut Context<Self>) {
        let settings = self.ai.settings_config();
        let command = AgentCommand::Refresh {
            codex: settings.codex.executable_path.clone(),
            claude: settings.claude_code.executable_path.clone(),
        };
        self.submit_ai_agent_command(command, cx);
    }

    pub(in crate::features) fn refresh_codex_account(&mut self, cx: &mut Context<Self>) {
        self.submit_ai_agent_command(
            AgentCommand::Account {
                codex: self.ai.settings_config().codex.executable_path.clone(),
            },
            cx,
        );
    }

    pub(in crate::features) fn refresh_claude_account(&mut self, cx: &mut Context<Self>) {
        self.submit_ai_agent_command(
            AgentCommand::ClaudeAccount {
                claude: self
                    .ai
                    .settings_config()
                    .claude_code
                    .executable_path
                    .clone(),
            },
            cx,
        );
    }

    pub(in crate::features) fn start_codex_login(
        &mut self,
        device_code: bool,
        cx: &mut Context<Self>,
    ) {
        self.submit_ai_agent_command(
            AgentCommand::Login {
                codex: self.ai.settings_config().codex.executable_path.clone(),
                device_code,
            },
            cx,
        );
    }

    pub(in crate::features) fn cancel_codex_login(&mut self, cx: &mut Context<Self>) {
        let Some(login_id) = self.ai.agent_management_view().login_id.clone() else {
            return;
        };
        self.submit_ai_agent_command(
            AgentCommand::Cancel {
                codex: self.ai.settings_config().codex.executable_path.clone(),
                login_id,
            },
            cx,
        );
    }

    pub(in crate::features) fn logout_codex(&mut self, cx: &mut Context<Self>) {
        self.submit_ai_agent_command(
            AgentCommand::Logout {
                codex: self.ai.settings_config().codex.executable_path.clone(),
            },
            cx,
        );
    }

    pub(in crate::features) fn copy_codex_device_code(&mut self, cx: &mut Context<Self>) {
        if let Some(code) = self.ai.agent_management_view().user_code.clone() {
            cx.write_to_clipboard(ClipboardItem::new_string(code));
        }
    }

    pub(in crate::features) fn copy_external_mcp_config(
        &mut self,
        client: &'static str,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .ai
            .agent_management_view()
            .mcp_helper_path
            .as_ref()
            .ok_or_else(|| "ZzClawTerm MCP helper is unavailable; refresh agent status".to_string())
            .and_then(|path| {
                let server = serde_json::json!({ "command": path.to_string_lossy(), "args": [] });
                let config = match client {
                    "codex" => serde_json::json!({ "mcp_servers": { "zzclawterm": server } }),
                    "claude" | "cursor" => {
                        serde_json::json!({ "mcpServers": { "zzclawterm": server } })
                    }
                    _ => return Err("Unsupported MCP client".to_string()),
                };
                serde_json::to_string_pretty(&config).map_err(|error| error.to_string())
            });
        match result {
            Ok(config) => cx.write_to_clipboard(ClipboardItem::new_string(config)),
            Err(error) => self.ai.set_agent_management_error(error),
        }
        self.request_settings_panel_refresh(cx);
    }

    fn submit_ai_agent_command(&mut self, command: AgentCommand, cx: &mut Context<Self>) {
        if self.ai.agent_management_view().pending {
            return;
        }
        let started = self.ai.submit_agent_command(command);
        self.request_settings_panel_refresh(cx);
        if !started {
            return;
        }
        let Some(mut events) = self.ai.take_agent_events() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this
                    .update(cx, |this, cx| {
                        let auth_url = this.ai.apply_agent_event(event);
                        if let Some(url) = auth_url
                            && let Ok(url) = url::Url::parse(&url)
                            && matches!(url.scheme(), "https" | "http")
                        {
                            cx.open_url(url.as_str());
                        }
                        this.request_settings_panel_refresh(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
