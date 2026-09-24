#!/usr/bin/env python3
"""Refresh the publication date of an existing rolling GitHub prerelease."""

import argparse
import json
import subprocess
import time
from datetime import datetime, timezone


def _publication_time(release: dict) -> datetime:
    value = release.get("published_at")
    if not isinstance(value, str):
        raise RuntimeError("snapshot release has no publication time")
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def _asset_names(release: dict) -> set[str]:
    assets = release.get("assets")
    if not isinstance(assets, list):
        raise RuntimeError("snapshot release has no asset list")
    return {asset["name"] for asset in assets}


def refresh_snapshot_release(get_release, edit_release, *, now=None, sleep=time.sleep) -> str:
    before = get_release()
    if before.get("prerelease") is not True:
        raise RuntimeError("rolling snapshot must be a prerelease")
    previous_time = None if before.get("draft") else _publication_time(before)
    previous_assets = _asset_names(before)
    started_at = (now or datetime.now(timezone.utc)).replace(microsecond=0)

    try:
        if not before.get("draft"):
            edit_release(True)
        edit_release(False)
    except Exception:
        try:
            edit_release(False)
        except Exception as restore_error:
            raise RuntimeError("failed to restore the snapshot release from draft") from restore_error
        raise

    for attempt in range(5):
        after = get_release()
        if (
            after.get("id") == before.get("id")
            and after.get("draft") is False
            and after.get("prerelease") is True
            and _asset_names(after) == previous_assets
            and (published_at := _publication_time(after)) >= started_at
            and (previous_time is None or published_at > previous_time)
        ):
            return after["published_at"]
        if attempt < 4:
            sleep(2)
    raise RuntimeError("snapshot publication time or release state did not refresh")


def _gh(*args: str) -> str:
    return subprocess.run(
        ["gh", *args], check=True, text=True, capture_output=True
    ).stdout


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args()

    def get_release() -> dict:
        return json.loads(_gh("api", f"repos/{args.repo}/releases/tags/{args.tag}"))

    def edit_release(draft: bool) -> None:
        flags = ["--draft"] if draft else ["--draft=false", "--prerelease"]
        _gh("release", "edit", args.tag, "--repo", args.repo, *flags)

    published_at = refresh_snapshot_release(get_release, edit_release)
    print(f"Snapshot release republished at {published_at}")


if __name__ == "__main__":
    main()
