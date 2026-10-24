# Cloud sync protocol and compatibility

Cloud sync transfers complete portable configuration snapshots. It excludes
device-local presentation preferences and command history. It does not perform
entity merges or provide a general backup history browser.

## Encryption

New encrypted snapshots have this binary envelope (integers are little-endian):

| Offset | Contents |
| --- | --- |
| 0 | Eight-byte magic `NYASNAP\0` |
| 8 | Envelope version `2` |
| 9 | KDF identifier `1` (Argon2id, algorithm version 0x13) |
| 10 | Memory cost, u32: 19456 KiB |
| 14 | Time cost, u32: 2 |
| 18 | Parallelism, u32: 1 |
| 22 | Random 16-byte salt |
| 38 | Random 12-byte AES-GCM nonce |
| 50 | AES-256-GCM ciphertext followed by its 16-byte tag |

The entire 50-byte header is authenticated as AAD. Readers validate the version
and the supported cost profile before allocating KDF memory. They do not accept
arbitrary remote-controlled memory or iteration costs. Derived key buffers are
zeroized. Password trimming remains compatible with historical behavior.

Readers retain both historical `nonce || ciphertext` key domains, NyaTerm and
Dragonfly. Old bytes are never rewritten just by reading. An actual upload or
explicit forced push writes the new envelope; unmodified-content checks do not
silently re-encrypt existing data. Unsupported future envelopes are reported as
unsupported, including through the decoder helper, and do not enable forced
overwrite as a corruption-recovery shortcut.

All devices sharing a namespace must be upgraded before publishing new-format
snapshots. Legacy readers cannot decrypt them. This is a read-compatible
migration, not a promise of old-client write interoperability. Keep an independent
local `.nya` backup when upgrading.

## Data and change detection

The redb snapshot schema remains version 3, and historical tables, document
keys, payload hashes, ZIP containers, and optional entity integrity hashes stay
readable. The codec uses a bounded in-memory redb backend instead of temporary
plaintext files. File-backed redb interoperability is tested in both directions.

`last_synced_content_hash` is a new optional local state field. Its
`entities-v1:` hash covers the complete sorted entity map, including VNC trust
records and unknown entities. `last_synced_payload_hash` retains its old meaning
for compatibility; it is no longer sufficient evidence for change detection.
An absent new baseline is treated conservatively: equal complete snapshots
establish the baseline, while differing snapshots cannot silently auto-pull over
data omitted by an old hash.

## Publication and concurrency

The remote layout is:

* `sync/snapshots/<revision>.redb.enc`: immutable encrypted data.
* `sync/heads/<sha256(device_id)>.json`: independently published device commit.
* `sync/heads/<sha256(device_id)>.pending.json`: protected staged revision.
* `sync/latest.redb`: best-effort compatibility pointer.
* `sync/current.redb.enc`: best-effort compatibility/recovery copy.

Each device has one persisted, unique device ID and one serialized sync writer.
Copying a live database to another device must not be used as a way of cloning
that writer identity; use portable import instead. The protocol assumes atomic
replacement of individual objects and eventually visible reads/listings, not an
atomic transaction across objects. Google Drive duplicate path names fail
closed rather than selecting an arbitrary file or folder.

Snapshot metadata and its pointer carry an optional `sync_clock` with per-device
monotonic counters. A clock supersedes another only if every counter is at least
as large and one is larger. Legacy pointers are linked through `legacy_revision`.
The clock is inside the authenticated snapshot and is checked against the head
when that snapshot is decoded. Wall-clock time is not a conflict winner.

A push registers its staged revision, uploads and verifies the immutable bytes,
then publishes its own device head. Cancellation is allowed before publication;
once publication begins the job finishes and reports the outcome. Failure to
refresh the shared compatibility objects does not undo an independently
published commit. Staging does not overwrite the previously committed head.

Two devices can both report successful publication. Their independent heads
retain both branches even if the shared compatibility pointer is overwritten.
The next check that can observe both branches reports a conflict. This is eventual
conflict detection, not linearizable compare-and-swap. Tests force both writers
past the historical check-before-write window before permitting publication.
With delayed listings, a conflict may appear on a later check.

An explicit local/cloud choice publishes a new causal commit covering the
observed branches. Cloud choice uses the branch read when the operation starts.
Unobserved later writes remain concurrent and produce a new conflict. Explicit
recovery also creates a new revision rather than rewinding another device's
head. Non-forced checks and pulls reject causal ancestors of the locally synced
baseline so a stale remote response cannot roll the device back.
They also reject a remote branch concurrent with that baseline even when a
delayed listing omits this device's own committed head. Explicit cloud choice
then publishes a new resolving revision using both causal histories.

## Local apply and efficiency

An automatic check carries its exact local snapshot, decoded remote snapshot,
and checked baseline into the subsequent push/pull. It never rebuilds a baseline
after network I/O and mistakes intervening saves for synchronized content.

The production store checks the expected complete content hash, writes a safety
backup, applies the incoming snapshot transactionally, rebuilds the normalized
baseline, and saves it in one serialized store job. No application save can run
between these steps. A crash between the data transaction and baseline write
leaves a conservative old baseline, causing revalidation/conflict rather than
marking unrelated edits synchronized. Even forced pulls reject local changes
made after their initial snapshot was captured.

Without retries or maintenance, a checked automatic push downloads the previous
snapshot once and the uploaded snapshot once for readback validation. It reuses
the uploaded ciphertext for the compatibility copy. Metadata calls and provider
path lookup calls are additional. Pulls reuse the checked remote snapshot and do
not upload a compatibility copy as a side effect. The transfer-count regression
test also confirms that a later local edit remains dirty.

Maintenance authenticates committed heads once per pass, skips protected snapshots
in the candidate scan, retains five recent eligible historical snapshots and a
24-hour grace period, and rechecks heads before deletion. A versioned candidate
must also be a strict causal ancestor of an individually authenticated commit;
absence from a delayed listing never proves that a branch is disposable.
Capacity-limited snippet cleanup may collect additional
verified old history, but never active, staged, foreign, or unreadable objects.
Device commit and staging files count toward provider file limits; if protected
data fills a snippet, synchronization fails with a capacity error instead of
discarding a branch. Existing `backups/` data is outside this cleanup policy.

## Validation

Regression coverage includes legacy envelope migration, wrong passwords,
tampered headers/ciphertext, bounded KDF parameters, complete-entity change
detection, saves during downloads and before apply, independently published
concurrent heads, explicit resolution in either direction, stale remote views,
staged/committed snapshot GC protection, compatibility-write failures, cancellation,
readback delays, and redb file/memory codec compatibility. Native provider behavior
still depends on each service honoring successful object writes and eventually
consistent listing semantics; no real cloud credentials are used by unit tests.
