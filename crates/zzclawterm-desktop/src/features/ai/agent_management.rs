use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde_json::{Value, json};
use zzclawterm_core::ai::{codex_initialize_request, codex_initialized_notification};

use super::helper_resolver::resolve_codex_executable;
use super::helper_resolver::resolve_mcp_helper;
use crate::thread_owner::spawn_joinable;

#[derive(Clone, Default, PartialEq, Eq)]
pub(in crate::features) struct AgentManagementView {
    pub codex_version: Option<String>,
    pub claude_version: Option<String>,
    pub claude_connected: bool,
    pub mcp_helper_path: Option<PathBuf>,
    pub codex_connected: bool,
    pub codex_auth_mode: Option<String>,
    pub codex_plan: Option<String>,
    pub codex_email: Option<String>,
    pub login_id: Option<String>,
    pub auth_url: Option<String>,
    pub verification_url: Option<String>,
    pub user_code: Option<String>,
    pub pending: bool,
    pub error: Option<String>,
}

pub(in crate::features) enum AgentCommand {
    Refresh {
        codex: Option<String>,
        claude: Option<String>,
    },
    Account {
        codex: Option<String>,
    },
    ClaudeAccount {
        claude: Option<String>,
    },
    Login {
        codex: Option<String>,
        device_code: bool,
    },
    Cancel {
        codex: Option<String>,
        login_id: String,
    },
    Logout {
        codex: Option<String>,
    },
}

pub(in crate::features) enum AgentEvent {
    Detected {
        codex: Option<String>,
        claude: Option<String>,
        mcp_helper_path: Option<PathBuf>,
        account: Result<Value, String>,
    },
    Account(Result<Value, String>),
    AccountUpdated(Value),
    ClaudeAccount(bool),
    Login(Result<Value, String>),
    Cancel(Result<(), String>),
    Logout(Result<(), String>),
}

pub(in crate::features) struct AgentManagementState {
    view: AgentManagementView,
    commands: Option<mpsc::Sender<AgentCommand>>,
    events: Option<UnboundedReceiver<AgentEvent>>,
    worker: Option<JoinHandle<()>>,
}

impl AgentManagementState {
    pub fn new() -> Self {
        Self {
            view: AgentManagementView::default(),
            commands: None,
            events: None,
            worker: None,
        }
    }

    pub fn view(&self) -> &AgentManagementView {
        &self.view
    }

    pub fn set_error(&mut self, error: String) {
        self.view.error = Some(error);
    }

    pub fn submit(&mut self, command: AgentCommand) -> bool {
        if self.commands.is_none() {
            let (command_tx, command_rx) = mpsc::channel();
            let (event_tx, event_rx) = unbounded();
            let worker = match spawn_joinable("zzclawterm-agent-settings", move || {
                run_agent_worker(command_rx, event_tx)
            }) {
                Ok(worker) => worker,
                Err(_) => {
                    self.view.error = Some("Failed to start the agent settings worker".to_string());
                    return false;
                }
            };
            self.commands = Some(command_tx);
            self.events = Some(event_rx);
            self.worker = Some(worker);
        }
        self.view.pending = true;
        self.view.error = None;
        if self
            .commands
            .as_ref()
            .is_some_and(|tx| tx.send(command).is_ok())
        {
            true
        } else {
            self.view.pending = false;
            self.view.error = Some("Agent settings worker stopped".to_string());
            self.stop_worker();
            false
        }
    }

    pub fn take_events(&mut self) -> Option<UnboundedReceiver<AgentEvent>> {
        self.events.take()
    }

