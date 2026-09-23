"""Upload a release asset using GitCode's signed URL contract."""

from pathlib import Path
from urllib.parse import urlsplit


REQUIRED_HEADERS = (
    "x-obs-meta-project-id",
    "x-obs-acl",
    "x-obs-callback",
    "content-type",
)


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
