from __future__ import annotations

import contextlib
import io
import sys
import unittest
from pathlib import Path
from unittest import mock

CI_SCRIPTS = Path(__file__).resolve().parents[1] / "ci"
sys.path.insert(0, str(CI_SCRIPTS))

import verify_remote_assets  # noqa: E402


LOCAL = {
    "zz.clawterm_0.0.3_windows_x64-setup.exe": (2048, "sha256:aa"),
    "zz.clawterm_0.0.3_linux_x64.AppImage": (1024, "sha256:bb"),
}


class CompareAssetsTests(unittest.TestCase):
    def test_identical_assets_have_no_problems(self) -> None:
        problems, prunable = verify_remote_assets.compare_assets(LOCAL, dict(LOCAL))
        self.assertEqual(problems, [])
        self.assertEqual(prunable, [])

    def test_local_asset_missing_from_remote(self) -> None:
        remote = {k: v for k, v in LOCAL.items() if k.endswith(".AppImage")}
        problems, prunable = verify_remote_assets.compare_assets(LOCAL, remote)
        self.assertEqual(
            problems,
            ["missing remote asset: zz.clawterm_0.0.3_windows_x64-setup.exe"],
        )
        self.assertEqual(prunable, [])

    def test_size_mismatch(self) -> None:
        remote = dict(LOCAL)
        name = "zz.clawterm_0.0.3_windows_x64-setup.exe"
        remote[name] = (2049, LOCAL[name][1])
        problems, _ = verify_remote_assets.compare_assets(LOCAL, remote)
        self.assertEqual(
            problems,
            [f"size mismatch: {name} (local 2048, remote 2049)"],
        )

    def test_digest_mismatch(self) -> None:
        remote = dict(LOCAL)
        name = "zz.clawterm_0.0.3_linux_x64.AppImage"
        remote[name] = (LOCAL[name][0], "sha256:cc")
        problems, _ = verify_remote_assets.compare_assets(LOCAL, remote)
        self.assertEqual(
            problems,
            [f"digest mismatch: {name} (local sha256:bb, remote sha256:cc)"],
        )

    def test_remote_asset_without_digest(self) -> None:
        remote = dict(LOCAL)
        name = "zz.clawterm_0.0.3_linux_x64.AppImage"
        remote[name] = (LOCAL[name][0], "")
        problems, _ = verify_remote_assets.compare_assets(LOCAL, remote)
        self.assertEqual(problems, [f"remote asset has no sha256 digest: {name}"])

    def test_size_and_digest_mismatch_on_one_asset_are_both_reported(self) -> None:
        remote = dict(LOCAL)
        name = "zz.clawterm_0.0.3_windows_x64-setup.exe"
        remote[name] = (1, "sha256:cc")
        problems, _ = verify_remote_assets.compare_assets(LOCAL, remote)
        self.assertEqual(len(problems), 2)
        self.assertTrue(problems[0].startswith("size mismatch: "))
        self.assertTrue(problems[1].startswith("digest mismatch: "))

    def test_unexpected_remote_asset_without_prune_is_a_problem(self) -> None:
        remote = dict(LOCAL, **{"stale.zip": (16, "sha256:dd")})
        problems, prunable = verify_remote_assets.compare_assets(LOCAL, remote)
        self.assertEqual(problems, ["unexpected remote asset: stale.zip"])
        self.assertEqual(prunable, [])

    def test_unexpected_remote_asset_with_prune_is_listed_for_deletion(self) -> None:
        remote = dict(LOCAL, **{"stale.zip": (16, "sha256:dd")})
        problems, prunable = verify_remote_assets.compare_assets(
            LOCAL, remote, prune_unexpected=True
        )
        self.assertEqual(problems, [])
        self.assertEqual(prunable, ["stale.zip"])

    def test_prune_does_not_mask_a_real_mismatch(self) -> None:
        remote = dict(LOCAL, **{"stale.zip": (16, "sha256:dd")})
        remote["zz.clawterm_0.0.3_linux_x64.AppImage"] = (1024, "sha256:cc")
        problems, prunable = verify_remote_assets.compare_assets(
            LOCAL, remote, prune_unexpected=True
        )
        self.assertEqual(
            problems,
            [
                "digest mismatch: zz.clawterm_0.0.3_linux_x64.AppImage "
                "(local sha256:bb, remote sha256:cc)"
            ],
        )
        self.assertEqual(prunable, ["stale.zip"])