    pub fn apply(&mut self, event: AgentEvent) -> Option<String> {
        let unsolicited = matches!(event, AgentEvent::AccountUpdated(_));
        if !unsolicited {
            self.view.pending = false;
        }
        let open_login_url = matches!(event, AgentEvent::Login(Ok(_)));
        let result = match event {
            AgentEvent::Detected {
                codex,
                claude,
                mcp_helper_path,
                account,
            } => {
                self.view.codex_version = codex;
                self.view.claude_version = claude;
                self.view.claude_connected = self.view.claude_version.is_some();
                self.view.mcp_helper_path = mcp_helper_path;
                account.map(|value| self.set_account(&value))
            }
            AgentEvent::Account(result) => result.map(|value| self.set_account(&value)),
            AgentEvent::AccountUpdated(value) => {
                self.set_account(&value);
                Ok(())
            }
            AgentEvent::ClaudeAccount(connected) => {
                self.view.claude_connected = connected;
                Ok(())
            }
            AgentEvent::Login(result) => result.map(|value| {
                self.view.login_id = value
                    .get("loginId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.view.auth_url = value
                    .get("authUrl")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.view.verification_url = value
                    .get("verificationUrl")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.view.user_code = value
                    .get("userCode")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }),
            AgentEvent::Cancel(result) => result.map(|()| self.clear_login()),
            AgentEvent::Logout(result) => result.map(|()| {
                self.clear_login();
                self.view.codex_connected = false;
                self.view.codex_auth_mode = None;
                self.view.codex_plan = None;
                self.view.codex_email = None;
            }),
        };
        if !unsolicited {
            self.view.error = result.err();
        }
        open_login_url.then(|| self.view.auth_url.clone()).flatten()
    }

    fn set_account(&mut self, value: &Value) {
        let account = &value["account"];
        self.view.codex_auth_mode = account["type"].as_str().map(str::to_owned);
        self.view.codex_connected = self.view.codex_auth_mode.is_some();
        self.view.codex_plan = account["planType"].as_str().map(str::to_owned);
        self.view.codex_email = account["email"].as_str().map(str::to_owned);
        if self.view.codex_connected {
            self.clear_login();
        }
    }

    fn clear_login(&mut self) {
        self.view.login_id = None;
        self.view.auth_url = None;
        self.view.verification_url = None;
        self.view.user_code = None;
    }

    fn stop_worker(&mut self) {
        self.commands.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AgentManagementState {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

fn run_agent_worker(commands: mpsc::Receiver<AgentCommand>, events: UnboundedSender<AgentEvent>) {
    let mut codex_client: Option<CodexAccountClient> = None;
    loop {
        let command = match commands.recv_timeout(Duration::from_millis(250)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(client) = codex_client.as_mut()
                    && let Some(account) = client.poll_account_update()
                    && events
                        .unbounded_send(AgentEvent::AccountUpdated(account))
                        .is_err()
                {
                    break;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let event = match command {
            AgentCommand::Refresh { codex, claude } => {
                let codex_path = resolve_codex_executable(codex.as_deref()).ok();
                let codex_version = codex_path.as_deref().and_then(cli_version);
                let claude_version = resolve_claude_executable(claude.as_deref())
                    .as_deref()
                    .and_then(cli_version);
                let account = request_codex(
                    &mut codex_client,
                    codex_path,
                    "account/read",
                    json!({ "refreshToken": false }),
                );
                AgentEvent::Detected {
                    codex: codex_version,
                    claude: claude_version,
                    mcp_helper_path: resolve_mcp_helper().ok(),
                    account,
                }
            }
            AgentCommand::Account { codex } => AgentEvent::Account(request_codex(
                &mut codex_client,
                resolve_codex_executable(codex.as_deref()).ok(),
                "account/read",
                json!({ "refreshToken": false }),
            )),
            AgentCommand::ClaudeAccount { claude } => AgentEvent::ClaudeAccount(
                resolve_claude_executable(claude.as_deref())
                    .as_deref()
                    .and_then(cli_version)
                    .is_some(),
            ),
            AgentCommand::Login { codex, device_code } => AgentEvent::Login(request_codex(
                &mut codex_client,
                resolve_codex_executable(codex.as_deref()).ok(),
                "account/login/start",
                if device_code {
                    json!({ "type": "chatgptDeviceCode" })
                } else {
                    json!({ "type": "chatgpt", "useHostedLoginSuccessPage": true, "appBrand": "codex" })
                },
            )),
            AgentCommand::Cancel { codex, login_id } => AgentEvent::Cancel(
                request_codex(
                    &mut codex_client,
                    resolve_codex_executable(codex.as_deref()).ok(),
                    "account/login/cancel",
                    json!({ "loginId": login_id }),
                )
                .map(|_| ()),
            ),
            AgentCommand::Logout { codex } => AgentEvent::Logout(
                request_codex(
                    &mut codex_client,
                    resolve_codex_executable(codex.as_deref()).ok(),
                    "account/logout",
                    json!({}),
                )
                .map(|_| ()),
            ),
        };
        if events.unbounded_send(event).is_err() {
            break;
        }
    }
}

fn request_codex(
    client: &mut Option<CodexAccountClient>,
    path: Option<PathBuf>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let result = with_codex(client, path).and_then(|client| client.request(method, params));
    if result.is_err() {
        *client = None;
    }
    result
}

fn with_codex(
    client: &mut Option<CodexAccountClient>,
    path: Option<PathBuf>,
) -> Result<&mut CodexAccountClient, String> {
    let path = path.ok_or_else(|| "Codex CLI was not found".to_string())?;
    if client.as_ref().is_some_and(|current| current.path != path) {
        *client = None;
    }
    if client.as_mut().is_some_and(|current| !current.is_alive()) {
        *client = None;
    }
    if client.is_none() {
        *client = Some(CodexAccountClient::start(path)?);
    }
    Ok(client.as_mut().expect("client was initialized"))
}

fn resolve_claude_executable(configured: Option<&str>) -> Option<PathBuf> {
    let configured = configured.map(str::trim).filter(|value| !value.is_empty());
    if let Some(path) = configured {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["claude.cmd", "claude.exe", "claude"]
    } else {
        &["claude"]
    };
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
            .find(|path| path.is_file())
    })
}

fn cli_version(path: &Path) -> Option<String> {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let mut output = String::new();
                use std::io::Read as _;
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return output
                    .lines()
                    .next()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned);
            }
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

struct CodexAccountClient {
    path: PathBuf,
    child: Child,
    writer: BufWriter<ChildStdin>,
    lines: mpsc::Receiver<String>,
    next_id: u64,
    account_update: Option<Value>,
    reader: Option<JoinHandle<()>>,
}

impl CodexAccountClient {
    fn start(path: PathBuf) -> Result<Self, String> {
        let mut command = Command::new(&path);
        command.args(["app-server", "--listen", "stdio://"]);
        Self::start_with_command(path, command)
    }

    fn start_with_command(path: PathBuf, mut command: Command) -> Result<Self, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to start Codex: {error}"))?;
        let writer = BufWriter::new(child.stdin.take().ok_or("Codex stdin unavailable")?);
        let stdout = child.stdout.take().ok_or("Codex stdout unavailable")?;
        let (tx, lines) = mpsc::channel();
        let reader = spawn_joinable("zzclawterm-codex-account-reader", move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| error.to_string())?;
        let mut client = Self {
            path,
            child,
            writer,
            lines,
            next_id: 1,
            account_update: None,
            reader: Some(reader),
        };
        client.send(&codex_initialize_request(1, env!("CARGO_PKG_VERSION")))?;
        client.wait_response(1)?;
        client.send(&codex_initialized_notification())?;
        client.next_id = 2;
        Ok(client)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "id": id, "method": method, "params": params }))?;
        self.wait_response(id)
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn send(&mut self, value: &Value) -> Result<(), String> {
        serde_json::to_writer(&mut self.writer, value).map_err(|error| error.to_string())?;
        self.writer
            .write_all(b"\n")
            .map_err(|error| error.to_string())?;
        self.writer.flush().map_err(|error| error.to_string())
    }

