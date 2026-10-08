# Shell Free Type hardening

The session-owned prediction, snapshot reconciliation, and Ready/Pending/Uncertain
state machine remain authoritative. This change tightens their consumers without
changing settings, saved sessions, encryption, or persistence formats.

## Editing authorization

All cursor, selection, delete, replacement, paste, and IME editing paths use
`EditCapability::allows_editing`. MarkedShell uses the explicit OSC 133 boundary.
EchoMatched additionally requires a Local/SSH session and a recognizable shell
prompt, a previously confirmed line-editor prompt, or a nonempty prompt belonging
to a positively identified child line editor. Matching the typed text and cursor
alone never authorizes editing.

Command navigation's interactive-program detector also includes pagers and TUIs,
so Free Type uses the narrower `command_starts_line_editor` policy. It excludes
cat/tee, pagers, full-screen editors, script/command execution, and compound
commands. Docker/Podman interactive shell launches are parsed through the actual
child command; an argument named `sh` to `cat` is not a shell launch. Arbitrary
SSH commands need prompt evidence instead of blanket authorization.

Prompt evidence is still heuristic without OSC 133. A process deliberately
imitating a shell prompt cannot be distinguished reliably from a shell by cells
alone. Unknown prompts and unsupported launch options fail conservatively.
Right prompts, decorations, complex graphemes, alternate screens, native mouse
reporting, and broadcast-input restrictions retain their existing behavior.

## Input retention and confidence

Session deactivation retains only Ready input authorized for editing. Pending,
Uncertain, and noneditable echo are zeroized and reset. Retained input becomes
Uncertain and needs reconciliation before reuse by command suggestions. Explicit
session invalidation, tracker resets, buffer replacement, and editor destruction
also zeroize the owned input string. TerminalInputState Debug masks its text.

`predicted_input` is for submission tracking and redacted diagnostic metadata.
`assist_input` allows ordinary typing predictions and confirmed input, but returns
an unavailable state after confidence is lost. Suggestion scheduling, manual
search, publication, and application all use this confidence-aware accessor.
Confirmation timeout also dismisses the active session's suggestions.

## Keyboard encoding

Synthetic Left/Right and Backspace use the same full-mode press/release encoder
as physical keyboard input. The logical recording/tracking payload retains
navigation intents and DEL; the wire payload follows DECCKM and Kitty flags.
Bracketed-paste framing and outgoing text encoding apply only to inserted text.
The existing physical encoder currently keeps plain arrows in CSI/SS3 form under
Kitty; Backspace can use CSI-u and event-type reporting. Sharing that encoder
prevents the two paths from diverging when its protocol support changes.

## Regression coverage

Tests cover foreground echo refusing move/delete/replace/paste, Docker custom
prompts, line-editor launch classification, pending/uncertain deactivation,
confirmed-input retention, suggestion confidence, redacted Debug, and exact
Kitty Backspace press/release bytes. Existing fresh-snapshot, write-failure,
selection, paste-encoding, and IME tests remain in place.

## Validation

Validated on Windows on 2026-10-04:

- `cargo check --workspace --locked`
- `cargo clippy --workspace --all-targets --locked`
- `cargo fmt --all -- --check`
- `cargo test --workspace --locked -- --test-threads=1`
- The ignored PowerShell/ConPTY Free Type test, executed from the compiled
  `shell_free_type_windows` test binary.

Cargo build checks used `--target-dir D:\CodexBuilds\zzclawterm-free-type-20261004`
after the shared E: drive ran out of space. The shared target directory was not
cleaned. The complete serial workspace run passed; parallel runs encountered
intermittent failures in the existing ConPTY close timeout and AI proxy test's
nonblocking socket read. The ConPTY close test also passed in isolation.
