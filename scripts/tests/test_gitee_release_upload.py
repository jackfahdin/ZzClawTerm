import tempfile
import threading
import time
import unittest
from pathlib import Path

from scripts.ci.gitee_release_upload import (
    ProgressReader,
    UploadCancelled,
    UploadProgress,
    attachment_ids_named,
    delete_attachments,
    release_id_of,
    upload_attachment_with_watchdog,
)


class FakeResponse:
    def __init__(self, status_code: int) -> None:
        self.status_code = status_code


class RecordingSession:
    """Reads the whole upload body through the ProgressReader, then replies."""

    def __init__(self, status_code: int = 201) -> None:
        self.status_code = status_code
        self.calls: list[tuple] = []
        self.closed = False

    def post(self, url, *, params, files, timeout):
        name, reader = files["file"]
        body = b""
        while chunk := reader.read(4):
            body += chunk
        self.calls.append((url, params, name, body, timeout))
        return FakeResponse(self.status_code)

    def close(self) -> None:
        self.closed = True


class StallingSession:
    """Sends a few bytes, then goes silent until the watchdog closes it."""

    def __init__(self) -> None:
        self.closed = False
        self.started = threading.Event()

    def post(self, url, *, params, files, timeout):
        self.started.set()
        reader = files["file"][1]
        reader.read(4)
        while not self.closed:
            time.sleep(0.005)
        # 连接被看门狗断开后，下一次读必须抛 UploadCancelled。
        reader.read(4)
        raise AssertionError("the cancel flag was not honoured")

    def close(self) -> None:
        self.closed = True


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0

    def __call__(self) -> float:
        return self.now

    def advance(self, seconds: float) -> None:
        self.now += seconds


class ProgressReaderTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.asset = Path(self.directory.name) / "release.zip"
        self.asset.write_bytes(b"package contents")
        self.clock = FakeClock()

    def reader(self):
        progress = UploadProgress(len(b"package contents"), self.clock)
        cancel = threading.Event()
        handle = self.asset.open("rb")
        self.addCleanup(handle.close)
        return ProgressReader(handle, progress, cancel), progress, cancel

    def test_read_counts_bytes_and_advances_the_progress_timestamp(self) -> None:
        reader, progress, _ = self.reader()

        first = reader.read(7)
        self.clock.advance(12)
        second = reader.read(100)

        self.assertEqual(first, b"package")
        self.assertEqual(second, b" contents")
        self.assertEqual(len(reader), len(b"package contents"))
        sent, last_progress = progress.snapshot()
        self.assertEqual(sent, len(b"package contents"))
        self.assertEqual(last_progress, 12.0)
        self.assertEqual(reader.read(1), b"")
        self.assertEqual(progress.snapshot()[0], len(b"package contents"))

    def test_read_raises_once_cancelled(self) -> None:
        reader, progress, cancel = self.reader()
        cancel.set()

        with self.assertRaises(UploadCancelled):
            reader.read(4)
        self.assertEqual(progress.snapshot()[0], 0)


class WatchdogTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.asset = Path(self.directory.name) / "release.zip"
        self.asset.write_bytes(b"package contents")

    def upload(self, factory, **overrides):
        options = {
            "attempts": 2,
            "stall_seconds": 0.05,
            "heartbeat_seconds": 0.01,
            "absolute_seconds": 300,
            "sleep": lambda _seconds: None,
            "log": lambda _message: None,
        }
        options.update(overrides)
        return upload_attachment_with_watchdog(
            factory, "https://gitee.com/api/v5/repos/o/r/releases/1/attach_files",
            "token", self.asset, **options,
        )

    def test_successful_upload_posts_multipart_with_token_and_timeouts(self) -> None:
        session = RecordingSession()

        result = self.upload(lambda: session)

        self.assertEqual(result, (True, ""))
        self.assertEqual(
            session.calls,
            [
                (
                    "https://gitee.com/api/v5/repos/o/r/releases/1/attach_files",
                    {"access_token": "token"},
                    "release.zip",
                    b"package contents",
                    (30, 60),
                )
            ],
        )

    def test_http_error_is_reported_and_retried(self) -> None:
        sessions = [RecordingSession(413), RecordingSession(201)]
        iterator = iter(sessions)

        result = self.upload(lambda: next(iterator))

        self.assertEqual(result, (True, ""))
        self.assertEqual(len(sessions[0].calls), 1)
        self.assertEqual(len(sessions[1].calls), 1)

    def test_stalled_attempt_is_aborted_and_retried_with_a_fresh_session(self) -> None:
        stalling = StallingSession()
        sessions = [stalling, RecordingSession(200)]
        iterator = iter(sessions)
        heartbeats = []

        result = self.upload(lambda: next(iterator), log=heartbeats.append)

        self.assertEqual(result, (True, ""))
        self.assertTrue(stalling.closed)
        self.assertTrue(
            any("release.zip" in message and "attempt 1/2" in message for message in heartbeats)
        )

    def test_persistent_stall_fails_after_all_attempts(self) -> None:
        stalls = [StallingSession(), StallingSession()]
        iterator = iter(stalls)

        success, reason = self.upload(lambda: next(iterator))

        self.assertFalse(success)
        self.assertIn("no new bytes", reason)
        self.assertTrue(all(session.closed for session in stalls))

    def test_absolute_limit_stops_without_retrying(self) -> None:
        stalling = StallingSession()
        attempts = []

        def factory():
            attempts.append(1)
            return stalling

        success, reason = self.upload(factory, absolute_seconds=0.02)

        self.assertFalse(success)
        self.assertIn("per-file limit", reason)
        self.assertEqual(len(attempts), 1)
        self.assertTrue(stalling.closed)


