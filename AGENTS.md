# Repository Guidelines

## Project Overview

ZzClawTerm is a native GPUI desktop application written in Rust. It provides SSH,
local shell, Telnet, Serial, RDP, VNC, SFTP, tunnels, OTP, AI assistance, and
encrypted sync and backup in one workspace.

This repository is a Rust 2024 Cargo workspace using resolver `3`. Preserve
compatibility with existing ZzClawTerm configuration, credentials, backups,
cloud-sync data, known hosts, and sessions. Any incompatible data-format change
must include explicit conversion logic and tests.

## Workspace Structure

* `crates/zzclawterm-app`: executable entry point, bundled assets, logging setup,
  and root-window creation.
* `crates/zzclawterm-core`: UI-independent domain models, parsing, policies, AI
  settings/risk/provider logic, schema-neutral serialization and encryption
  contracts, and shared pure logic. Do not add GPUI dependencies here.
* `crates/zzclawterm-desktop`: GPUI application composition, `AppShell`,
  `ZzClawTermApp`, feature state, views, platform adapters, background-job
  coordination, native HTTP adapters, and GPUI Entity stores.
* `crates/zzclawterm-terminal`: terminal state machine, snapshots,
  control-sequence handling, encoding, and graphics protocols. It must remain
  independent of UI frameworks.
* `crates/zzclawterm-terminal-gpui`: GPUI-specific terminal layout, input,
  highlighting, images, and painting.
* `crates/zzclawterm-transport`: local PTY, SSH, Telnet, Serial, SFTP, tunnels,
  remote operations, and transfer-protocol runtime. It must remain independent
  of GPUI and desktop presentation types.
* `crates/zzclawterm-ui`: shared GPUI theme tokens, the `gpui-component`
  integration boundary, and reusable ZzClawTerm presentation and interaction
  widgets.
* `crates/zzclawterm-store`: persistence implementation, transactions, redb
  schema, encryption adapters, and database compatibility readers. Keep pure
  data models and serialization policies in `zzclawterm-core`.
* `crates/zzclawterm-remote-desktop`: UI-independent RDP/VNC session management,
  framebuffer and input models, certificate policy, clipboard state, and the
  RDP/VNC helper IPC contracts. It owns no protocol decoder; both live in the
  helper crates below.
* `crates/zzclawterm-rdp-helper`: isolated IronRDP helper process that communicates
  with the application through the typed IPC protocol in
  `zzclawterm-remote-desktop`.
* `crates/zzclawterm-vnc-helper`: isolated VNC helper process using the same IPC
  protocol. It owns the forked `vnc-rs` decoders, the VNC reconnect ladder, and
  the server-facing policy gates (`view_only`, `shared`, clipboard enablement).
  Those gates must stay enforced here, not only in the application.
* `crates/zzclawterm-otp`: bundled HOTP/TOTP implementation.
* `crates/zzclawterm-app/assets`: bundled icons and images. Assets under
  `icons/**` are normally tintable and rendered through `svg()` or
  `mono_icon()`; full-color assets use `img()` or `color_icon()`. Preserve the
  rendering distinction when adding assets.