class RemoteAssetsTests(unittest.TestCase):
    def test_reads_name_size_and_digest(self) -> None:
        release = {
            "tag_name": "v0.0.3",
            "assets": [
                {"name": "a.zip", "size": 12, "digest": "sha256:aa"},
                {"name": "b.zip", "size": 34, "digest": "sha256:bb"},
            ],
        }
        self.assertEqual(
            verify_remote_assets.remote_assets(release),
            {"a.zip": (12, "sha256:aa"), "b.zip": (34, "sha256:bb")},
        )

    def test_absent_digest_becomes_empty_string(self) -> None:
        release = {"assets": [{"name": "a.zip", "size": 12}]}
        self.assertEqual(verify_remote_assets.remote_assets(release), {"a.zip": (12, "")})

    def test_null_digest_becomes_empty_string(self) -> None:
        release = {"assets": [{"name": "a.zip", "size": 12, "digest": None}]}
        self.assertEqual(verify_remote_assets.remote_assets(release), {"a.zip": (12, "")})

    def test_release_without_assets_is_empty(self) -> None:
        self.assertEqual(verify_remote_assets.remote_assets({}), {})


class ArgumentTests(unittest.TestCase):
    def test_missing_repository_and_environment_fails(self) -> None:
        with mock.patch.dict(
            verify_remote_assets.os.environ, {"GITHUB_REPOSITORY": ""}
        ):
            with contextlib.redirect_stderr(io.StringIO()) as stderr:
                code = verify_remote_assets.main(
                    ["--tag", "v0.0.3", "--directory", "."]
                )
        self.assertEqual(code, 1)
        self.assertIn("GITHUB_REPOSITORY", stderr.getvalue())

    def test_missing_directory_fails(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            code = verify_remote_assets.main(
                [
                    "--tag",
                    "v0.0.3",
                    "--directory",
                    "definitely-not-a-release-directory",
                    "--repository",
                    "owner/repo",
                ]
            )
        self.assertEqual(code, 1)
        self.assertIn("release directory not found", stderr.getvalue())


class FindReleaseTests(unittest.TestCase):
    """The tag endpoint misses drafts, so the list fallback is what a retry needs."""

    def run_find(self, responses: list[tuple[int, str, str]]) -> tuple[object, str]:
        calls = []

        def fake_run(arguments, **_kwargs):
            calls.append(arguments)
            code, stdout, stderr = responses[len(calls) - 1]
            return mock.Mock(returncode=code, stdout=stdout, stderr=stderr)

        with mock.patch.object(verify_remote_assets.subprocess, "run", fake_run):
            release, error = verify_remote_assets.find_release("owner/repo", "v0.0.3")
        self.calls = calls
        return release, error

    def test_tag_endpoint_hit_skips_the_list(self) -> None:
        release, error = self.run_find([(0, '{"id": 7, "assets": []}', "")])
        self.assertEqual(error, "")
        self.assertEqual(release, {"id": 7, "assets": []})
        self.assertEqual(len(self.calls), 1)

    def test_draft_is_found_through_the_release_list(self) -> None:
        release, error = self.run_find(
            [
                (1, "", "HTTP 404: Not Found"),
                (0, '[{"id": 9, "draft": true, "tag_name": "v0.0.3", "assets": []}]', ""),
            ]
        )
        self.assertEqual(error, "")
        self.assertEqual(release, {"id": 9, "draft": True, "tag_name": "v0.0.3", "assets": []})
        self.assertIn("?per_page=100&page=1", self.calls[1][2])

    def test_release_missing_from_both_lookups_reports_the_tag_failure(self) -> None:
        release, error = self.run_find(
            [(1, "", "HTTP 404: Not Found"), (0, "[]", "")]
        )
        self.assertIsNone(release)
        self.assertIn("no release found for v0.0.3", error)
        self.assertIn("HTTP 404", error)

    def test_unparseable_payload_fails_loudly(self) -> None:
        release, error = self.run_find([(0, "<html>", "")])
        self.assertIsNone(release)
        self.assertIn("is not JSON", error)


if __name__ == "__main__":
    unittest.main()
