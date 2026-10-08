"""Upload release assets to Gitee with a stall watchdog.

Gitee 的附件上传是普通 multipart POST，没有 GitCode 的签名 URL；跨境链路
慢但不该挂死，所以每个文件在独立线程里用独立 Session 上传，监控线程按
字节进度判停滞，连续无进展就打断连接、换全新 Session 从零重试。
"""

from pathlib import Path
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
