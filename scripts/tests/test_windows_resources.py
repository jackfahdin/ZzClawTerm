from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "release"))

import verify_native_package  # noqa: E402


@unittest.skipUnless(sys.platform == "win32" and shutil.which("cargo"), "requires Windows and Cargo")
class WindowsResourcesTests(unittest.TestCase):
    def test_build_script_embeds_cargo_version_in_linked_executable(self) -> None:
        # Exercise the real application build script with a tiny executable so
        # stable/prerelease cases do not require rebuilding the GPUI workspace.
        host = next(line.removeprefix("host: ") for line in subprocess.check_output(
            ["rustc", "-vV"], text=True,
        ).splitlines() if line.startswith("host: "))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy2(ROOT / "crates/nyaterm-app/build.rs", root / "build.rs")
            icons = root / "resources/icons"
            icons.mkdir(parents=True)
            shutil.copy2(ROOT / "crates/nyaterm-app/resources/icons/icon.ico", icons)
            (root / "src").mkdir()
            (root / "src/main.rs").write_text('#![windows_subsystem = "windows"]\nfn main() {}\n')
            env = {**os.environ, "CARGO_TARGET_DIR": str(root / "target")}

            def build(version: str) -> subprocess.CompletedProcess[str]:
                (root / "Cargo.toml").write_text(
                    '[package]\nname = "nyaterm-resource-test"\nedition = "2024"\n'
                    f'version = "{version}"\n[build-dependencies]\nembed-resource = "3.0"\n',
                )
                return subprocess.run(
                    ["cargo", "build", "--offline", "--target", host],
                    cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                )

            for version, numeric in (
                ("2.0.0", "2.0.0.0"),
                ("2.0.0-preview.9", "2.0.0.9"),
                ("2.0.0-preview.10+build.123", "2.0.0.10"),
                ("2.0.0-beta", "2.0.0.0"),
                ("65535.65535.65535-preview.65535", "65535.65535.65535.65535"),
            ):
                with self.subTest(version=version):
                    result = build(version)
                    self.assertEqual(result.returncode, 0, result.stdout)
                    binary = root / "target" / host / "debug/nyaterm-resource-test.exe"
                    verify_native_package.verify_windows_application_binary(binary.read_bytes(), binary.name)
                    verify_native_package.verify_windows_application_version(binary, version)
                    metadata = verify_native_package.windows_version_info(binary)
                    self.assertEqual(metadata["FileVersion"], numeric)
                    self.assertEqual(metadata["ProductVersion"], version)

            for version in ("65536.0.0", "2.0.0-preview.65536"):
                with self.subTest(version=version):
                    result = build(version)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("Windows version component exceeds 65535", result.stdout)
