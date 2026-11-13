from __future__ import annotations

import hashlib
import json
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
import prepare_conpty  # noqa: E402


def fake_pe(machine: int) -> bytes:
    data = bytearray(256)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x84, machine)
    return bytes(data)


class PrepareConptyTests(unittest.TestCase):
    def make_package(
        self, root: Path, *, missing: str | None = None, wrong_machine: str | None = None
    ) -> tuple[Path, Path]:
        package = root / "conpty.nupkg"
        with zipfile.ZipFile(package, "w") as archive:
            seen = set()
            for source, _, machine in prepare_conpty.FILES:
                if source != missing and source not in seen:
                    archive.writestr(
                        source,
                        fake_pe(0xAA64 if source == wrong_machine else machine),
                    )
                    seen.add(source)
        metadata = root / "conpty-package.json"
        metadata.write_text(
            json.dumps({
                "version": "test",
                "sha256": hashlib.sha256(package.read_bytes()).hexdigest(),
            }),
            encoding="utf-8",
        )
        return package, metadata

    def test_extracts_only_verified_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package, metadata = self.make_package(root)
            with mock.patch.object(prepare_conpty, "METADATA", metadata):
                prepare_conpty.extract_verified(package, root / "output")
            for _, destination, machine in prepare_conpty.FILES:
                self.assertEqual(
                    prepare_conpty.pe_machine((root / "output" / destination).read_bytes()),
                    machine,
                )

    def test_rejects_hash_mismatch_before_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package, metadata = self.make_package(root)
            metadata.write_text(json.dumps({"sha256": "0" * 64}), encoding="utf-8")
            with mock.patch.object(prepare_conpty, "METADATA", metadata):
                with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                    prepare_conpty.extract_verified(package, root / "output")
            self.assertFalse((root / "output").exists())

    def test_rejects_missing_file_before_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            missing = prepare_conpty.FILES[1][0]
            package, metadata = self.make_package(root, missing=missing)
            with mock.patch.object(prepare_conpty, "METADATA", metadata):
                with self.assertRaisesRegex(ValueError, "is missing"):
                    prepare_conpty.extract_verified(package, root / "output")
            self.assertFalse((root / "output").exists())

    def test_rejects_wrong_machine_before_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wrong = prepare_conpty.FILES[0][0]
            package, metadata = self.make_package(root, wrong_machine=wrong)
            with mock.patch.object(prepare_conpty, "METADATA", metadata):
                with self.assertRaisesRegex(ValueError, "architecture mismatch"):
                    prepare_conpty.extract_verified(package, root / "output")
            self.assertFalse((root / "output").exists())
