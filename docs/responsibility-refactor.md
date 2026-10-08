# Desktop responsibility refactor

## Boundaries

The aim is to reduce mutable ownership and business-rule coupling. File size is
only a discovery aid. A module split alone does not establish a new boundary.

| Area | Authoritative owner | Adapter responsibility |
| --- | --- | --- |
| Process and windows | `DesktopController` | Compose stores and windows, route activation, tray actions and tab moves, coordinate shutdown. |
| Automatic sync | `AutoSyncCoordinator` | Read store generation and window safety, submit the existing background job, distribute its typed result. |
| Remote monitoring | Docker, process, statistics and accelerator pane states | `RemoteOpsFeatureState` preserves the existing facade and coordinates session changes. Pane fields and jobs remain private. |
| AI | AI feature state for conversations and refresh scheduling; `AiPanel` for presentation caches and scroll state | `panel/actions.rs` adapts application operations; `panel/snapshot.rs` builds presentation snapshots; panel views render them. |
| Remote desktop | `RemoteDesktopFeatureState` and its session states; protocol managers in `zzclawterm-remote-desktop` | Lifecycle, event, trust, input, clipboard and framebuffer modules integrate GPUI and the existing managers. Decoders remain in helpers. |

Automatic-sync scheduling, debounce, focus throttling, retry and pause rules now
live on the coordinator. Its fields are private to its module, so the controller
cannot modify them directly. Starting a job captures the mutation generation.
Completion also observes the store's authoritative generation, because the
mutation notification can arrive after the completion notification. A successful
old job therefore cannot clear a newer local change. Idle and paused ticks do not
inspect windows; the safety callback uses static dispatch and runs only when a
job can start.

Remote-desktop resize normalization and filtering live on session state. The
feature state owns the debounce loop and manager dispatch. The application keeps
the existing entry points and converts viewport layout into display metrics.
The 150 ms debounce, 32-pixel threshold, three-second failure window, size bounds,
and handling of unsuccessful resize dispatches are preserved. Tests for these
rules move with them.

These changes introduce no locks, channels, dynamic dispatch, independently
mutable mirrors, framebuffer copies, or serialized-data changes. Existing sync
and helper scheduling mechanisms are retained.

## Large components retained after inspection

- `DesktopController` remains the process composition root. Window ownership,
  activation, tab handoff and shutdown require cross-window coordination. Domain
  rules such as automatic-sync scheduling must remain with their state owner.
- Remote-desktop event processing still adapts protocol events, application
  session metadata, GPUI textures and notifications. Moving those methods alone
  into another object would not isolate the protocol state. Resize rules are an
  identifiable state-only boundary and have been moved separately.
- `zzclawterm-transport/src/sftp/mod.rs` remains a cohesive SFTP service and transfer
  engine. Browsing, editor saves and transfers share authenticated sessions,
  compatibility caching, retries, path handling and progress contracts. Existing
  path, metadata, deletion and contract modules already isolate distinct rules.
  This pass does not add another mutable session/cache owner merely to shorten
  the service. A future change to transfer scheduling should reassess that seam.
- `zzclawterm-terminal/src/lib.rs` owns a terminal state machine: parsing, grid
  epochs, graphics ingress, search and snapshots must agree on the same grid and
  line identities. Graphics, editing, navigation, encoding and cell helpers are
  already separate. A second mutable grid or extra locking would weaken this
  boundary. Long terminal test collections are not composition components.
- AI visual tests are intentionally kept together where they share a GPUI host
  and test presentation interactions. Their length does not imply mutable
  business ownership.

## Validation

On Windows, with Rust 1.99.0, the final checks completed on 2026-10-05:

