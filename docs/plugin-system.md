# ZzClawTerm plugins V1

Plugins provide command drafts and transform text explicitly supplied by the
user. They cannot execute commands or access sessions, terminal output, history,
clipboard, credentials, files, network or processes. ZzClawTerm implements this
system independently under Apache-2.0; Zed's GPL extension architecture was a
read-only architectural reference.

## Install and use

Open **Plugins** using the extension icon in the bottom-left activity bar,
between **Sync / Backup** and **Settings** by default. The side panel follows the
workspace's docked/floating mode and can be moved or hidden like other panels.
Click its activity-bar icon again to hide it; the plugin panel has no top-right
close button.
Choose a directory containing `plugin.toml`, or a ZIP with
that file at its root, then select **Install**. Installation copies and validates
a managed snapshot; later edits to the source do not affect it.

The manager shows metadata, API/host requirements, origin (managed/development),
status, contributed actions and actionable errors. **Installed (disabled)** means
no contributions are available. Declarative plugins become **Enabled**; a live
component becomes **Running**. Incompatible packages, loading failures and runtime
faults have distinct states. A runtime fault revokes the instance's actions;
reload it to retry. Failed replacement leaves the old plugin available.

* **Enable/Disable** persists across restarts. Disable cancels calls and removes
  contributions immediately when the operation completes.
* **Update from source path above** validates a package with the same plugin ID,
  prepares its replacement, then switches it. Local updates are explicit; there
  is no automatic networking or marketplace.
* **Reload** recreates a managed instance. For a registered development directory,
  it first takes another validated snapshot of the source.
* **Uninstall (keep data)** removes the managed package, preference and all
  contributions, retaining `work/data/<plugin-id>`. V1 guests have no KV or file
  access, so the SDK itself currently writes no persistent plugin data.

Select an action, fill its typed parameters, and choose **Generate / transform**.
**Cancel** applies only to action calls. Cancelling an executing guest retires its
instance; reload it before invoking it again. An abandoned queued call does not
execute. Installation, update, enable/disable, reload and uninstall run to
completion or roll back and cannot be cancelled from the panel. Hiding the panel
retains its state; closing a workspace cancels its action call while accepted
management continues in the shared process service.
Text actions receive only text entered/pasted into their input box. The user must
explicitly copy terminal selection and paste it there; plugins never subscribe to
selection, terminal output or the clipboard. Results appear as ordinary editable
text and can be copied. Text results can be reused as another text input.

For command results, **Fill send box** appends to an existing draft; **Replace
send-box draft** explicitly replaces it. The send box opens, but neither action
sends bytes, presses Enter, changes targets or chooses sessions. Stop any active
send and select text mode first. Review the final draft and use ZzClawTerm's usual
send controls yourself. Plugin contributions never modify saved QuickCommands.
The text draft retains plugin/action IDs and revisions for its current lifetime.
Appending preserves existing sources; replacing starts a new provenance record.
Editing retains the source with an edited marker, and clearing removes it. The
send-box footer displays plugin sources. This metadata is not persisted.

For a local demonstration from a checkout:

```sh
rustup target add wasm32-unknown-unknown
python scripts/plugins/build.py --fixtures
cargo build --locked
cargo run -p zzclawterm-app --bin zzclawterm
```

The default build includes the MCP, RDP and VNC helpers beside the application.
Open the manager and install `target/plugin-examples/templates.zip`,
`diagnostic-command.zip` and `text-tools.zip`. Generate a directory-listing or
diagnostic draft, edit its preview and explicitly fill the send box. For the text
example, paste lines to sort or JSON to format, then copy/reuse the result.
Disable and re-enable a plugin to check action removal/restoration. To exercise
development reload, register a package directory instead of its ZIP, edit its
manifest/template or rebuild its component, and reload. An invalid replacement
reports an error while preserving the previous version. Uninstall removes its
contributions; enabled preferences survive application restart.

## Directories and data compatibility

The root is `AppRuntime.data_dir()/plugins`, so portable installations use their
local data directory and installed stable/preview builds remain isolated.

| Path | Purpose |
| --- | --- |
| `installed/<plugin-id>/` | Validated managed package snapshot |
| `staging/<transaction-id>/` | Copy/extraction, replacement and rollback trees |
| `work/data/<plugin-id>/` | Retained, isolated data area; not exposed to V1 guests |
| `dev/preferences.redb` | Isolated enabled preferences and canonical development-source registrations |

