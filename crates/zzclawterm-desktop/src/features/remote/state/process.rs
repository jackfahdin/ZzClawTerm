use crate::features::remote::job_state::{RemoteJobState, RemoteJobTicket};
use crate::features::runtime_jobs::{ProcessJobOutput, ProcessJobResult};
use crate::models::{RemoteProcessSortDirection, RemoteProcessSortKey};
use futures::channel::mpsc::UnboundedReceiver;
use std::sync::Arc;
use std::time::Instant;
use zzclawterm_transport::RemoteProcess;

pub(in crate::features) enum ProcessApplyOutcome {
    Ignored,
    CompletedInactive,
    Applied { status: String },
}

pub(super) struct ProcessPaneState {
    job: RemoteJobState<ProcessJobResult>,
    items: Arc<[RemoteProcess]>,
    /// Bumped by every mutation that changes what `process_presentation` returns.
    revision: u64,
    data_generation: u64,
    derived: Option<ProcessDerivedCache>,
    /// Which sort keys the current panel width can show a column for.
    ///
    /// Pushed by the app when the right panel is resized. Held here because the sort
    /// key must be constrained to it whenever *either* changes, and the render pass is
    /// no longer allowed to do that constraining itself.
    sort_columns: ProcessSortColumns,
    snapshot_loaded: bool,
    status: String,
    search_draft: String,
    sort_key: RemoteProcessSortKey,
    sort_direction: RemoteProcessSortDirection,
    selected_pid: Option<u32>,
    menu_pid: Option<u32>,
    nice_draft: String,
}

/// Whether the process table is wide enough to sort by memory and by user.
///
/// Defaults to permissive, matching the widest layout: a narrow panel pushes the real
/// values in before anything is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::features) struct ProcessSortColumns {
    pub allow_memory: bool,
    pub allow_user: bool,
}