class AttachmentReplacementTests(unittest.TestCase):
    def test_release_id_requires_a_numeric_id(self) -> None:
        self.assertEqual(release_id_of({"id": 42}), 42)
        for invalid in ({}, {"id": "42"}, {"id": True}):
            with self.assertRaises(RuntimeError):
                release_id_of(invalid)

    def test_same_name_attachments_are_selected_for_replacement(self) -> None:
        attachments = [
            {"id": 1, "name": "release.zip"},
            {"id": 2, "name": "other.zip"},
            {"id": 3, "name": "release.zip"},
        ]

        self.assertEqual(attachment_ids_named(attachments, "release.zip"), [1, 3])
        self.assertEqual(attachment_ids_named(attachments, "missing.zip"), [])
        with self.assertRaises(RuntimeError):
            attachment_ids_named([{"name": "release.zip"}], "release.zip")

    def test_delete_tolerates_already_removed_attachments(self) -> None:
        calls = []

        def request(method, path, *, expected, allow_404):
            calls.append((method, path, expected, allow_404))

        delete_attachments(request, "/releases/9/attach_files", [1, 3])

        self.assertEqual(
            calls,
            [
                ("DELETE", "/releases/9/attach_files/1", (200, 204), True),
                ("DELETE", "/releases/9/attach_files/3", (200, 204), True),
            ],
        )


if __name__ == "__main__":
    unittest.main()


class FakeDownloadResponse:
    status_code = 200

    def __init__(self, content, headers):
        self.content = content
        self.headers = headers

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def iter_content(self, chunk_size):
        yield self.content


class DownloadSession:
    def __init__(self, content=b"previous manifest", failures=0, headers=None) -> None:
        self.content = content
        self.failures = failures
        self.headers = headers or {
            "Content-Length": str(len(content)),
            "Content-Type": "application/octet-stream",
        }
        self.urls: list[str] = []

    def get(self, url, *, stream, timeout):
        self.urls.append(url)
        if self.failures:
            self.failures -= 1
            raise ConnectionResetError("connection reset by peer")
        return FakeDownloadResponse(self.content, self.headers)


class FakeApi:
    """Records requests and answers from a (method, key-substring) table."""

    def __init__(self, responses) -> None:
        self.responses = responses
        self.calls: list[tuple] = []

    def __call__(self, method, path, *, expected=(200, 201), allow_404=False, **kwargs):
        self.calls.append((method, path, kwargs))
        for (want_method, fragment), response in self.responses.items():
            if method == want_method and fragment in path:
                if isinstance(response, Exception):
                    raise response
                return response
        raise AssertionError(f"unexpected API call {method} {path}")


class BackupReleaseAssetTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.destination = Path(self.directory.name) / "latest.json"
        self.asset = {
            "name": "latest.json",
            "browser_download_url": "https://gitee.com/o/r/releases/download/update-stable/latest.json",
        }

    def test_backup_downloads_the_old_manifest(self) -> None:
        session = DownloadSession(b'{"version": "0.1.0"}')

        from scripts.ci.gitee_release_upload import backup_release_asset

        backup_release_asset(session, self.asset, self.destination, sleep=lambda _s: None)

        self.assertEqual(self.destination.read_bytes(), b'{"version": "0.1.0"}')
        self.assertEqual(session.urls, [self.asset["browser_download_url"]])

    def test_backup_rejects_untrusted_hosts(self) -> None:
        from scripts.ci.gitee_release_upload import backup_release_asset

        self.asset["browser_download_url"] = "https://example.com/latest.json"
        with self.assertRaises(RuntimeError):
            backup_release_asset(
                DownloadSession(), self.asset, self.destination, sleep=lambda _s: None,
            )

    def test_backup_rejects_truncated_and_html_downloads(self) -> None:
        from scripts.ci.gitee_release_upload import backup_release_asset

        truncated = DownloadSession(b"abc", headers={"Content-Length": "10"})
        with self.assertRaises(RuntimeError):
            backup_release_asset(truncated, self.asset, self.destination, sleep=lambda _s: None)
        html = DownloadSession(b"<html></html>", headers={"Content-Type": "text/html"})
        with self.assertRaises(RuntimeError):
            backup_release_asset(html, self.asset, self.destination, sleep=lambda _s: None)
        self.assertFalse(self.destination.exists())


class PublishStableManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.manifest = Path(self.directory.name) / "latest.json"
        self.old_body = b'{"version": "0.1.0", "platforms": {}}'
        self.manifest.write_bytes(b'{"version": "0.2.0", "platforms": {}}')

    def publish(self, api, sessions, manifest=None):
        from scripts.ci.gitee_release_upload import publish_stable_manifest

        iterator = iter(sessions)
        publish_stable_manifest(
            lambda: next(iterator), api, "https://gitee.com/api/v5", "token",
            "/repos/o/r/releases", manifest or self.manifest, "sha",
            sleep=lambda _s: None,
        )

    def test_creates_the_stable_release_and_uploads_the_manifest(self) -> None:
        api = FakeApi({
            ("GET", "/releases/tags/update-stable"): None,
            ("POST", "/repos/o/r/releases"): {"id": 7},
            ("GET", "/releases/7/attach_files"): [{"name": "latest.json"}],
        })
        upload = RecordingSession(201)

        self.publish(api, [upload])

        methods = [(method, path) for method, path, _ in api.calls]
        self.assertIn(("POST", "/repos/o/r/releases"), methods)
        self.assertEqual(len(upload.calls), 1)
        self.assertTrue(upload.calls[0][0].endswith("/releases/7/attach_files"))

    def test_replace_rejects_an_older_manifest_version(self) -> None:
        from scripts.ci.gitee_release_upload import publish_stable_manifest

        api = FakeApi({
            ("GET", "/releases/tags/update-stable"): {"id": 7},
            ("GET", "/releases/7/attach_files"): [{
                "id": 3,
                "name": "latest.json",
                "browser_download_url": "https://gitee.com/o/r/releases/download/update-stable/latest.json",
            }],
        })
        download = DownloadSession(b'{"version": "0.3.0"}')
        with self.assertRaisesRegex(RuntimeError, "older version"):
            publish_stable_manifest(
                lambda: download, api, "https://gitee.com/api/v5", "token",
                "/repos/o/r/releases", self.manifest, "sha", sleep=lambda _s: None,
            )

    def test_replace_uploads_after_deleting_the_same_name_attachment(self) -> None:
        api = FakeApi({
            ("GET", "/releases/tags/update-stable"): {"id": 7},
            ("GET", "/releases/7/attach_files"): [
                {
                    "id": 3,
                    "name": "latest.json",
                    "browser_download_url": "https://gitee.com/o/r/releases/download/update-stable/latest.json",
                },
            ],
            ("DELETE", "/releases/7/attach_files/3"): None,
        })
        download = DownloadSession(self.old_body)
        upload = RecordingSession(201)
        sessions = iter([download, upload])

        self.publish(api, sessions)

        methods = [(method, path) for method, path, _ in api.calls]
        self.assertIn(("DELETE", "/repos/o/r/releases/7/attach_files/3"), methods)
        self.assertEqual(upload.calls[0][3], self.manifest.read_bytes())

    def test_failed_upload_restores_the_previous_manifest(self) -> None:
        api = FakeApi({
            ("GET", "/releases/tags/update-stable"): {"id": 7},
            ("GET", "/releases/7/attach_files"): [
                {
                    "id": 3,
                    "name": "latest.json",
                    "browser_download_url": "https://gitee.com/o/r/releases/download/update-stable/latest.json",
                },
            ],
            ("DELETE", "/releases/7/attach_files/3"): None,
        })
        download = DownloadSession(self.old_body)
        # 看门狗每次失败都会换新 Session 重试（默认 3 次），让三次全部失败，
        # 之后才轮到恢复上传。
        restore_upload = RecordingSession(201)
        sessions = iter([
            download,
            RecordingSession(500),
            RecordingSession(500),
            RecordingSession(500),
            restore_upload,
        ])

        from scripts.ci.gitee_release_upload import publish_stable_manifest

        with self.assertRaisesRegex(RuntimeError, "stable update manifest failed"):
            publish_stable_manifest(
                lambda: next(sessions),
                api, "https://gitee.com/api/v5", "token",
                "/repos/o/r/releases", self.manifest, "sha", sleep=lambda _s: None,
            )
        # 恢复上传拿到的是备份的旧清单内容。
        self.assertEqual(restore_upload.calls[0][3], self.old_body)



class StableVersionTests(unittest.TestCase):
    def test_stable_version_parsing(self) -> None:
        from scripts.ci.gitee_release_upload import _stable_version

        self.assertEqual(_stable_version({"version": "0.2.10"}), (0, 2, 10))
        self.assertEqual(_stable_version({"version": "1.2.3+build.5"}), (1, 2, 3))
        for invalid in ({}, {"version": "0.01.2"}, {"version": "1.2"}, {"version": "v1.2.3"}):
            with self.assertRaises(RuntimeError):
                _stable_version(invalid)
