use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use futures::channel::mpsc::UnboundedSender;
use nyaterm_core::{AiPermissionMode, CapabilityScope, OutputStore};
use nyaterm_mcp_protocol::{
    AuthParams, CapabilityExecuteParams, ClientIdentifyParams, DiscoveryDocument,
    MAX_INLINE_OUTPUT_BYTES, MAX_RPC_LINE_BYTES, OutputReadArgs, RequestCancelParams, RpcError,
    RpcRequest, RpcResponse, definition_for_tool, tool, validate_tool_arguments,
    validate_tool_result,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::thread_owner::spawn_joinable;

use super::discovery::DiscoveryStore;

const REQUEST_REPLY_TIMEOUT: Duration = Duration::from_secs(305);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(in crate::features) struct McpHostRequest {
    pub connection_id: String,
    pub request_id: String,
    pub generation: String,
    pub client: String,
    pub permission_mode: AiPermissionMode,
    pub scope: CapabilityScope,
    pub tool: String,
    pub arguments: Value,
    pub cancellation: CancellationToken,
    pub approved: bool,
    pub approval_decision: Option<String>,
    pub reply: oneshot::Sender<Result<Value, RpcError>>,
}

#[derive(Clone)]
struct HostCredential {
    token: String,
    generation: String,
    permission_mode: AiPermissionMode,
    scope: CapabilityScope,
}

impl std::fmt::Debug for HostCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostCredential")
            .field("credential", &"[REDACTED]")
            .field("permission_mode", &self.permission_mode)
            .field("scope", &self.scope)
            .finish()
    }
}

pub(in crate::features) struct McpHostEndpoint {
    pub port: u16,
    pub token: String,
    pub generation: String,
}

pub(in crate::features) enum McpHostEvent {
    Execute(Box<McpHostRequest>),
    Cancelled { request_id: String },
    Disconnected { connection_id: String },
}

impl std::fmt::Debug for McpHostEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpHostEndpoint")
            .field("port", &self.port)
            .field("credential", &"[REDACTED]")
            .finish()
    }
}

pub(in crate::features) struct McpHostRuntime {
    endpoint: McpHostEndpoint,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    discovery: Option<DiscoveryStore>,
}

impl McpHostRuntime {
    pub fn start(
        permission_mode: AiPermissionMode,
        scope: CapabilityScope,
        requests: UnboundedSender<McpHostEvent>,
        discovery: Option<DiscoveryStore>,
    ) -> anyhow::Result<Self> {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = hex::encode(rand::random::<[u8; 32]>());
        let generation = uuid::Uuid::new_v4().to_string();
        if let Some(store) = discovery.as_ref() {
            store.remove()?;
            store.write(&DiscoveryDocument {
                version: nyaterm_mcp_protocol::PROTOCOL_VERSION,
                pid: std::process::id(),
                host: "127.0.0.1".to_string(),
                port,
                token: token.clone(),
                generation: generation.clone(),
                permission_mode: permission_mode_name(&permission_mode).to_string(),
            })?;
        }
        let credential = Arc::new(Mutex::new(HostCredential {
            token: token.clone(),
            generation: generation.clone(),
            permission_mode,
            scope,
        }));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();
        let thread = spawn_joinable("nyaterm-mcp-host", move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    tracing::error!(error = %error, "failed to create MCP Host runtime");
                    return;
                }
            };
            runtime.block_on(run_listener(
                listener,
                credential,
                requests,
                thread_shutdown,
            ));
        })?;
        Ok(Self {
            endpoint: McpHostEndpoint {
                port,
                token,
                generation,
            },
            shutdown,
            thread: Some(thread),
            discovery,
        })
    }

    pub fn endpoint(&self) -> &McpHostEndpoint {
        &self.endpoint
    }
}

impl Drop for McpHostRuntime {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, self.endpoint.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(discovery) = self.discovery.as_ref()
            && let Err(error) = discovery.remove()
        {
            tracing::warn!(error = %error, "failed to remove MCP discovery document");
        }
    }
}

async fn run_listener(
    listener: std::net::TcpListener,
    credential: Arc<Mutex<HostCredential>>,
    requests: UnboundedSender<McpHostEvent>,
    shutdown: Arc<AtomicBool>,
) {
    let listener = match TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(error = %error, "failed to initialize MCP Host listener");
            return;
        }
    };
    let cancellations = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    while !shutdown.load(Ordering::Acquire) {
        match tokio::time::timeout(ACCEPT_POLL_INTERVAL, listener.accept()).await {
            Ok(Ok((stream, address))) if address.ip().is_loopback() => {
                tokio::spawn(handle_connection(
                    stream,
                    credential.clone(),
                    requests.clone(),
                    cancellations.clone(),
                ));
            }
            Ok(Ok((_stream, _))) => {}
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "MCP Host accept failed");
            }
            Err(_) => {}
        }
    }
}