The installed directory is the catalog. It is scanned at startup; there is no
irreplaceable contribution index. Missing preferences default to disabled.
Corrupt or future preference documents stop plugin startup with a visible error;
they are never replaced with defaults or silently rewritten. Restore/repair the
isolated file before restarting. Plugin packages, preferences and execution state
are excluded from the existing cloud-sync and `.nya` backup contracts. Existing
settings, redb tables, credential formats and user command data are unchanged.

Installation rejects symlinks and Windows reparse points, traversal, absolute
resource paths, ADS/device aliases, archive links and duplicate case-insensitive
paths. Limits are 512 entries, 64 MiB compressed archive, 128 MiB expanded package,
and 32 MiB per file/component. Replacement reads components into memory before
compiling; it holds no installed-file handles while executing. On Windows, a
foreign process blocking directory replacement produces an error and preserves
the old installation. Transaction commit failure restores the old directory.
If even rollback fails due to external filesystem interference, the recovery tree
is retained in staging and the error tells the user to preserve it.

## Manifest schema 1

Plugin semantic version, manifest schema, and Wasm API version are distinct.
The current schema is `1` and the API is exactly `1.0.0`. Host compatibility uses
semver requirements; prerelease builds must explicitly match a prerelease, for
example `>=2.0.0-preview.4`. All unknown fields in schema-1 manifests, nested
actions/parameters and preference documents are rejected. Arrays of authors,
actions and parameters are extensible within their size limits; optional/default
fields below may be omitted. There is no arbitrary extension/config JSON in V1.
Future schemas must be explicitly implemented with compatibility fixtures.

```toml
schema_version = 1
id = "shell-templates"
name = "Shell templates"
version = "1.0.0"
authors = ["Your name"]
description = "Prepare commands for review"
host_version = ">=0.1.0"
api_version = "1.0.0"
# component = "plugin.wasm" # Optional component-model binary
# capabilities = []        # Empty only; every extra capability is rejected

[[actions]]
id = "inspect"
name = "Inspect files"
description = "Draft a POSIX directory listing"
result = "command"        # command or text
template = "templates/inspect.txt" # Optional; otherwise implemented by component
quoting = "posix"         # literal (default), posix, powershell

[[actions.parameters]]
id = "path"
name = "Directory"
kind = "string"           # string, integer (signed 64-bit), boolean (true/false)
required = true           # false by default
default = "."             # Optional string parsed according to declared kind
```

IDs start with a lowercase ASCII letter and contain lowercase letters, digits,
`-` or `_`, up to 96 bytes; Windows reserved names are forbidden. Contributions
are addressed as `plugin-id:action-id`. Duplicate IDs cannot overwrite another
plugin. Metadata name/author strings are limited to 160 bytes; descriptions to
4096 bytes; manifests to 128 KiB, actions to 128 and parameters per action to 32.

Resource paths are relative UTF-8 paths with `/` separators, up to 240 bytes.
Both paths and every package entry follow the cross-platform safety rules.
Template files are UTF-8 and support only `${declared-parameter}` substitution.
Unclosed/undeclared substitutions are rejected. There are no expressions,
conditionals, includes or implicit script evaluation. `posix` single-quotes each
substituted value and escapes single quotes; `powershell` doubles single quotes
inside a single-quoted value. `literal` inserts the value verbatim into the draft.
Quoting applies to parameters, not to the whole resource. Every result remains a
user-reviewed draft, and the quoting mode must match its intended shell.

## Rust SDK and WIT

`zzclawterm-plugin-api` version `1.0.0` has no desktop, core, store, transport or GPUI
dependency. Its published-package layout includes `wit/plugin.wit`; both guest
wit-bindgen and host Wasmtime bindings are generated from this same file. A third
party can use the SDK from a ZzClawTerm Git checkout:

```toml
[package]
name = "my-plugin"
version = "1.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
zzclawterm-plugin-api = { git = "https://github.com/nyakang/zzclawterm" }
```

Pin a reviewed Git revision when distributing a plugin. No SDK publication is
performed by this change. For local development use a path to the SDK crate.

```rust
use zzclawterm_plugin_api::{ActionInput, ActionResult, Plugin, PluginError};

#[derive(Default)]
struct SortLines;
impl Plugin for SortLines {
    fn invoke(&mut self, input: ActionInput) -> Result<ActionResult, PluginError> {
        let text = input.text.unwrap_or_default();
        let mut lines: Vec<_> = text.lines().collect();
        lines.sort();
        Ok(ActionResult::Text(lines.join("\n")))
    }
}
zzclawterm_plugin_api::register_plugin!(SortLines);
```

