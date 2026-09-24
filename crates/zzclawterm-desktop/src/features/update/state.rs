//! Process-wide authoritative state for native updates.

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use zzclawterm_core::NativeUpdateInfo;
use zzclawterm_core::updater::{UpdateRepository, UpdateSource, parse_current_version};
use zzclawterm_transport::connection_attempt::ConnectionAttempt;

use super::download::DownloadState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateCheckKind {
    Silent,
    Manual,
}

#[derive(Clone, Debug)]
pub(crate) enum UpdatePhase {
    Idle,
    Checking,
    Available,
    Downloading { received: u64, total: Option<u64> },
    Ready,
    Applying,
    UpToDate,
    Failed { message: String, download: bool },
}

pub(crate) enum UpdateEvent {
    Check {
        generation: u64,
        kind: UpdateCheckKind,
        result: Result<(NativeUpdateInfo, UpdateRepository), String>,
    },
    Download {
        generation: u64,
        state: DownloadState,
    },
    DownloadSource {
        generation: u64,
        repository: UpdateRepository,
    },
}

pub(crate) struct UpdateStore {
    phase: UpdatePhase,
    info: Option<NativeUpdateInfo>,
    source: UpdateSource,
    repository: Option<UpdateRepository>,
    download_repository: Option<UpdateRepository>,
    status: String,
    last_silent_error: Option<String>,
    check_generation: u64,
    startup_check_started: bool,
    pub(in crate::features) download: DownloadState,
    pub(in crate::features) download_generation: u64,
    pub(in crate::features) download_cancel: ConnectionAttempt,
    pub(in crate::features) install_requested: bool,
    tx: UnboundedSender<UpdateEvent>,
    rx: Option<UnboundedReceiver<UpdateEvent>>,
}

