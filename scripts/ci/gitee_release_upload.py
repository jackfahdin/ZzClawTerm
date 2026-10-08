"""Upload release assets to Gitee with a stall watchdog.

Gitee 的附件上传是普通 multipart POST，没有 GitCode 的签名 URL；跨境链路
慢但不该挂死，所以每个文件在独立线程里用独立 Session 上传，监控线程按
字节进度判停滞，连续无进展就打断连接、换全新 Session 从零重试。
"""

from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.parse import urlsplit
import json
import re
import threading
import time


HEARTBEAT_SECONDS = 30
STALL_SECONDS = 180
MAX_ATTEMPTS = 3
ABSOLUTE_SECONDS = 45 * 60


class UploadCancelled(Exception):
    """The watchdog aborted a stalled upload; safe to retry with a new session."""


class UploadProgress:
    """Thread-safe byte counter shared by the upload thread and the watchdog."""

    def __init__(self, total: int, clock=time.monotonic) -> None:
        self.total = total
        self._sent = 0
        self._last_progress = clock()
        self._clock = clock
        self._lock = threading.Lock()

    def advance(self, count: int) -> None:
        with self._lock:
            self._sent += count
            self._last_progress = self._clock()

    def snapshot(self) -> tuple[int, float]:
        with self._lock:
            return self._sent, self._last_progress


class ProgressReader:
    """File wrapper that feeds UploadProgress and honours the cancel flag."""

    def __init__(self, handle, progress: UploadProgress, cancel: threading.Event) -> None:
        self._handle = handle
        self._progress = progress
        self._cancel = cancel

    def __len__(self) -> int:
        # requests 用 super_len 推导 multipart 总长，给它一个稳定的值。
        return self._progress.total

    def read(self, size: int = -1) -> bytes:
        if self._cancel.is_set():
            raise UploadCancelled("upload cancelled by the stall watchdog")
        data = self._handle.read(size)
        if data:
            self._progress.advance(len(data))
        return data


def release_id_of(release: dict) -> int:
    """Gitee releases are addressed by numeric id, unlike GitCode's tag path."""
    release_id = release.get("id")
    if not isinstance(release_id, int) or isinstance(release_id, bool):
        raise RuntimeError("Gitee release has no numeric id")
    return release_id


def attachment_ids_named(attachments: list[dict], name: str) -> list[int]:
    """Existing same-name attachments that must be deleted before re-upload."""
    ids = []
    for item in attachments:
        if item.get("name") != name:
            continue
        attach_id = item.get("id")
        if not isinstance(attach_id, int) or isinstance(attach_id, bool):
            raise RuntimeError(f"Gitee attachment {name} has no id")
        ids.append(attach_id)
    return ids


def delete_attachments(request, attach_path: str, ids: list[int]) -> None:
    """Delete attachments by id; a 404 means another side already removed it."""
    for attach_id in ids:
        request("DELETE", f"{attach_path}/{attach_id}", expected=(200, 204), allow_404=True)


def upload_attachment_with_watchdog(
    session_factory, url: str, token: str, asset: Path, *,
    attempts: int = MAX_ATTEMPTS,
    stall_seconds: float = STALL_SECONDS,
    heartbeat_seconds: float = HEARTBEAT_SECONDS,
    absolute_seconds: float = ABSOLUTE_SECONDS,
    clock=time.monotonic,
    sleep=time.sleep,
    log=print,
) -> tuple[bool, str]:
    """Upload one asset; a stalled connection is aborted and retried from zero."""
    total = asset.stat().st_size
    deadline = clock() + absolute_seconds
    reason = ""
    for attempt in range(1, attempts + 1):
        cancel = threading.Event()
        progress = UploadProgress(total, clock)
        result: list[tuple[bool, str]] = []
        # requests.Session 不是线程安全的，每次尝试都用全新实例，打断旧连接
        # 之后新上传不会复用到半死的连接池。
        session = session_factory()
        worker = threading.Thread(
            target=_post_attachment,
            args=(session, url, token, asset, progress, cancel, result),
            daemon=True,
        )
        worker.start()
        stalled = False
        sent_at_heartbeat = 0
        while worker.is_alive():
            worker.join(timeout=heartbeat_seconds)
            if not worker.is_alive():
                break
            sent, last_progress = progress.snapshot()
            rate = (sent - sent_at_heartbeat) / 1048576 / heartbeat_seconds
            sent_at_heartbeat = sent
            log(
                f"Gitee upload {asset.name}: "
                f"{sent / 1048576:.1f}/{total / 1048576:.1f} MB, "
                f"{rate:.2f} MB/s, attempt {attempt}/{attempts}"
            )
            if clock() >= deadline:
                cancel.set()
                session.close()
                _reap(worker)
                return False, (
                    f"exceeded the {int(absolute_seconds // 60)} minute per-file limit"
                )
            if clock() - last_progress >= stall_seconds:
                log(
                    f"Gitee upload {asset.name}: no new bytes for "
                    f"{int(stall_seconds)}s; aborting attempt {attempt}"
                )
                stalled = True
                cancel.set()
                # 关掉会话连接池，让卡在 socket 上的请求立即报错退出。
                session.close()
                break
        _reap(worker)
        if result and result[0][0]:
            return True, ""
        if stalled:
            reason = f"no new bytes for {int(stall_seconds)}s"
        elif result:
            reason = result[0][1]
        if attempt < attempts:
            sleep(5 * attempt)
    return False, reason or "upload failed without a response"


def _reap(worker: threading.Thread, grace_seconds: float = 30) -> None:
    # 取消后线程通常因连接关闭立刻退出；宽限后仍不退就交给 daemon 语义兜底。
    worker.join(timeout=grace_seconds)


