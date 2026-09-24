"""Upload a release asset using GitCode's signed URL contract."""

from pathlib import Path
from urllib.parse import urlsplit
import time


REQUIRED_HEADERS = (
    "x-obs-meta-project-id",
    "x-obs-acl",
    "x-obs-callback",
    "content-type",
)


def replace_release_assets(
    request, release_path: str, release: dict, *, names: set[str] | None = None,
    sleep=time.sleep,
) -> None:
    """Remove old uploaded files before reusing a GitCode release tag."""
    attached = [
        asset for asset in release.get("assets", [])
        if asset.get("type") == "attach"
        and (names is None or asset.get("name") in names)
    ]
    if not attached:
        return
    for asset in attached:
        asset_id = asset.get("id")
        if not isinstance(asset_id, (int, str)) or not str(asset_id):
            raise RuntimeError(f"GitCode attachment {asset.get('name')} has no id")
        request("DELETE", f"{release_path}/attach_files/{asset_id}", expected=(200, 202, 204))

    releases_path, tag = release_path.rsplit("/", 1)
    deleted_ids = {asset["id"] for asset in attached}
    for attempt in range(6):
        current = request("GET", f"{releases_path}/tags/{tag}")
        remaining = {
            asset.get("id")
            for asset in current.get("assets", [])
            if asset.get("type") == "attach"
        }
        if not remaining.intersection(deleted_ids):
            return
        if attempt < 5:
            sleep(2)
    raise RuntimeError("GitCode did not remove old release attachments")


def backup_release_asset(session, asset: dict, destination: Path) -> None:
    """Keep the old file available for rollback if its replacement upload fails."""
    url = asset.get("browser_download_url")
    host = urlsplit(url).hostname if isinstance(url, str) else None
    if not host or (host != "gitcode.com" and not host.endswith(".gitcode.com")):
        raise RuntimeError(f"GitCode attachment {asset.get('name')} has no trusted download URL")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with session.get(url, stream=True, timeout=600) as response:
        if response.status_code != 200:
            raise RuntimeError(
                f"GitCode attachment {asset.get('name')} backup failed: HTTP {response.status_code}"
            )
        with destination.open("wb") as handle:
            for chunk in response.iter_content(chunk_size=1024 * 1024):
                handle.write(chunk)


def replace_asset_with_backup(
    session, request, release_path: str, previous: dict, asset: Path,
    backup: Path, *, sleep=time.sleep,
) -> tuple[bool, str, bool]:
    """Replace one attachment, restoring the old bytes if replacement fails."""
    backup_release_asset(session, previous, backup)
    releases_path, tag = release_path.rsplit("/", 1)
    try:
        replace_release_assets(
            request, release_path, {"assets": [previous]},
            names={asset.name}, sleep=sleep,
        )
        success, reason = _upload_and_confirm(
            session, request, release_path, asset, sleep=sleep,
        )
    except Exception as error:
        success, reason = False, f"{type(error).__name__} while replacing the old attachment"
    if success:
        backup.unlink()
        return True, "", False

    try:
        current = request("GET", f"{releases_path}/tags/{tag}")
        old_present = any(
            item.get("type") == "attach" and item.get("id") == previous.get("id")
            for item in current.get("assets", [])
        )
    except Exception:
        old_present = False
    if old_present:
        return False, reason, True

    restored, restore_reason = _upload_and_confirm(
        session, request, release_path, backup, sleep=sleep,
    )
    if not restored:
        reason += f"; restoring the old attachment failed: {restore_reason}"
    return False, reason, restored


def _upload_and_confirm(session, request, release_path: str, asset: Path, *, sleep) -> tuple[bool, str]:
    releases_path, tag = release_path.rsplit("/", 1)
    reason = ""
    for attempt in range(1, 4):
        try:
            success, reason = upload_asset(session, request, release_path, asset)
            if success:
                for poll in range(20):
                    current = request("GET", f"{releases_path}/tags/{tag}")
                    if any(
                        item.get("type") == "attach" and item.get("name") == asset.name
                        for item in current.get("assets", [])
                    ):
                        return True, ""
                    if poll < 19:
                        sleep(3)
                reason = "GitCode did not register the uploaded attachment"
        except Exception as error:
            reason = f"{type(error).__name__} while uploading or verifying the attachment"
        if attempt < 3:
            sleep(5 * attempt)
    return False, reason


def upload_asset(session, request, release_path: str, asset: Path) -> tuple[bool, str]:
    info = request(
        "GET",
        f"{release_path}/upload_url",
        params={"file_name": asset.name},
    )
    if not isinstance(info, dict):
        return False, "GitCode upload_url returned an invalid response"

    data = info.get("data") if isinstance(info.get("data"), dict) else {}
    url = info.get("url") or info.get("upload_url") or data.get("url")
    headers = info.get("headers") or data.get("headers")
    if not isinstance(url, str) or urlsplit(url).scheme != "https":
        return False, "GitCode upload_url returned no HTTPS URL"
    if not isinstance(headers, dict):
        return False, "GitCode upload_url returned no upload headers"
    missing = set(REQUIRED_HEADERS) - {str(key).lower() for key in headers}
    if missing:
        return False, f"GitCode upload_url omitted required headers: {', '.join(sorted(missing))}"

    host = urlsplit(url).hostname or "GitCode upload host"
    try:
        with asset.open("rb") as handle:
            response = session.put(url, data=handle, headers=headers, timeout=300)
    except Exception as error:
        return False, f"{type(error).__name__} while uploading to {host}"
    if response.status_code in (200, 201, 204):
        return True, ""
    return False, f"HTTP {response.status_code} from {host}"