type CancellationMap = Arc<tokio::sync::Mutex<HashMap<(String, String), CancellationToken>>>;

async fn handle_connection(
    stream: TcpStream,
    credentials: Arc<Mutex<HostCredential>>,
    requests: UnboundedSender<McpHostEvent>,
    cancellations: CancellationMap,
) {
    let peer = stream.peer_addr().ok();
    if peer.is_none_or(|address| !address.ip().is_loopback()) {
        return;
    }
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let Some(line) = read_rpc_line(&mut reader).await.ok().flatten() else {
        return;
    };
    let Ok(request) = serde_json::from_slice::<RpcRequest>(&line) else {
        return;
    };
    if request.method != "auth" {
        let _ = write_response(
            &mut writer,
            rpc_error(request.id, "authentication_required", "Authenticate first."),
        )
        .await;
        return;
    }
    let Ok(auth) = serde_json::from_value::<AuthParams>(request.params) else {
        let _ = write_response(
            &mut writer,
            rpc_error(request.id, "invalid_argument", "Invalid auth request."),
        )
        .await;
        return;
    };
    if !credential_matches(&credentials, &auth) {
        let _ = write_response(
            &mut writer,
            rpc_error(
                request.id,
                "authentication_failed",
                "Invalid or expired MCP credential.",
            ),
        )
        .await;
        return;
    }
    if write_response(
        &mut writer,
        rpc_ok(request.id, json!({ "authenticated": true })),
    )
    .await
    .is_err()
    {
        return;
    }

    let mut client = "external MCP client".to_string();
    let connection_id = uuid::Uuid::new_v4().to_string();
    let mut outputs = OutputStore::default();
    loop {
        let line = match read_rpc_line(&mut reader).await {
            Ok(Some(line)) => line,
            _ => break,
        };
        let request = match serde_json::from_slice::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(_) => break,
        };
        if !credential_matches(&credentials, &auth) {
            let response = rpc_error(
                request.id,
                "authentication_failed",
                "MCP credential was rotated or expired.",
            );
            let _ = write_response(&mut writer, response).await;
            break;
        }
        let response = match request.method.as_str() {
            "client.identify" => {
                match serde_json::from_value::<ClientIdentifyParams>(request.params) {
                    Ok(identity)
                        if !identity.name.trim().is_empty()
                            && identity.name.len() <= 128
                            && identity
                                .version
                                .as_ref()
                                .is_none_or(|value| value.len() <= 64) =>
                    {
                        client = identity.version.map_or(identity.name.clone(), |version| {
                            format!("{} {}", identity.name, version)
                        });
                        rpc_ok(request.id, json!({ "accepted": true }))
                    }
                    _ => rpc_error(
                        request.id,
                        "invalid_argument",
                        "Invalid MCP client metadata.",
                    ),
                }
            }
            "request.cancel" => {
                handle_cancel(
                    request.id,
                    request.params,
                    &auth.generation,
                    &cancellations,
                    &requests,
                )
                .await
            }
            "capability.execute" => {
                handle_execute(
                    request.id,
                    request.params,
                    &auth,
                    &connection_id,
                    &client,
                    &credentials,
                    &requests,
                    &cancellations,
                    &mut outputs,
                )
                .await
            }
            _ => rpc_error(request.id, "method_not_found", "Unknown MCP Host method."),
        };
        if write_response(&mut writer, response).await.is_err() {
            break;
        }
    }
    let _ = requests.unbounded_send(McpHostEvent::Disconnected { connection_id });
}

async fn handle_cancel(
    id: u64,
    params: Value,
    generation: &str,
    cancellations: &CancellationMap,
    requests: &UnboundedSender<McpHostEvent>,
) -> RpcResponse {
    let Ok(params) = serde_json::from_value::<RequestCancelParams>(params) else {
        return rpc_error(id, "invalid_argument", "Invalid cancellation request.");
    };
    let request_id = params.request_id;
    let key = (generation.to_string(), request_id.clone());
    let cancelled = if let Some(token) = cancellations.lock().await.get(&key) {
        token.cancel();
        let _ = requests.unbounded_send(McpHostEvent::Cancelled { request_id });
        true
    } else {
        false
    };
    rpc_ok(id, json!({ "cancelled": cancelled }))
}

