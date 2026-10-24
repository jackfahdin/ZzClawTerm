# Session recording

Recording settings are snapshotted when recording starts. Changes affect the next
recording; the history text budget can be changed while sessions are running.
Existing path templates, extensions and settings keys retain their meanings.
`recording.include_input` is an additive setting and defaults to `false` when
loading older documents or backups. Unknown settings fields remain intact.

## Desktop controls

The recording panel and recording shortcut start directly with the effective
recording profile: configured directory, template, default mode and connection
overrides. An empty directory uses Downloads with the existing fallback. The
Transcript/Raw menu actions override only the requested mode. Saving session text
exports to the configured directory with a unique `session-...log` name.
These actions do not open a path picker or insert application notices into the
terminal; completion and errors are reported in the status bar.

Recording rows reserve separate lines for the session, mode/size, path and error.
Text stays on one line with ellipsis and a full-value tooltip. A fixed action area
contains start/stop and a menu for mode selection, opening/revealing the current
file and text export. Row height stays fixed across recording/error transitions.

## Capture

By default only session output is recorded. Transcript removes terminal control
strings with a streaming parser; local application messages use SYSTEM records.
Raw records bytes delivered by the session before charset decoding and before AI
output processing. These bytes exclude SSH/Telnet network protocol framing.
Neither local messages nor metadata headers/footers enter Raw files.

When input is enabled, Transcript includes logical INPUT records. Raw writes the
encoded session input, including bracketed paste framing, to
`<output filename>.input.bin`. Output and input rotate together. Recognized
password/OTP prompts are tracked independently for every session, including sync
peers. Dedicated sensitive-input adapters and terminal protocol replies are
always excluded. Unknown hidden-input prompts cannot reliably be detected;
this limitation is shown beside the input switch.

Automatic terminal execution records the original command as INPUT, subject to
the same input switch and sensitive-prompt checks. Internal shell wrappers and
capture markers are excluded from Transcript. Captured output is recorded
incrementally, independently of the bounded MCP result and AI card line limit;
SendOnly output continues through the ordinary recording path once. Cancellation
and timeout retain partial output, and late output keeps recording with internal
markers removed. Background SSH/local execution records its submitted command
and returned stdout/stderr without inserting the AI presentation into the log.
Raw still records only session wire bytes, including shell wrappers, and excludes
background execution that does not pass through that session. Prompt detection
does not redact arbitrary secrets embedded in commands or command output.

The binary-transfer switch selects either session bytes before transfer filters
or terminal bytes after the existing X/Y/ZMODEM and trzsz filters. The Raw capture
marker on terminal submissions prevents duplicate capture after decoding.

## Files, pressure and shutdown

Duplicate manual starts fail before opening a destination; automatic starts are
idempotent. Active output/input destinations are protected by file identity,
including hard-link aliases and Windows case/path variants. Rotation protects
previous segments and preserves the original session start time. Explicitly
selected paths remain the base for subsequent segments; append mode counts
existing output and input bytes toward the rotation threshold.

Exports use a temporary file in the destination directory, flush/sync it, and
replace the destination only on success. Automatic exports reserve unique names
atomically. A companion-file open failure cannot truncate existing output.

Per-session records, unfinished text, echo matching and reconnect bytes share the
configured text budget. UTF-8 fragments are at most 64 KiB; disk output retains
all accepted content while history reports truncation. Incremental newline and
ANSI parsing avoid repeated scans and front removal for every line. Timestamps
are generated once per incoming batch.

The writer queue accepts at most 4 MiB and 4096 payload commands. Queue and drop
counts are tracked per session. Overflow separates partial records, resets
parsing continuity and reports degraded capture. Status counts update at most
once per 250 ms; state/path changes notify immediately. Pending status and search
replies are coalesced. Buffered files flush every second.

Stop/export jobs wait for already submitted terminal output in the background.
Normal shutdown waits for the output bridge, terminal producer and recording
writer in that order. Stop returns final output/input paths together with any
write or flush error. Partial lines and transcript footers are finalized.

## Regression coverage

Tests use synthetic data and local TCP fixtures. Transport regression tests live
in `crates/zzclawterm-transport/src/recording/audit_tests.rs`; desktop pipeline tests
are beside `models/recording.rs`. App tests cover automatic batch starts,
connection overrides, reconnects, UI Raw capture, and session-local sensitive
keyboard/paste/raw input. Store tests cover legacy settings, unknown fields,
Sync/Backup snapshots and `.nya` restore.

Run the throughput benchmark with:

```text
cargo test -p zzclawterm-transport recording_stream_processing --lib -- --ignored --nocapture
```

On Windows in an unoptimized build, processing 1 MiB took approximately 19 ms
without newlines and 185 ms for dense short lines, with retained text bounded to
4 KiB. Times are indicative, not an assertion on other hardware.

## Validation status

The Windows run passed `cargo test --workspace`, `cargo check --workspace`,
`cargo clippy --workspace --all-targets`, and `cargo fmt --all -- --check`.
Cargo used `--target-dir target/recording-check` to isolate these checks from
concurrent builds. Check and Clippy report an unrelated unused
`RemoteOpsFeatureState::apply_docker_overview` method.

Failure coverage uses synthetic write/flush failures and blocked destinations;
it does not fill a real disk. A fresh portable application build was launched
for GUI verification, but the Windows automation native pipe became unavailable
and stayed unavailable after retry/reset. Manual verification of automatic
recording, opening the current log, stopping, reconnecting, and normal GUI exit
remains pending; automated lifecycle and routing tests cover those code paths.