    fn wait_response(&mut self, id: u64) -> Result<Value, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Codex account request timed out".to_string());
            }
            let line = self
                .lines
                .recv_timeout(remaining)
                .map_err(|_| "Codex app-server closed or timed out".to_string())?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(account) = account_update_from_notification(&value) {
                self.account_update = Some(account);
                continue;
            }
            if let (Some(request_id), Some(_)) = (
                value.get("id").and_then(Value::as_u64),
                value.get("method").and_then(Value::as_str),
            ) {
                self.send(&json!({ "id": request_id, "error": { "code": -32601, "message": "Unsupported request" } }))?;
                continue;
            }
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex request failed")
                    .to_string());
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn poll_account_update(&mut self) -> Option<Value> {
        if let Some(account) = self.account_update.take() {
            return Some(account);
        }
        while let Ok(line) = self.lines.try_recv() {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(account) = account_update_from_notification(&value) {
                return Some(account);
            }
        }
        None
    }
}

fn account_update_from_notification(value: &Value) -> Option<Value> {
    (value.get("method").and_then(Value::as_str) == Some("account/updated")).then(|| {
        let params = &value["params"];
        json!({ "account": {
            "type": params.get("authMode"),
            "planType": params.get("planType")
        } })
    })
}

impl Drop for CodexAccountClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, Write};
    use std::process::Command;

    use serde_json::json;

    use super::{
        AgentEvent, AgentManagementState, CodexAccountClient, account_update_from_notification,
    };

    #[test]
    fn mock_codex_app_server() {
        if std::env::var_os("ZZCLAWTERM_MOCK_CODEX_ACCOUNT_SERVER").is_none() {
            return;
        }
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line.expect("request line");
            let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let Some(id) = request.get("id").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            let method = request["method"].as_str().unwrap_or_default();
            if method == "account/read" {
                writeln!(
                    stdout,
                    "{}",
                    json!({ "method": "account/updated", "params": { "authMode": "chatgpt", "planType": "plus" } })
                )
                .unwrap();
            }
            let response = match method {
                "initialize" => json!({ "id": id, "result": {} }),
                "account/read" => {
                    json!({ "id": id, "result": { "account": { "type": "chatgpt", "email": "fixture@example.invalid" } } })
                }
                "account/login/start" => {
                    json!({ "id": id, "result": { "loginId": "fixture-login", "userCode": "FIXTURE-CODE" } })
                }
                "account/login/cancel" | "account/logout" => json!({ "id": id, "result": {} }),
                _ => {
                    json!({ "id": id, "error": { "code": -32601, "message": "mock rejected request" } })
                }
            };
            writeln!(stdout, "{response}").unwrap();
            stdout.flush().unwrap();
        }
    }

    #[test]
    fn codex_account_client_handles_interleaved_updates_and_login_lifecycle() {
        let path = std::env::current_exe().expect("test executable");
        let mut command = Command::new(&path);
        command
            .args([
                "--exact",
                "features::ai::agent_management::tests::mock_codex_app_server",
                "--nocapture",
            ])
            .env("ZZCLAWTERM_MOCK_CODEX_ACCOUNT_SERVER", "1");
        let mut client =
            CodexAccountClient::start_with_command(path, command).expect("start mock app-server");
        let account = client
            .request("account/read", json!({ "refreshToken": false }))
            .expect("account response");
        assert_eq!(account["account"]["email"], "fixture@example.invalid");
        assert_eq!(
            client.poll_account_update().expect("interleaved update")["account"]["planType"],
            "plus"
        );
        let login = client
            .request(
                "account/login/start",
                json!({ "type": "chatgptDeviceCode" }),
            )
            .expect("login response");
        assert_eq!(login["loginId"], "fixture-login");
        client
            .request(
                "account/login/cancel",
                json!({ "loginId": "fixture-login" }),
            )
            .expect("cancel response");
        client
            .request("account/logout", json!({}))
            .expect("logout response");
        assert_eq!(
            client.request("unknown", json!({})).unwrap_err(),
            "mock rejected request"
        );
    }

    #[test]
    fn account_notification_extracts_state_without_exposing_device_code() {
        let update = account_update_from_notification(&json!({
            "method": "account/updated",
            "params": { "authMode": "chatgpt", "planType": "plus" }
        }))
        .expect("account update");
        assert_eq!(update["account"]["type"], "chatgpt");
        assert_eq!(update["account"]["planType"], "plus");
        assert!(
            account_update_from_notification(&json!({
                "method": "account/login/completed",
                "params": { "userCode": "SECRET" }
            }))
            .is_none()
        );
    }

    #[test]
    fn account_login_cancel_and_logout_update_only_transient_state() {
        let mut state = AgentManagementState::new();
        state.apply(AgentEvent::Detected {
            codex: Some("codex 1.2".to_string()),
            claude: None,
            mcp_helper_path: None,
            account: Ok(json!({ "account": null })),
        });
        assert_eq!(state.view().codex_version.as_deref(), Some("codex 1.2"));
        assert!(!state.view().codex_connected);
        state.apply(AgentEvent::ClaudeAccount(true));
        assert!(state.view().claude_connected);
        state.apply(AgentEvent::ClaudeAccount(false));
        assert!(!state.view().claude_connected);
        state.view.pending = true;
        state.apply(AgentEvent::AccountUpdated(json!({
            "account": { "type": "chatgpt" }
        })));
        assert!(state.view().pending);

        let url = state.apply(AgentEvent::Login(Ok(json!({
            "loginId": "login-1",
            "authUrl": "https://example.invalid/login",
            "verificationUrl": "https://example.invalid/device",
            "userCode": "TEST-CODE"
        }))));
        assert_eq!(url.as_deref(), Some("https://example.invalid/login"));
        assert_eq!(state.view().user_code.as_deref(), Some("TEST-CODE"));
        assert!(state.apply(AgentEvent::Account(Ok(json!({
            "account": { "type": "chatgpt", "planType": "plus", "email": "test@example.invalid" }
        })))).is_none());
        assert!(state.view().codex_connected);
        assert!(state.view().user_code.is_none());

        state.apply(AgentEvent::Login(Ok(
            json!({ "loginId": "login-2", "userCode": "SECOND" }),
        )));
        state.apply(AgentEvent::Cancel(Err("cancel failed".to_string())));
        assert_eq!(state.view().login_id.as_deref(), Some("login-2"));
        assert_eq!(state.view().error.as_deref(), Some("cancel failed"));
        state.apply(AgentEvent::Cancel(Ok(())));
        assert!(state.view().login_id.is_none());
        state.apply(AgentEvent::Logout(Err("logout failed".to_string())));
        assert!(state.view().codex_connected);
        state.apply(AgentEvent::Logout(Ok(())));
        assert!(!state.view().codex_connected);
    }
}
