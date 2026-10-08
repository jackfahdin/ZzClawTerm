# Plugin system V1 acceptance

This records evidence for the attached V1 requirements. Automated GPUI rendering
and state tests are separate from native Windows visual/keyboard acceptance.
The lifecycle closeout below records current Windows x64 functional acceptance.
The repository architecture gate still reports the separately tracked transfer
path-bar violation; it is not hidden or counted as a passing check.
The planned Windows x64 functional walkthrough is complete; other platforms and
the unrelated repository gate are not declared accepted.

## Lifecycle closeout acceptance (2026-10-04)

`python scripts/plugins/verify.py --workspace` preserved every result under
`target/plugin-verification/run-32ce1f26b0754b64bdf730518b4fd92d/`.
Guest formatting, SDK source builds/component generation, SDK package verification,
core plugin tests (4), store preference tests (3), host tests (7 service + 13
lifecycle), desktop plugin tests (5), native default build, workspace check,
default-parallel workspace tests (3305 passed, 13 existing ignored), formatting
and all-targets Clippy (no warnings) passed. Architecture failed only at the
existing `features/pages/transfers/path_bar.rs` raw scroll container; the plugin
crate checks passed. Architecture guard unit tests (3) and actionlint 1.7.7 passed.
There was no default-parallel failure in this run; historical precise reruns and
serial results below remain separate evidence.

Service tests use temporary portable data and real SDK components. Dispatch
barriers cover abandoned queued calls, stale revisions after reload, management
completion after receiver close, shutdown queue draining and transaction commit,
16-slot capacity, startup failure, repeated/concurrent shutdown and Windows
handle cleanup. A real guest epoch checkpoint controls running cancellation.
GPUI tests cover management cancellation guards, programmatic invocation guards,
success/error feedback across contribution invalidation, window cancellation
preventing preview publication, and service restart after aborted update shutdown.
The final feedback regression also delivers catalog notifications before and
after completion explicitly, so neither ordering can overwrite management feedback.
The final-source full rerun, after this additional fix, is preserved in
`target/plugin-verification/run-4731ebbc699a4c3a924b1c7e8d8b3127/`: the same twelve
checks passed (3305 workspace tests, 13 ignored, Clippy without warnings), and the
same existing architecture violation was the sole failure. All 48 Python CI
helper tests also passed, including the three new dependency-guard tests.

Native computer-use recovered after the historical pipe failure below. The
walkthrough used `target/plugin-verification/native-closeout-b840f092/`, its own
portable marker/data, and source-built application/helpers and SDK components.
No real user configuration was opened or modified, and no terminal session was
created or draft sent. Observed Windows behavior:

* Installed the diagnostic command and declarative template ZIPs, and registered
  an isolated copy of the text SDK example as a development directory. All three
  generated their expected results (`nslookup`/`ping`, `ls -lah`, formatted JSON).
* Edited a command preview, explicitly filled the send box, appended to an existing
  draft and replaced it. Generating results left the existing draft unchanged.
  The text result copy control worked and reuse replaced the action's text input;
  text results offered no command-fill controls.
* Invalid command update and invalid development Wasm reload showed failure
  feedback, kept version 1.0.0/actions, and the retained SDK instances still ran.
* Opened another workspace through File / New Window. Disable, enable and uninstall
  in that window removed/restored/removed the command contribution in both windows.
  Closing the second workspace left the first usable.
* A real infinite-loop SDK fixture reached Runtime fault with the CPU-budget
  message, revoked its actions and cleared its form. The text plugin still ran,
  and scrolling, management controls and application close remained responsive.
* Normal close exited the isolated process. Restart restored the enabled template,
  disabled development plugin and command-plugin uninstall. The final normal close
  left zero isolated processes; exclusive opens of both redb databases and retained
  Wasm files succeeded. Precise in-flight close/exit races are covered by barriers
  in automated tests, not inferred from the fast native operations.

In-process compilation and filesystem work have no hard timeout; application exit
may wait for compilation and an already-started management transaction to finish
or roll back. Management has no user cancellation. Queued requests not dispatched
when shutdown begins return `Shutdown`; dropped invocation waiters are skipped.
The WIT, SDK ABI, manifest schema and persistence formats are unchanged.

The final binary, including the feedback-order fix, was also launched against
the same isolated data (SHA256
`0B7B478CEE9788CEAA26C457ACC29F7A6851CADC2C989308B007446F00BB44C6`).
It restored preferences, executed a real SDK text result, copied it and pasted
the exact value back into the action input using Ctrl+V. Disabling that selected
plugin cleared the form/preview while retaining `Plugin operation completed`.
Normal exit again left zero isolated processes and released all database/Wasm
handles. The detailed first walkthrough and final smoke check are separate;
deterministic event-order verification is the GPUI test described above.

## Side-panel change acceptance (2026-10-04)

The plugin side-panel change was exercised on Windows x64 in an isolated portable
instance, using the repository's declarative template example:

* The `MdExtension` entry defaults between Sync / Backup and Settings at the bottom
  left. Opening and closing it keeps the main window alive; the old settings and
  quick-command entry points have been removed.
* Directory-path entry and installation work. The installed enabled plugin survives
  application restart. Inputs support keyboard selection, Tab navigation and Enter
  to generate a preview without sending a command.
* At the minimum 160px sidebar width, text wraps, long buttons stay within the panel
  with ellipsis/full-label tooltips, and the vertical scrollbar stays in the viewport.
* Moving the entry to the right opens the panel on the right. Docked/floating
  switches and panel close/reopen retain parameters and result previews.

These earlier checks cover this UI change. The current broader walkthrough is
recorded above; historical close/reopen observations precede removal of the panel
close buttons, which this closeout deliberately preserves.

