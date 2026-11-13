use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::io::{Read as _, Write as _};
use std::path::Path;
use zzclawterm_core::updater::{
    SelectedUpdateArtifact, UpdateChannel, UpdateManifest, UpdatePackageKind, UpdateRepository,
    UpdateSource, UpdateTarget, parse_current_version,
};

const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENBM0E2Mzg3MzNGOUM3MDYKUldRR3gva3poMk02eXFGbGhobkVuaFVCZ0V2bnFheFRGQVlnVS9pSmFxRGc4WXNaMmZYeUZOM1cK";

pub enum DownloadProgress {
    Source(UpdateRepository),
    Bytes { received: u64, total: Option<u64> },
}

pub fn supports_native_install(portable: bool) -> bool {
    if cfg!(debug_assertions) {
        return false;
    }
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    #[cfg(windows)]
    {
        let Some(directory) = executable.parent() else {
            return false;
        };
        return if portable {
            directory.join("zzclawterm-portable").is_file()
        } else {
            super::install::is_installed_copy(directory)
        };
    }
    #[cfg(target_os = "macos")]
    {
        if portable {
            return false;
        }
        let Some(bundle) = executable.ancestors().nth(3) else {
            return false;
        };
        return bundle.extension().and_then(|value| value.to_str()) == Some("app")
            && !bundle
                .components()
                .any(|part| part.as_os_str() == "Caskroom" || part.as_os_str() == "Cellar");
    }
    #[cfg(target_os = "linux")]
    {
        let _ = executable;
        return !portable && std::env::var_os("APPIMAGE").is_some();
    }
    #[allow(unreachable_code)]
    false
}

fn decode_update_signature(value: &str) -> Result<minisign_verify::Signature, String> {
    let signature = STANDARD
        .decode(value)
        .map_err(|_| "invalid update signature encoding")?;
    minisign_verify::Signature::decode(
        std::str::from_utf8(&signature).map_err(|_| "invalid update signature")?,
    )
    .map_err(|_| "invalid update signature".to_string())
}

fn download_candidates(
    source: UpdateSource,
    repository: UpdateRepository,
    channel: UpdateChannel,
) -> &'static [UpdateRepository] {
    let order = source.repositories(channel);
    let start = order
        .iter()
        .position(|&candidate| candidate == repository)
        .unwrap_or(0);
    &order[start..]
}

fn download_attempts(
    selected: &SelectedUpdateArtifact,
    candidates: &[UpdateRepository],
    selected_index: usize,
    version: &semver::Version,
    filename: &str,
) -> Vec<(String, UpdateRepository)> {
    // 下载链：选中仓库的地址，其后每个候选仓库的同路径地址依次兜底；
    // 核心侧 fallback_url（预览通道的 GitHub 固定地址）若不在链上则补到末尾。
    let mut attempts: Vec<(String, UpdateRepository)> = vec![];
    for (index, &candidate) in candidates.iter().enumerate().skip(selected_index) {
        let url = if index == selected_index {
            selected.url.clone()
        } else {
            candidate.artifact_url(version, filename)
        };
        if !attempts.iter().any(|(existing, _)| existing == &url) {
            attempts.push((url, candidate));
        }
    }
    if let Some(fallback) = selected.fallback_url.clone()
        && !attempts.iter().any(|(url, _)| url == &fallback)
    {
        attempts.push((fallback, UpdateRepository::GitHub));
    }
    attempts
}

