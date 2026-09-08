from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path


RELEASE_SCRIPTS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_SCRIPTS))

import generate_release_metadata  # noqa: E402


class GenerateReleaseMetadataTests(unittest.TestCase):
    def make_release(self, directory: Path, version: str = "2.0.0") -> None:
        for name in generate_release_metadata.expected_artifacts(version):
            (directory / name).write_bytes(f"payload:{name}".encode())
        updater_names = {
            template.format(version=version)
            for template in generate_release_metadata.UPDATER_ARTIFACTS.values()
        }
        for name in updater_names:
            (directory / f"{name}.sig").write_text(
                f"signature-for-{name}\n", encoding="utf-8"
            )

    def test_generates_download_and_legacy_updater_manifests(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.make_release(directory)
            downloads, updater = generate_release_metadata.generate(
                directory,
                version="2.0.0",
                tag="v2.0.0",
                base_url="https://downloads.nyaterm.app/",
                notes="release notes",
                pub_date="2026-08-31T00:00:00Z",
            )

            self.assertEqual(len(downloads["platforms"]), 8)
            self.assertEqual(len(updater["platforms"]), 8)
            self.assertNotIn("windows-x86_64-portable", updater["platforms"])
            self.assertEqual(
                downloads["platforms"]["darwin-aarch64"]["url"],
                "https://downloads.nyaterm.app/releases/v2.0.0/"
                "NyaTerm_2.0.0_macos_arm64.dmg",
            )
            self.assertEqual(
                updater["platforms"]["windows-x86_64"],
                updater["platforms"]["windows-x86_64-nsis"],
            )
            self.assertEqual(
                json.loads((directory / "downloads.json").read_text()), downloads
            )

            checksum_lines = (directory / "SHA256SUMS").read_text().splitlines()
            self.assertEqual(checksum_lines, sorted(checksum_lines, key=lambda line: line[66:]))
            portable = directory / "NyaTerm_2.0.0_windows_x64_portable.zip"
            self.assertIn(
                f"{hashlib.sha256(portable.read_bytes()).hexdigest()}  {portable.name}",
                checksum_lines,
            )

    def test_rejects_mismatched_tag_and_missing_signature(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.make_release(directory)
            with self.assertRaises(ValueError):
                generate_release_metadata.generate(
                    directory,
                    version="2.0.0",
                    tag="v2.0.1",
                    base_url="https://downloads.nyaterm.app",
                    notes="",
                    pub_date="2026-08-31T00:00:00Z",
                )
            next(directory.glob("*.sig")).unlink()
            with self.assertRaisesRegex(RuntimeError, "missing updater signature"):
                generate_release_metadata.generate(
                    directory,
                    version="2.0.0",
                    tag="v2.0.0",
                    base_url="https://downloads.nyaterm.app",
                    notes="",
                    pub_date="2026-08-31T00:00:00Z",
                )

    def test_rejects_empty_signature_and_keeps_prerelease_urls_versioned(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            version = "2.1.0-beta.1"
            self.make_release(directory, version)
            signature = next(directory.glob("*.sig"))
            signature.write_text("\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "empty updater signature"):
                generate_release_metadata.generate(
                    directory,
                    version=version,
                    tag=f"v{version}",
                    base_url="https://downloads.nyaterm.app",
                    notes="",
                    pub_date="2026-08-31T00:00:00Z",
                )

            signature.write_text("verified-signature\n", encoding="utf-8")
            downloads, updater = generate_release_metadata.generate(
                directory,
                version=version,
                tag=f"v{version}",
                base_url="https://downloads.nyaterm.app",
                notes="prerelease",
                pub_date="2026-08-31T00:00:00Z",
            )
            for manifest in (downloads, updater):
                for platform in manifest["platforms"].values():
                    self.assertIn(f"/releases/v{version}/", platform["url"])


if __name__ == "__main__":
    unittest.main()
