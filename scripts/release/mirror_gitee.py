"""Mirror GitHub release attachments to Gitee from a domestic network.

Gitee resets large attachment uploads coming from overseas IPs (GitHub-hosted
runners included), so the CI mirror can only carry small files. This script runs
on a machine in China and uploads the same assets with the stall watchdog from
`scripts/ci/gitee_release_upload.py`.

Usage:
    GITEE_TOKEN=... python -m scripts.release.mirror_gitee --tag v0.1.2 --assets-dir path/to/assets
"""

import argparse
import json
import os
import pathlib
import sys
import urllib.parse

import requests

from scripts.ci.gitee_release_upload import (
    attachment_ids_named,
    delete_attachments,
    release_id_of,
    upload_attachment_with_watchdog,
)

API = "https://gitee.com/api/v5"


def make_request(session: requests.Session, token: str):
    def request(method, path, *, expected=(200, 201, 204), allow_404=False, **kwargs):
        params = kwargs.pop("params", {})
        params["access_token"] = token
        response = session.request(method, f"{API}{path}", params=params, timeout=300, **kwargs)
        if allow_404 and response.status_code == 404:
            return None
        if response.status_code not in expected:
            raise RuntimeError(f"Gitee API {method} {path}: HTTP {response.status_code} {response.text[:300]}")
        if method == "DELETE":
            return None
        return response.json() if response.text.strip() else None

    return request


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--assets-dir", required=True, type=pathlib.Path)
    parser.add_argument("--owner", default="jackfahdin")
    parser.add_argument("--repo", default="ZzClawTerm")
    args = parser.parse_args()

    token = os.environ.get("GITEE_TOKEN", "")
    if not token:
        sys.exit("GITEE_TOKEN is required")

    session = requests.Session()
    request = make_request(session, token)
    owner = urllib.parse.quote(args.owner, safe="")
    repo = urllib.parse.quote(args.repo, safe="")
    tag_q = urllib.parse.quote(args.tag, safe="")
    releases_path = f"/repos/{owner}/{repo}/releases"

    release = request("GET", f"{releases_path}/tags/{tag_q}", allow_404=True)
    if release is None:
        sys.exit(f"Gitee release {args.tag} does not exist; run the CI mirror first")
    release_id = release_id_of(release)
    attach_path = f"{releases_path}/{release_id}/attach_files"
    existing = request("GET", attach_path) or []
    by_name = {item["name"]: item for item in existing}

    assets = sorted(args.assets_dir.iterdir(), key=lambda path: path.name.lower())
    if not assets:
        sys.exit(f"no assets under {args.assets_dir}")

    failed = []
    for asset in assets:
        size = asset.stat().st_size
        present = by_name.get(asset.name)
        if present and present.get("size") == size:
            print(f"skip {asset.name} (already on Gitee)")
            continue
        if present:
            ids = attachment_ids_named(existing, asset.name)
            delete_attachments(request, attach_path, ids)
        ok, reason = upload_attachment_with_watchdog(
            requests.Session,
            f"{API}{attach_path}",
            token,
            asset,
        )
        if ok:
            print(f"uploaded {asset.name}")
        else:
            print(f"FAILED {asset.name}: {reason}")
            failed.append(asset.name)

    if failed:
        sys.exit(f"{len(failed)} attachments failed: {failed}")
    print(f"Gitee release {args.tag}: mirrored {len(assets)} attachments")
    return 0


if __name__ == "__main__":
    sys.exit(main())
