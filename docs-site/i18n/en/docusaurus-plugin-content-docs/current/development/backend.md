# Runtime, Transport, and Storage Development

NyaTerm has no separate Web backend. Domain logic, protocol runtimes, and persistence are split across Rust crates and communicate with the GPUI desktop layer through typed interfaces.

## Crate boundaries

| Crate | Behavior it owns |
|------|------------------|
| `nyaterm-core` | Pure models, parsing, policies, compatibility formats, schema-neutral serialization |
| `nyaterm-transport` | PTY, SSH, SFTP, Telnet, Serial, tunnels, remote operations, and transfer runtimes |
| `nyaterm-store` | redb, transactions, storage workers, encryption adapters, compatibility readers |
| `nyaterm-terminal` | Terminal state, snapshots, control sequences, encodings, and graphics protocols |
| `nyaterm-remote-desktop` | RDP/VNC session management, framebuffer and input models, certificate policy, clipboard state, and the helper IPC contracts |
| `nyaterm-rdp-helper` / `nyaterm-vnc-helper` | Isolated helper processes owning their protocol decoders |
| `nyaterm-otp` | HOTP/TOTP compatibility implementation |

Low-level crates do not import GPUI or desktop presentation types. Use a small typed adapter at a boundary instead of pushing application-wide models into transport or storage code.

Protocol decoders that parse server-controlled bytes live only in the helper crates. `nyaterm-remote-desktop` owns no decoder, and both helpers must translate a decoder panic into a fatal IPC error. See [Architecture → RDP / VNC process isolation](./architecture#rdp--vnc-process-isolation).

## Session and transport runtime

The `SessionManager` in `nyaterm-transport` manages active sessions and provides a common lifecycle entry point for local PTY, SSH, Telnet, raw TCP, Serial, RDP, and VNC sessions.

Session output returns through `SessionEvent` variants:

- `Output` / `OutputDropped`
- `CwdChanged`
- `CommandAccepted`
- `Exited`
- `Error`

The event queue coalesces and bounds high-throughput output. The desktop drains events in batches instead of allowing background threads to mutate GPUI state. New session events use typed enum variants and include tests for ordering, queue limits, and close behavior.

SSH, SFTP, tunnel, remote-process, and Docker operations also remain asynchronous or run on dedicated workers. Transport code does not depend on windows, dialogs, or desktop feature types.

## Terminal runtime

Transport wire bytes enter `nyaterm-terminal`. That crate maintains the grid, scrollback, modes, OSC marks, search results, and graphics-protocol state, then produces UI-neutral snapshots.

Terminal-semantic keyboard and mouse protocol encoding stays at the terminal/core boundary. Pixel sizing, GPUI key events, and painting stay in `nyaterm-terminal-gpui`.

## StoreRuntime and persistence

`nyaterm_store::StoreRuntime` owns the database worker. The desktop submits typed `StoreRequest` implementations through `StoreUiClient` or `StoreBlockingClient`, then updates UI state from the task result.

Database implementation is grouped under `nyaterm-store/src/storage/`. Storage changes preserve:

- Existing redb table names, keys, and document keys
- Serialized field names and unknown-field handling
- Master-key wrapping, encryption prefixes, and fallback decryption
- `.nya` backups, portable snapshots, and Dragonfly compatibility readers
- Existing user data until replacement data has passed validation

Pure compatibility models and policies belong in `nyaterm-core`; database transactions, file I/O, and compatibility readers belong in `nyaterm-store`.

## AI, sync, and native HTTP

AI providers, translation, cloud sync, and update checks use native HTTP adapters. Request construction, risk policy, and schema-neutral provider settings stay in `nyaterm-core` where practical. The desktop owns secret masking, interaction state, and task coordination.

Cloud sync treats portable snapshots as a compatibility contract. Pulled data is decrypted, parsed, and validated before it is applied; conflicts, retries, and errors return to the UI as typed results.

## Errors and security

Low-level libraries return typed errors, and desktop adapters decide how to present them. Errors, logs, and `Debug` output must never include passwords, private keys, OTP values, API secrets, or unredacted terminal context.

Background tasks handle cancellation, window closure, and runtime shutdown. On exit, `AppShell` requests a store flush and shutdown so database writes do not outlive the UI lifecycle.

## Tests

- Pure parsing, policy, and compatibility-format tests live in `nyaterm-core`.
- PTY, SSH, SFTP, Telnet, Serial, tunnel, and event-queue tests live in `nyaterm-transport`.
- Terminal parsing, snapshots, graphics, and encoding tests live in `nyaterm-terminal`.
- Storage changes include new-data round trips, representative legacy data, invalid-password cases, and corrupted data.
- RDP/VNC protocol, framebuffer, input mapping, IPC, certificate, clipboard, and reconnect tests belong in `nyaterm-remote-desktop`, `nyaterm-rdp-helper`, or `nyaterm-vnc-helper`. Both helper crates' `tests/lifecycle.rs` cover the handshake, an ordinary disconnect, and crash/hang reaping; keep them in step when the IPC contract changes.
- Cross-crate behavior is tested through small adapters without real credentials or production services.
