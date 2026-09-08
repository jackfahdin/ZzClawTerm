use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedSender;

use serde::Deserialize;
use zed_reqwest::StatusCode;
use zed_reqwest::blocking::{Client, Response};

use crate::models::{GithubGistAuthEvent, GithubGistAuthJobEvent};

const GITHUB_GIST_CLIENT_ID: Option<&str> = option_env!("NYATERM_GITHUB_GIST_CLIENT_ID");
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_API_ENDPOINT: &str = "https://api.github.com";
const GITHUB_API_VERSION: &str = "2022-11-28";
const GITHUB_GIST_SCOPE: &str = "gist";

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    #[serde(default)]
    access_token: Option<nyaterm_core::SecretString>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubUserResponse {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GithubGistResponse {
    id: String,
}

pub(crate) fn run_github_gist_device_flow(
    job_id: u64,
    existing_gist_id: Option<String>,
    cancel: Arc<AtomicBool>,
    tx: UnboundedSender<GithubGistAuthJobEvent>,
) {
    let result = run_device_flow(job_id, existing_gist_id, &cancel, &tx);
    if cancel.load(Ordering::Relaxed) {
        send_event(&tx, job_id, GithubGistAuthEvent::Cancelled);
    } else if let Err(error) = result {
        send_event(&tx, job_id, GithubGistAuthEvent::Failed(error));
    }
}

fn run_device_flow(
    job_id: u64,
    existing_gist_id: Option<String>,
    cancel: &AtomicBool,
    tx: &UnboundedSender<GithubGistAuthJobEvent>,
) -> Result<(), String> {
    let client_id = github_client_id()?;
    let client = github_client()?;
    let device: DeviceCodeResponse = decode_json(
        client
            .post(GITHUB_DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&[("client_id", client_id), ("scope", GITHUB_GIST_SCOPE)])
            .send()
            .map_err(map_http_error)?,
        "GitHub device authorization",
    )?;
    let mut interval = device.interval.unwrap_or(5).max(1);
    let deadline = Instant::now() + Duration::from_secs(device.expires_in.max(1));
    tx.unbounded_send(GithubGistAuthJobEvent {
        job_id,
        event: GithubGistAuthEvent::Started {
            user_code: device.user_code,
            verification_uri: device.verification_uri,
        },
    })
    .map_err(|_| "GitHub authorization UI is no longer available".to_string())?;

    loop {
        if wait_cancelled(Duration::from_secs(interval), cancel) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("GitHub device authorization expired".to_string());
        }

        let payload: AccessTokenResponse = decode_json(
            client
                .post(GITHUB_ACCESS_TOKEN_URL)
                .header("Accept", "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("device_code", device.device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .map_err(map_http_error)?,
            "GitHub access token",
        )?;

        match payload.error.as_deref() {
            Some("authorization_pending") => {
                send_event(
                    tx,
                    job_id,
                    GithubGistAuthEvent::Polling { slow_down: false },
                );
                continue;
            }
            Some("slow_down") => {
                interval = interval.saturating_add(5);
                send_event(tx, job_id, GithubGistAuthEvent::Polling { slow_down: true });
                continue;
            }
            Some("expired_token") => {
                return Err(payload
                    .error_description
                    .unwrap_or_else(|| "GitHub device authorization expired".to_string()));
            }
            Some("access_denied") => {
                return Err(payload
                    .error_description
                    .unwrap_or_else(|| "GitHub device authorization was denied".to_string()));
            }
            Some(error) => {
                return Err(payload
                    .error_description
                    .unwrap_or_else(|| format!("GitHub OAuth error: {error}")));
            }
            None => {}
        }

        let access_token = payload
            .access_token
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "GitHub OAuth response did not include a token".to_string())?;
        let scope = payload.scope.unwrap_or_default();
        if !has_gist_scope(&scope) {
            return Err("GitHub authorization did not grant the gist scope".to_string());
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }

        let login = fetch_github_login(&client, access_token.expose_secret())?;
        let gist_id =
            resolve_github_gist_id(&client, access_token.expose_secret(), existing_gist_id)?;
        tx.unbounded_send(GithubGistAuthJobEvent {
            job_id,
            event: GithubGistAuthEvent::Succeeded {
                access_token,
                gist_id,
                login,
            },
        })
        .map_err(|_| "GitHub authorization UI is no longer available".to_string())?;
        return Ok(());
    }
}

