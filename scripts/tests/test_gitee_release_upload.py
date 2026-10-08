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
