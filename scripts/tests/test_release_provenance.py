from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
import generate_release_metadata
import package_native
import release_provenance


class ReleaseProvenanceTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.dist = self.root / "dist"
        self.records = self.root / "records"
        self.dist.mkdir()
        self.records.mkdir()
        self.sha = "a" * 40
        self.version = "2.0.0"
        self.run = "1234"
        for name in generate_release_metadata.expected_artifacts(self.version):
            (self.dist / name).write_bytes(name.encode())
        for target in package_native.TARGETS:
            value = release_provenance.record(self.dist, target, self.sha, self.version, self.run)
            (self.records / f"{target}.json").write_text(json.dumps(value), encoding="utf-8")

    def verify(self, **overrides) -> None:
        values = dict(source_sha=self.sha, version=self.version, run_id=self.run)
        values.update(overrides)
        release_provenance.verify(self.dist, self.records, **values)

    def test_complete_packages_match_actual_checkout(self) -> None:
        self.verify()

    def test_rejects_other_checkout_or_run(self) -> None:
        for values in ({"source_sha": "b" * 40}, {"run_id": "5678"}):
            with self.subTest(values=values), self.assertRaises(ValueError):
                self.verify(**values)

    def test_rejects_wrong_recorded_version(self) -> None:
        path = next(self.records.iterdir())
        value = json.loads(path.read_text())
        value["version"] = "2.0.1"
        path.write_text(json.dumps(value))
        with self.assertRaises(ValueError):
            self.verify()

    def test_rejects_missing_legacy_provenance(self) -> None:
        next(self.records.iterdir()).unlink()
        with self.assertRaises(ValueError):
            self.verify()

    def test_rejects_changed_package(self) -> None:
        next(self.dist.iterdir()).write_bytes(b"damaged")
        with self.assertRaises(ValueError):
            self.verify()

    def test_rejects_missing_package(self) -> None:
        next(self.dist.iterdir()).unlink()
        with self.assertRaises(FileNotFoundError):
            self.verify()


if __name__ == "__main__":
    unittest.main()
