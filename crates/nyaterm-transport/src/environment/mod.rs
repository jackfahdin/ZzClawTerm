//! Load and cache the user's shell environment in memory.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

mod parser;
mod shell;
#[cfg(all(test, any(unix, windows)))]
mod test;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
// Environment variable names are embedded in query scripts. Keep a runtime
// limit independent of any persistence format to prevent unbounded scripts and
// error messages while leaving room for common platform extensions.
const MAX_ENVIRONMENT_VARIABLE_NAME_LENGTH: usize = 1024;
// Batch queries embed variable names in shell scripts. Limit both the count and
// estimated script size to prevent excessive CPU or memory use and Windows
// command-line length failures.
const MAX_VARIABLES_PER_BATCH: usize = 256;
#[cfg(windows)]
const MAX_BATCH_SCRIPT_BYTES: usize = 8 * 1024;
#[cfg(not(windows))]
const MAX_BATCH_SCRIPT_BYTES: usize = 64 * 1024;
// Requested variables may be added on demand. Limit the total entry count and
// UTF-8 byte size so repeated large values from an external shell cannot grow
// the cache without bound during a long-running process.
const MAX_CACHED_ENVIRONMENT_VARIABLES: usize = 8192;
const MAX_CACHED_ENVIRONMENT_VALUE_BYTES: usize = 8 * 1024 * 1024;
// The missing-variable set is only a negative cache. Drop old entries at the
// limit so it cannot grow without bound in a long-running process.
const MAX_NEGATIVE_CACHE_ENTRIES: usize = 4096;
// Used only by the query shell to identify its purpose; never expose it in a
// complete snapshot or local terminal.
pub(super) const SHELL_ENV_READER_VARIABLE: &str = "NYATERM_SHELL_ENV_READER";
#[cfg(any(unix, windows))]
pub(super) const COMPLETE_SNAPSHOT_SENTINEL_VARIABLE: &str = "__NYATERM_ENV_COMPLETE_SENTINEL__";

/// A variable value returned by a shell environment query.
///
/// The contents are zeroed when the last copy is dropped. Debug output is
/// deliberately redacted so callers cannot accidentally log the value.
#[derive(Clone)]
pub struct EnvironmentValue(Arc<Zeroizing<String>>);

impl EnvironmentValue {
    pub(crate) fn new(value: String) -> Self {
        // Query results may be held by the cache, snapshots, and callers. Share
        // immutable values through Arc instead of copying potentially large
        // strings for every `cached`/`resolve` call. `Zeroizing<String>` still
        // clears the backing buffer when the last copy is dropped.
        Self(Arc::new(Zeroizing::new(value)))
    }

    /// Borrow the variable value for a short-lived operation.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Debug for EnvironmentValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// Errors produced while reading environment variables from a shell.
#[derive(Debug, Error)]
pub enum ShellEnvironmentError {
    #[error("invalid environment variable name")]
    InvalidVariableName,
    #[error("too many environment variables requested in one batch")]
    RequestTooLarge,
    #[error("cached shell environment exceeds the memory limit")]
    CacheLimitExceeded,
    #[error("failed to start the user shell: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("the user shell did not finish before the timeout")]
    Timeout,
    #[error("the user shell produced too much output")]
    OutputTooLarge,
    #[error("the user shell exited unsuccessfully")]
    ShellExit,
    #[error("failed to read the user shell output: {0}")]
    Read(#[source] std::io::Error),
    #[error("failed to decode the user shell output")]
    OutputEncoding,
    #[error("failed to decode an environment variable value")]
    ValueEncoding,
}

/// A complete environment snapshot returned by one shell query.
///
/// The snapshot is kept in memory and shares its backing `Arc` with the cache.
/// Reading it does not copy every variable; copy-on-write occurs only when a
/// later incremental refresh mutates the map. [`EnvironmentValue`] still
/// zeroes each value, and Debug output exposes only the variable count. If the
/// shell query fails, the cache provides an inherited-environment fallback
/// without removing variables that cannot be represented as UTF-8 from the
/// local terminal.
#[derive(Clone)]
pub struct EnvironmentSnapshot {
    values: Arc<HashMap<String, EnvironmentValue>>,
    exact: bool,
    source_shell: Option<PathBuf>,
}

impl EnvironmentSnapshot {
    /// Read a variable value from the snapshot by name.
    pub fn get(&self, variable: &str) -> Option<EnvironmentValue> {
        // Try the exact name first: complete snapshots allow `$` in names, so
        // supporting the `$NAME` convenience form must not make such a valid
        // variable permanently unreadable. Treat a leading `$` as convenience
        // syntax only when the exact name is absent.
        if is_snapshot_environment_variable_name(variable)
            && let Some(value) = self.values.get(&environment_cache_key(variable)).cloned()
        {
            return Some(value);
        }
        let variable = variable.strip_prefix('$')?;
        is_snapshot_environment_variable_name(variable)
            .then(|| self.values.get(&environment_cache_key(variable)).cloned())
            .flatten()
    }