* Third-party dependencies that ZzClawTerm patches are not vendored. Each is a
  patch series on a fork under <https://github.com/jackfahdin> on branch
  `nyaterm` (forked from the original series under
  <https://github.com/nyakang>), mirrored to
  <https://gitcode.com/JackfahdinImport> for faster access in mainland China
  and consumed from there at a revision pinned in the root `Cargo.toml`:
  `alacritty` (`alacritty_terminal`), `gpui-component`, `IronRDP`
  (`ironrdp-client`, `ironrdp-connector`), `russh`, `russh-sftp`, `sspi-rs`,
  `vnc-rs`, `zed` (`gpui`, `gpui_platform`), and `zmodem2`. Each branch carries
  a `NYATERM.md` recording its base revision, its patches, and how they were
  validated.
* `temp/vendor/`: untracked, read-only copies of those sources, kept only so
  they can be read locally. Nothing under `temp/` is compiled. **Editing
  anything there has no effect on the build and produces no error** — change
  the fork branch instead.

## Application Architecture

`ZzClawTermApp` is the central GPUI composition owner. Feature-specific state
lives in focused structs for connections, commands, remote operations, remote
desktop, security, settings, AI, terminal presentation, sessions, transfers,
sync, translation, updates, tunnels, recording, and shell behavior.

Persisted collections and compatibility-sensitive catalogs belong to their
feature owners. Schema-neutral persistence formats and parsing contracts belong
in `zzclawterm-core`; database execution and compatibility readers belong in
`zzclawterm-store`.

The GPUI Entity stores each own state that `ZzClawTermApp` does not own:

* `StartupRestoreStore`: startup-restore queue.
* `OverlayStore`: authoritative quick-switch overlay state.

For new or substantially changed features:

* Prefer a focused feature-state struct or a deliberately authoritative GPUI
  Entity over new top-level `ZzClawTermApp` fields.
* Keep exactly one authoritative mutable owner for each piece of state. Do not
  mirror independently mutable state between `ZzClawTermApp`, feature state, and
  Entity stores.
* If a method only reads and writes one feature-state struct, place that logic
  on the state and keep the `ZzClawTermApp` method as a notifier or adapter when
  needed.
* Keep GPUI element construction in views. Pure state types should not render
  UI.
* Use typed events to communicate from background jobs to GPUI state.
* Never perform filesystem, database, network, SSH, SFTP, subprocess, image
  decoding, or other blocking work in a render path or a long-running GPUI
  update callback.
* Keep terminal parsing, snapshots, graphics protocols, and wire handling in
  `zzclawterm-terminal`.
* Keep terminal layout, GPUI input adapters, highlighting, image presentation,
  and painting in `zzclawterm-terminal-gpui`.
* Keep transport code independent of GPUI and desktop presentation types.
* Keep remote-desktop protocol and session logic in
  `zzclawterm-remote-desktop`; keep GPUI presentation in `zzclawterm-desktop`.
* Keep protocol decoders that parse server-controlled bytes in the helper
  crates, never in a crate the application links. Both helpers must translate a
  decoder panic into a fatal IPC error rather than dying silently.
* Keep persistence logic out of GPUI views.

Use normal Rust module trees with `mod.rs` files or sibling module
declarations. Do not add `#[path = "..."]` declarations or `use super::*`
imports. Name dependencies explicitly, including in tests.

Avoid broad exports from crate roots and shared feature preludes. Import
models, services, GPUI types, and helpers from their authoritative modules.

## UI and Input Rules

Ordinary forms, prompts, searches, menus, selects, switches, and dialogs should
use the wrappers exposed by `zzclawterm-ui`. That crate owns the
`gpui-component` integration, theme mapping, and stable ZzClawTerm component API;
desktop feature modules must not depend directly on `gpui-component`.

Ordinary text inputs should use `zzclawterm-ui::ZzClawInput` and `ZzClawInputState`,
either owned by a focused feature or through the id-keyed registry in
`features/text_inputs.rs`. Do not add hand-painted ordinary text inputs.

Terminal input, paste review, and `RemoteTextEditor` are full editing surfaces
with dedicated selection, undo, IME, and command handling. Do not replace them
with single-line registry fields.

Text-field wrappers must have a definite height and width. Parent click
handlers must not immediately steal focus from a field. Keep controls usable
with keyboard navigation and ensure overlays and child windows restore focus
predictably.

GPUI's `overflow_y_scroll()` enables scrolling but paints nothing, and `scrollbar_width` only reserves gutter space, so use `ZzClawScrollable` (the `ScrollableElement` trait re-exported by `zzclawterm-ui`):

* In-flow containers: use `overflow_*_scrollbar()` instead of
  `overflow_*_scroll()`, and do not also reserve a `scrollbar_width` gutter -
  the bar overlays the content.
* Virtualized lists (`uniform_list`) and any caller-owned scroll handle: keep
  `track_scroll` on the list and attach `.vertical_scrollbar(&handle)` to the
  non-scrolling parent. An absolutely positioned overlay inside a scrolling
  element is translated by the scroll offset and would scroll away.
* Absolutely positioned popups must not use `overflow_*_scrollbar()`. The
  wrapper inherits only size and flex styles, so `position`/`top`/`left` would
  move onto the inner content node and break the popup's placement. Give the
  popup a non-scrolling shell and scroll an in-flow child instead.
* A `Scrollbar` inside a caller-positioned overlay needs
  `viewport_from_layout()`; otherwise it paints at the scroll handle's own
  bounds and ignores the overlay.

Scrollbars auto-hide (`ScrollbarMode::Hover`), installed once in
`theme_bridge.rs`. A bar fades in while the pointer is anywhere over its scroll
viewport or that axis is scrolling, and out after idle, so a horizontal bar is
discoverable without pinning it open. Two further consequences:

* A hand-positioned `Scrollbar` overlay must span the whole scroll viewport,
  not just the strip the track occupies. Reveal-on-hover watches the bar's own
  hitbox, so a strip-sized overlay only reacts within a track width of the edge.
  `overflow_*_scrollbar()` and `vertical_scrollbar(&handle)` already do this.
* Where the two axes ride different scroll handles they are separate
  `Scrollbar` elements with separate reveal state, and the upstream
  component's corner-avoidance does not apply, so inset one overlay by a track
  width.

Anything that writes `Theme::scrollbar_mode` or the `scrollbar*` colors directly
must call `Theme::sync_base(cx)` afterwards. `Scrollbar` reads the
`gpui_base::Theme` projection, which those assignments do not touch.

The terminal scrollback scrollbar is hand-rolled as a reserved flex column with
its own drag and overview ruler, and is deliberately exempt from all of the
above.

## Persistence and Compatibility

Changes involving redb, credentials, known hosts, OTP, cloud sync, portable
snapshots, `.zz` backups, AI or translation secrets, and application settings
are compatibility-sensitive.

Before changing these areas:

* Preserve table names, keys, serialized field names, document keys,
  encryption prefixes, master-key wrapping, backup formats, and fallback
  decryption behavior unless the change explicitly updates the data contract.
* Test both new-data round trips and loading representative data written by
  supported ZzClawTerm versions.
* Do not silently discard unknown or unsupported fields.
* Validate fully before overwriting existing user data.
* Keep secret-bearing values masked when returning settings to the UI.

`zzclawterm-store/src/storage/mod.rs` owns the database implementation, with
domain-specific modules under `zzclawterm-store/src/storage/`. Treat existing
redb data, `.zz` backups, master-key wrapping, encrypted payload formats, and
text-document fallbacks as public compatibility contracts.

## Security Rules

Never log or commit:

* passwords or decrypted saved credentials;
* private-key contents or passphrases;
* OTP secrets or generated codes;
* AI, cloud-sync, OAuth, snippet, or translation API secrets;
* unredacted diagnostics containing terminal output, command context, or user
  data.

Use custom redacted `Debug` implementations for secret-bearing structs when
debug output is required. Prefer typed secret wrappers and zeroize sensitive
buffers where practical.

Patching a third-party dependency means committing to its fork branch, pushing,
and bumping the pinned revision in the root `Cargo.toml`. Do not edit `temp/`.
Record the reason for the change and the validation performed on the patch
commit and in that branch's `NYATERM.md`, and keep the patch series split by
concern rather than squashed. Prefer rebasing the series onto a newer upstream
revision over accumulating snapshots.

## Build and Development Commands

Use package-specific checks while iterating:

* `cargo check -p zzclawterm-app`
* `cargo test -p <crate-name>`
* `cargo run -p zzclawterm-app --bin zzclawterm`

RDP and VNC each run in a helper process that the application resolves beside its
own executable. `cargo run -p zzclawterm-app --bin zzclawterm` builds only the
application, so build the helpers into the same target directory first or both
protocols fail with `HelperMissing`:

* `cargo build -p zzclawterm-rdp-helper -p zzclawterm-vnc-helper`

A bare `cargo build` or `cargo check` covers all three: they are the workspace
`default-members`. `ZZCLAWTERM_RDP_HELPER` and `ZZCLAWTERM_VNC_HELPER` override the
lookup with an explicit path. `scripts/release/package_native.py` is what puts the
helpers next to the application in release packages; its `HELPER_BINS` list must
name every helper. Windows packages may additionally bundle the VcXsrv X server
(GPLv3, resolved from `ZZCLAWTERM_VCXSRV_DIST` or `vendor/vcxsrv/`, never
committed) as a `vcxsrv/` directory beside the executable, with a `NOTICE.txt`
recording its license and source.

Before review, run the relevant broader checks:

* `cargo check --workspace`
* `cargo test --workspace`
* `cargo fmt --all -- --check`
* `cargo clippy --workspace --all-targets`

Use `cargo fmt --all` only when intentionally applying formatting changes.

Platform-specific GPUI, PTY, Serial, SSH, clipboard, window, path, RDP/VNC,
and icon-rendering behavior must be verified on the affected operating system.

## Coding Style

Use standard `rustfmt` formatting and Rust 2024 idioms.

* Use `snake_case` for modules, functions, fields, and variables.
* Use `PascalCase` for structs, enums, and traits.
* Use `SCREAMING_SNAKE_CASE` for constants.
* Prefer explicit imports and narrow public interfaces.
* Prefer typed models and errors over loosely structured strings.
* Use small adapters at crate boundaries instead of importing
  application-wide models into low-level crates.
* Comments should explain invariants, compatibility constraints, ownership, or
  non-obvious performance decisions rather than restating code.

When splitting a large file, preserve its public facade where practical so
structural changes do not cause unrelated call-site churn. Prefer domain cuts
that move constants, records, helpers, tests, and dependencies together over
type-only splits that leave coupling behind.

## Testing Guidelines

Add tests beside the behavior being changed.

* Terminal parsing, snapshots, graphics, selection, input, and rendering tests
  belong in `zzclawterm-terminal` or `zzclawterm-terminal-gpui`.
* SSH, SFTP, Telnet, Serial, tunnel, transfer, and terminal-session lifecycle
  tests belong in `zzclawterm-transport`.
* RDP/VNC protocol, framebuffer, input mapping, IPC, certificate, clipboard,
  and reconnect tests belong in `zzclawterm-remote-desktop`, `zzclawterm-rdp-helper`,
  or `zzclawterm-vnc-helper`. Helper crates carry a `tests/lifecycle.rs` covering
  the handshake, an ordinary disconnect, and crash/hang reaping; keep both in
  step when the IPC contract changes.
* Storage changes require round-trip and supported-format compatibility tests.
* Credential and encryption changes require success, invalid-password,
  corrupted-data, and compatibility-format tests.
* GPUI state changes should test state transitions separately from visual
  rendering where possible.

Use descriptive, behavior-oriented test names.

## Commits and Pull Requests

Use Conventional Commit-style subjects:

`type(scope): imperative summary`

Common scopes include `terminal`, `transport`, `desktop`, `storage`, `ui`,
`remote-desktop`, `ai`, and `sync`.

Pull requests should include:

* a concise description of behavior and architectural impact;
* linked issues where applicable;
* commands and platforms tested;
* explicit notes for persistence, credentials, data compatibility, or forked
  dependency changes, including the fork branch and revision a bump moves to.
