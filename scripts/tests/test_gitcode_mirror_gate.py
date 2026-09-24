import unittest

from scripts.ci.gitcode_mirror_gate import master_ready, release_ready


class GitCodeMirrorGateTests(unittest.TestCase):
    def test_master_waits_for_both_successful_ci_runs_on_current_sha(self) -> None:
        runs = {
            "rust.yml": [{"head_sha": "abc", "event": "push", "conclusion": "success"}],
            "docs.yml": [{"head_sha": "abc", "event": "push", "conclusion": "failure"}],
        }
        self.assertFalse(master_ready("abc", "abc", runs))
        runs["docs.yml"][0]["conclusion"] = "success"
        self.assertTrue(master_ready("abc", "abc", runs))
        self.assertFalse(master_ready("abc", "newer", runs))

    def test_master_rejects_stale_or_non_push_success(self) -> None:
        runs = {
            "rust.yml": [
                {"head_sha": "abc", "event": "push", "conclusion": "failure"},
                {"head_sha": "abc", "event": "push", "conclusion": "success"},
            ],
            "docs.yml": [{"head_sha": "abc", "event": "workflow_dispatch", "conclusion": "success"}],
        }
        self.assertFalse(master_ready("abc", "abc", runs))

    def test_release_requires_successful_publish_job_for_same_tag_and_sha(self) -> None:
        run = {
            "head_sha": "abc",
            "head_branch": "v0.0.6",
            "conclusion": "success",
            "jobs": [{"name": "Publish GitHub release", "conclusion": "skipped"}],
        }
        self.assertFalse(release_ready("v0.0.6", "abc", "abc", run))
        run["jobs"][0]["conclusion"] = "success"
        self.assertTrue(release_ready("v0.0.6", "abc", "abc", run))
        self.assertFalse(release_ready("v0.0.6", "abc", "different", run))
        run["head_branch"] = "master"
        run["event"] = "workflow_dispatch"
        self.assertTrue(release_ready("v0.0.6", "abc", "abc", run))