    /// Return the number of variables in the snapshot.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Return whether the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Return whether the snapshot came from the shell selected by the caller.
    ///
    /// Without an explicit shell, the platform candidate chain is used and any
    /// complete snapshot may be reused. With an explicit shell, require an
    /// exact source match so another login configuration is not injected into a
    /// local terminal.
    pub(crate) fn matches_shell_path(&self, requested: Option<&Path>) -> bool {
        match requested {
            None => true,
            Some(requested) => self
                .source_shell
                .as_deref()
                .is_some_and(|source| source == requested),
        }
    }

    /// Iterate over snapshot variables without copying their values.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &EnvironmentValue)> {
        self.values
            .iter()
            .map(|(variable, value)| (variable.as_str(), value))
    }

    pub(crate) fn replaces_inherited_environment(&self) -> bool {
        if !self.exact {
            return false;
        }

        // The complete protocol carries only names and values representable as
        // UTF-8 and valid in an environment block. If the parent process has
        // an unrepresentable name or value, do not clear portable-pty's
        // inherited environment or that variable would be removed by mistake.
        std::env::vars_os().all(|(variable, value)| {
            #[cfg(windows)]
            if variable.to_str().is_some_and(|name| name.starts_with('=')) {
                // Windows environment blocks contain current-directory pseudo
                // variables such as `=C:`. They are not ordinary variables,
                // are not carried by the complete shell protocol, and are
                // handled separately when creating the child process.
                return true;
            }
            variable
                .to_str()
                .is_some_and(is_snapshot_environment_variable_name)
                && value.to_str().is_some()
        })
    }
}

impl std::fmt::Debug for EnvironmentSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnvironmentSnapshot")
            .field("variable_count", &self.values.len())
            .finish()
    }
}

struct CacheState {
    values: Arc<HashMap<String, EnvironmentValue>>,
    missing: HashSet<String>,
    initialized: bool,
    auto_refreshes: HashSet<String>,
    auto_refresh_pending: HashSet<String>,
    auto_refresh_in_progress: bool,
    exact: bool,
    source_shell: Option<PathBuf>,
    cached_value_bytes: usize,
}

struct CacheLookup {
    value: Option<EnvironmentValue>,
    missing: bool,
    initialized: bool,
    auto_refresh_attempted: bool,
    auto_refresh_in_progress: bool,
}

enum AutoRefreshClaim {
    Leader,
    Wait,
    AlreadyCompleted,
}

/// Release the global refresh state if an automatic refresh is cancelled, so
/// later queries do not wait forever.
struct AutoRefreshGuard<'a> {
    cache: &'a ShellEnvironmentCache,
    active: bool,
}

impl<'a> AutoRefreshGuard<'a> {
    fn new(cache: &'a ShellEnvironmentCache) -> Self {
        Self {
            cache,
            active: true,
        }
    }

    fn finish(&mut self) -> Result<(), ShellEnvironmentError> {
        if self.active {
            self.cache.finish_auto_refresh()?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for AutoRefreshGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.cache.cancel_auto_refresh();
        }
    }
}

/// Load and cache variables exported by the user's login shell.
///
/// The cache exists only for the lifetime of the process; it is never persisted,
/// logged, or serialized. Call [`ShellEnvironmentCache::initialize`] during
/// application startup to obtain a complete snapshot. Once ready, local
/// terminals and other callers can read it without starting another shell.
/// Single-variable queries remain available on demand, while explicit
/// `refresh`/`refresh_many` calls rerun the shell. A variable missing from a
/// complete snapshot triggers one automatic full refresh, preventing stale
/// data from lasting forever while refresh markers coalesce concurrent requests.
pub struct ShellEnvironmentCache {
    values: RwLock<CacheState>,
    load_lock: Mutex<()>,
    auto_refresh_notify: tokio::sync::Notify,
    timeout: Duration,
    shell_path: Option<PathBuf>,
    detected_shell: RwLock<Option<PathBuf>>,
}

