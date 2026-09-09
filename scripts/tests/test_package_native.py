from __future__ import annotations

import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


RELEASE_SCRIPTS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_SCRIPTS))

import package_native  # noqa: E402


class PackageNativeTests(unittest.TestCase):
    def test_release_tag_is_normalized(self) -> None:
        self.assertEqual(package_native.validate_version("v0.0.1"), "0.0.1")
        self.assertEqual(
            package_native.validate_version("0.0.1-preview.1"),
            "0.0.1-preview.1",
        )

    def test_invalid_or_mismatched_version_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            package_native.validate_version("release-2")
        with self.assertRaisesRegex(ValueError, "does not match"):
            package_native.validate_version("v0.0.2", "0.0.1")

    def test_snapshot_is_only_allowed_as_an_artifact_label(self) -> None:
        self.assertEqual(
            package_native.validate_artifact_version("main-snapshot"),
            "main-snapshot",
        )
        with self.assertRaises(ValueError):
            package_native.validate_version("main-snapshot")
        with self.assertRaises(ValueError):
            package_native.validate_artifact_version("nightly")
        self.assertEqual(
            package_native.artifact_names(
                "x86_64-pc-windows-msvc", "main-snapshot"
            ),
            {
                "ZzClawTerm_main-snapshot_windows_x64_portable.zip",
                "ZzClawTerm_main-snapshot_windows_x64-setup.exe",
            },
        )

    def test_all_release_targets_have_expected_artifact_names(self) -> None:
        expected = {
            "aarch64-apple-darwin": {
                "ZzClawTerm_0.0.1_macos_arm64.dmg",
                "ZzClawTerm_0.0.1_macos_arm64.app.tar.gz",
            },
            "x86_64-apple-darwin": {
                "ZzClawTerm_0.0.1_macos_x64.dmg",
                "ZzClawTerm_0.0.1_macos_x64.app.tar.gz",
            },
            "aarch64-unknown-linux-gnu": {
                "ZzClawTerm_0.0.1_linux_arm64.AppImage",
                "ZzClawTerm_0.0.1_linux_arm64.deb",
                "ZzClawTerm_0.0.1_linux_arm64.rpm",
            },
            "x86_64-unknown-linux-gnu": {
                "ZzClawTerm_0.0.1_linux_x64.AppImage",
                "ZzClawTerm_0.0.1_linux_x64.deb",
                "ZzClawTerm_0.0.1_linux_x64.rpm",
            },
            "aarch64-pc-windows-msvc": {
                "ZzClawTerm_0.0.1_windows_arm64_portable.zip",
                "ZzClawTerm_0.0.1_windows_arm64-setup.exe",
            },
            "x86_64-pc-windows-msvc": {
                "ZzClawTerm_0.0.1_windows_x64_portable.zip",
                "ZzClawTerm_0.0.1_windows_x64-setup.exe",
            },
        }
        for target, names in expected.items():
            with self.subTest(target=target):
                self.assertEqual(package_native.artifact_names(target, "v0.0.1"), names)

    def test_release_binary_always_uses_explicit_target_directory(self) -> None:
        with mock.patch.dict("os.environ", {}, clear=True):
            linux = package_native.release_binary_path("x86_64-unknown-linux-gnu")
            windows = package_native.release_binary_path("aarch64-pc-windows-msvc")
        self.assertEqual(
            linux.relative_to(package_native.ROOT_DIR).as_posix(),
            "target/x86_64-unknown-linux-gnu/release/zzclawterm",
        )
        self.assertEqual(
            windows.relative_to(package_native.ROOT_DIR).as_posix(),
            "target/aarch64-pc-windows-msvc/release/zzclawterm.exe",
        )

    def test_helper_binaries_resolve_beside_the_application(self) -> None:
        self.assertIn("zzclawterm-rdp-helper", package_native.HELPER_BINS)
        self.assertIn("zzclawterm-mcp", package_native.HELPER_BINS)
        with mock.patch.dict("os.environ", {}, clear=True):
            linux = package_native.helper_binary_paths("x86_64-unknown-linux-gnu")
            windows = package_native.helper_binary_paths("aarch64-pc-windows-msvc")
        self.assertEqual(
            [path.name for path in linux], list(package_native.HELPER_BINS)
        )
        self.assertEqual(
            [path.name for path in windows],
            [f"{name}.exe" for name in package_native.HELPER_BINS],
        )
        application = package_native.release_binary_path("x86_64-unknown-linux-gnu")
        for path in linux:
            self.assertEqual(path.parent, application.parent)

    def test_copy_helpers_stages_every_helper_beside_the_application(self) -> None:
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sources = root / "build"
            destination = root / "package"
            destination.mkdir()
            staged = []
            for path in package_native.helper_binary_paths(target):
                fake = sources / path.name
                fake.parent.mkdir(parents=True, exist_ok=True)
                fake.write_bytes(b"helper")
                staged.append(fake)
            with mock.patch.object(
                package_native, "helper_binary_paths", return_value=staged
            ):
                copied = package_native.copy_helpers(destination, target)
            self.assertEqual(
                sorted(path.name for path in copied),
                sorted(package_native.HELPER_BINS),
            )
            for path in copied:
                self.assertTrue(path.is_file())
                self.assertEqual(path.parent, destination)

    def test_windows_installer_script_installs_and_removes_every_helper(self) -> None:
        target = "x86_64-pc-windows-msvc"
        info = package_native.target_info(target)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "DIST_DIR", root / "dist"),
                mock.patch.object(package_native, "run"),
                mock.patch.object(
                    package_native, "find_makensis", return_value="makensis"
                ),
            ):
                package_native.WORK_DIR.mkdir(parents=True)
                package_native.DIST_DIR.mkdir(parents=True)
                application = root / "zzclawterm.exe"
                application.write_bytes(b"MZ")
                for path in package_native.helper_binary_paths(target):
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"MZ")
                package_native.create_windows_packages(
                    application, info, "0.0.1", "0.0.1"
                )
                script = (
                    package_native.WORK_DIR / "zzclawterm-installer.nsi"
                ).read_text(encoding="utf-8")
        for name in package_native.HELPER_BINS:
            filename = f"{name}.exe"
            with self.subTest(helper=filename):
                self.assertRegex(script, rf'File ".*{filename}"')
                self.assertIn(f'Delete "$INSTDIR\\{filename}"', script)
        self.assertIn(
            r'WriteRegStr HKCU "Software\Classes\zzclawterm" "URL Protocol" ""',
            script,
        )
        self.assertIn(
            r'WriteRegStr HKCU "Software\Classes\zzclawterm\shell\open\command" "" "$\"$INSTDIR\ZzClawTerm.exe$\" $\"%1$\""',
            script,
        )
        self.assertIn(
            r'DeleteRegKey HKCU "Software\Classes\zzclawterm"',
            script,
        )
        self.assertNotIn(r"Software\Classes\ssh", script)
        self.assertNotIn(r"Software\Classes\telnet", script)

    def test_vcxsrv_source_prefers_environment_over_vendor_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env_dist = root / "vcxsrv-env"
            vendor_dist = root / "vendor" / "vcxsrv"
            for dist in (env_dist, vendor_dist):
                dist.mkdir(parents=True)
                (dist / package_native.VCXSRV_EXE).write_bytes(b"MZ")
            with (
                mock.patch.dict(
                    "os.environ",
                    {package_native.VCXSRV_ENV_VAR: str(env_dist)},
                ),
                mock.patch.object(package_native, "ROOT_DIR", root),
            ):
                self.assertEqual(package_native.resolve_vcxsrv_source(), env_dist)
            with (
                mock.patch.dict("os.environ", {}, clear=True),
                mock.patch.object(package_native, "ROOT_DIR", root),
            ):
                self.assertEqual(package_native.resolve_vcxsrv_source(), vendor_dist)

    def test_vcxsrv_source_requires_the_executable_and_allows_absence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env_dist = root / "vcxsrv-env"
            env_dist.mkdir()
            with mock.patch.dict(
                "os.environ", {package_native.VCXSRV_ENV_VAR: str(env_dist)}
            ):
                with self.assertRaisesRegex(RuntimeError, "vcxsrv.exe"):
                    package_native.resolve_vcxsrv_source()
            with mock.patch.dict(
                "os.environ",
                {package_native.VCXSRV_ENV_VAR: str(root / "missing")},
            ):
                with self.assertRaisesRegex(RuntimeError, "does not point"):
                    package_native.resolve_vcxsrv_source()
            vendor_dist = root / "vendor" / "vcxsrv"
            vendor_dist.mkdir(parents=True)
            with (
                mock.patch.dict("os.environ", {}, clear=True),
                mock.patch.object(package_native, "ROOT_DIR", root),
            ):
                with self.assertRaisesRegex(RuntimeError, "vcxsrv.exe"):
                    package_native.resolve_vcxsrv_source()
                vendor_dist.rmdir()
                self.assertIsNone(package_native.resolve_vcxsrv_source())

    def test_stage_vcxsrv_copies_the_tree_and_writes_a_gpl_notice(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "vcxsrv-dist"
            (source / "fonts").mkdir(parents=True)
            (source / package_native.VCXSRV_EXE).write_bytes(b"MZ")
            (source / "fonts" / "fonts.dir").write_text("1\n", encoding="utf-8")
            destination = root / "package"
            destination.mkdir()
            with mock.patch.dict(
                "os.environ", {package_native.VCXSRV_ENV_VAR: str(source)}
            ):
                staged = package_native.stage_vcxsrv(destination)
            self.assertEqual(staged, destination / package_native.VCXSRV_DIRNAME)
            self.assertTrue((staged / package_native.VCXSRV_EXE).is_file())
            self.assertTrue((staged / "fonts" / "fonts.dir").is_file())
            notice = (staged / "NOTICE.txt").read_text(encoding="utf-8")
            self.assertIn("GPLv3", notice)
            self.assertIn("sourceforge.net/projects/vcxsrv", notice)

    def test_stage_vcxsrv_warns_and_skips_without_a_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "package"
            destination.mkdir()
            with (
                mock.patch.dict("os.environ", {}, clear=True),
                mock.patch.object(package_native, "ROOT_DIR", root),
                mock.patch("builtins.print") as printed,
            ):
                self.assertIsNone(package_native.stage_vcxsrv(destination))
            warning = " ".join(
                " ".join(str(argument) for argument in call.args)
                for call in printed.call_args_list
            )
            self.assertIn("WARNING", warning)
            self.assertIn(package_native.VCXSRV_ENV_VAR, warning)
            self.assertFalse(
                (destination / package_native.VCXSRV_DIRNAME).exists()
            )

    def test_windows_packages_bundle_vcxsrv_in_zip_and_installer(self) -> None:
        target = "x86_64-pc-windows-msvc"
        info = package_native.target_info(target)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            vcxsrv = root / "vcxsrv-dist"
            (vcxsrv / "fonts").mkdir(parents=True)
            (vcxsrv / package_native.VCXSRV_EXE).write_bytes(b"MZ")
            (vcxsrv / "fonts" / "fonts.dir").write_text("1\n", encoding="utf-8")
            helpers = []
            for name in package_native.HELPER_BINS:
                fake = root / "build" / f"{name}.exe"
                fake.parent.mkdir(parents=True, exist_ok=True)
                fake.write_bytes(b"MZ")
                helpers.append(fake)
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "DIST_DIR", root / "dist"),
                mock.patch.object(package_native, "run"),
                mock.patch.object(
                    package_native, "find_makensis", return_value="makensis"
                ),
                mock.patch.object(
                    package_native, "helper_binary_paths", return_value=helpers
                ),
                mock.patch.dict(
                    "os.environ", {package_native.VCXSRV_ENV_VAR: str(vcxsrv)}
                ),
            ):
                package_native.WORK_DIR.mkdir(parents=True)
                package_native.DIST_DIR.mkdir(parents=True)
                application = root / "zzclawterm.exe"
                application.write_bytes(b"MZ")
                package_native.create_windows_packages(
                    application, info, "0.0.1", "0.0.1"
                )
                script = (
                    package_native.WORK_DIR / "zzclawterm-installer.nsi"
                ).read_text(encoding="utf-8")
                portable = next(package_native.DIST_DIR.glob("*_portable.zip"))
                with zipfile.ZipFile(portable) as archive:
                    names = set(archive.namelist())
        self.assertRegex(script, r'File /r ".*\\vcxsrv"')
        self.assertIn(r'RMDir /r "$INSTDIR\vcxsrv"', script)
        self.assertIn("ZzClawTerm-portable/vcxsrv/vcxsrv.exe", names)
        self.assertIn("ZzClawTerm-portable/vcxsrv/NOTICE.txt", names)
        self.assertIn("ZzClawTerm-portable/vcxsrv/fonts/fonts.dir", names)

    def test_linux_desktop_registers_only_zzclawterm_url_scheme(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "zzclawterm.desktop"
            package_native.write_desktop_file(path, "/opt/zzclawterm/zzclawterm")
            desktop = path.read_text(encoding="utf-8")
        self.assertIn("Exec=/opt/zzclawterm/zzclawterm %U\n", desktop)
        self.assertIn("MimeType=x-scheme-handler/zzclawterm;\n", desktop)
        self.assertNotIn("x-scheme-handler/ssh", desktop)
        self.assertNotIn("x-scheme-handler/telnet", desktop)

    def test_deb_dependencies_cover_helper_binaries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = [root / "zzclawterm", root / "zzclawterm-rdp-helper"]
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(
                    package_native, "require_tool", return_value="dpkg-shlibdeps"
                ),
                mock.patch.object(
                    package_native.subprocess,
                    "check_output",
                    return_value="shlibs:Depends=libc6, libx11-6\n",
                ) as check_output,
            ):
                dependencies = package_native.linux_deb_dependencies(binaries)
        self.assertEqual(dependencies, "libc6, libx11-6")
        command = check_output.call_args.args[0]
        for binary in binaries:
            self.assertIn(str(binary), command)
        self.assertEqual(command.count("-e"), len(binaries))

    def test_release_binary_respects_absolute_cargo_target_dir(self) -> None:
        # Build the absolute path for the running platform: Path("/cache/cargo") has
        # no drive letter, so is_absolute() is False on Windows and the assertion
        # would compare against a path joined onto the repository root instead.
        target_dir = Path(tempfile.gettempdir(), "zzclawterm-cargo-target").resolve()
        with mock.patch.dict("os.environ", {"CARGO_TARGET_DIR": str(target_dir)}):
            path = package_native.release_binary_path("x86_64-unknown-linux-gnu")
        self.assertEqual(
            path, target_dir / "x86_64-unknown-linux-gnu" / "release" / "zzclawterm"
        )

    def test_platform_package_versions_are_normalized(self) -> None:
        self.assertEqual(package_native.windows_numeric_version("2.4.6-beta.1"), "2.4.6.0")
        self.assertEqual(package_native.linux_rpm_version("2.4.6"), ("2.4.6", "1"))
        self.assertEqual(
            package_native.linux_rpm_version("2.4.6-beta.1"),
            ("2.4.6", "0.beta.1"),
        )

    def test_dpkg_dependency_output_is_parsed(self) -> None:
        output = "ignored=value\nshlibs:Depends=libc6 (>= 2.34), libx11-6\n"
        self.assertEqual(
            package_native.parse_dpkg_dependencies(output),
            "libc6 (>= 2.34), libx11-6",
        )
        with self.assertRaises(RuntimeError):
            package_native.parse_dpkg_dependencies("shlibs:Depends=\n")

    def test_native_icon_resources_have_expected_formats_and_sizes(self) -> None:
        expected_png_sizes = {
            "32x32.png": (32, 32),
            "64x64.png": (64, 64),
            "128x128.png": (128, 128),
            "256x256.png": (256, 256),
            "512x512.png": (512, 512),
        }
        for name, expected_size in expected_png_sizes.items():
            with self.subTest(name=name):
                data = (package_native.ICON_DIR / name).read_bytes()
                self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                self.assertEqual(struct.unpack(">II", data[16:24]), expected_size)

        self.assertEqual(
            (package_native.ICON_DIR / "icon.icns").read_bytes()[:4], b"icns"
        )
        self.assertEqual(
            (package_native.ICON_DIR / "icon.ico").read_bytes()[:4], b"\0\0\1\0"
        )


if __name__ == "__main__":
    unittest.main()
