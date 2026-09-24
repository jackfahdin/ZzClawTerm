import tempfile
import unittest
from pathlib import Path

from scripts.ci.gitcode_release_upload import (
    backup_release_asset,
    publish_stable_manifest,
    replace_release_assets,
    replace_asset_with_backup,
    upload_asset,
)


class FakeResponse:
    def __init__(self, status_code: int, text: str = "") -> None:
        self.status_code = status_code
        self.text = text


class FakeSession:
    def __init__(self, response: FakeResponse | Exception, download=b"previous package") -> None:
        self.response = response
        self.download = download
        self.calls: list[tuple[str, bytes, dict[str, str], int]] = []

    def put(self, url, *, data, headers, timeout):
        self.calls.append((url, data.read(), headers, timeout))
        if isinstance(self.response, Exception):
            raise self.response
        return self.response

    def get(self, url, *, stream, timeout):
        self.calls.append((url, stream, timeout))
        return FakeDownloadResponse(self.download)


class FakeDownloadResponse:
    status_code = 200

    def __init__(self, content):
        self.content = content

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def iter_content(self, chunk_size):
        yield self.content


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

    def test_replacement_deletes_only_uploaded_assets_and_waits_for_removal(self) -> None:
        assets = [
            {"id": 12, "name": "old.zip", "type": "attach"},
            {"name": "v0.0.5.zip", "type": "source"},
        ]

        def request(method, path, **_kwargs):
            if method == "DELETE":
                self.assertEqual(path, "/releases/v0.0.5/attach_files/12")
                assets[:] = [asset for asset in assets if asset.get("id") != 12]
                return None
            self.assertEqual(path, "/releases/tags/v0.0.5")
            return {"assets": assets.copy()}

        replace_release_assets(request, "/releases/v0.0.5", {"assets": assets.copy()}, sleep=lambda _: None)
        self.assertEqual(assets, [{"name": "v0.0.5.zip", "type": "source"}])

    def test_replacement_rejects_attachment_without_deletable_id(self) -> None:
        release = {"assets": [{"name": "old.zip", "type": "attach"}]}
        with self.assertRaisesRegex(RuntimeError, "no id"):
            replace_release_assets(lambda *_args: None, "/releases/v0.0.5", release)

    def test_replacement_can_delete_only_a_named_attachment(self) -> None:
        assets = [
            {"id": 12, "name": "old.zip", "type": "attach"},
            {"id": 13, "name": "keep.zip", "type": "attach"},
        ]

        def request(method, path, **_kwargs):
            if method == "DELETE":
                self.assertEqual(path, "/releases/v0.0.5/attach_files/12")
                assets.pop(0)
                return None
            return {"assets": assets.copy()}

        replace_release_assets(
            request, "/releases/v0.0.5", {"assets": assets.copy()},
            names={"old.zip"}, sleep=lambda _: None,
        )
        self.assertEqual([asset["name"] for asset in assets], ["keep.zip"])

    def test_previous_attachment_is_backed_up_before_deletion(self) -> None:
        session = FakeSession(FakeResponse(200))
        backup = Path(self.directory.name) / "old.zip"
        backup_release_asset(
            session,
            {"name": "old.zip", "browser_download_url": "https://gitcode.com/a/old.zip"},
            backup,
        )
        self.assertEqual(backup.read_bytes(), b"previous package")

    def test_failed_deletion_check_restores_old_attachment(self) -> None:
        assets = [{"id": 12, "name": "release.zip", "type": "attach"}]
        backup = Path(self.directory.name) / "backup" / "release.zip"
        session = FakeSession(FakeResponse(200))
        get_calls = 0

        def request(method, path, **_kwargs):
            nonlocal get_calls
            if method == "DELETE":
                assets.clear()
                return None
            if path.endswith("/upload_url"):
                return {"url": self.signed_url, "headers": self.headers}
            get_calls += 1
            if get_calls == 1:
                raise TimeoutError("GitCode did not answer the deletion check")
            if get_calls == 2:
                return {"assets": assets.copy()}
            assets.append({"id": 13, "name": "release.zip", "type": "attach"})
            return {"assets": assets.copy()}

        success, reason, restored = replace_asset_with_backup(
            session, request, "/releases/v0.0.5",
            {"id": 12, "name": "release.zip", "type": "attach",
             "browser_download_url": "https://gitcode.com/a/release.zip"},
            self.asset, backup, sleep=lambda _: None,
        )

        self.assertFalse(success)
        self.assertTrue(restored)
        self.assertIn("TimeoutError", reason)
        self.assertEqual(session.calls[-1][1], b"previous package")

    def test_successful_replacement_uploads_new_bytes(self) -> None:
        assets = [{"id": 12, "name": "release.zip", "type": "attach"}]
        backup = Path(self.directory.name) / "backup" / "release.zip"
        session = FakeSession(FakeResponse(200))

        def request(method, path, **_kwargs):
            if method == "DELETE":
                assets.clear()
                return None
            if path.endswith("/upload_url"):
                return {"url": self.signed_url, "headers": self.headers}
            if not assets and any(call[1] == b"package contents" for call in session.calls if len(call) == 4):
                assets.append({"id": 13, "name": "release.zip", "type": "attach"})
            return {"assets": assets.copy()}

        success, reason, restored = replace_asset_with_backup(
            session, request, "/releases/v0.0.5",
            {"id": 12, "name": "release.zip", "type": "attach",
             "browser_download_url": "https://gitcode.com/a/release.zip"},
            self.asset, backup, sleep=lambda _: None,
        )

        self.assertTrue(success)
        self.assertEqual(reason, "")
        self.assertFalse(restored)
        self.assertEqual(session.calls[-1][1], b"package contents")
        self.assertFalse(backup.exists())

    def test_failed_delete_keeps_existing_attachment(self) -> None:
        backup = Path(self.directory.name) / "backup" / "release.zip"
        session = FakeSession(FakeResponse(200))

        def request(method, path, **_kwargs):
            if method == "DELETE":
                raise TimeoutError("deletion failed")
            return {"assets": [{"id": 12, "name": "release.zip", "type": "attach"}]}

        success, reason, restored = replace_asset_with_backup(
            session, request, "/releases/v0.0.5",
            {"id": 12, "name": "release.zip", "type": "attach",
             "browser_download_url": "https://gitcode.com/a/release.zip"},
            self.asset, backup, sleep=lambda _: None,
        )

        self.assertFalse(success)
        self.assertTrue(restored)
        self.assertIn("TimeoutError", reason)
        self.assertEqual(len(session.calls), 1)

    def test_stable_manifest_is_published_only_after_upload_registration(self) -> None:
        manifest = Path(self.directory.name) / "latest.json"
        manifest.write_text('{"version":"0.0.5"}', encoding="utf-8")
        session = FakeSession(FakeResponse(200))
        created = []

        def request(method, path, **kwargs):
            if method == "GET" and path.endswith("/tags/update-stable"):
                if not created:
                    return None
                return {"assets": [{"id": 20, "name": "latest.json", "type": "attach"}]}
            if method == "POST":
                created.append(kwargs["data"])
                return {}
            if path.endswith("/upload_url"):
                return {"url": self.signed_url, "headers": self.headers}
            raise AssertionError((method, path))

        publish_stable_manifest(
            session, request, "/repos/owner/repo/releases", manifest, "a0dc46d",
            sleep=lambda _: None,
        )
        self.assertEqual(created[0]["tag_name"], "update-stable")
        self.assertEqual(session.calls[-1][1], b'{"version":"0.0.5"}')

    def test_stable_manifest_replacement_preserves_previous_file_on_failure(self) -> None:
        manifest = Path(self.directory.name) / "latest.json"
        manifest.write_text('{"version":"0.0.6"}', encoding="utf-8")
        session = FakeSession(FakeResponse(409), b'{"version":"0.0.5"}')
        previous = {
            "id": 20, "name": "latest.json", "type": "attach",
            "browser_download_url": "https://gitcode.com/a/latest.json",
        }
        assets = [previous.copy()]

        def request(method, path, **_kwargs):
            if method == "DELETE":
                assets.clear()
                return None
            if path.endswith("/upload_url"):
                return {"url": self.signed_url, "headers": self.headers}
            return {"assets": assets.copy()}

        with self.assertRaisesRegex(RuntimeError, "stable update manifest"):
            publish_stable_manifest(
                session, request, "/repos/owner/repo/releases", manifest, "new-sha",
                sleep=lambda _: None,
            )
        self.assertEqual(session.calls[0][0], "https://gitcode.com/a/latest.json")

    def test_stable_manifest_rejects_an_older_release(self) -> None:
        manifest = Path(self.directory.name) / "latest.json"
        manifest.write_text('{"version":"0.0.4"}', encoding="utf-8")
        session = FakeSession(FakeResponse(200), b'{"version":"0.0.5"}')

        def request(method, path, **_kwargs):
            self.assertEqual(method, "GET")
            self.assertTrue(path.endswith("/tags/update-stable"))
            return {"assets": [{
                "id": 20, "name": "latest.json", "type": "attach",
                "browser_download_url": "https://gitcode.com/a/latest.json",
            }]}

        with self.assertRaisesRegex(RuntimeError, "older version"):
            publish_stable_manifest(
                session, request, "/repos/owner/repo/releases", manifest, "old-sha",
                sleep=lambda _: None,
            )
        self.assertEqual(len(session.calls), 1)