impl UpdateStore {
    pub(crate) fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            phase: UpdatePhase::Idle,
            info: None,
            source: UpdateSource::Auto,
            repository: None,
            download_repository: None,
            status: format!("Current version {}", env!("CARGO_PKG_VERSION")),
            last_silent_error: None,
            check_generation: 0,
            startup_check_started: false,
            download: DownloadState::Idle,
            download_generation: 0,
            download_cancel: Default::default(),
            install_requested: false,
            tx,
            rx: Some(rx),
        }
    }

    pub(in crate::features) fn phase(&self) -> &UpdatePhase {
        &self.phase
    }

    #[cfg(test)]
    fn status(&self) -> &str {
        &self.status
    }

    pub(in crate::features) fn info(&self) -> Option<&NativeUpdateInfo> {
        self.info.as_ref()
    }

    pub(crate) fn source(&self) -> UpdateSource {
        self.source
    }

    pub(in crate::features) fn repository(&self) -> Option<UpdateRepository> {
        self.download_repository.or(self.repository)
    }

    pub(in crate::features) fn set_source(&mut self, source: UpdateSource) -> bool {
        if self.source == source
            || matches!(
                self.phase,
                UpdatePhase::Downloading { .. } | UpdatePhase::Ready | UpdatePhase::Applying
            )
        {
            return false;
        }
        self.source = source;
        self.check_generation = self.check_generation.wrapping_add(1);
        self.phase = UpdatePhase::Idle;
        self.info = None;
        self.repository = None;
        self.download_repository = None;
        true
    }

    pub(in crate::features) fn is_pending(&self) -> bool {
        matches!(self.phase, UpdatePhase::Checking)
    }

    pub(crate) fn mark_startup_check_started(&mut self) -> bool {
        if self.startup_check_started {
            return false;
        }
        self.startup_check_started = true;
        true
    }

    pub(crate) fn begin_check(
        &mut self,
        _kind: UpdateCheckKind,
    ) -> Option<(UnboundedSender<UpdateEvent>, u64)> {
        if matches!(
            self.phase,
            UpdatePhase::Checking
                | UpdatePhase::Downloading { .. }
                | UpdatePhase::Ready
                | UpdatePhase::Applying
        ) {
            return None;
        }
        self.check_generation = self.check_generation.wrapping_add(1);
        self.phase = UpdatePhase::Checking;
        self.status = "checking for updates...".to_string();
        self.info = None;
        self.repository = None;
        self.download_repository = None;
        self.download = DownloadState::Idle;
        self.install_requested = false;
        Some((self.tx.clone(), self.check_generation))
    }

    pub(crate) fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<UpdateEvent>> {
        self.rx.take()
    }

    pub(crate) fn begin_download(
        &mut self,
        cancel: ConnectionAttempt,
    ) -> Option<(
        NativeUpdateInfo,
        UpdateRepository,
        u64,
        UnboundedSender<UpdateEvent>,
    )> {
        if matches!(
            self.phase,
            UpdatePhase::Downloading { .. } | UpdatePhase::Applying
        ) {
            return None;
        }
        let info = self.info.as_ref().filter(|info| info.available)?.clone();
        let repository = self.repository?;
        self.download_generation = self.download_generation.wrapping_add(1);
        self.download_cancel = cancel;
        self.download_repository = None;
        self.download = DownloadState::Downloading {
            received: 0,
            total: None,
        };
        self.phase = UpdatePhase::Downloading {
            received: 0,
            total: None,
        };
        self.status = "downloading update...".to_string();
        Some((info, repository, self.download_generation, self.tx.clone()))
    }

    pub(crate) fn cancel_download(&mut self) {
        self.download_cancel.cancel();
        self.download_generation = self.download_generation.wrapping_add(1);
        self.download = DownloadState::Idle;
        self.download_repository = None;
        self.phase = if self.info.as_ref().is_some_and(|info| info.available) {
            UpdatePhase::Available
        } else {
            UpdatePhase::Idle
        };
        self.status = "update download cancelled".to_string();
    }

    pub(crate) fn mark_applying(&mut self) {
        self.phase = UpdatePhase::Applying;
        self.status = "installing update...".to_string();
    }

    pub(crate) fn clear_install_request(&mut self) {
        self.install_requested = false;
        if matches!(self.download, DownloadState::Ready(_)) {
            self.phase = UpdatePhase::Ready;
            self.status = "update ready to install".to_string();
        }
    }

    /// Apply one process-wide event. Stale generations cannot overwrite newer state.
    pub(crate) fn apply_event(&mut self, event: UpdateEvent) -> bool {
        match event {
            UpdateEvent::Check {
                generation,
                kind,
                result,
            } => {
                if generation != self.check_generation
                    || !matches!(self.phase, UpdatePhase::Checking)
                {
                    return false;
                }
                match result {
                    Ok((info, repository)) => {
                        self.last_silent_error = None;
                        self.status = if info.available {
                            format!(
                                "update available: {} -> {}",
                                info.current_version, info.latest_version
                            )
                        } else {
                            format!("ZzClawTerm is up to date ({})", info.current_version)
                        };
                        self.phase = if info.available {
                            UpdatePhase::Available
                        } else {
                            UpdatePhase::UpToDate
                        };
                        self.info = Some(info);
                        self.repository = Some(repository);
                        self.download_repository = None;
                    }
                    Err(error) if kind == UpdateCheckKind::Silent => {
                        self.last_silent_error = Some(error);
                        self.phase = UpdatePhase::Idle;
                        self.status = format!("Current version {}", env!("CARGO_PKG_VERSION"));
                        self.info = None;
                        self.repository = None;
                        self.download_repository = None;
                    }
                    Err(error) => {
                        self.status = format!("update check failed: {error}");
                        self.phase = UpdatePhase::Failed {
                            message: error,
                            download: false,
                        };
                        self.info = None;
                        self.repository = None;
                        self.download_repository = None;
                    }
                }
                true
            }
            UpdateEvent::Download { generation, state } => {
                if generation != self.download_generation {
                    return false;
                }
                self.phase = match &state {
                    DownloadState::Idle => UpdatePhase::Available,
                    DownloadState::Downloading { received, total } => UpdatePhase::Downloading {
                        received: *received,
                        total: *total,
                    },
                    DownloadState::Ready(_) => UpdatePhase::Ready,
                    DownloadState::Failed(error) => UpdatePhase::Failed {
                        message: error.clone(),
                        download: true,
                    },
                };
                self.status = match &state {
                    DownloadState::Idle => "update available".to_string(),
                    DownloadState::Downloading { .. } => "downloading update...".to_string(),
                    DownloadState::Ready(_) => "update ready to install".to_string(),
                    DownloadState::Failed(error) => format!("update download failed: {error}"),
                };
                self.download = state;
                true
            }
            UpdateEvent::DownloadSource {
                generation,
                repository,
            } => {
                if generation != self.download_generation
                    || !matches!(self.phase, UpdatePhase::Downloading { .. })
                {
                    return false;
                }
                self.download_repository = Some(repository);
                if let Some(info) = self.info.as_mut()
                    && let Ok(version) = parse_current_version(&info.latest_version)
                {
                    info.html_url = Some(repository.release_url(&version));
                }
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UpdateCheckKind, UpdateEvent, UpdatePhase, UpdateStore};
    use zzclawterm_core::updater::{UpdateRepository, UpdateSource};
    use zzclawterm_transport::connection_attempt::ConnectionAttempt;

    fn check_event(
        generation: u64,
        kind: UpdateCheckKind,
        result: Result<zzclawterm_core::NativeUpdateInfo, String>,
    ) -> UpdateEvent {
        UpdateEvent::Check {
            generation,
            kind,
            result: result.map(|info| (info, UpdateRepository::GitHub)),
        }
    }

    #[test]
    fn startup_check_is_admitted_once_per_process() {
        let mut state = UpdateStore::new();
        assert!(state.mark_startup_check_started());
        assert!(!state.mark_startup_check_started());
    }

    #[test]
    fn update_check_admission_prevents_overlapping_jobs() {
        let mut state = UpdateStore::new();
        assert!(state.begin_check(UpdateCheckKind::Manual).is_some());
        assert!(matches!(state.phase(), UpdatePhase::Checking));
        assert!(state.begin_check(UpdateCheckKind::Manual).is_none());
    }

    #[test]
    fn silent_failures_do_not_enter_the_user_visible_failed_state() {
        let mut state = UpdateStore::new();
        let (_, generation) = state.begin_check(UpdateCheckKind::Silent).unwrap();
        assert!(state.apply_event(check_event(
            generation,
            UpdateCheckKind::Silent,
            Err("offline".to_string()),
        )));
        assert!(matches!(state.phase(), UpdatePhase::Idle));
        assert!(!state.status().contains("offline"));
    }

    #[test]
    fn manual_failures_remain_visible_and_stale_results_are_ignored() {
        let mut state = UpdateStore::new();
        let (_, generation) = state.begin_check(UpdateCheckKind::Manual).unwrap();
        assert!(!state.apply_event(check_event(
            generation.wrapping_add(1),
            UpdateCheckKind::Manual,
            Err("stale".to_string()),
        )));
        assert!(state.apply_event(check_event(
            generation,
            UpdateCheckKind::Manual,
            Err("offline".to_string()),
        )));
        assert!(matches!(
            state.phase(),
            UpdatePhase::Failed {
                download: false,
                ..
            }
        ));
        assert!(state.status().contains("offline"));
    }

    #[test]
    fn changing_source_rejects_an_in_flight_check_result() {
        let mut state = UpdateStore::new();
        let (_, old_generation) = state.begin_check(UpdateCheckKind::Manual).unwrap();
        assert!(state.set_source(UpdateSource::GitHub));
        assert_eq!(state.source(), UpdateSource::GitHub);
        assert!(matches!(state.phase(), UpdatePhase::Idle));
        assert!(!state.apply_event(check_event(
            old_generation,
            UpdateCheckKind::Manual,
            Err("old source failed".to_string()),
        )));
        assert!(state.begin_check(UpdateCheckKind::Manual).is_some());
    }

    #[test]
    fn download_uses_the_repository_selected_by_the_completed_check() {
        let mut state = UpdateStore::new();
        let (_, generation) = state.begin_check(UpdateCheckKind::Manual).unwrap();
        let info = zzclawterm_core::NativeUpdateInfo {
            current_version: "0.0.5".into(),
            latest_version: "0.0.6".into(),
            release_date: None,
            release_notes: None,
            html_url: None,
            available: true,
        };
        assert!(state.apply_event(UpdateEvent::Check {
            generation,
            kind: UpdateCheckKind::Manual,
            result: Ok((info, UpdateRepository::GitCode)),
        }));
        let (_, repository, _, _) = state.begin_download(ConnectionAttempt::default()).unwrap();
        assert_eq!(repository, UpdateRepository::GitCode);
        assert!(!state.set_source(UpdateSource::GitHub));
    }

    #[test]
    fn automatic_download_fallback_updates_the_visible_repository() {
        let mut state = UpdateStore::new();
        let (_, generation) = state.begin_check(UpdateCheckKind::Manual).unwrap();
        let info = zzclawterm_core::NativeUpdateInfo {
            current_version: "0.0.5".into(),
            latest_version: "0.0.6".into(),
            release_date: None,
            release_notes: None,
            html_url: None,
            available: true,
        };
        assert!(state.apply_event(UpdateEvent::Check {
            generation,
            kind: UpdateCheckKind::Manual,
            result: Ok((info, UpdateRepository::GitCode)),
        }));
        let (_, _, download_generation, _) =
            state.begin_download(ConnectionAttempt::default()).unwrap();
        assert!(state.apply_event(UpdateEvent::DownloadSource {
            generation: download_generation,
            repository: UpdateRepository::GitHub,
        }));
        assert_eq!(state.repository(), Some(UpdateRepository::GitHub));
        assert_eq!(
            state.info().and_then(|info| info.html_url.as_deref()),
            Some("https://github.com/jackfahdin/ZzClawTerm/releases/tag/v0.0.6")
        );
        assert!(!state.apply_event(UpdateEvent::DownloadSource {
            generation: download_generation.wrapping_add(1),
            repository: UpdateRepository::GitCode,
        }));
    }
}