#[allow(clippy::too_many_arguments)]
async fn handle_execute(
    id: u64,
    params: Value,
    auth: &AuthParams,
    connection_id: &str,
    client: &str,
    credentials: &Arc<Mutex<HostCredential>>,
    requests: &UnboundedSender<McpHostEvent>,
    cancellations: &CancellationMap,
    outputs: &mut OutputStore,
) -> RpcResponse {
    let Ok(params) = serde_json::from_value::<CapabilityExecuteParams>(params) else {
        return rpc_error(id, "invalid_argument", "Invalid capability request.");
    };
    let Some(_definition) = definition_for_tool(&params.tool) else {
        return rpc_error(id, "invalid_argument", "Unknown NyaTerm MCP tool.");
    };
    if let Err(error) = validate_tool_arguments(&params.tool, &params.arguments) {
        return rpc_error(id, "invalid_argument", &error.to_string());
    }
    if params.tool == tool::OUTPUT_READ {
        let args = serde_json::from_value::<OutputReadArgs>(params.arguments)
            .expect("validated output read arguments");
        return match outputs.read(
            &args.output_id,
            args.offset,
            args.max_bytes.unwrap_or(MAX_INLINE_OUTPUT_BYTES),
        ) {
            Ok(chunk) => match serde_json::to_value(chunk) {
                Ok(value) => rpc_ok(id, value),
                Err(_) => rpc_error(id, "internal_error", "Cannot serialize output chunk."),
            },
            Err(error) => rpc_error(id, "output_not_found", &error.to_string()),
        };
    }

    let credential = credentials
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if credential.generation != auth.generation
        || !constant_time_eq(credential.token.as_bytes(), auth.token.as_bytes())
    {
        return rpc_error(id, "authentication_failed", "MCP credential expired.");
    }
    let request_id = params
        .request_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let cancellation = CancellationToken::new();
    let key = (credential.generation.clone(), request_id.clone());
    cancellations
        .lock()
        .await
        .insert(key.clone(), cancellation.clone());
    let (reply, result) = oneshot::channel();
    let request = McpHostRequest {
        connection_id: connection_id.to_string(),
        request_id,
        generation: credential.generation,
        client: client.to_string(),
        permission_mode: credential.permission_mode,
        scope: credential.scope,
        tool: params.tool.clone(),
        arguments: params.arguments,
        cancellation: cancellation.clone(),
        approved: false,
        approval_decision: None,
        reply,
    };
    if requests
        .unbounded_send(McpHostEvent::Execute(Box::new(request)))
        .is_err()
    {
        cancellations.lock().await.remove(&key);
        return rpc_error(id, "host_unavailable", "NyaTerm MCP Host is unavailable.");
    }
    let result = tokio::select! {
        _ = cancellation.cancelled() => Err(rpc_failure("cancelled", "The MCP request was cancelled.")),
        result = tokio::time::timeout(REQUEST_REPLY_TIMEOUT, result) => match result {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(rpc_failure("host_unavailable", "NyaTerm closed the MCP request.")),
            Err(_) => Err(rpc_failure("host_timeout", "NyaTerm MCP Host request timed out.")),
        }
    };
    cancellation.cancel();
    cancellations.lock().await.remove(&key);
    match result {
        Ok(value) => {
            if let Err(error) = validate_tool_result(&params.tool, &value) {
                return rpc_error(id, "internal_error", &error.to_string());
            }
            match protect_output(outputs, value) {
                Ok(value) => rpc_ok(id, value),
                Err(error) => rpc_error(id, "internal_error", &error),
            }
        }
        Err(error) => RpcResponse {
            id,
            result: None,
            error: Some(error),
        },
    }
}

fn protect_output(store: &mut OutputStore, value: Value) -> Result<Value, String> {
    let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    if text.len() <= MAX_INLINE_OUTPUT_BYTES {
        return Ok(value);
    }
    serde_json::to_value(store.protect(text, MAX_INLINE_OUTPUT_BYTES))
        .map_err(|error| error.to_string())
}

fn credential_matches(credentials: &Arc<Mutex<HostCredential>>, auth: &AuthParams) -> bool {
    let credential = credentials
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    credential.generation == auth.generation
        && constant_time_eq(credential.token.as_bytes(), auth.token.as_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    Sha256::digest(left)
        .iter()
        .zip(Sha256::digest(right).iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

async fn read_rpc_line<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "MCP Host request line is not newline terminated",
            ));
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > MAX_RPC_LINE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MCP Host request line is too large",
            ));
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: RpcResponse,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
    if bytes.len().saturating_add(1) > MAX_RPC_LINE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MCP Host response line is too large",
        ));
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await
}

fn rpc_ok(id: u64, result: Value) -> RpcResponse {
    RpcResponse {
        id,
        result: Some(result),
        error: None,
    }
}

fn rpc_error(id: u64, code: &str, message: &str) -> RpcResponse {
    RpcResponse {
        id,
        result: None,
        error: Some(rpc_failure(code, message)),
    }
}