pub fn download_signed_update(
    version: &str,
    repository: UpdateRepository,
    source: UpdateSource,
    directory: &Path,
    portable: bool,
    check_cancel: impl Fn() -> Result<(), String>,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<super::install::PreparedUpdate, String> {
    let version = parse_current_version(version).map_err(|error| error.to_string())?;
    if !zzclawterm_core::app_identity::AppFlavor::current().accepts_update(&version) {
        return Err("update belongs to a different application flavor".into());
    }
    let target = UpdateTarget::from_rust_target(std::env::consts::OS, std::env::consts::ARCH)
        .map_err(|error| error.to_string())?;
    let package = if portable {
        UpdatePackageKind::WindowsPortable
    } else {
        UpdatePackageKind::Installed
    };
    let client = zed_reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())?;
    // 候选仓库取来源全顺序中从已确认仓库开始的尾段：自动模式依次尝试
    // Gitee → GitCode → GitHub，显式单源只有一段，预览通道只有 GitHub。
    let candidates = download_candidates(source, repository, UpdateChannel::for_version(&version));
    let mut last_error = String::new();
    let mut selected = None;
    for (index, &candidate) in candidates.iter().enumerate() {
        let result = (|| {
            let mut manifest_body = String::new();
            client
                .get(candidate.version_manifest_url(&version))
                .send()
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?
                .take(1024 * 1024 + 1)
                .read_to_string(&mut manifest_body)
                .map_err(|error| error.to_string())?;
            if manifest_body.len() > 1024 * 1024 {
                return Err("update manifest too large".into());
            }
            UpdateManifest::parse_for_version(&manifest_body, &version)
                .map_err(|error| error.to_string())?
                .select_artifact_from(
                    &version,
                    target,
                    package,
                    candidate,
                    source == UpdateSource::Auto,
                )
                .map_err(|error| error.to_string())
        })();
        match result {
            Ok(artifact) => {
                selected = Some((artifact, index));
                break;
            }
            Err(error) => last_error = error,
        }
    }
    let (selected, selected_index) = selected.ok_or(last_error)?;
    let signature = decode_update_signature(&selected.signature)?;
    let public_key = STANDARD
        .decode(PUBLIC_KEY)
        .map_err(|_| "invalid public key")?;
    let public_key = minisign_verify::PublicKey::decode(
        std::str::from_utf8(&public_key).map_err(|_| "invalid public key")?,
    )
    .map_err(|_| "invalid public key")?;
    check_cancel()?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let name = selected.filename.clone();
    let partial = directory.join(format!("{name}.partial"));
    let artifact = directory.join(&name);
    let attempts = download_attempts(&selected, candidates, selected_index, &version, &name);
    let mut last_error = "no update download URL is available".to_string();
    for (url, candidate) in &attempts {
        let candidate = *candidate;
        check_cancel()?;
        progress(DownloadProgress::Source(candidate));
        let result = (|| -> Result<(), String> {
            let mut response = client
                .get(url)
                .send()
                .and_then(|response| response.error_for_status())
                .map_err(|error| error.to_string())?;
            let total = response.content_length();
            if total.is_some_and(|total| total > 1024 * 1024 * 1024) {
                return Err("update too large".into());
            }
            let mut verifier = public_key
                .verify_stream(&signature)
                .map_err(|_| "unsupported update signature")?;
            let mut output = std::fs::File::create(&partial).map_err(|error| error.to_string())?;
            let mut buffer = vec![0; 256 * 1024];
            let mut received = 0;
            loop {
                check_cancel()?;
                let count = response
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                if count == 0 {
                    break;
                }
                received += count as u64;
                if received > 1024 * 1024 * 1024 {
                    return Err("update too large".into());
                }
                verifier.update(&buffer[..count]);
                output
                    .write_all(&buffer[..count])
                    .map_err(|error| error.to_string())?;
                progress(DownloadProgress::Bytes { received, total });
            }
            if total.is_some_and(|total| received != total) {
                return Err("update download truncated".into());
            }
            verifier
                .finalize()
                .map_err(|_| "update signature verification failed")?;
            output.sync_all().map_err(|error| error.to_string())?;
            drop(output);
            check_cancel()?;
            std::fs::rename(&partial, &artifact).map_err(|error| error.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                let target = std::env::current_exe().map_err(|error| error.to_string())?;
                return super::install::prepare_artifact(artifact, target, portable);
            }
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                check_cancel()?;
                last_error = error;
            }
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::{
        PUBLIC_KEY, decode_update_signature, download_attempts, download_candidates,
        supports_native_install,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    #[test]
    fn auto_download_candidates_follow_the_full_mirror_order() {
        use semver::Version;
        use zzclawterm_core::updater::{
            SelectedUpdateArtifact, UpdateChannel, UpdateRepository, UpdateSource,
        };

        let channel = UpdateChannel::Stable;
        // 检查阶段确认 Gitee 后，候选是完整三段；确认 GitCode 后只剩尾段。
        assert_eq!(
            download_candidates(UpdateSource::Auto, UpdateRepository::Gitee, channel),
            &[
                UpdateRepository::Gitee,
                UpdateRepository::GitCode,
                UpdateRepository::GitHub
            ]
        );
        assert_eq!(
            download_candidates(UpdateSource::Auto, UpdateRepository::GitCode, channel),
            &[UpdateRepository::GitCode, UpdateRepository::GitHub]
        );
        assert_eq!(
            download_candidates(UpdateSource::Auto, UpdateRepository::GitHub, channel),
            &[UpdateRepository::GitHub]
        );
        assert_eq!(
            download_candidates(UpdateSource::Gitee, UpdateRepository::Gitee, channel),
            &[UpdateRepository::Gitee]
        );

        let version = Version::parse("0.2.0").unwrap();
        let filename = "ZzClawTerm_0.2.0_windows_x64-setup.exe";
        let selected = |url: String, fallback_url: Option<String>| SelectedUpdateArtifact {
            url,
            fallback_url,
            signature: "signature".into(),
            filename: filename.to_string(),
        };
        let candidates = download_candidates(UpdateSource::Auto, UpdateRepository::Gitee, channel);
        let attempts = download_attempts(
            &selected(
                format!(
                    "https://gitee.com/jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                ),
                None,
            ),
            candidates,
            0,
            &version,
            filename,
        );
        assert_eq!(
            attempts,
            vec![
                (
                    format!(
                        "https://gitee.com/jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                    ),
                    UpdateRepository::Gitee,
                ),
                (
                    format!(
                        "https://gitcode.com/Jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                    ),
                    UpdateRepository::GitCode,
                ),
                (
                    format!(
                        "https://github.com/jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                    ),
                    UpdateRepository::GitHub,
                ),
            ]
        );
        // 从 GitCode 段选中时 Gitee 不再出现，GitHub 仍在链尾。
        let candidates =
            download_candidates(UpdateSource::Auto, UpdateRepository::GitCode, channel);
        let attempts = download_attempts(
            &selected(
                format!(
                    "https://gitcode.com/Jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                ),
                Some(format!(
                    "https://github.com/jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                )),
            ),
            candidates,
            0,
            &version,
            filename,
        );
        assert_eq!(
            attempts
                .iter()
                .map(|(_, repository)| *repository)
                .collect::<Vec<_>>(),
            vec![UpdateRepository::GitCode, UpdateRepository::GitHub]
        );
        // 显式单源只有选中地址本身。
        let candidates = download_candidates(UpdateSource::Gitee, UpdateRepository::Gitee, channel);
        let attempts = download_attempts(
            &selected(
                format!(
                    "https://gitee.com/jackfahdin/ZzClawTerm/releases/download/v{version}/{filename}"
                ),
                None,
            ),
            candidates,
            0,
            &version,
            filename,
        );
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].1, UpdateRepository::Gitee);
        // 预览通道选中 GitHub 后，核心侧的固定版本地址补在链尾。
        let preview = Version::parse("0.2.0-preview.1").unwrap();
        let candidates = download_candidates(
            UpdateSource::Auto,
            UpdateRepository::GitHub,
            UpdateChannel::Preview,
        );
        let attempts = download_attempts(
            &selected(
                format!(
                    "https://github.com/jackfahdin/ZzClawTerm/releases/download/continuous-build/{filename}"
                ),
                Some(format!(
                    "https://github.com/jackfahdin/ZzClawTerm/releases/download/v{preview}/{filename}"
                )),
            ),
            candidates,
            0,
            &preview,
            filename,
        );
        assert_eq!(attempts.len(), 2);
        assert!(attempts[0].0.contains("continuous-build"));
        assert!(attempts[1].0.contains(&format!("v{preview}")));
    }

    #[test]
    fn published_signing_key_loads_and_debug_builds_do_not_self_update() {
        let key = STANDARD.decode(PUBLIC_KEY).unwrap();
        assert!(minisign_verify::PublicKey::decode(std::str::from_utf8(&key).unwrap()).is_ok());
        if cfg!(debug_assertions) {
            assert!(!supports_native_install(true));
            assert!(!supports_native_install(false));
        }
    }

    #[test]
    fn malformed_update_signatures_are_rejected_before_download() {
        assert!(decode_update_signature("not-base64").is_err());
        assert!(decode_update_signature(&STANDARD.encode("not a minisign signature")).is_err());
    }

    /// 内置公钥必须与发布流程用来签名的公钥一致。只断言"能解析"是不够的：换了
    /// 签名密钥却忘了同步这里时，每次更新都会在验签阶段以 UnexpectedKeyId 失败，
    /// 而单测与构建都不会察觉。
    #[test]
    fn embedded_signing_key_matches_the_publishing_workflows() {
        let workflows =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows");
        let mut checked = 0;
        for entry in std::fs::read_dir(&workflows).expect("read workflows directory") {
            let path = entry.expect("workflow directory entry").path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("yml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read workflow");
            for line in text.lines() {
                if let Some(value) = line.trim().strip_prefix("TAURI_UPDATER_PUBLIC_KEY_B64: ") {
                    checked += 1;
                    assert_eq!(PUBLIC_KEY, value.trim(), "{}", path.display());
                }
            }
        }
        assert!(
            checked > 0,
            "no workflow declares TAURI_UPDATER_PUBLIC_KEY_B64"
        );
    }
}
