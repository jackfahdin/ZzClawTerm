# Plugin system V1 design

ZzClawTerm implements this system independently under Apache-2.0. Zed's extension
manifest, proxy, builder, guest API/WIT, store, Wasm host, capability checker,
headless host and lifecycle tests were inspected as architectural references;
their GPL implementation is not copied or linked.

## Boundaries and ownership

* `zzclawterm-core::plugins` owns strict manifest/preferences contracts, identifiers,
  compatibility, parameters, bounded template expansion and result validation.
* `zzclawterm-plugin-api` owns the single versioned WIT and Rust guest registration.
* `zzclawterm-plugin-host` owns managed snapshots, installation transactions and
  the authoritative registry. Each component has one worker, Store and serial
  bounded queue. Different components run independently.
* `zzclawterm-store::plugin_preferences` persists an isolated versioned document
  under AppRuntime's plugin directory. Existing redb, sync and backup contracts
  are untouched. Corrupt preferences fail closed and are never overwritten.
* DesktopController owns one process service. Each workspace's side-panel Entity owns input entities,
  action selection and previews; immutable snapshots are presentation only.
  The activity-bar extension entry opens the manager/action surface, at the bottom
  left by default. All host work runs off UI.

## Interface and execution

Schema 1, semantic plugin versions and API 1.0.0 are distinct. WIT exports
version, initialize, invoke and shutdown; arguments/results/errors are typed.
Guest binaries carry a custom API section. Both the section and actual typed
component exports are checked. V1 exposes no host imports, WASI environment,
filesystem, network, process, clipboard, credentials or terminal access.
Unsupported capability declarations are rejected.

Wasmtime 39.0.1 uses synchronous component calls on dedicated workers. Fuel is
a finite CPU budget; an epoch ticker interrupts deadlines and cancellation with
a trap (never async yield). Store limits cap guest memory and instance counts.
All inputs/results/queues/packages are bounded. Cancellation invalidates a
generation before stopping its worker. Old results cannot revive contributions.
Trap/timeout/invalid results retire that instance. Shutdown joins workers off UI.

The service serializes submission, dispatch start and shutdown through a short
lock never held during filesystem work, compilation or joining. Invoke requests
include the selected revision, checked before guest dispatch. Abandoned queued
calls are skipped; dropping a dispatched call cancels its ticket. Management
transactions run to completion or roll back even when their receiver closes.
Shutdown rejects requests not yet dispatched and finishes a started transaction.
Startup-error shutdown also preserves its diagnostic, publishes stopped state
and closes events. Concurrent shutdown callers all await resource reclamation.

Compilation stays in-process on the management thread, bounded by package size.
Fuel/epoch deadlines cover guest execution, not compilation or filesystem I/O;
orderly shutdown may wait for these stages and has no hard compilation timeout.

## Installation and persistence

Directories and ZIP packages are copied into staging, rejecting links, reparse
points, traversal, Windows device/ADS paths, duplicates and package limits.
Validation, component compilation, instantiation and initialization happen on
the snapshot before promotion. Updates/reloads prepare the replacement before
switching; failure leaves the old instance and contributions intact. File bytes
are read before compilation, so component execution holds no package file
handles. Directory rename rollback and preference transactions protect Windows
replacement. Development sources also become validated snapshots.

Layout: `data_dir/plugins/{installed,staging,work/data,dev}` plus isolated
preferences. The installed tree is the rebuildable catalog; preferences carry
enabled state and development source, not contributions. Uninstall retains
plugin data, explicitly described in the UI and user documentation.

## Product behavior

Templates and component actions share namespaced contributions; they never
modify QuickCommandsConfig. Parameter substitution supports only `${name}`
and explicit shell quoting rules, without script evaluation. Actions receive
only user-supplied values/text. Results are ordinary text, with controls rejected.
Command results are editable previews. Filling the send box is explicit and
preserves an existing draft using append or an explicit replacement action.
No plugin operation selects sessions, writes terminal bytes or presses Enter.

Window pending state distinguishes management transactions from invocations.
Only invocations expose cancellation. Catalog invalidation clears retired forms
and previews without cancelling management waiters or losing their feedback.
Management feedback survives catalog notifications delivered after completion
as well as before it, until action selection or new work replaces that feedback.
Workspace shutdown cancels its action waiter; accepted management remains owned
by the process service. Hidden/moved panels preserve their editing state.

## Verification

Core/store contract tests, actual SDK-built components and host lifecycle tests
cover installation, rollback, isolation, failures, cancellation, generations,
preferences and cleanup. Desktop tests verify process sharing and draft-only
adaptation. Scripts build examples/fixtures and validate packages. Windows is
the current platform; other platforms must be listed as unverified until tested.

The isolated Windows x64 source snapshot passed the default build and all four
workspace checks with a fresh target cache: 3280 tests passed with 13 pre-existing
ignored tests. SDK packaging and Windows ARM64 host cross-compilation passed.
At that initial stage, native startup was smoke-tested using isolated portable
data; the unavailable computer-use pipe left visual/keyboard interaction unverified.
ARM64 execution and Linux/macOS behavior are also unverified.

These counts and the pipe failure describe the initial implementation. The
connection was restored during lifecycle closeout and the planned Windows x64
functional walkthrough completed in isolated portable data. Final workspace tests
passed 3305 with 13 existing ignored; build/check/fmt/Clippy and actionlint passed.
Architecture still fails only on the existing transfer path bar, with no allowlist
change. Current results are recorded
in plugin-system-progress.md and plugin-system-acceptance.md. The required SDK CI
job rebuilds guest components, checks guest formatting, runs host tests against
rebuilt fixtures and verifies the SDK package including WIT. Plugin dependency
guards resolve aliases and target-specific dependency tables. No schema, ABI or
persistence contract changed in closeout.