pub(in crate::features) fn rpc_failure(code: &str, message: &str) -> RpcError {
    RpcError {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn permission_mode_name(mode: &AiPermissionMode) -> &'static str {
    match mode {
        AiPermissionMode::Observer => "observer",
        AiPermissionMode::Confirm => "confirm",
        AiPermissionMode::Auto => "auto",
        AiPermissionMode::FullAccess => "full_access",
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use futures::StreamExt;
    use nyaterm_core::{AiPermissionMode, CapabilityScope};
    use nyaterm_mcp_protocol::{
        AuthParams, MAX_INLINE_OUTPUT_BYTES, MAX_RPC_LINE_BYTES, PROTOCOL_VERSION, RpcRequest,
        RpcResponse,
    };
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    use super::{McpHostEndpoint, McpHostEvent, McpHostRuntime, rpc_failure};

    async fn rpc(
        reader: &mut tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
        writer: &mut tokio::net::tcp::OwnedWriteHalf,
        request: RpcRequest,
    ) -> RpcResponse {
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        writer.write_all(&bytes).await.unwrap();
        serde_json::from_str(&reader.next_line().await.unwrap().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn host_authenticates_forwards_writes_and_pages_connection_local_output() {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let runtime = McpHostRuntime::start(
            AiPermissionMode::Auto,
            CapabilityScope::AllSessions,
            tx,
            None,
        )
        .unwrap();
        let endpoint = runtime.endpoint();
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port))
            .await
            .unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let auth = rpc(
            &mut lines,
            &mut writer,
            RpcRequest {
                id: 1,
                method: "auth".into(),
                params: serde_json::to_value(AuthParams {
                    token: endpoint.token.clone(),
                    generation: endpoint.generation.clone(),
                })
                .unwrap(),
            },
        )
        .await;
        assert_eq!(auth.result.unwrap()["authenticated"], true);

        let write = tokio::spawn(async move {
            let response = rpc(
                &mut lines,
                &mut writer,
                RpcRequest {
                    id: 2,
                    method: "capability.execute".into(),
                    params: json!({ "tool": "sftp_delete", "arguments": { "sessionId": "s", "path": "/tmp/x" } }),
                },
            )
            .await;
            (response, lines, writer)
        });
        let McpHostEvent::Execute(request) = rx.next().await.unwrap() else {
            panic!("expected execute request");
        };
        assert_eq!(request.tool, "sftp_delete");
        request
            .reply
            .send(Err(rpc_failure("permission_denied", "denied by policy")))
            .unwrap();
        let (write_response, mut lines, mut writer) = write.await.unwrap();
        assert_eq!(write_response.error.unwrap().code, "permission_denied");

        let call = tokio::spawn(async move {
            rpc(
                &mut lines,
                &mut writer,
                RpcRequest {
                    id: 3,
                    method: "capability.execute".into(),
                    params: json!({ "tool": "connection_list", "arguments": {} }),
                },
            )
            .await
        });
        let McpHostEvent::Execute(request) = rx.next().await.unwrap() else {
            panic!("expected execute request");
        };
        request
            .reply
            .send(Ok(json!({
                "connections": [{
                    "id": "c",
                    "name": "x".repeat(MAX_INLINE_OUTPUT_BYTES),
                    "type": "ssh",
                    "groupPath": []
                }]
            })))
            .unwrap();
        let response = call.await.unwrap().result.unwrap();
        assert_eq!(response["truncated"], true);
        assert!(response["outputId"].as_str().unwrap().starts_with("out_"));
        drop(runtime);
    }

    #[tokio::test]
    async fn invalid_token_and_oversized_lines_fail_closed() {
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let runtime = McpHostRuntime::start(
            AiPermissionMode::Observer,
            CapabilityScope::AllSessions,
            tx,
            None,
        )
        .unwrap();
        let endpoint = runtime.endpoint();
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port))
            .await
            .unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let denied = rpc(
            &mut lines,
            &mut writer,
            RpcRequest {
                id: 1,
                method: "auth".into(),
                params: json!({ "token": "wrong", "generation": endpoint.generation }),
            },
        )
        .await;
        assert_eq!(denied.error.unwrap().code, "authentication_failed");

        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port))
            .await
            .unwrap();
        let (_reader, mut writer) = stream.into_split();
        writer
            .write_all(&vec![b'x'; MAX_RPC_LINE_BYTES + 1])
            .await
            .unwrap();
        drop(runtime);
    }

    #[test]
    fn endpoint_debug_redacts_token() {
        let endpoint = McpHostEndpoint {
            port: 1,
            token: "fixture-secret".into(),
            generation: "generation".into(),
        };
        let debug = format!("{endpoint:?}");
        assert!(!debug.contains("fixture-secret"));
        assert!(debug.contains("[REDACTED]"));
        assert_eq!(PROTOCOL_VERSION, 1);
        assert!(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1)
                .ip()
                .is_loopback()
        );
    }
}
