# Event-driven file-browser CWD synchronization (#933)

The file browser follows changes in the terminal's reported working directory.
It does not poll CWD or periodically refresh directory contents. Manual browsing
leaves Auto Sync enabled and remains in place until a real CWD change or a new
activation boundary.

## Ownership and triggers

`SessionFeatureState::cwds` remains the sole CWD source. Both parsed terminal OSC
effects and `SessionEvent::CwdChanged` enter `apply_session_cwd`. Repeated reports
of the same directory do not navigate; local Windows comparisons also accept
case and separator differences.

Automatic navigation runs when CWD changes in the visible active session, when
Auto Sync is enabled, when the browser opens, or when another session activates.
Hidden/background sessions only update their CWD. Snapshot construction and
ordinary paints issue no requests. A window-bounds event reconciles drawer
visibility when the layout crosses a compact breakpoint.

Explicit Sync uses the same cached CWD without requiring a change. A loaded or
already requested target is reused. Clicking Sync on an automatic request
promotes it to explicit so turning Auto Sync off will not cancel the user's
request. Unknown CWD produces a localized status for explicit Sync and otherwise
waits; a valid listing is preserved. SSH tracking mode Off remains unsupported;
ShellIntegration and RcFile both consume the session report. RC installation and
injection fallback are unchanged; no separate SSH Exec `pwd` probe is used.

## Navigation and asynchronous results

CWD synchronization shares ordinary navigation's directory service, stable
rollback snapshot, loading state, history, selection cleanup, listing cache and
tree updates. `SyncCwd { reason }` is an internal job and stays outside the user
transfer queue. List and tree modes expose the same controls; successful CWD
navigation reveals and focuses the target in the tree.

Each session's `navigation_jobs` entry owns its latest directory request. Both
success and failure check ownership before changing browser or tree state.
Starting a newer navigation retires the older request and inherits its last
stable snapshot. This permits independent sessions and fast consecutive `cd`
changes without a global synchronization lock. Failures restore the stable
directory and selection. Inactive replies update their own cache while retaining
the active browser's state and scroll handles.

Turning Auto Sync off, hiding the browser, or switching sessions retires only
automatic navigation and restores its stable snapshot. Explicit Sync and normal
navigation retain ownership. Reconnect clears the old CWD and navigation
ownership, retains stable browser/tree caches, and retires tree requests from
the old connection. The existing reconnect command is built before clearing the
old CWD.

## Editing

Beginning path editing retires an automatic request. While editing, a boolean
intent records that a follow is pending; the target is always read from session
CWD rather than copied into another mutable directory cache. Cancel applies the
latest intent. Submitting a valid manual path discards it. Disabling Auto Sync,
closing the panel, or switching sessions clears the intent; closing the panel
also dismisses path editing.

## Validation and compatibility

Regression tests capture directory requests after navigation bookkeeping and
inject typed completion events, without SSH or wall-clock sleeps. They cover
request counts, virtual time, stale successes/failures, rapid CWD changes,
rollback, activation, background sessions, editing, reconnects, unknown CWD,
RcFile, parsed OSC, tree reveal, compact drawers and Windows path comparison.

All added state is runtime-only. Existing settings, connection-level switches,
database and backup formats are unchanged. Tauri reference sources, transfer
protocols and third-party dependencies are untouched.
