# Update signing keys and rotation

In-app updates only accept packages that verify against a fixed public key, so the **signing key is the root of trust for the release chain**: whoever holds the private key can sign packages that every installed copy treats as an official update. This page records where that key lives, which files reference it, and what a rotation has to touch.

## What the key protects

The update path works like this:

1. the release workflow signs every updatable artifact with the private key;
2. the signature lands in the `signature` field of `latest.json`;
3. an installed application downloads the artifact and verifies the signature against the public key **compiled into the binary**, and installs only what passes.

The application trusts exactly this one key. When the public key is stale or wrong, the update check still finds the new version but the install fails with `unsupported update signature` (minisign's `UnexpectedKeyId`). The project hit this once early on: the CI side was rotated but the constant in the application source was not.

## The current key

- minisign key id: `CA3A638733F9C706`
- generated with: `npx @tauri-apps/cli signer generate`
- the private key exists only in GitHub Actions secrets; no private key material may appear in the repository, the docs, or logs

## Where the values live

| Value | Location | Notes |
| --- | --- | --- |
| private key | GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY_B64` | the only source the release workflow signs with |
| private key password | GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | used together with the key |
| public key | see "Which files reference the public key" | the public key is not a secret and ships with the repository |

The private key and its password **must** be kept in a second place outside the repository (a password manager or an encrypted backup). If the GitHub secrets are lost with no local copy, no future update can be signed that an installed copy would accept.

## Which files reference the public key

A rotation has to change all of these together:

- `TAURI_UPDATER_PUBLIC_KEY_B64` in `.github/workflows/release.yml`: the release workflow verifies with it
- the same constant in `.github/workflows/continuous-build.yml`: the daily snapshot's preview channel uses the same key
- `PUBLIC_KEY` in `crates/zzclawterm-desktop/src/features/update/download.rs`: compiled into the application

The first two are literals in the workflows; the third is the same public key wrapped in base64. The unit test `embedded_signing_key_matches_the_publishing_workflows` scans every `TAURI_UPDATER_PUBLIC_KEY_B64` in `.github/workflows/*.yml` and compares it byte for byte with the application constant, so forgetting the application side fails during `cargo test`.

## Rotation steps

1. Generate a new key: `npx @tauri-apps/cli signer generate -w <path outside the repository> -p '<password>'`;
2. store the new private key and password in the two GitHub secrets listed above;
3. update the three places listed under "Which files reference the public key";
4. check locally: `cargo test -p zzclawterm-desktop --lib update::download`;
5. push a tag to run a real release: the workflow verifies the fresh signatures with the new public key through `minisign -Vm` and fails before uploading if the pair does not match;
6. schedule the transition release described in the next section.

## The cost of rotating: a transition release

Because the public key is baked into the binary, **packages signed with a new key are rejected by every older version**. The safe order is:

1. publish a transition version that trusts both the old and the new public key;
2. sign with the new key only after that version has spread;
3. drop the old public key in a later version.

Rotating before the transition version is out leaves installed users with a manual reinstall as their only path.

## If the private key is lost

A lost key and password cannot be recovered by re-signing, so the rotation above becomes mandatory: publish a version that trusts the new public key, have users install it manually once, and then return to the normal release cadence.

## Where this is enforced

- the key-consistency test in `cargo test` keeps the application constant and the workflows from drifting apart;
- the release workflow runs `minisign -Vm` with the public key right after signing, so a mismatched pair fails before upload;
- a stable release compares every remote artifact's size and sha256 before publishing (`scripts/ci/verify_remote_assets.py`).