Validation also passed `cargo check --workspace`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets` (no warnings), and
`cargo test --workspace -- --test-threads=1` (3297 passed, 13 existing ignored).
An initial default-parallel workspace run passed; a later run hit the previously
recorded `windows_local_session_close_releases_conpty_reader` timeout. Its exact
rerun and the final serial workspace run passed. No transport code was changed.

## Completion criteria

| Requirement | Implementation and evidence | Current Windows x64 native acceptance |
| --- | --- | --- |
| Build application and SDK examples from a clean checkout | Isolated HEAD archive with only plugin changes passed native default build and all workspace checks. SDK guests built independently; examples were componentized, initialized and ZIP packaged. Packaged SDK WIT also compiled. | None for Windows x64 compilation. |
| Start application and open manager | Isolated portable startup created application and plugin databases. GPUI tests cover shared service and editing-state retention. | Passed; opened bottom-left entry, scrolled and widened the panel. Earlier keyboard/layout checks retained above. |
| Install declarative and Wasm examples | Manager lifecycle tests and SDK source build validate all three packages. | Passed; two ZIP installations and text example development registration. |
| Invoke, inspect and explicitly fill draft | Actual SDK components, GPUI preview/draft/revocation tests and composer-only adapter regression. | Passed; all three results, preview edits, append/replace, text copy/reuse, no automatic send. |
| Disable removes actions; enable restores them | Contribution revocation and fresh-instance tests; shared process snapshots. | Passed in both windows. |
| Failed update/reload preserves previous version | Invalid replacement, transaction/preference rollback and Windows occupied-file tests. | Passed; failure feedback and retained SDK calls. |
| Uninstall leaves no contributions | Package/contribution removal, staging cleanup and retained-data tests. | Passed in both windows and after restart. |
| Restart restores preferences | Real redb compatibility tests and service restart regression. | Passed; enabled template and disabled development plugin restored. |
| Fault isolation keeps application/other plugins usable | Real SDK deadlines, fuel, memory, trap, queue and cancellation tests. | Passed; CPU-budget fault, action revocation, healthy text call and normal exit. |
| Management, call and shutdown lifecycle | Seven service regressions and GPUI feedback/cancellation/restart tests described above. | Normal workspace close and application exit passed; deterministic race evidence is automated. |
| Repository checks pass | Current default-parallel workspace tests: 3305 passed / 13 ignored. Build/check/fmt/Clippy/actionlint passed; historical runs retained below/in progress. | Architecture gate remains failed on the pre-existing transfer path bar; no plugin boundary violation. |

## Requirement coverage

* **Contracts and compatibility:** core tests load schema/API V1 fixtures and
  reject unknown fields, unsupported capabilities, incompatible versions,
  duplicate actions and unsafe IDs/paths. Host tests reject duplicate plugin IDs,
  forged ABI markers/interfaces and invalid initialization/input. SDK and host
  generate typed bindings from the same WIT; V1 has no host imports or WASI.
* **Templates and text:** tests exercise bounded substitution and explicit POSIX/
  PowerShell quoting, with no expression execution. Namespaced contributions are
  separate from user commands. SDK guests retain per-instance mutable state;
  separate plugins do not share it. Text input is supplied explicitly and output
  control sequences are rejected before presentation or draft adaptation.
* **Installation safety:** actual directory/ZIP snapshots reject traversal,
  absolute/device/ADS paths, links, duplicate entries and package limits.
  Windows tests cover junctions, file occupation and cleanup of handles. Source
  mutation cannot change an installed snapshot. Development reload uses the same
  validation and preparation transaction as managed packages.
* **Execution and ownership:** bounded guest queues are serial per Store;
  independent guest workers run separately. Infinite execution is actually
  stopped by trapping epoch deadlines/cancellation, with fuel as another budget.
  Tests cover queue-full feedback, cancellation, generation invalidation, reload
  and joined shutdown. DesktopController creates the sole process service;
  closing one workspace does not stop another's guest state. Window entities own
  form/selection/preview state, not a second mutable registry.
* **Data boundaries:** `plugin_preferences_and_packages_stay_out_of_backup_and_cloud_sync`
  uses real Backup/Sync snapshot builders and codecs, `.nya` export/decode and
  native database export/import. Entire exported entity maps are unchanged;
  user QuickCommands restore intact, and the plugin preference table is absent
  from the native backup. Plugin preferences and retained data survive locally.
  Existing credential/encryption/sync/backup formats are not changed.
* **Application integration:** all ordinary fields/buttons/scrolling use
  zzclawterm-ui wrappers; six locale catalogs contain the plugin keys. AppShell
  shutdown joins the process service in background work and supports restart
  after an aborted application update. No guest can send terminal bytes, select
  a session, read terminal output or obtain user secrets.
* **Delivery:** independent Apache-2.0 implementation, shared WIT/Rust SDK,
  three installable examples, checked SDK-built Wasm fixtures, build/package/
  verification scripts, design and user/developer documentation. No third-party
  fork changes, commits, pushes or real user configuration changes were made.

## Historical native verification blocker (resolved in lifecycle closeout)

The supported computer-use connection fails before window discovery with:

```text
Computer Use native pipe is unavailable: failed to connect native pipe:
系统找不到指定的文件。 (os error 2)
```

It has failed in three consecutive continuation turns, including another
`sky.list_windows()` probe on 2026-10-04. No UI input was sent by these probes.
Kernel reinitialization did not restore the connection. Indirect startup and
GPUI tests do not establish that the native walkthrough above passed.

The connection later recovered and the isolated walkthrough above replaced this
blocker as current evidence. macOS/Linux and Windows ARM64 runtime behavior remain
unverified; ARM64 host cross-compilation alone passed. The unrelated architecture
gate failure prevents describing all repository gates as green.
