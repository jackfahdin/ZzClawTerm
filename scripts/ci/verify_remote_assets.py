#!/usr/bin/env python3
"""Confirm a draft release's remote assets match the local artifact directory.

The release job uploads ``dist-release`` before publishing, and an upload can
succeed while carrying the wrong bytes: a truncated transfer, a name that drifted
away from what the manifest expects, or a leftover asset from an earlier attempt
at the same tag. This gate re-reads the release through ``gh`` and compares every
local artifact's size and sha256 against its remote asset, so a bad upload stops
the publish instead of reaching users.

Run from the repository root:
    python3 scripts/ci/verify_remote_assets.py --tag v0.0.3 --directory dist-release
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

HASH_CHUNK_BYTES = 1024 * 1024
DIGEST_PREFIX = "sha256:"


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(HASH_CHUNK_BYTES), b""):
            digest.update(chunk)
    return f"{DIGEST_PREFIX}{digest.hexdigest()}"


def local_assets(directory: Path) -> dict[str, tuple[int, str]]:
    """Map every plain file in ``directory`` to its size and sha256 digest."""
    assets: dict[str, tuple[int, str]] = {}
    for entry in sorted(directory.iterdir()):
        if entry.is_file():
            assets[entry.name] = (entry.stat().st_size, sha256_of(entry))
    return assets


def remote_assets(release: dict[str, object]) -> dict[str, tuple[int, str]]:
    """Map a release API payload's assets to name, size, and digest.

    An asset whose ``digest`` is missing or null maps to an empty digest, so the
    comparison reports it instead of silently skipping the byte check.
    """
    assets: dict[str, tuple[int, str]] = {}
    for asset in release.get("assets") or []:
        if not isinstance(asset, dict):
            continue
        name = str(asset.get("name", ""))
        if not name:
            continue
        size = asset.get("size")
        digest = asset.get("digest")
        assets[name] = (
            int(size) if isinstance(size, int) else -1,
            str(digest) if digest else "",
        )
    return assets


def compare_assets(
    local: dict[str, tuple[int, str]],
    remote: dict[str, tuple[int, str]],
    *,
    prune_unexpected: bool = False,
) -> tuple[list[str], list[str]]:
    """Compare the two asset maps.

    Returns the problems found and, when ``prune_unexpected`` is set, the remote
    asset names that have no local counterpart. Without pruning those names are
    problems too, since an unexpected asset is a failed verification either way.
    """
    problems: list[str] = []
    for name in sorted(local):
        size, digest = local[name]
        if name not in remote:
            problems.append(f"missing remote asset: {name}")
            continue
        remote_size, remote_digest = remote[name]
        if remote_size != size:
            problems.append(
                f"size mismatch: {name} (local {size}, remote {remote_size})"
            )
        if not remote_digest:
            problems.append(f"remote asset has no sha256 digest: {name}")
        elif remote_digest != digest:
            problems.append(
                f"digest mismatch: {name} (local {digest}, remote {remote_digest})"
            )

    unexpected = sorted(name for name in remote if name not in local)
    if prune_unexpected:
        return problems, unexpected
    problems += [f"unexpected remote asset: {name}" for name in unexpected]
    return problems, []


def run_gh(arguments: list[str]) -> tuple[str, str]:
    """Run ``gh`` and return its stdout plus an error message that is empty on success."""
    result = subprocess.run(
        ["gh", *arguments], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        return "", result.stderr.strip() or f"gh exited with {result.returncode}"
    return result.stdout, ""


def parse_object(stdout: str, what: str) -> tuple[dict[str, object] | None, str]:
    """Parse an API payload that has to be a JSON object."""
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError as decode_error:
        return None, f"{what} is not JSON: {decode_error}"
    if not isinstance(payload, dict):
        return None, f"{what} payload is not an object"
    return payload, ""


def find_release(repository: str, tag: str) -> tuple[dict[str, object] | None, str]:
    """Read the release for ``tag``, drafts included.

    GitHub's ``releases/tags`` endpoint only locates published releases, so a
    rerun that resumes a draft has to be found in the authenticated release
    list instead — the same fallback ``gh release view`` performs.
    """
    stdout, tag_error = run_gh(["api", f"repos/{repository}/releases/tags/{tag}"])
    if not tag_error:
        return parse_object(stdout, f"release payload for {tag}")

    page = 1
    while True:
        stdout, error = run_gh(
            ["api", f"repos/{repository}/releases?per_page=100&page={page}"]
        )
        if error:
            return None, f"tag lookup failed ({tag_error}); release list failed ({error})"
        try:
            releases = json.loads(stdout)
        except json.JSONDecodeError as decode_error:
            return None, f"release list is not JSON: {decode_error}"
        if not isinstance(releases, list):
            return None, "release list payload is not an array"
        for release in releases:
            if isinstance(release, dict) and release.get("tag_name") == tag:
                return release, ""
        if len(releases) < 100:
            return None, f"no release found for {tag} (tag lookup said: {tag_error})"
        page += 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Verify a draft release's assets")
    parser.add_argument(
        "--tag",
        required=True,
        help="release tag, for example v0.0.3",
    )
    parser.add_argument(
        "--directory",
        type=Path,
        required=True,
        help="directory holding the artifacts uploaded to the release",
    )
    parser.add_argument(
        "--repository",
        help="owner/repo; defaults to the GITHUB_REPOSITORY environment variable",
    )
    parser.add_argument(
        "--prune-unexpected",
        action="store_true",
        help="delete remote assets that have no local counterpart",
    )
    args = parser.parse_args(argv)

    repository = args.repository or os.environ.get("GITHUB_REPOSITORY", "")
    if not repository:
        print(
            "no repository given and GITHUB_REPOSITORY is unset",
            file=sys.stderr,
        )
        return 1

    directory = args.directory.resolve()
    if not directory.is_dir():
        print(f"release directory not found: {directory}", file=sys.stderr)
        return 1

    release, error = find_release(repository, args.tag)
    if error:
        print(f"cannot read release {args.tag}: {error}", file=sys.stderr)
        return 1
    if not isinstance(release.get("assets"), list):
        print(f"release payload for {args.tag} has no assets array", file=sys.stderr)
        return 1

    local = local_assets(directory)
    if not local:
        # 空目录会让比对"全部通过"（0 个本地文件对 0 个远端资产），而发布流程会
        # 因此公开一个没有任何附件的 release——下游同步才会发现。
        print(f"no artifacts to verify in {directory}", file=sys.stderr)
        return 1
    problems, prunable = compare_assets(
        local, remote_assets(release), prune_unexpected=args.prune_unexpected
    )
    if problems:
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1

    # Pruning runs only once the comparison is clean: deleting first would let a
    # removed asset turn a genuine mismatch into a passing verification.
    for name in prunable:
        _, delete_error = run_gh(
            [
                "release",
                "delete-asset",
                args.tag,
                name,
                "--repo",
                repository,
                "--yes",
            ]
        )
        if delete_error:
            print(f"cannot delete {name}: {delete_error}", file=sys.stderr)
            return 1
        print(f"pruned unexpected remote asset: {name}")

    print(f"verified {len(local)} remote assets for {args.tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