WIT exports `api-version`, `initialize(identity)`, `invoke(action-input)` and
`shutdown`. Parameters use typed text/integer/boolean variants; results are either
a command record (title, command) or text. Errors distinguish invalid input,
unsupported action and plugin failure. The registration macro retains one mutable
guest for serial calls; reload starts a fresh instance. Guest errors are presented
as bounded generic host messages to avoid leaking user input from guest diagnostics.

The SDK embeds a `zzclawterm:plugin-api` custom section. The host requires exactly
one marker matching `1.0.0`, verifies actual typed component exports using the
generated bindings, and calls the exported version function. A forged manifest
or marker cannot authorize a different interface. Only this ABI is implemented.
The versioned package declaration in `wit/plugin.wit` is the ABI authority. The
SDK build generates the Rust version, host validation constants and marker bytes
from it, including the marker length. The default `guest` feature exposes the
SDK; `default-features = false` exposes only ABI constants for the core contracts.

## Scheduling and shutdown

The process registry blocks on explicit requests and runtime fault notifications.
At most two background preparation threads copy/validate package snapshots,
compile components and initialize guests. The registry serializes promotion and
preference commits, checking the expected revision again before replacing an
entry. Requests targeting the same plugin keep submission order; other plugins
can invoke or prepare while that plugin prepares. New installations retain a
management barrier until the package ID is known. Invocations of existing other
plugins can still proceed during installation.

Idle guest workers block on their mailboxes. The epoch clock parks whenever no
guest calls are active, including initialization and bounded shutdown exports.
Active calls retain fuel, memory and deadline enforcement. Shutdown explicitly
wakes the registry and guest mailboxes, rejects queued requests, completes or
rolls back started transactions, and joins all preparation/runtime workers.

## Build, debug and package

Install the Rust target once, then build examples and validation fixtures:

```sh
rustup target add wasm32-unknown-unknown
python scripts/plugins/build.py --fixtures
python scripts/plugins/verify.py
```

The build script obtains native-tool and guest target directories from Cargo
metadata. It honors `CARGO_TARGET_DIR` and target-directory configuration. For
example, in PowerShell, put build artifacts on a drive with sufficient space:

```powershell
$env:CARGO_TARGET_DIR = 'D:\ZzClawTermBuild'
python scripts/plugins/build.py --fixtures
cargo build --locked
```

Installable examples still appear under the checkout's `target/plugin-examples`.

The guest examples have an isolated workspace at `examples/plugins/guests` with
a checked Cargo.lock. This avoids nested-workspace conflicts and does not link
the desktop into guests. The script builds core modules using the SDK, then uses
`wit-component` to produce real components, validates their ABI/initialization,
and writes installable packages under `target/plugin-examples`:

1. `templates`: declarative `ls` draft with a quoted parameter.
2. `diagnostic-command`: Rust/Wasm hostname diagnostics draft with per-instance
   invocation count; never performs DNS/network/process operations itself.
3. `text-tools`: Rust/Wasm line sorting and JSON pretty-printing.

The SDK-built adversarial fixture is also reproducible. Checked binary fixtures
under `zzclawterm-plugin-host/tests/fixtures` allow host tests from a clean checkout
without installing a guest target; rebuilding uses the same SDK and WIT.

For another guest:

```sh
cargo build --manifest-path /path/to/my-plugin/Cargo.toml --release --target wasm32-unknown-unknown
cargo run -p zzclawterm-plugin-host --bin zzclawterm-plugin-tool -- component /path/to/my_plugin.wasm /path/to/package/plugin.wasm
cargo run -p zzclawterm-plugin-host --bin zzclawterm-plugin-tool -- validate /path/to/package
cargo run -p zzclawterm-plugin-host --bin zzclawterm-plugin-tool -- pack /path/to/package /path/to/plugin.zip
```

Register the package directory for development, edit/build its resources, then
reload explicitly. Validation and initialization errors preserve the previous
instance. The manager shows diagnostics, never raw Wasmtime stacks or user text.

## Isolation and limits

Wasmtime `39.0.1` (MSRV 1.89) runs Component Model guests on dedicated serial
workers. wit-bindgen `0.49.0` has MSRV 1.82; the SDK declares 1.89. The tested
toolchain is Rust 1.97.1 on Windows x64. Each instance owns its Store and mutable
guest; different plugins have separate workers. The process manager accepts at
most 16 queued management operations; each guest accepts 8 queued calls.