impl Default for ProcessSortColumns {
    fn default() -> Self {
        Self {
            allow_memory: true,
            allow_user: true,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ProcessDerivedKey {
    data_generation: u64,
    normalized_query: String,
    sort_key: RemoteProcessSortKey,
    sort_direction: RemoteProcessSortDirection,
}

struct ProcessDerivedCache {
    key: ProcessDerivedKey,
    items: Arc<[RemoteProcess]>,
}

#[derive(Clone)]
pub(in crate::features) struct ProcessPresentationState {
    pub items: Arc<[RemoteProcess]>,
    pub snapshot_loaded: bool,
    pub status: String,
    pub search_draft: String,
    pub sort_key: RemoteProcessSortKey,
    pub sort_direction: RemoteProcessSortDirection,
    pub selected_pid: Option<u32>,
    pub menu_pid: Option<u32>,
    pub nice_draft: String,
    pub pending: bool,
}

impl ProcessPaneState {
    pub(super) fn is_pending(&self) -> bool {
        self.job.is_pending()
    }

    pub(super) fn last_refresh_at(&self) -> Option<Instant> {
        self.job.last_refresh_at()
    }

    pub(super) fn is_pending_for(&self, session_id: &str) -> bool {
        self.job.is_pending_for(session_id)
    }

    pub(super) fn begin_job(&mut self, session_id: String) -> RemoteJobTicket<ProcessJobResult> {
        // `pending` is part of the presentation, so starting and finishing a job moves the
        // revision. A status change happens to accompany both today, which masked it.
        self.touch();
        self.job.begin(session_id)
    }

    pub(super) fn mark_refresh_started(&mut self) {
        self.job.mark_refresh_started();
    }

    pub(super) fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<ProcessJobResult>> {
        self.job.take_event_receiver()
    }

    pub(super) fn complete_event(&mut self, job_id: u64, session_id: &str) -> bool {
        self.touch();
        self.job.complete_if_matches(job_id, session_id)
    }

    pub(super) fn reset_refresh_failures(&mut self) {
        self.job.reset_refresh_failures();
    }

    pub(super) fn record_refresh_failure(&mut self, terminal: bool) -> u8 {
        self.job.record_refresh_failure(terminal)
    }

    pub(super) fn apply_search(&mut self, text: String) {
        self.search_draft = text;
        self.selected_pid = None;
        self.menu_pid = None;
        self.nice_draft = "0".to_string();
        self.reconcile();
    }

    pub(super) fn toggle_sort(&mut self, key: RemoteProcessSortKey) {
        if self.sort_key == key {
            self.sort_direction = self.sort_direction.reversed();
        } else {
            self.sort_key = key;
            self.sort_direction = match key {
                RemoteProcessSortKey::Cpu | RemoteProcessSortKey::Memory => {
                    RemoteProcessSortDirection::Descending
                }
                RemoteProcessSortKey::Pid
                | RemoteProcessSortKey::User
                | RemoteProcessSortKey::Command => RemoteProcessSortDirection::Ascending,
            };
        }
        self.reconcile();
        self.status = format!(
            "sorted processes by {} {}",
            self.sort_key.label(),
            self.sort_direction.marker()
        );
    }

    pub(super) fn toggle_selection(&mut self, pid: u32) {
        self.touch();
        self.menu_pid = None;
        self.selected_pid = (self.selected_pid != Some(pid)).then_some(pid);
        self.nice_draft = "0".to_string();
    }

    pub(super) fn apply_nice_input(&mut self, text: String) {
        let negative = text.starts_with('-');
        let digits: String = text.chars().filter(char::is_ascii_digit).take(3).collect();
        self.nice_draft = if negative {
            format!("-{digits}")
        } else {
            digits
        };
        self.touch();
    }

    pub(super) fn validated_nice_draft(&mut self) -> Option<(u32, i32)> {
        let Some(pid) = self.selected_pid else {
            self.set_status("select a process before applying nice");
            return None;
        };
        let Ok(nice) = self.nice_draft.trim().parse::<i32>() else {
            self.set_status("nice must be an integer from -20 to 19");
            return None;
        };
        if !(-20..=19).contains(&nice) {
            self.set_status("nice must be between -20 and 19");
            return None;
        }
        Some((pid, nice))
    }

    pub(super) fn apply_processes(&mut self, processes: Vec<RemoteProcess>) {
        let contains_pid = |pid| processes.iter().any(|process| process.pid == pid);
        if self.selected_pid.is_some_and(|pid| !contains_pid(pid)) {
            self.selected_pid = None;
            self.nice_draft = "0".to_string();
        }
        if self.menu_pid.is_some_and(|pid| !contains_pid(pid)) {
            self.menu_pid = None;
        }
        self.items = processes.into();
        self.data_generation = self.data_generation.wrapping_add(1);
        self.derived = None;
        self.snapshot_loaded = true;
        self.reconcile();
    }

    fn clear_data(&mut self) {
        self.items = Arc::from([]);
        self.data_generation = self.data_generation.wrapping_add(1);
        self.derived = None;
        self.snapshot_loaded = false;
        self.selected_pid = None;
        self.menu_pid = None;
        self.reconcile();
    }

    /// Record that the presentation changed.
    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.touch();
    }

    pub(super) fn toggle_menu(&mut self, pid: u32) {
        self.menu_pid = (self.menu_pid != Some(pid)).then_some(pid);
        self.touch();
    }

    pub(super) fn close_menu(&mut self) {
        self.menu_pid = None;
        self.touch();
    }

    /// Bring the derived list and sort key back in step.
    ///
    /// Called by every mutator that changes one of their inputs, so a reader never has
    /// to trigger the recompute -- which is what the render pass used to do, by calling
    /// `derived_items` and `clamp_*` through `&mut self` while building elements.
    ///
    /// Cheap to call redundantly: the recompute is keyed, so an unchanged key returns
    /// the cached list.
    fn reconcile(&mut self) {
        // Sort first: the derived list is keyed on the sort key, so constraining after
        // recomputing would sort by a key the table cannot show.
        if (!self.sort_columns.allow_user && self.sort_key == RemoteProcessSortKey::User)
            || (!self.sort_columns.allow_memory && self.sort_key == RemoteProcessSortKey::Memory)
        {
            self.sort_key = RemoteProcessSortKey::Cpu;
        }
        self.derived_items();
        self.touch();
    }

    pub(super) fn set_sort_columns(&mut self, columns: ProcessSortColumns) -> bool {
        if self.sort_columns == columns {
            return false;
        }
        self.sort_columns = columns;
        self.reconcile();
        true
    }

    /// The filtered, sorted list, without recomputing.
    ///
    /// `reconcile` guarantees the cache is populated and current, so this is a read.
    pub(super) fn derived(&self) -> Arc<[RemoteProcess]> {
        self.derived
            .as_ref()
            .map(|cache| cache.items.clone())
            .unwrap_or_else(|| Arc::from([]))
    }

    fn derived_items(&mut self) -> Arc<[RemoteProcess]> {
        let key = ProcessDerivedKey {
            data_generation: self.data_generation,
            normalized_query: self.search_draft.trim().to_ascii_lowercase(),
            sort_key: self.sort_key,
            sort_direction: self.sort_direction,
        };
        if let Some(cache) = self.derived.as_ref()
            && cache.key == key
        {
            return cache.items.clone();
        }
        let mut items = self
            .items
            .iter()
            .filter(|process| process_matches(process, &key.normalized_query))
            .cloned()
            .collect::<Vec<_>>();
        sort_processes(&mut items, key.sort_key, key.sort_direction);
        let items: Arc<[RemoteProcess]> = items.into();
        self.derived = Some(ProcessDerivedCache {
            key,
            items: items.clone(),
        });
        items
    }

    pub(super) fn reset_for_session_switch(&mut self) {
        self.job.reset_for_session_switch();
        self.clear_data();
        self.status = "ready".to_string();
    }
}

fn process_matches(process: &RemoteProcess, normalized_query: &str) -> bool {
    if normalized_query.is_empty() {
        return true;
    }
    format!(
        "{} {} {} {} {} {}",
        process.pid,
        process.ppid,
        process.user,
        process.state,
        process.command,
        process.command_line
    )
    .to_ascii_lowercase()
    .contains(normalized_query)
}

fn sort_processes(
    processes: &mut [RemoteProcess],
    key: RemoteProcessSortKey,
    direction: RemoteProcessSortDirection,
) {
    processes.sort_by(|left, right| {
        let ordering = match key {
            RemoteProcessSortKey::Command => left
                .command
                .cmp(&right.command)
                .then_with(|| left.pid.cmp(&right.pid)),
            RemoteProcessSortKey::Memory => left
                .memory_percent
                .partial_cmp(&right.memory_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.rss_kb
                        .partial_cmp(&right.rss_kb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.pid.cmp(&right.pid)),
            RemoteProcessSortKey::Pid => left.pid.cmp(&right.pid),
            RemoteProcessSortKey::User => left
                .user
                .cmp(&right.user)
                .then_with(|| left.pid.cmp(&right.pid)),
            RemoteProcessSortKey::Cpu => left
                .cpu_percent
                .partial_cmp(&right.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.memory_percent
                        .partial_cmp(&right.memory_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.pid.cmp(&right.pid)),
        };

        match direction {
            RemoteProcessSortDirection::Ascending => ordering,
            RemoteProcessSortDirection::Descending => ordering.reverse(),
        }
    });
}

impl ProcessPaneState {
    pub(super) fn new() -> Self {
        Self {
            job: RemoteJobState::new(),
            items: Arc::from([]),
            revision: 0,
            data_generation: 0,
            derived: None,
            sort_columns: ProcessSortColumns::default(),
            snapshot_loaded: false,
            status: "ready".to_string(),
            search_draft: String::new(),
            sort_key: RemoteProcessSortKey::Cpu,
            sort_direction: RemoteProcessSortDirection::Descending,
            selected_pid: None,
            menu_pid: None,
            nice_draft: "0".to_string(),
        }
    }
}

impl ProcessPaneState {
    pub(super) fn process_presentation(&self) -> ProcessPresentationState {
        ProcessPresentationState {
            items: self.items.clone(),
            snapshot_loaded: self.snapshot_loaded,
            status: self.status.clone(),
            search_draft: self.search_draft.clone(),
            sort_key: self.sort_key,
            sort_direction: self.sort_direction,
            selected_pid: self.selected_pid,
            menu_pid: self.menu_pid,
            nice_draft: self.nice_draft.clone(),
            pending: self.is_pending(),
        }
    }

    pub(super) fn process_status(&self) -> &str {
        &self.status
    }

    pub(super) fn loaded_process_count(&self) -> Option<usize> {
        (self.snapshot_loaded && !self.items.is_empty()).then_some(self.items.len())
    }

    pub(super) fn apply_process_event(
        &mut self,
        event: ProcessJobResult,
        active_session_id: Option<&str>,
    ) -> ProcessApplyOutcome {
        if !self.complete_event(event.job_id, &event.session_id) {
            return ProcessApplyOutcome::Ignored;
        }
        if active_session_id != Some(event.session_id.as_str()) {
            return ProcessApplyOutcome::CompletedInactive;
        }
        let was_list_refresh = self.status == "listing remote processes";
        let status = match event.result {
            Ok(ProcessJobOutput::Listed(processes)) => {
                self.reset_refresh_failures();
                let status = format!("loaded {} remote process(es)", processes.len());
                self.apply_processes(processes);
                status
            }
            Ok(ProcessJobOutput::Signalled {
                pid,
                signal,
                processes,
            }) => {
                self.apply_processes(processes);
                format!("sent {signal} to pid {pid}")
            }
            Ok(ProcessJobOutput::Reniced {
                pid,
                nice,
                processes,
            }) => {
                self.apply_processes(processes);
                format!("reniced pid {pid} to {nice}")
            }
            Err(error) => {
                if was_list_refresh {
                    let terminal =
                        error.contains(zzclawterm_transport::PROCESS_LIST_UNSUPPORTED_ERROR);
                    if self.record_refresh_failure(terminal) >= 3 {
                        self.clear_data();
                    }
                }
                format!("process operation failed: {error}")
            }
        };
        self.set_status(status.clone());
        ProcessApplyOutcome::Applied { status }
    }
}
