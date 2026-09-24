import unittest
from datetime import datetime, timezone

from scripts.ci.refresh_snapshot_release import refresh_snapshot_release


class RefreshSnapshotReleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.release = {
            "id": 42,
            "draft": False,
            "prerelease": True,
            "published_at": "2026-09-10T04:49:10Z",
            "assets": [{"name": "snapshot.zip"}],
        }
        self.now = datetime(2026, 9, 24, 10, 0, 0, tzinfo=timezone.utc)

    def test_republishes_existing_snapshot_and_preserves_release_and_assets(self) -> None:
        edits = []

        def edit_release(draft):
            edits.append(draft)
            self.release["draft"] = draft
            if not draft:
                self.release["published_at"] = "2026-09-24T10:00:01Z"

        published_at = refresh_snapshot_release(
            lambda: self.release.copy(), edit_release, now=self.now
        )

        self.assertEqual(edits, [True, False])
        self.assertEqual(published_at, "2026-09-24T10:00:01Z")
        self.assertFalse(self.release["draft"])

    def test_fails_when_github_does_not_refresh_publication_time(self) -> None:
        edits = []

        def edit_release(draft):
            edits.append(draft)
            self.release["draft"] = draft

        with self.assertRaisesRegex(RuntimeError, "publication time"):
            refresh_snapshot_release(
                lambda: self.release.copy(), edit_release, now=self.now, sleep=lambda _: None
            )

        self.assertEqual(edits, [True, False])

    def test_fails_if_republishing_loses_an_asset(self) -> None:
        def edit_release(draft):
            self.release["draft"] = draft
            if not draft:
                self.release["published_at"] = "2026-09-24T10:00:01Z"
                self.release["assets"] = []

        with self.assertRaisesRegex(RuntimeError, "release state"):
            refresh_snapshot_release(
                lambda: self.release.copy(), edit_release, now=self.now, sleep=lambda _: None
            )

    def test_restores_public_release_when_republishing_fails(self) -> None:
        edits = []

        def edit_release(draft):
            edits.append(draft)
            if len(edits) == 2:
                raise RuntimeError("temporary GitHub API error")
            self.release["draft"] = draft

        with self.assertRaisesRegex(RuntimeError, "temporary GitHub API error"):
            refresh_snapshot_release(lambda: self.release.copy(), edit_release, now=self.now)

        self.assertEqual(edits, [True, False, False])
        self.assertFalse(self.release["draft"])

    def test_recovers_a_draft_left_by_an_interrupted_publish(self) -> None:
        self.release["draft"] = True
        self.release["published_at"] = None
        edits = []

        def edit_release(draft):
            edits.append(draft)
            self.release["draft"] = draft
            self.release["published_at"] = "2026-09-24T10:00:01Z"

        published_at = refresh_snapshot_release(
            lambda: self.release.copy(), edit_release, now=self.now
        )

        self.assertEqual(edits, [False])
        self.assertEqual(published_at, "2026-09-24T10:00:01Z")

    def test_rejects_a_non_prerelease_without_changing_it(self) -> None:
        self.release["prerelease"] = False
        edits = []

        with self.assertRaisesRegex(RuntimeError, "prerelease"):
            refresh_snapshot_release(
                lambda: self.release.copy(), edits.append, now=self.now
            )

        self.assertEqual(edits, [])
