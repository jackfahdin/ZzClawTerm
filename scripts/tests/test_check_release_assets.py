from __future__ import annotations

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path


CI_SCRIPTS = Path(__file__).resolve().parents[1] / "ci"
sys.path.insert(0, str(CI_SCRIPTS))

import check_release_assets  # noqa: E402


class CheckReleaseAssetsTests(unittest.TestCase):
    def test_snapshot_label_accepts_exact_set_and_rejects_drift(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in check_release_assets.expected_artifacts("continuous-build"):
                (directory / name).touch()
            args = [
                "--dist",
                str(directory),
                "--version",
                "0.0.1",
                "--artifact-version",
                "continuous-build",
            ]
            self.assertEqual(check_release_assets.main(args), 0)

            (directory / "unexpected.zip").touch()
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(check_release_assets.main(args), 1)
            (directory / "unexpected.zip").unlink()

            next(directory.iterdir()).unlink()
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(check_release_assets.main(args), 1)


if __name__ == "__main__":
    unittest.main()