fn send_event(
    tx: &UnboundedSender<GithubGistAuthJobEvent>,
    job_id: u64,
    event: GithubGistAuthEvent,
) {
    let _ = tx.unbounded_send(GithubGistAuthJobEvent { job_id, event });
}

fn fetch_github_login(client: &Client, access_token: &str) -> Result<String, String> {
    let payload: GithubUserResponse = decode_json(
        github_api_request(
            client.get(format!("{GITHUB_API_ENDPOINT}/user")),
            access_token,
        )
        .send()
        .map_err(map_http_error)?,
        "GitHub user",
    )?;
    Ok(payload.login)
}

fn resolve_github_gist_id(
    client: &Client,
    access_token: &str,
    existing_gist_id: Option<String>,
) -> Result<String, String> {
    if let Some(gist_id) = existing_gist_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let response = github_api_request(
            client.get(format!("{GITHUB_API_ENDPOINT}/gists/{gist_id}")),
            access_token,
        )
        .send()
        .map_err(map_http_error)?;
        if response.status().is_success() {
            return Ok(gist_id.to_string());
        }
        if response.status() != StatusCode::NOT_FOUND {
            return Err(response_error(response, "GitHub Gist lookup"));
        }
    }

    let payload: GithubGistResponse = decode_json(
        github_api_request(
            client.post(format!("{GITHUB_API_ENDPOINT}/gists")),
            access_token,
        )
        .json(&serde_json::json!({
            "description": "NyaTerm encrypted cloud sync storage",
            "public": false,
            "files": {
                "nyaterm-readme.txt": {
                    "content": "This private gist stores encrypted NyaTerm cloud sync objects."
                }
            }
        }))
        .send()
        .map_err(map_http_error)?,
        "GitHub Gist creation",
    )?;
    Ok(payload.id)
}

fn github_api_request(
    request: zed_reqwest::blocking::RequestBuilder,
    access_token: &str,
) -> zed_reqwest::blocking::RequestBuilder {
    request
        .bearer_auth(access_token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
}

fn decode_json<T: serde::de::DeserializeOwned>(
    response: Response,
    operation: &str,
) -> Result<T, String> {
    let status = response.status();
    let body = response.text().map_err(map_http_error)?;
    if !status.is_success() {
        return Err(format!("{operation} failed ({status}): {}", body.trim()));
    }
    serde_json::from_str(&body).map_err(|error| format!("{operation} response is invalid: {error}"))
}

fn response_error(response: Response, operation: &str) -> String {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    format!("{operation} failed ({status}): {}", body.trim())
}

fn github_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("NyaTerm")
        .build()
        .map_err(map_http_error)
}

fn github_client_id() -> Result<&'static str, String> {
    GITHUB_GIST_CLIENT_ID
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "GitHub Gist OAuth Client ID is not configured at build time".to_string())
}

fn map_http_error(error: zed_reqwest::Error) -> String {
    if error.is_timeout() {
        format!("GitHub device flow operation timed out: {error}")
    } else {
        format!("GitHub device flow request failed: {error}")
    }
}

fn wait_cancelled(duration: Duration, cancel: &AtomicBool) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100)),
        );
    }
    cancel.load(Ordering::Relaxed)
}

fn has_gist_scope(scope: &str) -> bool {
    scope
        .split([',', ' '])
        .map(str::trim)
        .any(|value| value == GITHUB_GIST_SCOPE)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    use super::{has_gist_scope, wait_cancelled};

    #[test]
    fn gist_scope_parser_accepts_space_or_comma_separated_values() {
        assert!(has_gist_scope("read:user gist"));
        assert!(has_gist_scope("read:user,gist"));
        assert!(!has_gist_scope("read:user repo"));
    }

    #[test]
    fn cancelled_wait_returns_promptly() {
        let cancel = AtomicBool::new(true);
        let started = Instant::now();
        assert!(wait_cancelled(Duration::from_secs(30), &cancel));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
