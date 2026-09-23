import tempfile
import unittest
from pathlib import Path

from scripts.ci.gitcode_release_upload import upload_asset


class FakeResponse:
    def __init__(self, status_code: int, text: str = "") -> None:
        self.status_code = status_code
        self.text = text


class FakeSession:
    def __init__(self, response: FakeResponse | Exception) -> None:
        self.response = response
        self.calls: list[tuple[str, bytes, dict[str, str], int]] = []

    def put(self, url, *, data, headers, timeout):
        self.calls.append((url, data.read(), headers, timeout))
        if isinstance(self.response, Exception):
            raise self.response
        return self.response


class GitCodeReleaseUploadTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.asset = Path(self.directory.name) / "release.zip"
        self.asset.write_bytes(b"package contents")
        self.signed_url = "https://file.gitcode.com/upload?Signature=secret"
        self.headers = {
            "x-obs-meta-project-id": "project-id",
            "x-obs-acl": "private",
            "x-obs-callback": "callback-value",
            "Content-Type": "application/zip",
        }

    def upload(self, info, response):
        session = FakeSession(response)
        calls = []

        def request(method, path, **kwargs):
            calls.append((method, path, kwargs))
            return info

        result = upload_asset(session, request, "/releases/v0.0.5", self.asset)
        return result, session, calls

    def test_upload_uses_all_headers_from_gitcode_response(self) -> None:
        info = {"url": self.signed_url, "headers": self.headers}
        result, session, calls = self.upload(info, FakeResponse(200))

        self.assertEqual(result, (True, ""))
        self.assertEqual(
            calls,
            [
                (
                    "GET",
                    "/releases/v0.0.5/upload_url",
                    {"params": {"file_name": "release.zip"}},
                )
            ],
        )
        self.assertEqual(
            session.calls,
            [(self.signed_url, b"package contents", self.headers, 300)],
        )

    def test_upload_rejects_missing_required_headers(self) -> None:
        result, session, _ = self.upload({"url": self.signed_url}, FakeResponse(200))

        self.assertFalse(result[0])
        self.assertIn("headers", result[1])
        self.assertEqual(session.calls, [])

    def test_upload_failure_does_not_expose_signed_url(self) -> None:
        info = {"url": self.signed_url, "headers": self.headers}
        for response in (
            FakeResponse(401, "Unauthorized"),
            FakeResponse(409, "already exists"),
            RuntimeError(f"request failed for {self.signed_url}"),
        ):
            with self.subTest(response=response):
                result, _, _ = self.upload(info, response)
                self.assertFalse(result[0])
                self.assertNotIn("Signature=", result[1])