impl ShellEnvironmentCache {
    /// Create a cache using the platform shell policy and a ten-second timeout.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            values: RwLock::new(CacheState {
                values: Arc::new(HashMap::new()),
                missing: HashSet::new(),
                initialized: false,
                auto_refreshes: HashSet::new(),
                auto_refresh_pending: HashSet::new(),
                auto_refresh_in_progress: false,
                exact: false,
                source_shell: None,
                cached_value_bytes: 0,
            }),
            load_lock: Mutex::new(()),
            auto_refresh_notify: tokio::sync::Notify::new(),
            timeout: DEFAULT_TIMEOUT,
            shell_path: None,
            detected_shell: RwLock::new(None),
        })
    }

    /// Return the process-wide runtime cache shared by transport services.
    pub fn global() -> Arc<Self> {
        static GLOBAL: OnceLock<Arc<ShellEnvironmentCache>> = OnceLock::new();
        GLOBAL.get_or_init(Self::new).clone()
    }

    /// Ensure the complete shell environment is loaded. An existing exact
    /// snapshot is reused; an inherited fallback snapshot retries the shell.
    ///
    /// If the user's shell cannot start, first store the inherited environment as
    /// a fallback and then return the original error. Callers can decide whether
    /// to notify the user while the local terminal can still start.
    pub async fn initialize(&self) -> Result<(), ShellEnvironmentError> {
        self.initialize_until(Instant::now() + self.timeout).await
    }

    /// Force a complete reload of the shell environment.
    ///
    /// If the refresh fails, retain the previous valid snapshot so callers can
    /// continue using the old environment; still return the error to callers
    /// that need to observe the failure.
    pub async fn refresh_all(&self) -> Result<(), ShellEnvironmentError> {
        self.refresh_all_until(Instant::now() + self.timeout).await
    }

    /// Return the current complete snapshot without starting a shell.
    pub fn snapshot(&self) -> Result<Option<Arc<EnvironmentSnapshot>>, ShellEnvironmentError> {
        let state = self
            .values
            .read()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        if !state.initialized {
            return Ok(None);
        }
        Ok(Some(Arc::new(EnvironmentSnapshot {
            values: Arc::clone(&state.values),
            exact: state.exact,
            source_shell: state.source_shell.clone(),
        })))
    }

    /// Return whether a snapshot (exact or inherited fallback) is installed
    /// without starting a shell.
    pub fn is_initialized(&self) -> Result<bool, ShellEnvironmentError> {
        self.values
            .read()
            .map(|state| state.initialized)
            .map_err(|_| ShellEnvironmentError::ShellExit)
    }

    /// Return whether the current snapshot was successfully loaded by the target
    /// shell without starting a shell.
    pub fn has_exact_snapshot(&self) -> Result<bool, ShellEnvironmentError> {
        self.values
            .read()
            .map(|state| state.initialized && state.exact)
            .map_err(|_| ShellEnvironmentError::ShellExit)
    }

    async fn initialize_until(&self, deadline: Instant) -> Result<(), ShellEnvironmentError> {
        let _load_guard = self.acquire_load_guard(deadline).await?;
        if self.has_exact_snapshot()? {
            return Ok(());
        }

        match self.load_complete_environment_until(deadline).await {
            Ok((values, selected_shell)) => {
                let source_shell = selected_shell.clone().or_else(|| self.shell_path.clone());
                self.store_complete_snapshot(values, true, true, source_shell)?;
                if let Some(selected_shell) = selected_shell {
                    self.remember_detected_shell(selected_shell)?;
                }
                Ok(())
            }
            Err(error) => {
                // Startup must not block the local terminal just because the
                // user's shell is temporarily unavailable. The inherited
                // environment is a usable fallback snapshot. Later reads of
                // missing variables still attempt a refresh, while the
                // original error is returned to the direct `initialize` caller.
                self.store_complete_snapshot(inherited_environment_snapshot(), true, false, None)?;
                Err(error)
            }
        }
    }

    async fn refresh_all_until(&self, deadline: Instant) -> Result<(), ShellEnvironmentError> {
        self.refresh_all_until_with_mode(deadline, true).await
    }

    async fn refresh_all_auto_until(&self, deadline: Instant) -> Result<(), ShellEnvironmentError> {
        self.refresh_all_until_with_mode(deadline, false).await
    }

    async fn refresh_all_until_with_mode(
        &self,
        deadline: Instant,
        clear_auto_refreshes: bool,
    ) -> Result<(), ShellEnvironmentError> {
        let _load_guard = self.acquire_load_guard(deadline).await?;
        if clear_auto_refreshes && self.shell_path.is_none() {
            // An explicit refresh reevaluates SHELL/the candidate chain so a
            // login-shell change cannot leave us reusing a stale path forever.
            // Automatic refreshes keep the verified path to limit startup cost.
            self.clear_detected_shell()?;
        }
        let (values, selected_shell) = self.load_complete_environment_until(deadline).await?;
        let source_shell = selected_shell.clone().or_else(|| self.shell_path.clone());
        self.store_complete_snapshot(values, clear_auto_refreshes, true, source_shell)?;
        if let Some(selected_shell) = selected_shell {
            self.remember_detected_shell(selected_shell)?;
        }
        Ok(())
    }

    /// Read a cached value without starting a shell.
    pub fn cached(
        &self,
        variable: &str,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        Ok(self.cache_entry_normalized(&variable)?.value)
    }

    fn cache_entry_normalized(&self, variable: &str) -> Result<CacheLookup, ShellEnvironmentError> {
        let values = self
            .values
            .read()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        let key = environment_cache_key(variable);
        Ok(CacheLookup {
            value: values.values.get(&key).cloned(),
            missing: values.missing.contains(&key),
            initialized: values.initialized,
            auto_refresh_attempted: values.auto_refreshes.contains(&key)
                || values.auto_refresh_pending.contains(&key),
            auto_refresh_in_progress: values.auto_refresh_in_progress,
        })
    }

    /// Resolve a variable, loading it from the shell when it is not cached.
    pub async fn resolve(
        &self,
        variable: &str,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        self.resolve_until(variable, Instant::now() + self.timeout)
            .await
    }

    /// Resolve a variable before the specified deadline.
    pub(crate) async fn resolve_until(
        &self,
        variable: &str,
        deadline: Instant,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        let mut lookup = self.cache_entry_normalized(&variable)?;
        if let Some(value) = lookup.value.take() {
            return Ok(Some(value));
        }

        // If startup warm-up is still pending, the first on-demand read also
        // loads a complete snapshot. This avoids splitting one batch across
        // multiple expensive shell launches. Initialization failures fall back
        // to the current process environment.
        if !lookup.initialized {
            let initialize_result = self.initialize_until(deadline).await;
            lookup = self.cache_entry_normalized(&variable)?;
            if let Some(value) = lookup.value {
                return Ok(Some(value));
            }
            // On shell failure, initialize installs an inherited fallback
            // snapshot. Continue through the automatic full-refresh path so the
            // current request can still discover a variable that became
            // available later. If even the fallback cannot be installed, return
            // the initialization error instead of silently reporting a miss.
            if !lookup.initialized {
                initialize_result?;
            }
        }

        if lookup.initialized {
            if lookup.auto_refresh_attempted {
                if lookup.auto_refresh_in_progress {
                    return self.wait_for_auto_refresh(&variable, deadline).await;
                }
                return Ok(None);
            }
            match self.claim_auto_refresh(&variable)? {
                AutoRefreshClaim::AlreadyCompleted => return Ok(None),
                AutoRefreshClaim::Wait => {
                    return self.wait_for_auto_refresh(&variable, deadline).await;
                }
                AutoRefreshClaim::Leader => {}
            }

            // A variable may be absent because shell configuration took effect
            // after application startup. Automatically refresh once per
            // variable in the current snapshot; explicit `refresh_all` clears
            // these markers.
            let mut auto_refresh_guard = AutoRefreshGuard::new(self);
            let refresh_result = self.refresh_all_auto_until(deadline).await;
            auto_refresh_guard.finish()?;
            refresh_result?;
            let refreshed = self.cache_entry_normalized(&variable)?;
            if let Some(value) = refreshed.value {
                return Ok(Some(value));
            }
            return Ok(None);
        }

        if lookup.missing {
            return Ok(None);
        }

        // This path is only expected when initialization failed and no inherited
        // fallback was available. Keep targeted loading as a final compatibility
        // path so custom callers are not blocked by the complete-snapshot flow.
        self.warm_internal(std::slice::from_ref(&variable), false, deadline)
            .await?;
        Ok(self.cache_entry_normalized(&variable)?.value)
    }

    /// Rerun the user's shell and force-refresh one variable.
    pub async fn refresh(
        &self,
        variable: &str,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        self.refresh_until(variable, Instant::now() + self.timeout)
            .await
    }

    /// Force-refresh a variable before the specified deadline.
    pub(crate) async fn refresh_until(
        &self,
        variable: &str,
        deadline: Instant,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        self.warm_internal(std::slice::from_ref(&variable), true, deadline)
            .await?;
        Ok(self.cache_entry_normalized(&variable)?.value)
    }

    /// Load all requested variables that are not already cached.
    pub async fn warm(&self, variables: &[String]) -> Result<(), ShellEnvironmentError> {
        self.warm_internal(variables, false, Instant::now() + self.timeout)
            .await
    }

    /// Force-refresh all requested variables with one shell invocation.
    pub async fn refresh_many(&self, variables: &[String]) -> Result<(), ShellEnvironmentError> {
        self.warm_internal(variables, true, Instant::now() + self.timeout)
            .await
    }

    async fn warm_internal(
        &self,
        variables: &[String],
        force_refresh: bool,
        deadline: Instant,
    ) -> Result<(), ShellEnvironmentError> {
        let variables = normalize_variable_names(variables)?;
        if variables.is_empty() {
            return Ok(());
        }

        let _load_guard = self.acquire_load_guard(deadline).await?;
        let variables_to_load = self.take_variables_needing_load(variables, force_refresh)?;
        if variables_to_load.is_empty() {
            return Ok(());
        }

        let loaded = match self
            .load_from_shell_until(&variables_to_load, deadline)
            .await
        {
            Ok(loaded) => loaded,
            Err(error) if !force_refresh => {
                let (used_inherited_value, all_resolved) =
                    self.store_inherited_values(&variables_to_load)?;
                if used_inherited_value && all_resolved {
                    return Ok(());
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        self.store_shell_result(&variables_to_load, loaded)
    }

    /// Acquire the batch-load lock before the specified deadline.
    ///
    /// Use an asynchronous timeout inside a Tokio runtime. Synchronous callers
    /// have no Tokio scheduler, so poll with `try_lock` instead of waiting
    /// without a bound. The synchronous path blocks only its calling thread,
    /// never a Tokio worker.
    async fn acquire_load_guard(
        &self,
        deadline: Instant,
    ) -> Result<tokio::sync::MutexGuard<'_, ()>, ShellEnvironmentError> {
        let remaining = remaining_until(deadline)?;
        if tokio::runtime::Handle::try_current().is_ok() {
            return tokio::time::timeout(remaining, self.load_lock.lock())
                .await
                .map_err(|_| ShellEnvironmentError::Timeout);
        }

        self.acquire_load_guard_without_runtime(deadline)
    }

    fn acquire_load_guard_without_runtime(
        &self,
        deadline: Instant,
    ) -> Result<tokio::sync::MutexGuard<'_, ()>, ShellEnvironmentError> {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(ShellEnvironmentError::Timeout);
            }
            if let Ok(guard) = self.load_lock.try_lock() {
                return Ok(guard);
            }
            // Without a Tokio runtime there is no Tokio timer; a millisecond
            // sleep avoids a busy loop.
            let remaining = deadline.saturating_duration_since(now);
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
        }
    }

    fn take_variables_needing_load(
        &self,
        variables: Vec<String>,
        force_refresh: bool,
    ) -> Result<Vec<String>, ShellEnvironmentError> {
        let mut state = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        let mut variables_to_load = Vec::with_capacity(variables.len());
        for variable in variables {
            let key = environment_cache_key(&variable);
            if force_refresh {
                if let Some(previous) = Arc::make_mut(&mut state.values).remove(&key) {
                    state.cached_value_bytes = state
                        .cached_value_bytes
                        .saturating_sub(previous.as_str().len());
                }
                state.missing.remove(&key);
                state.auto_refreshes.remove(&key);
                state.auto_refresh_pending.remove(&key);
                variables_to_load.push(variable);
            } else if !state.values.contains_key(&key) && !state.missing.contains(&key) {
                variables_to_load.push(variable);
            }
        }
        Ok(variables_to_load)
    }

    fn store_inherited_values(
        &self,
        variables: &[String],
    ) -> Result<(bool, bool), ShellEnvironmentError> {
        let mut inherited = HashMap::new();
        let mut all_resolved = true;
        for variable in variables {
            if let Some(value) = inherited_environment_value(variable) {
                inherited.insert(variable.clone(), value);
            } else {
                all_resolved = false;
            }
        }
        let used_inherited_value = !inherited.is_empty();
        self.store_values(inherited)?;
        Ok((used_inherited_value, all_resolved))
    }

    fn store_shell_result(
        &self,
        requested: &[String],
        mut loaded: HashMap<String, EnvironmentValue>,
    ) -> Result<(), ShellEnvironmentError> {
        loaded = canonicalize_environment_values(loaded);
        let mut missing = HashSet::new();
        for variable in requested {
            let key = environment_cache_key(variable);
            if loaded.contains_key(&key) {
                continue;
            }
            missing.insert(key);
        }

        let mut state = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        ensure_cached_values_fit(&state, &loaded)?;
        for (variable, value) in loaded {
            state.missing.remove(&variable);
            state.auto_refreshes.remove(&variable);
            state.auto_refresh_pending.remove(&variable);
            let value_bytes = value.as_str().len();
            let previous = Arc::make_mut(&mut state.values).insert(variable, value);
            if let Some(previous) = previous {
                state.cached_value_bytes = state
                    .cached_value_bytes
                    .saturating_sub(previous.as_str().len());
            }
            state.cached_value_bytes = state.cached_value_bytes.saturating_add(value_bytes);
        }
        for variable in missing {
            if let Some(previous) = Arc::make_mut(&mut state.values).remove(&variable) {
                state.cached_value_bytes = state
                    .cached_value_bytes
                    .saturating_sub(previous.as_str().len());
            }
            insert_bounded_negative(&mut state.missing, variable);
        }
        Ok(())
    }

    async fn wait_for_auto_refresh(
        &self,
        variable: &str,
        deadline: Instant,
    ) -> Result<Option<EnvironmentValue>, ShellEnvironmentError> {
        // Synchronous compatibility callers without a Tokio runtime do not wait
        // concurrently. Read the current snapshot directly instead of blocking
        // a thread without timer support.
        if tokio::runtime::Handle::try_current().is_err() {
            return Ok(self.cache_entry_normalized(variable)?.value);
        }

        loop {
            let notified = self.auto_refresh_notify.notified();
            let lookup = self.cache_entry_normalized(variable)?;
            if let Some(value) = lookup.value {
                return Ok(Some(value));
            }
            if !lookup.auto_refresh_in_progress {
                return Ok(None);
            }
            let remaining = remaining_until(deadline)?;
            tokio::time::timeout(remaining, notified)
                .await
                .map_err(|_| ShellEnvironmentError::Timeout)?;
        }
    }

    fn claim_auto_refresh(
        &self,
        variable: &str,
    ) -> Result<AutoRefreshClaim, ShellEnvironmentError> {
        let mut state = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        let key = environment_cache_key(variable);
        if state.auto_refreshes.contains(&key) {
            return Ok(AutoRefreshClaim::AlreadyCompleted);
        }
        insert_bounded_negative(&mut state.auto_refresh_pending, key);
        if state.auto_refresh_in_progress {
            Ok(AutoRefreshClaim::Wait)
        } else {
            state.auto_refresh_in_progress = true;
            Ok(AutoRefreshClaim::Leader)
        }
    }

    fn finish_auto_refresh(&self) -> Result<(), ShellEnvironmentError> {
        let mut state = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        state.auto_refresh_in_progress = false;
        let pending = std::mem::take(&mut state.auto_refresh_pending);
        for variable in &pending {
            if !state.values.contains_key(variable) {
                insert_bounded_negative(&mut state.missing, variable.clone());
            }
        }
        for variable in pending {
            insert_bounded_negative(&mut state.auto_refreshes, variable);
        }
        self.auto_refresh_notify.notify_waiters();
        Ok(())
    }

    fn cancel_auto_refresh(&self) {
        let Ok(mut state) = self.values.write() else {
            return;
        };
        state.auto_refresh_in_progress = false;
        state.auto_refresh_pending.clear();
        self.auto_refresh_notify.notify_waiters();
    }

    fn store_complete_snapshot(
        &self,
        values: HashMap<String, EnvironmentValue>,
        clear_auto_refreshes: bool,
        exact: bool,
        source_shell: Option<PathBuf>,
    ) -> Result<(), ShellEnvironmentError> {
        let mut state = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        let values = canonicalize_environment_values(values);
        ensure_environment_map_fit(&values)?;
        state.cached_value_bytes = values.values().map(|value| value.as_str().len()).sum();
        state.values = Arc::new(values);
        state.missing.clear();
        if clear_auto_refreshes {
            state.auto_refreshes.clear();
            state.auto_refresh_pending.clear();
        }
        state.initialized = true;
        state.exact = exact;
        state.source_shell = source_shell;
        Ok(())
    }

    async fn load_from_shell_until(
        &self,
        variables: &[String],
        deadline: Instant,
    ) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
        let configured_shell = self.shell_path.clone();
        let cached_shell = self.detected_shell_path()?;
        let shell_path = configured_shell.clone().or(cached_shell.clone());
        if tokio::runtime::Handle::try_current().is_err() {
            let (loaded, selected_shell) = match self
                .load_environment_on_runtime_thread(variables, deadline, shell_path)
                .await
            {
                Ok(result) => result,
                // An automatically detected shell may be uninstalled or removed
                // from PATH while the application is running. Retry the
                // candidate chain only for this cached path; an explicitly
                // configured path must follow the user's choice strictly.
                Err(ShellEnvironmentError::Spawn(_))
                    if configured_shell.is_none() && cached_shell.is_some() =>
                {
                    self.load_environment_on_runtime_thread(variables, deadline, None)
                        .await?
                }
                Err(error) => return Err(error),
            };
            if let Some(selected_shell) = selected_shell {
                self.remember_detected_shell(selected_shell)?;
            }
            return Ok(loaded);
        }
        let timeout = remaining_until(deadline)?;
        let (loaded, selected_shell) = match shell::load_environment_from_shell_with_fallback(
            shell_path.as_deref(),
            timeout,
            variables,
        )
        .await
        {
            Ok(result) => result,
            // An automatically detected shell may be uninstalled or removed
            // from PATH while the application is running. Retry the candidate
            // chain only for this cached path; an explicitly configured path
            // must follow the user's choice strictly.
            Err(ShellEnvironmentError::Spawn(_))
                if configured_shell.is_none() && cached_shell.is_some() =>
            {
                let timeout = remaining_until(deadline)?;
                shell::load_environment_from_shell_with_fallback(None, timeout, variables).await?
            }
            Err(error) => return Err(error),
        };
        if let Some(selected_shell) = selected_shell {
            self.remember_detected_shell(selected_shell)?;
        }
        Ok(loaded)
    }

    async fn load_complete_environment_until(
        &self,
        deadline: Instant,
    ) -> Result<(HashMap<String, EnvironmentValue>, Option<PathBuf>), ShellEnvironmentError> {
        let configured_shell = self.shell_path.clone();
        let cached_shell = self.detected_shell_path()?;
        let shell_path = configured_shell.clone().or(cached_shell.clone());
        if tokio::runtime::Handle::try_current().is_err() {
            let (loaded, selected_shell) = match self
                .load_complete_environment_on_runtime_thread(deadline, shell_path)
                .await
            {
                Ok(result) => result,
                // An automatically detected shell may be uninstalled or removed
                // from PATH while the application is running; retry only the
                // candidate chain. An explicitly configured path must follow
                // the user's choice strictly.
                Err(ShellEnvironmentError::Spawn(_))
                    if configured_shell.is_none() && cached_shell.is_some() =>
                {
                    self.load_complete_environment_on_runtime_thread(deadline, None)
                        .await?
                }
                Err(error) => return Err(error),
            };
            return Ok((loaded, selected_shell));
        }

        let timeout = remaining_until(deadline)?;
        let (loaded, selected_shell) =
            match shell::load_complete_environment_from_shell_with_fallback(
                shell_path.as_deref(),
                timeout,
            )
            .await
            {
                Ok(result) => result,
                Err(ShellEnvironmentError::Spawn(_))
                    if configured_shell.is_none() && cached_shell.is_some() =>
                {
                    let timeout = remaining_until(deadline)?;
                    shell::load_complete_environment_from_shell_with_fallback(None, timeout).await?
                }
                Err(error) => return Err(error),
            };
        Ok((loaded, selected_shell))
    }

    fn detected_shell_path(&self) -> Result<Option<PathBuf>, ShellEnvironmentError> {
        self.detected_shell
            .read()
            .map(|path| path.clone())
            .map_err(|_| ShellEnvironmentError::ShellExit)
    }

    fn remember_detected_shell(&self, shell_path: PathBuf) -> Result<(), ShellEnvironmentError> {
        *self
            .detected_shell
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)? = Some(shell_path);
        Ok(())
    }

    fn clear_detected_shell(&self) -> Result<(), ShellEnvironmentError> {
        *self
            .detected_shell
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)? = None;
        Ok(())
    }

    async fn load_environment_on_runtime_thread(
        &self,
        variables: &[String],
        deadline: Instant,
        shell_path: Option<PathBuf>,
    ) -> Result<(HashMap<String, EnvironmentValue>, Option<PathBuf>), ShellEnvironmentError> {
        let variables = variables.to_vec();
        let (sender, receiver) = futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name("nyaterm-shell-environment".to_string())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(ShellEnvironmentError::Spawn)
                    .and_then(|runtime| {
                        let timeout = remaining_until(deadline)?;
                        runtime.block_on(shell::load_environment_from_shell_with_fallback(
                            shell_path.as_deref(),
                            timeout,
                            &variables,
                        ))
                    });
                let _ = sender.send(result);
            })
            .map_err(ShellEnvironmentError::Spawn)?;
        receiver
            .await
            .map_err(|_| ShellEnvironmentError::ShellExit)?
    }

    async fn load_complete_environment_on_runtime_thread(
        &self,
        deadline: Instant,
        shell_path: Option<PathBuf>,
    ) -> Result<(HashMap<String, EnvironmentValue>, Option<PathBuf>), ShellEnvironmentError> {
        let (sender, receiver) = futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name("nyaterm-shell-environment".to_string())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(ShellEnvironmentError::Spawn)
                    .and_then(|runtime| {
                        let timeout = remaining_until(deadline)?;
                        runtime.block_on(shell::load_complete_environment_from_shell_with_fallback(
                            shell_path.as_deref(),
                            timeout,
                        ))
                    });
                let _ = sender.send(result);
            })
            .map_err(ShellEnvironmentError::Spawn)?;
        receiver
            .await
            .map_err(|_| ShellEnvironmentError::ShellExit)?
    }

    fn store_values(
        &self,
        values_to_store: impl IntoIterator<Item = (String, EnvironmentValue)>,
    ) -> Result<(), ShellEnvironmentError> {
        let values_to_store: HashMap<_, _> = values_to_store
            .into_iter()
            .map(|(variable, value)| (environment_cache_key(&variable), value))
            .collect();
        let mut state = self
            .values
            .write()
            .map_err(|_| ShellEnvironmentError::ShellExit)?;
        ensure_cached_values_fit(&state, &values_to_store)?;
        for (variable, value) in values_to_store {
            let value_bytes = value.as_str().len();
            state.missing.remove(&variable);
            state.auto_refreshes.remove(&variable);
            state.auto_refresh_pending.remove(&variable);
            let previous = Arc::make_mut(&mut state.values).insert(variable, value);
            if let Some(previous) = previous {
                state.cached_value_bytes = state
                    .cached_value_bytes
                    .saturating_sub(previous.as_str().len());
            }
            state.cached_value_bytes = state.cached_value_bytes.saturating_add(value_bytes);
        }
        Ok(())
    }

    pub(crate) fn is_missing_cached(&self, variable: &str) -> Result<bool, ShellEnvironmentError> {
        let variable = normalize_environment_variable_name(variable)?;
        Ok(self.cache_entry_normalized(&variable)?.missing)
    }
}

