use crate::features::{AutoSyncResult, AutoSyncTrigger};
use std::time::{Duration, Instant};

pub(in crate::app_shell::controller) struct AutoSyncCoordinator {
    running: bool,
    pending: Option<AutoSyncTrigger>,
    next_due: Instant,
    next_periodic: Instant,
    last_focus: Option<Instant>,
    mutation_generation: u64,
    in_flight_generation: u64,
    change_since: Option<Instant>,
    remote_available: bool,
    failures: usize,
    paused: bool,
}

pub(super) struct AutoSyncJob {
    pub(super) trigger: AutoSyncTrigger,
    pub(super) change_elapsed: Duration,
    pub(super) pull_safe: bool,
}

impl AutoSyncCoordinator {
    pub(super) fn next_job(
        &mut self,
        now: Instant,
        generation: u64,
        pull_safe: impl FnOnce() -> bool,
    ) -> Option<AutoSyncJob> {
        if self.running {
            return None;
        }
        self.record_sync_mutation(generation);
        if now >= self.next_periodic {
            self.next_periodic = now + Duration::from_secs(15 * 60);
            if self.pending.is_none() {
                self.pending = Some(AutoSyncTrigger::Periodic);
                self.next_due = now;
            }
        }
        if self.paused {
            return None;
        }
        let deferred_pull = self.remote_available && self.pending.is_none();
        if !deferred_pull && (now < self.next_due || self.pending.is_none()) {
            return None;
        }
        // Inspect windows only when a job can start, preserving the idle tick cost.
        let pull_safe = pull_safe();
        if deferred_pull && pull_safe {
            self.pending = Some(AutoSyncTrigger::DeferredPull);
            self.next_due = now;
        }
        if now < self.next_due {
            return None;
        }
        let trigger = self.pending.take()?;
        self.running = true;
        self.in_flight_generation = self.mutation_generation;
        Some(AutoSyncJob {
            trigger,
            pull_safe,
            change_elapsed: self
                .change_since
                .map_or(Duration::ZERO, |since| now.saturating_duration_since(since)),
        })
    }

    pub(super) fn complete(
        &mut self,
        trigger: AutoSyncTrigger,
        result: &Result<AutoSyncResult, zzclawterm_core::CloudSyncError>,
        observed_generation: Option<u64>,
    ) {
        // Mutation events and job completions arrive independently. Observe the
        // store generation before deciding whether this job cleared all changes.
        if let Some(generation) = observed_generation {
            self.record_sync_mutation(generation);
        }
        let now = Instant::now();
        self.running = false;
        match result {
            Ok(AutoSyncResult::Debouncing(remaining)) => {
                self.pending = Some(AutoSyncTrigger::Change);
                self.next_due = now + *remaining;
            }
            Ok(AutoSyncResult::Busy) => {
                self.pending = Some(trigger);
                self.next_due = now + Duration::from_secs(5);
            }
            Ok(AutoSyncResult::RemoteAvailable { retry_when_safe }) => {
                self.remote_available = *retry_when_safe;
                self.failures = 0;
            }
            Ok(AutoSyncResult::Synced(_)) | Ok(AutoSyncResult::UpToDate(_)) => {
                self.remote_available = false;
                self.failures = 0;
                if self.mutation_generation == self.in_flight_generation {
                    self.change_since = None;
                    if self.pending == Some(AutoSyncTrigger::Change) {
                        self.pending = None;
                    }
                }
            }
            Err(zzclawterm_core::CloudSyncError::Conflict(_)) => {
                self.remote_available = false;
            }
            Err(error) => {
                self.paused = permanent_auto_sync_error(error);
                let backoff = auto_sync_backoff(self.failures);
                self.failures = self.failures.saturating_add(1);
                self.pending = Some(AutoSyncTrigger::Retry);
                self.next_due = now + backoff;
            }
            _ => {}
        }
    }

    pub(in crate::app_shell::controller) fn running(&self) -> bool {
        self.running
    }

    pub(super) fn record_sync_mutation(&mut self, generation: u64) {
        if generation <= self.mutation_generation {
            return;
        }
        self.mutation_generation = generation;
        let now = Instant::now();
        self.change_since = Some(now);
        self.pending = Some(AutoSyncTrigger::Change);
        self.next_due = now + Duration::from_secs(1);
    }

    pub(in crate::app_shell::controller) fn new(mutation_generation: u64) -> Self {
        let now = Instant::now();
        Self {
            running: false,
            pending: Some(AutoSyncTrigger::Startup),
            next_due: now + Duration::from_secs(3),
            next_periodic: now + Duration::from_secs(15 * 60),
            last_focus: None,
            mutation_generation,
            in_flight_generation: mutation_generation,
            change_since: None,
            remote_available: false,
            failures: 0,
            paused: false,
        }
    }

    pub(in crate::app_shell::controller) fn settings_changed(&mut self) {
        self.paused = false;
        self.failures = 0;
        if self.pending != Some(AutoSyncTrigger::Change) {
            self.pending = Some(AutoSyncTrigger::Settings);
            self.next_due = Instant::now();
        }
    }

    pub(in crate::app_shell::controller) fn focused(&mut self) {
        let now = Instant::now();
        if self
            .last_focus
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(120))
        {
            self.last_focus = Some(now);
            if self.pending.is_none() {
                self.pending = Some(AutoSyncTrigger::Focus);
                self.next_due = now;
            }
        }
    }
}

fn permanent_auto_sync_error(error: &zzclawterm_core::CloudSyncError) -> bool {
    match error {
        zzclawterm_core::CloudSyncError::LocalStore(message)
            if message == "crypto" || message == "invalid_data" =>
        {
            true
        }
        zzclawterm_core::CloudSyncError::Disabled
        | zzclawterm_core::CloudSyncError::PortableSnapshot(
            zzclawterm_core::PortableSnapshotError::MissingMasterPassword
            | zzclawterm_core::PortableSnapshotError::Decrypt { .. },
        ) => true,
        zzclawterm_core::CloudSyncError::Remote(message) => {
            let message = message.to_ascii_lowercase();
            [
                "401",
                "403",
                "unauthorized",
                "forbidden",
                "invalid token",
                "missing token",
                "access token is required",
                "endpoint is required",
                "bucket is required",
                "credential is required",
            ]
            .iter()
            .any(|marker| message.contains(marker))
        }
        _ => false,
    }
}

fn auto_sync_backoff(failures: usize) -> Duration {
    Duration::from_secs([1, 5, 15, 60][failures.min(3)] * 60)
}

#[cfg(test)]
mod tests;