def _post_attachment(session, url, token, asset, progress, cancel, result) -> None:
    try:
        with asset.open("rb") as handle:
            response = session.post(
                url,
                params={"access_token": token},
                files={"file": (asset.name, ProgressReader(handle, progress, cancel))},
                # 连接 30s、单次 socket 读 60s；整体停滞判定交给看门狗。
                timeout=(30, 60),
            )
        if response.status_code in (200, 201):
            result.append((True, ""))
        else:
            result.append((False, f"HTTP {response.status_code}"))
    except UploadCancelled:
        result.append((False, "cancelled by the stall watchdog"))
    except Exception as error:
        result.append((False, f"{type(error).__name__} while uploading"))



def backup_release_asset(session, asset: dict, destination: Path, *, sleep=time.sleep) -> None:
    """Keep the old manifest available for rollback and version comparison."""
    url = asset.get("browser_download_url")
    host = urlsplit(url).hostname if isinstance(url, str) else None
    if not host or (host != "gitee.com" and not host.endswith(".gitee.com")):
        raise RuntimeError(f"Gitee attachment {asset.get('name')} has no trusted download URL")
    destination.parent.mkdir(parents=True, exist_ok=True)
    for attempt in range(1, 4):
        try:
            with session.get(url, stream=True, timeout=(20, 120)) as response:
                if response.status_code != 200:
                    raise RuntimeError(f"HTTP {response.status_code}")
                content_type = response.headers.get("Content-Type", "").lower()
                if "text/html" in content_type:
                    raise RuntimeError("Gitee returned an HTML page instead of an attachment")
                expected_length = response.headers.get("Content-Length")
                received = 0
                with destination.open("wb") as handle:
                    for chunk in response.iter_content(chunk_size=1024 * 1024):
                        handle.write(chunk)
                        received += len(chunk)
                if expected_length is not None and received != int(expected_length):
                    raise RuntimeError("Gitee attachment backup was truncated")
            return
        except Exception as error:
            destination.unlink(missing_ok=True)
            if attempt == 3:
                raise RuntimeError(
                    f"Gitee attachment {asset.get('name')} backup failed after 3 attempts: "
                    f"{type(error).__name__}"
                ) from error
            sleep(5 * attempt)


def publish_stable_manifest(
    session_factory, request, api_base: str, token: str, releases_path: str,
    manifest: Path, target: str, *, sleep=time.sleep,
) -> None:
    """Advance the fixed Gitee update endpoint after the version release is complete."""
    new_version = _stable_version(json.loads(manifest.read_text(encoding="utf-8")))
    existing = request("GET", f"{releases_path}/tags/update-stable", allow_404=True)
    if existing is None:
        release = request("POST", releases_path, data={
            "tag_name": "update-stable",
            "name": "Stable update channel",
            "body": "Stable update manifest for installed applications.",
            "target_commitish": target,
            "prerelease": "true",
        })
        release_id = release_id_of(release)
        attach_path = f"{releases_path}/{release_id}/attach_files"
        success, reason = _upload_and_confirm(
            session_factory, request, api_base, token, attach_path, manifest, sleep,
        )
        if not success:
            raise RuntimeError(f"Gitee stable update manifest failed: {reason}")
        return

    release_id = release_id_of(existing)
    attach_path = f"{releases_path}/{release_id}/attach_files"
    previous = next(
        (item for item in (request("GET", attach_path) or [])
         if item.get("name") == manifest.name),
        None,
    )
    if previous is None:
        success, reason = _upload_and_confirm(
            session_factory, request, api_base, token, attach_path, manifest, sleep,
        )
        if not success:
            raise RuntimeError(f"Gitee stable update manifest failed: {reason}")
        return

    with TemporaryDirectory() as directory:
        backup = Path(directory) / manifest.name
        backup_release_asset(session_factory(), previous, backup)
        old_version = _stable_version(json.loads(backup.read_text(encoding="utf-8")))
        if new_version < old_version:
            raise RuntimeError("Gitee stable update manifest would move to an older version")
        delete_attachments(
            request, attach_path, attachment_ids_named([previous], manifest.name),
        )
        success, reason = _upload_and_confirm(
            session_factory, request, api_base, token, attach_path, manifest, sleep,
        )
        if success:
            return
        restored, restore_reason = _upload_and_confirm(
            session_factory, request, api_base, token, attach_path, backup, sleep,
        )
        if not restored:
            reason += f"; restoring the old manifest failed: {restore_reason}"
        raise RuntimeError(f"Gitee stable update manifest failed: {reason}")


def _upload_and_confirm(
    session_factory, request, api_base: str, token: str, attach_path: str,
    manifest: Path, sleep,
) -> tuple[bool, str]:
    success, reason = upload_attachment_with_watchdog(
        session_factory, f"{api_base.rstrip('/')}{attach_path}", token, manifest,
        sleep=sleep,
    )
    if not success:
        return False, reason
    # 上传返回 200/201 不等于附件已登记，轮询列表确认（与 GitCode 版一致）。
    for poll in range(20):
        current = request("GET", attach_path) or []
        if any(item.get("name") == manifest.name for item in current):
            return True, ""
        if poll < 19:
            sleep(3)
    return False, "Gitee did not register the uploaded attachment"


def _stable_version(manifest: dict) -> tuple[int, int, int]:
    # 与 gitcode 版保持同一套版本格式约定；两个 helper 模块刻意互不依赖。
    version = manifest.get("version")
    match = re.fullmatch(
        r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[0-9A-Za-z.-]+)?",
        version if isinstance(version, str) else "",
    )
    if match is None:
        raise RuntimeError("Gitee stable update manifest has an invalid version")
    return tuple(int(part) for part in match.groups())