fn normalize_variable_names(variables: &[String]) -> Result<Vec<String>, ShellEnvironmentError> {
    if variables.len() > MAX_VARIABLES_PER_BATCH {
        return Err(ShellEnvironmentError::RequestTooLarge);
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(variables.len());
    let mut estimated_script_bytes = 0usize;
    for variable in variables {
        let variable = normalize_environment_variable_name(variable)?;
        if seen.insert(environment_cache_key(&variable)) {
            // Names are repeated in protocol markers, query commands, and end
            // frames. Estimate four times the name length plus conservative
            // fixed overhead instead of underestimating Windows command-line or
            // POSIX script size.
            let entry_bytes = variable.len().saturating_mul(4).saturating_add(256);
            estimated_script_bytes = estimated_script_bytes.saturating_add(entry_bytes);
            if estimated_script_bytes > MAX_BATCH_SCRIPT_BYTES {
                return Err(ShellEnvironmentError::RequestTooLarge);
            }
            normalized.push(variable);
        }
    }
    Ok(normalized)
}

/// Bound the negative-cache set. It only avoids repeated queries, so clearing old
/// entries at the limit is safer than retaining arbitrary caller input forever
/// in a long-running desktop process; the next read verifies the value again.
fn insert_bounded_negative(set: &mut HashSet<String>, value: String) {
    if !set.contains(&value) && set.len() >= MAX_NEGATIVE_CACHE_ENTRIES {
        set.clear();
    }
    set.insert(value);
}

fn ensure_environment_map_fit(
    values: &HashMap<String, EnvironmentValue>,
) -> Result<(), ShellEnvironmentError> {
    let value_bytes = values.values().try_fold(0usize, |total, value| {
        total.checked_add(value.as_str().len())
    });
    if values.len() > MAX_CACHED_ENVIRONMENT_VARIABLES
        || value_bytes.is_none_or(|bytes| bytes > MAX_CACHED_ENVIRONMENT_VALUE_BYTES)
    {
        return Err(ShellEnvironmentError::CacheLimitExceeded);
    }
    Ok(())
}

fn ensure_cached_values_fit(
    state: &CacheState,
    incoming: &HashMap<String, EnvironmentValue>,
) -> Result<(), ShellEnvironmentError> {
    let mut variable_count = state.values.len();
    let mut value_bytes = state.cached_value_bytes;
    for (variable, value) in incoming {
        if let Some(previous) = state.values.get(variable) {
            value_bytes = value_bytes.saturating_sub(previous.as_str().len());
        } else {
            variable_count = variable_count.saturating_add(1);
        }
        value_bytes = value_bytes.saturating_add(value.as_str().len());
        if variable_count > MAX_CACHED_ENVIRONMENT_VARIABLES
            || value_bytes > MAX_CACHED_ENVIRONMENT_VALUE_BYTES
        {
            return Err(ShellEnvironmentError::CacheLimitExceeded);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn environment_cache_key(variable: &str) -> String {
    variable.to_ascii_uppercase()
}

#[cfg(not(windows))]
fn environment_cache_key(variable: &str) -> String {
    variable.to_string()
}

fn canonicalize_environment_values(
    values: HashMap<String, EnvironmentValue>,
) -> HashMap<String, EnvironmentValue> {
    values
        .into_iter()
        .filter(|(variable, _)| {
            is_snapshot_environment_variable_name(variable)
                && !variable.eq_ignore_ascii_case(SHELL_ENV_READER_VARIABLE)
        })
        .map(|(variable, value)| (environment_cache_key(&variable), value))
        .collect()
}

fn inherited_environment_snapshot() -> HashMap<String, EnvironmentValue> {
    std::env::vars_os()
        .filter_map(|(variable, value)| {
            let variable = variable.into_string().ok()?;
            let value = value.into_string().ok()?;
            (is_snapshot_environment_variable_name(&variable)
                && !variable.eq_ignore_ascii_case(SHELL_ENV_READER_VARIABLE))
            .then_some(())?;
            Some((variable, EnvironmentValue::new(value)))
        })
        .collect()
}

/// Normalize and validate a shell environment variable name.
///
/// Accept only names made of ASCII letters, digits, and underscores, while
/// allowing callers to use the common `$NAME` form. The returned value is safe
/// to embed in platform query scripts; the length limit only bounds runtime
/// input size.
pub fn normalize_environment_variable_name(
    variable: &str,
) -> Result<String, ShellEnvironmentError> {
    let variable = variable.trim();
    let variable = variable.strip_prefix('$').unwrap_or(variable);
    if !is_canonical_environment_variable_name(variable) {
        return Err(ShellEnvironmentError::InvalidVariableName);
    }
    Ok(variable.to_string())
}

pub(super) fn is_canonical_environment_variable_name(variable: &str) -> bool {
    if variable.len() > MAX_ENVIRONMENT_VARIABLE_NAME_LENGTH {
        return false;
    }
    let mut chars = variable.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return false;
    }
    true
}

/// Validate a variable name in a complete snapshot.
///
/// Complete snapshots do not re-embed names in shell scripts, so the targeted
/// query's ASCII-identifier restriction does not apply. Names such as
/// `ProgramFiles(x86)` common on Windows must be preserved; reject only `=`,
/// NUL, and control characters that would break an environment block or the
/// NUL-delimited protocol, and bound the size of each name.
pub(super) fn is_snapshot_environment_variable_name(variable: &str) -> bool {
    if variable.is_empty() || variable.len() > MAX_ENVIRONMENT_VARIABLE_NAME_LENGTH {
        return false;
    }
    !variable
        .chars()
        .any(|character| character == '=' || character == '\0' || character.is_control())
}

/// Validate a variable name in the legacy line-oriented snapshot frame.
///
/// The legacy protocol surrounds names with colons and cannot represent names
/// containing a colon. It accepts only identifier-shaped names so ordinary
/// shell noise is not mistaken for a corrupt Base64 frame. The new complete
/// protocol uses a NUL-delimited blob and [`is_snapshot_environment_variable_name`],
/// so it has no such compatibility restriction.
fn is_legacy_complete_frame_variable_name(variable: &str) -> bool {
    if variable.is_empty() || variable.len() > MAX_ENVIRONMENT_VARIABLE_NAME_LENGTH {
        return false;
    }
    let mut characters = variable.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn inherited_environment_value(variable: &str) -> Option<EnvironmentValue> {
    if variable.eq_ignore_ascii_case(SHELL_ENV_READER_VARIABLE) {
        return None;
    }
    std::env::var(variable).ok().map(EnvironmentValue::new)
}

fn remaining_until(deadline: Instant) -> Result<Duration, ShellEnvironmentError> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or(ShellEnvironmentError::Timeout)
}