Initialization and calls have a 2-second deadline including queued wait, a
20-million fuel budget and epoch checks every 10 ms. Epoch callbacks trap on
deadline/cancellation; they do not yield. Timeout actually stops execution.
Memory is limited to 32 MiB per linear memory, at most 4 memories, 4 tables of
16,384 elements, 8 core instances, and 512 KiB Wasm stack. Inputs and outputs
are limited to 256 KiB; command titles to 160 bytes. Results containing control
characters other than ordinary tabs/newlines are rejected. Guest memory, trap,
budget, deadline and invalid-result failures retire only that instance.

Cancellation, disabling, replacement and uninstall invalidate the old generation
and retire its worker. Dropping a pending invocation cancels it. Shutdown joins
all workers before AppShell quits, off the GPUI thread. Resource ownership is
explicit: snapshots own temporary directories, the registry owns instances,
instances own Stores/queues, and the engine owns its epoch clock. Components
receive no inherited stdin/stdout/stderr or WASI environment. No guest logging
import is exposed. CPU execution can be interrupted; compilation is bounded by
package size and runs on the manager thread, rather than in a render/update path.

Orderly application exit rejects new and queued operations, finishes any started
management transaction, then joins plugin workers and the epoch clock. Execution
timeouts do not cover Wasmtime compilation or filesystem I/O. These remain
in-process background work, so shutdown may wait for them; V1 does not promise a
hard compilation timeout. Selected revisions are checked before guest dispatch,
preventing stale form inputs from reaching replacement instances.

## Verification and current limits

Run the complete repository checks with:

```sh
python scripts/plugins/verify.py --workspace
```

This builds the application and runtime helpers, checks the workspace, runs all
tests, verifies formatting, and runs Clippy. Actual command results and any
baseline failures are maintained in `plugin-system-progress.md`.
The script also checks guest formatting, rebuilds SDK components, verifies the
packaged SDK and checks architecture boundaries. Each invocation owns a fresh
`target/plugin-verification/run-<id>/` directory, preserving previous failures.

Windows x64 verification used a HEAD source snapshot with only plugin changes,
and a fresh build cache: default native build, workspace check, all tests (3280
passed, 13 existing ignored), formatting and Clippy all passed. No new plugin
tests are ignored. The SDK's packaged WIT was independently compiled; the host
also passed ARM64 Windows cross-compilation, without ARM64 runtime execution.

The current workspace, including subsequent terminal changes and the explicit
backup/sync isolation regression, also passed check, formatting and all-targets
Clippy. Serial workspace tests passed 3290 with 13 existing ignored tests.
Default-parallel reruns encountered an unchanged ConPTY close timeout and an
unchanged HTTP test socket timeout; each exact rerun passed. Their failures and
the serial result remain recorded in the progress document. The detailed
[acceptance table](plugin-system-acceptance.md) separates automated evidence from
the current Windows native walkthrough and historical results.

Windows x64 host tests exercise real SDK component initialization/calls, state
isolation, archive/path safety, junction rejection, file occupation, transactional
rollback, enable persistence, cancellation, stale-result rejection, resource
failures and shutdown handle release. GPUI tests cover shared process ownership
and composer-only adaptation. The lifecycle closeout passed SDK source builds,
SDK packaging, plugin regressions and the default-parallel workspace suite
(3305 passed, 13 existing ignored), build/check/fmt/Clippy and workflow syntax.
The architecture gate still reports the existing transfer path-bar violation;
plugin crate boundaries pass and no allowlist was expanded.

The recovered native computer-use connection exercised all three examples in
an isolated Windows x64 portable instance: result editing and explicit draft
append/replace, text copy/reuse, shared-window enable/disable/uninstall, failed
update/development reload, guest fault isolation, restart preferences and normal
exit with released database/Wasm handles. No command was sent and real user data
was untouched. Precise lifecycle races use automated barriers. macOS/Linux and
Windows ARM64 execution are not claimed as tested. The
[acceptance record](plugin-system-acceptance.md) lists the evidence and limits.

V1 has no marketplace, automatic updates, plugin sync/backup, arbitrary UI,
protocol decoders, KV or host capabilities. Add a capability only with a narrow
WIT import, host enforcement of declaration ∩ user grant ∩ supported operation,
revocation cancellation and compatibility tests. Extend schema/API versions
explicitly; never infer ABI compatibility from the manifest alone.