| Command | Result |
| --- | --- |
| `cargo check --workspace` | Passed. |
| `cargo test --workspace` | Passed: 3,401 tests, zero failures, 14 ignored; desktop: 1,778 passed and seven ignored. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --workspace --all-targets` | Passed without warnings in the recorded run. |

Regression coverage includes monitoring cache identity/reuse, session reset and
stale job filtering; AI streaming coalescing, sibling paint counts and scroll
anchors; tray routing after tab moves; window/workspace close and restore
policies; resize filtering, modifier/key mapping, reconnect classification,
certificate/clipboard state, and both helpers' handshake/disconnect/crash/hang
lifecycle tests. New coordinator tests exercise changes during a running job,
completion before mutation notification, deferred-pull safety, periodic versus
change precedence, and paused/idle ticks avoiding window inspection.

The first desktop rerun failed with six missing `recording_include_input` field
errors, although the field was present in the source. Other worktrees were
building against the shared target directory. The final workspace check rebuilt
the local core and its dependants and the full tests then passed; no recording
source change was needed to resolve those errors. The initial refactor was also
committed by concurrent work in `cb2544fe4`, so the remaining diff represents the
ownership fixes documented above, not the entire earlier split. Menu and SSH
editor changes from concurrent work are outside this refactor.

Detailed command output is retained in the ignored `temp/refactor-final-*.log`
files. Successful compilation alone is not evidence of unchanged latency or
memory use.

## Synthetic performance comparison

The existing ignored AI benchmark uses 2,000 messages and 1,000 streaming deltas,
once with immediate snapshot refreshes and once with coalesced refreshes. Both
variants used the same Windows machine (Ryzen 7 5700G), Rust 1.99.0, development
profile and fixture. Each binary ran three times, alternating order between
rounds. Timings below are the benchmark's internal workload timings, excluding
compilation and fixture setup. Peak working set covers the whole test process,
including both scheduling variants and setup; it is sampled from Windows'
`PeakWorkingSetSize` counter, not an application allocation measurement.

| Metric | Before split | Current |
| --- | --- | --- |
| Immediate refresh snapshots / parses, every run | 1,000 / 1,000 | 1,000 / 1,000 |
| Coalesced refresh snapshots / parses, every run | 1 / 1 | 1 / 1 |
| Immediate workload median | 11.318 s | 11.459 s |
| Immediate workload range | 10.886–14.936 s | 11.345–13.299 s |
| Coalesced workload median | 29.65 ms | 28.87 ms |
| Coalesced workload range | 28.24–37.92 ms | 28.64–34.50 ms |
| Whole-process peak working set median | 61.82 MiB | 61.70 MiB |

The refresh and parse counts agree in all runs. Timing ranges overlap; three
development-profile samples do not establish a production performance guarantee.
The ordinary GPUI tests separately assert sibling paint isolation, content cache
reuse and scroll-anchor behavior. This experiment does not independently measure
cache hit rate, remote monitoring cost, RDP/VNC frame latency or end-to-end input
latency.

The before-split source tree is the detached worktree
`E:/Projects/Rust/zzclawterm-refactor-baseline`, based on `cb2544fe4`, with only
`controller.rs`, AI `panel.rs`, remote `state.rs` and remote-desktop `runtime.rs`
restored from `cb2544fe4^`. Recording changes from the same commit are retained;
parallel menu and SSH editor edits were copied to that tree to hold unrelated
source changes constant. The existing benchmark itself was unchanged.

The copied test executables, six run logs, Windows memory sampler and JSON data
are retained under the ignored `temp/refactor-ai-*` and
`temp/refactor-profile.py` paths. Re-run the comparison without compilation using
`python temp/refactor-profile.py`. The JSON records the SHA-256 hashes of both
executables. The individual Cargo benchmark command is:

```powershell
cargo test -p zzclawterm-desktop compare_immediate_and_coalesced_stream_updates_for_two_thousand_messages -- --ignored --nocapture
```

Live SSH monitoring, remote-desktop input/clipboard/certificate flows and
end-to-end latency require a dedicated test environment. Synthetic GPUI and
helper lifecycle tests cover specific invariants, not every real server or
desktop integration. The settings test that is explicitly ignored because its
fixture does not park must not be reported as passing.
