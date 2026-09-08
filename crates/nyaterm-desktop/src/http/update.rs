use std::time::Duration;

use nyaterm_core::{NativeUpdateInfo, parse_github_latest_release};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(20);
const RELEASES_URL: &str = "https://api.github.com/repos/nyakang/nyaterm/releases/latest";
const USER_AGENT: &str = concat!("nyaterm-app/", env!("CARGO_PKG_VERSION"));

pub fn check_native_update() -> Result<NativeUpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let client = zed_reqwest::blocking::Client::builder()
        .timeout(UPDATE_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| format!("build updater HTTP client failed: {error}"))?;
    let response = client
        .get(RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| format!("update check request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("read update response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!("update endpoint returned {status}: {body}"));
    }
    parse_github_latest_release(&body, &current_version)
}
