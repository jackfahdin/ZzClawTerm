use std::io::Read as _;
use std::time::Duration;

use zzclawterm_core::NativeUpdateInfo;
use zzclawterm_core::updater::{
    UpdateChannel, UpdateManifest, UpdateRepository, UpdateSource, parse_current_version,
};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(20);
const AUTO_GITHUB_PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const USER_AGENT: &str = concat!("zzclawterm-app/", env!("CARGO_PKG_VERSION"));

pub fn check_native_update(
    source: UpdateSource,
) -> Result<(NativeUpdateInfo, UpdateRepository), String> {
    let current_version =
        parse_current_version(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    let channel = UpdateChannel::for_version(&current_version);
    let client = zed_reqwest::blocking::Client::builder()
        .timeout(UPDATE_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| format!("build updater HTTP client failed: {error}"))?;
    let mut last_error = String::new();
    let mut preferred = None;
    for &repository in source.repositories(channel) {
        let result = (|| {
            let mut request = client
                .get(channel.manifest_url_for(repository))
                .header("Accept", "application/json");
            if source == UpdateSource::Auto
                && repository == UpdateRepository::GitHub
                && preferred.is_some()
            {
                request = request.timeout(AUTO_GITHUB_PROBE_TIMEOUT);
            }
            let response = request
                .send()
                .map_err(|error| format!("update check request failed: {error}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("update manifest endpoint returned {status}"));
            }
            let mut body = String::new();
            response
                .take(MAX_MANIFEST_BYTES + 1)
                .read_to_string(&mut body)
                .map_err(|error| format!("read update manifest failed: {error}"))?;
            if body.len() as u64 > MAX_MANIFEST_BYTES {
                return Err("update manifest is too large".to_string());
            }
            UpdateManifest::parse(&body).map_err(|error| error.to_string())
        })();
        match result {
            Ok(manifest) => {
                if preferred.as_ref().is_none_or(
                    |(current, _): &(UpdateManifest, UpdateRepository)| {
                        manifest.version > current.version
                    },
                ) {
                    preferred = Some((manifest, repository));
                }
            }
            Err(error) => last_error = error,
        }
    }
    preferred
        .map(|(manifest, repository)| {
            (
                manifest.update_info_from(&current_version, repository),
                repository,
            )
        })
        .ok_or(last_error)
}
