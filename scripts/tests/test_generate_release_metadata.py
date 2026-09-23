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

BASE_URL = "https://github.com/jackfahdin/ZzClawTerm"


class GenerateReleaseMetadataTests(unittest.TestCase):
    def make_release(self, directory: Path, version: str = "0.0.1") -> None:
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
                version="0.0.1",
                tag="v0.0.1",
                base_url=f"{BASE_URL}/",
                notes="release notes",
                pub_date="2026-08-31T00:00:00Z",
            )

            self.assertEqual(len(downloads["platforms"]), 8)
            self.assertEqual(len(updater["platforms"]), 8)
            self.assertNotIn("windows-x86_64-portable", updater["platforms"])
            self.assertEqual(
                downloads["platforms"]["darwin-aarch64"]["url"],
                f"{BASE_URL}/releases/download/v0.0.1/"
                "ZzClawTerm_0.0.1_macos_arm64.dmg",
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
            portable = directory / "ZzClawTerm_0.0.1_windows_x64_portable.zip"
            self.assertIn(
                f"{hashlib.sha256(portable.read_bytes()).hexdigest()}  {portable.name}",
                checksum_lines,
            )

    def test_stable_and_prerelease_publish_to_separate_channel_manifests(self) -> None:
        self.assertEqual(
            generate_release_metadata.channel_manifest_destinations("2.0.0"),
            {
                "latest.json": "latest.json",
                "downloads.json": "downloads.json",
            },
        )
        preview = generate_release_metadata.channel_manifest_destinations(
            "2.0.0-preview.2"
        )
        self.assertEqual(
            preview,
            {
                "latest.json": "channels/preview/latest.json",
                "downloads.json": "channels/preview/downloads.json",
            },
        )
        self.assertNotIn("latest.json", preview.values())
        self.assertNotIn("downloads.json", preview.values())

    def test_rejects_mismatched_tag_and_missing_signature(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.make_release(directory)
            with self.assertRaises(ValueError):
                generate_release_metadata.generate(
                    directory,
                    version="0.0.1",
                    tag="v0.0.2",
                    base_url=BASE_URL,
                    notes="",
                    pub_date="2026-08-31T00:00:00Z",
                )
            next(directory.glob("*.sig")).unlink()
            with self.assertRaisesRegex(RuntimeError, "missing updater signature"):
                generate_release_metadata.generate(
                    directory,
                    version="0.0.1",
                    tag="v0.0.1",
                    base_url=BASE_URL,
                    notes="",
                    pub_date="2026-08-31T00:00:00Z",
                )

    def test_rejects_empty_signature_and_keeps_prerelease_urls_versioned(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            version = "0.1.0-beta.1"
            self.make_release(directory, version)
            signature = next(directory.glob("*.sig"))
            signature.write_text("\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "empty updater signature"):
                generate_release_metadata.generate(
                    directory,
                    version=version,
                    tag=f"v{version}",
                    base_url=BASE_URL,
                    notes="",
                    pub_date="2026-08-31T00:00:00Z",
                )

            signature.write_text("verified-signature\n", encoding="utf-8")
            downloads, updater = generate_release_metadata.generate(
                directory,
                version=version,
                tag=f"v{version}",
                base_url=BASE_URL,
                notes="prerelease",
                pub_date="2026-08-31T00:00:00Z",
            )
            for manifest in (downloads, updater):
                for platform in manifest["platforms"].values():
                    self.assertIn(f"/releases/download/v{version}/", platform["url"])


    def test_snapshot_manifest_serves_the_preview_channel(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            version = "0.1.0-preview.3"
            self.make_release(directory, version)
            downloads, updater = generate_release_metadata.generate(
                directory,
                version=version,
                tag=generate_release_metadata.SNAPSHOT_TAG,
                base_url=BASE_URL,
                notes="snapshot",
                pub_date="2026-09-23T00:00:00Z",
            )

            self.assertEqual(updater["version"], version)
            for manifest in (downloads, updater):
                for platform in manifest["platforms"].values():
                    self.assertIn(
                        f"/releases/download/{generate_release_metadata.SNAPSHOT_TAG}/",
                        platform["url"],
                    )

    def test_snapshot_manifest_rejects_a_stable_version(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.make_release(directory, "0.1.0")
            with self.assertRaisesRegex(ValueError, "prerelease version"):
                generate_release_metadata.generate(
                    directory,
                    version="0.1.0",
                    tag=generate_release_metadata.SNAPSHOT_TAG,
                    base_url=BASE_URL,
                    notes="",
                    pub_date="2026-09-23T00:00:00Z",
                )


if __name__ == "__main__":
    unittest.main()
