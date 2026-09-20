use crate::features::ZzClawTermApp;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gpui::{Context, Window};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use zzclawterm_core::updater::{UpdateManifest, UpdateTarget, parse_current_version};
use zzclawterm_transport::connection_attempt::ConnectionAttempt;

const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDgyQUYxQTA2NTYyQTNEOTkKUldTWlBTcFdCaHF2Z29pS0pEdE13U3ZUMVZVTlpGVmQ0YlU2cWlORkdNWU1BY005MU01YjFiU2IK";

#[derive(Clone, Debug)]
pub(in crate::features) enum DownloadState {
    Idle,
    Downloading { received: u64, total: Option<u64> },
    Ready { artifact: PathBuf, target: PathBuf },
    Failed(String),
}

pub(in crate::features) fn supports_native_install(portable: bool) -> bool {
    !cfg!(debug_assertions)
        && !portable
        && (cfg!(windows) || cfg!(target_os = "macos") || std::env::var_os("APPIMAGE").is_some())
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

fn download_signed_update(
    version: &str,
    directory: &Path,
    cancel: &ConnectionAttempt,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(PathBuf, PathBuf), String> {
    let version = parse_current_version(version).map_err(|error| error.to_string())?;
    if !zzclawterm_core::app_identity::AppFlavor::current().accepts_update(&version) {
        return Err("update belongs to a different application flavor".into());
    }
    let target = UpdateTarget::from_rust_target(std::env::consts::OS, std::env::consts::ARCH)
        .map_err(|error| error.to_string())?;
    let client = zed_reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())?;
    let manifest_url = if version.pre.is_empty() {
        format!("https://github.com/jackfahdin/ZzClawTerm/releases/download/v{version}/latest.json")
    } else {
        "https://github.com/jackfahdin/ZzClawTerm/releases/download/continuous-build/latest.json"
            .to_string()
    };
    let mut manifest_body = String::new();
    client
        .get(&manifest_url)
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
    let manifest = UpdateManifest::parse_for_version(&manifest_body, &version)
        .map_err(|error| error.to_string())?;
    let selected = manifest
        .select_artifact(&version, target)
        .map_err(|error| error.to_string())?;
    let signature = decode_update_signature(&selected.signature)?;
    let public_key = STANDARD
        .decode(PUBLIC_KEY)
        .map_err(|_| "invalid public key")?;
    let public_key = minisign_verify::PublicKey::decode(
        std::str::from_utf8(&public_key).map_err(|_| "invalid public key")?,
    )
    .map_err(|_| "invalid public key")?;
    let mut verifier = public_key
        .verify_stream(&signature)
        .map_err(|_| "unsupported update signature")?;
    cancel.check()?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let name = selected.filename;
    let partial = directory.join(format!("{name}.partial"));
    let artifact = directory.join(&name);
    let result = (|| {
        let mut response = client
            .get(&selected.url)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let total = response.content_length();
        if total.is_some_and(|total| total > 1024 * 1024 * 1024) {
            return Err("update too large".into());
        }
        let mut output = std::fs::File::create(&partial).map_err(|error| error.to_string())?;
        let mut buffer = vec![0; 256 * 1024];
        let mut received = 0;
        loop {
            cancel.check()?;
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
            progress(received, total);
        }
        if total.is_some_and(|total| received != total) {
            return Err("update download truncated".into());
        }
        verifier
            .finalize()
            .map_err(|_| "update signature verification failed")?;
        output.sync_all().map_err(|error| error.to_string())?;
        drop(output);
        cancel.check()?;
        std::fs::rename(&partial, &artifact).map_err(|error| error.to_string())?;
        let target = std::env::current_exe().map_err(|error| error.to_string())?;
        super::install::prepare_artifact(artifact, target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(partial);
    }
    result
}

impl ZzClawTermApp {
    pub(in crate::features) fn start_native_update_download(&mut self, cx: &mut Context<Self>) {
        if !supports_native_install(self.runtime.mode() == zzclawterm_core::RuntimeMode::Portable) {
            return;
        }
        if matches!(self.update.download, DownloadState::Downloading { .. }) {
            return;
        }
        let Some(info) = self.update.info().filter(|info| info.available).cloned() else {
            return;
        };
        self.update.download_generation = self.update.download_generation.wrapping_add(1);
        let generation = self.update.download_generation;
        let cancel = ConnectionAttempt::default();
        self.update.download_cancel = cancel.clone();
        self.update.download = DownloadState::Downloading {
            received: 0,
            total: None,
        };
        let directory = self
            .runtime
            .cache_dir()
            .join("updates")
            .join(zzclawterm_core::uuid());
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let result_tx = tx.clone();
        let scheduled = self
            .blocking_jobs
            .submit_detached("native-update-download", move |_| {
                let result = download_signed_update(
                    &info.latest_version,
                    &directory,
                    &cancel,
                    |received, total| {
                        let _ = tx.unbounded_send(DownloadState::Downloading { received, total });
                    },
                );
                let event = match result {
                    Ok((artifact, target)) => DownloadState::Ready { artifact, target },
                    Err(error) => DownloadState::Failed(error),
                };
                let _ = result_tx.unbounded_send(event);
            });
        if let Err(error) = scheduled {
            self.update.download = DownloadState::Failed(error.to_string());
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            use futures::StreamExt as _;
            while let Some(state) = rx.next().await {
                let done = !matches!(state, DownloadState::Downloading { .. });
                if this
                    .update(cx, |app, cx| {
                        if app.update.download_generation == generation {
                            app.update.download = state;
                            cx.notify();
                        }
                    })
                    .is_err()
                    || done
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(in crate::features) fn cancel_native_update_download(&mut self, cx: &mut Context<Self>) {
        self.update.download_cancel.cancel();
        self.update.download_generation = self.update.download_generation.wrapping_add(1);
        self.update.download = DownloadState::Idle;
        cx.notify();
    }

    pub(in crate::features) fn prepare_native_update_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.update.install_launch_pending {
            return false;
        }
        if !self.update.install_requested {
            return true;
        }
        self.update.install_requested = false;
        let DownloadState::Ready { artifact, target } = self.update.download.clone() else {
            return false;
        };
        self.update.install_launch_pending = true;
        let task = self
            .blocking_jobs
            .submit_task("update-installer", move |_| {
                super::install::launch_installer(&artifact, &target)
            });
        cx.spawn(async move |this, cx| {
            let result = crate::features::runtime_jobs::await_blocking_job(task)
                .await
                .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.update.install_launch_pending = false;
                match result {
                    Ok(()) => cx.emit(crate::features::AppLifecycleEvent::ShutdownRequested),
                    Err(error) => {
                        app.update.download = DownloadState::Failed(error);
                        app.notify_operation(
                            "update-install",
                            zzclawterm_ui::notification::ZzClawNotificationKind::Error,
                            rust_i18n::t!("updater.installFailed").to_string(),
                            cx,
                        );
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        false
    }

    pub(in crate::features) fn request_native_update_install(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.update.download, DownloadState::Ready { .. }) {
            return;
        }
        if self.notes.has_open_editor_windows()
            || self
                .transfer
                .editor_workspace()
                .is_some_and(|workspace| workspace.tabs.iter().any(|tab| tab.dirty || tab.saving))
        {
            self.notify_operation(
                "update-unsaved",
                zzclawterm_ui::notification::ZzClawNotificationKind::Warning,
                rust_i18n::t!("updater.saveEditorsFirst").to_string(),
                cx,
            );
            return;
        }
        self.update.install_requested = true;
        self.close_update_dialog(window, cx);
        self.handle_window_close_request(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{PUBLIC_KEY, decode_update_signature, supports_native_install};
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    #[test]
    fn published_signing_key_loads_and_portable_installs_never_self_replace() {
        let key = STANDARD.decode(PUBLIC_KEY).unwrap();
        assert!(minisign_verify::PublicKey::decode(std::str::from_utf8(&key).unwrap()).is_ok());
        assert!(!supports_native_install(true));
        if cfg!(debug_assertions) {
            assert!(!supports_native_install(false));
        }
    }

    #[test]
    fn malformed_update_signatures_are_rejected_before_download() {
        assert!(decode_update_signature("not-base64").is_err());
        assert!(decode_update_signature(&STANDARD.encode("not a minisign signature")).is_err());
    }
}
