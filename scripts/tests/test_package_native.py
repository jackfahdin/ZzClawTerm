from __future__ import annotations

import plistlib
import os
import io
import shutil
import subprocess
import struct
import tarfile
from contextlib import contextmanager
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


RELEASE_SCRIPTS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_SCRIPTS))

import package_native  # noqa: E402
import verify_native_package  # noqa: E402


@contextmanager
def staged_package(target: str):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        binary = root / "build" / "zzclawterm"
        binary.parent.mkdir()
        binary.write_bytes(b"application")
        suffix = ".exe" if "windows" in target else ""
        helpers = [binary.parent / f"{name}{suffix}" for name in package_native.helper_bins(target)]
        for helper in helpers:
            helper.write_bytes(b"helper")
        if "windows" in target:
            for relative in package_native.conpty_files(target):
                path = root / "staging/resources/windows/conpty" / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"MZ")

        def run(command, **kwargs):
            if command[0] == "rpmbuild":
                output = root / "work" / "rpm" / "rpmbuild" / "RPMS" / package_native.linux_rpm_arch(target)
                output.mkdir(parents=True)
                (output / "test.rpm").write_bytes(b"rpm")

        with (
            mock.patch.object(package_native, "WORK_DIR", root / "work"),
            mock.patch.object(package_native, "DIST_DIR", root / "dist"),
            mock.patch.object(package_native.prepare_conpty, "STAGING", root / "staging"),
            mock.patch.object(package_native, "helper_binary_paths", return_value=helpers),
            mock.patch.object(package_native, "require_tool", side_effect=lambda name: name),
            mock.patch.object(package_native, "find_iscc", return_value="iscc"),
            mock.patch.object(package_native, "run", side_effect=run),
            mock.patch.object(package_native, "linux_deb_dependencies", return_value="libc6"),
        ):
            package_native.WORK_DIR.mkdir()
            package_native.DIST_DIR.mkdir()
            yield root, binary, package_native.target_info(target)


class PackageNativeTests(unittest.TestCase):
    def setUp(self):
        from scripts.tests.test_verify_native_package import stage_android_fixture
        patcher = mock.patch.object(package_native, "copy_android_resources", side_effect=stage_android_fixture)
        patcher.start(); self.addCleanup(patcher.stop)

    @unittest.skipUnless(shutil.which("dpkg-deb"), "dpkg-deb is not installed")
    def test_real_deb_icons_are_readable_root_owned_and_repaired_on_configure(self) -> None:
        for version in ("2.0.0", "2.0.0-preview.1"):
            with self.subTest(version=version), staged_package("x86_64-unknown-linux-gnu") as (root, binary, info):
                icon_source = root / "icons"
                shutil.copytree(package_native.ICON_DIR, icon_source)
                for source in icon_source.glob("*.png"):
                    source.chmod(0o600)
                with (
                    mock.patch.object(package_native, "ICON_DIR", icon_source),
                    mock.patch.object(package_native, "run", side_effect=lambda command: subprocess.run(
                        command, check=True, stdout=subprocess.DEVNULL)),
                ):
                    package_native.create_linux_deb(binary, info, version, version)
                identity = package_native.release_identity(version)
                deb = next((root / "dist").glob("*.deb"))
                verify_native_package.verify_deb(deb, info.target, version)
                data = subprocess.check_output(["dpkg-deb", "--fsys-tarfile", str(deb)])
                with tarfile.open(fileobj=io.BytesIO(data)) as archive:
                    self.assertTrue(all(member.uid == member.gid == 0 for member in archive))
                postinst = root / "work/deb/DEBIAN/postinst"
                self.assertEqual(postinst.stat().st_mode & 0o777, 0o755)
                subprocess.run(["sh", "-n", str(postinst)], check=True)
                hicolor = root / "work/deb/usr/share/icons/hicolor"
                script = root / "postinst-test"
                script.write_text(postinst.read_text().replace("/usr/share/icons/hicolor", str(hicolor)))
                icon = hicolor / "128x128/apps" / f"{identity.desktop_id}.png"
                unrelated = icon.with_name("unrelated.png")
                unrelated.write_bytes(b"unrelated")
                unrelated.chmod(0o600)
                # A removed optional size must not break upgrade/reconfigure.
                (hicolor / "32x32/apps" / f"{identity.desktop_id}.png").unlink()
                icon.chmod(0o600)
                subprocess.run(["sh", str(script), "abort-upgrade"], check=True)
                self.assertEqual(icon.stat().st_mode & 0o777, 0o600)
                for _ in range(2):
                    subprocess.run(["sh", str(script), "configure"], check=True)
                    self.assertEqual(icon.stat().st_mode & 0o777, 0o644)
                    self.assertEqual(unrelated.stat().st_mode & 0o777, 0o600)
                icon.unlink()
                icon.symlink_to(unrelated)
                subprocess.run(["sh", str(script), "configure"], check=True)
                self.assertEqual(unrelated.stat().st_mode & 0o777, 0o600)

    def test_windows_release_builds_use_one_static_crt_and_preserve_existing_flags(self) -> None:
        for target in ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-unknown-linux-gnu"):
            for existing in ({"RUSTFLAGS": "-C debuginfo=1"}, {"CARGO_ENCODED_RUSTFLAGS": "-C\x1fdebuginfo=1"}):
                with self.subTest(target=target, existing=existing), tempfile.TemporaryDirectory() as directory:
                    binary = Path(directory) / "binary"
                    binary.write_bytes(b"fixture")
                    with (mock.patch.dict(package_native.os.environ, existing, clear=True),
                          mock.patch.object(package_native, "release_binary_path", return_value=binary),
                          mock.patch.object(package_native, "run") as run):
                        self.assertEqual(package_native.build_binary("zzclawterm-app", "zzclawterm", target), binary)
                    environment = run.call_args.kwargs["env"]
                    if target.endswith("-windows-msvc"):
                        self.assertEqual(environment["CARGO_ENCODED_RUSTFLAGS"],
                                         "-C\x1fdebuginfo=1\x1f-C\x1ftarget-feature=+crt-static")
                    else:
                        self.assertIsNone(environment)

    def test_portable_packages_match_shared_contract_for_both_architectures(self) -> None:
        for target in ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"):
            with self.subTest(target=target), staged_package(target) as (root, binary, info):
                package_native.create_windows_packages(binary, info, "2.0.1", "2.0.1")
                path = root / f"dist/ZzClawTerm_2.0.1_{info.label}_portable.zip"
                with zipfile.ZipFile(path) as archive:
                    actual = {entry.filename for entry in archive.infolist()
                              if not entry.is_dir()}
                self.assertEqual(actual, package_native.windows_portable.archive_files(target))

    @unittest.skipUnless(shutil.which("makensis"), "NSIS is not installed")

    def test_semver_resolves_application_identity(self) -> None:
        for version in ("2.0.0", "2.0.1", "2.1.0", "2.0.0+build-with-hyphen"):
            self.assertEqual(package_native.release_identity(version), package_native.STABLE_IDENTITY)
        for version in ("2.0.0-preview.1", "2.0.0-preview.2", "2.0.0-beta.1", "2.0.0-rc.1"):
            self.assertEqual(package_native.release_identity(version), package_native.PREVIEW_IDENTITY)
        for version in ("02.0.0", "2.0.0-preview.01", "2.0.0-", "2.0.0+", "2.0.0-a..b"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                package_native.release_identity(version)


    @unittest.skipIf(os.name == "nt", "macOS DMG symlink creation requires POSIX")
    def test_macos_bundle_and_updater_archive_use_the_same_identity(self) -> None:
        for version in ("2.0.0", "2.0.0-preview.1"):
            with self.subTest(version=version), staged_package("aarch64-apple-darwin") as (root, binary, info):
                identity = package_native.release_identity(version)
                package_native.create_macos_packages(binary, info, version, version)
                bundle = root / "work" / identity.macos_bundle_name
                plist = plistlib.loads((bundle / "Contents" / "Info.plist").read_bytes())
                self.assertEqual(plist["CFBundleDisplayName"], identity.display_name)
                self.assertEqual(plist["CFBundleName"], identity.display_name)
                self.assertEqual(plist["CFBundleIdentifier"], identity.macos_identifier)
                self.assertEqual(plist["CFBundleURLTypes"][0]["CFBundleURLSchemes"], list(identity.url_schemes))
                self.assertTrue((root / "work" / "dmg" / identity.macos_bundle_name).is_dir())
                self.assertEqual((root / "work" / "dmg" / "Applications").readlink().as_posix(), "/Applications")
                archive = next((root / "dist").glob("*.tar.gz"))
                with tarfile.open(archive) as handle:
                    self.assertIn(f"{identity.macos_bundle_name}/Contents/MacOS/ZzClawTerm", handle.getnames())

    def test_run_with_retry_retries_transient_command_failures(self) -> None:
        failure = subprocess.CalledProcessError(1, ["hdiutil", "create"])
        with (
            mock.patch.object(package_native, "run", side_effect=[failure, None]) as run,
            mock.patch.object(package_native.time, "sleep") as sleep,
        ):
            package_native.run_with_retry(
                ["hdiutil", "create"], attempts=3, delay_seconds=0.5
            )
        self.assertEqual(run.call_count, 2)
        sleep.assert_called_once_with(0.5)

    def test_linux_formats_use_isolated_package_desktop_icons_and_install_paths(self) -> None:
        for version in ("2.0.0", "2.0.0-preview.1"):
            with self.subTest(version=version), staged_package("x86_64-unknown-linux-gnu") as (root, binary, info):
                identity = package_native.release_identity(version)
                package_native.create_linux_packages(binary, info, version, version)
                control = (root / "work" / "deb" / "DEBIAN" / "control").read_text()
                self.assertIn(f"Package: {identity.desktop_id}\n", control)
                spec = (root / "work" / "rpm" / "rpmbuild" / "SPECS" / "zzclawterm.spec").read_text()
                self.assertIn(f"Name: {identity.desktop_id}\n", spec)
                self.assertIn(f"/opt/{identity.desktop_id}\n", spec)
                for payload in (root / "work" / "deb", root / "work" / "rpm" / "payload", root / "work" / "ZzClawTerm.AppDir"):
                    desktop = payload / "usr" / "share" / "applications" / identity.linux_desktop_file
                    executable = "zzclawterm" if payload.name == "ZzClawTerm.AppDir" else f"/opt/{identity.desktop_id}/zzclawterm"
                    verify_native_package.verify_linux_desktop(desktop.read_text(), executable, str(desktop), identity)
                    for size in package_native.linux_icons.ICON_SIZES:
                        icon = payload / "usr/share/icons/hicolor" / size / "apps" / f"{identity.desktop_id}.png"
                        self.assertTrue(icon.is_file())
                        if os.name == "posix":
                            self.assertEqual(icon.stat().st_mode & 0o777, 0o644)
                    app_root = payload / "usr" / "bin" if payload.name == "ZzClawTerm.AppDir" else payload / "opt" / identity.desktop_id
                    for name in ("zzclawterm", *package_native.helper_bins("x86_64-unknown-linux-gnu")):
                        self.assertTrue((app_root / name).is_file())

    def test_release_tag_is_normalized(self) -> None:
        self.assertEqual(package_native.validate_version("v2.0.0"), "2.0.0")
        self.assertEqual(
            package_native.validate_version("2.0.0-preview.1"),
            "2.0.0-preview.1",
        )

    def test_invalid_or_mismatched_version_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            package_native.validate_version("release-2")
        with self.assertRaisesRegex(ValueError, "does not match"):
            package_native.validate_version("v2.0.1", "2.0.0")

    def test_snapshot_is_only_allowed_as_an_artifact_label(self) -> None:
        self.assertEqual(
            package_native.validate_artifact_version("continuous-build"),
            "continuous-build",
        )
        with self.assertRaises(ValueError):
            package_native.validate_version("continuous-build")
        with self.assertRaises(ValueError):
            package_native.validate_artifact_version("nightly")
        self.assertEqual(
            package_native.artifact_names(
                "x86_64-pc-windows-msvc", "continuous-build"
            ),
            {
                "ZzClawTerm_continuous-build_windows_x64_portable.zip",
                "ZzClawTerm_continuous-build_windows_x64-setup.exe",
            },
        )

    def test_all_release_targets_have_expected_artifact_names(self) -> None:
        expected = {
            "aarch64-apple-darwin": {
                "ZzClawTerm_2.0.0_macos_arm64.dmg",
                "ZzClawTerm_2.0.0_macos_arm64.app.tar.gz",
            },
            "x86_64-apple-darwin": {
                "ZzClawTerm_2.0.0_macos_x64.dmg",
                "ZzClawTerm_2.0.0_macos_x64.app.tar.gz",
            },
            "aarch64-unknown-linux-gnu": {
                "ZzClawTerm_2.0.0_linux_arm64.AppImage",
                "ZzClawTerm_2.0.0_linux_arm64.deb",
                "ZzClawTerm_2.0.0_linux_arm64.rpm",
            },
            "x86_64-unknown-linux-gnu": {
                "ZzClawTerm_2.0.0_linux_x64.AppImage",
                "ZzClawTerm_2.0.0_linux_x64.deb",
                "ZzClawTerm_2.0.0_linux_x64.rpm",
            },
            "aarch64-pc-windows-msvc": {
                "ZzClawTerm_2.0.0_windows_arm64_portable.zip",
                "ZzClawTerm_2.0.0_windows_arm64-setup.exe",
            },
            "x86_64-pc-windows-msvc": {
                "ZzClawTerm_2.0.0_windows_x64_portable.zip",
                "ZzClawTerm_2.0.0_windows_x64-setup.exe",
            },
        }
        for target, names in expected.items():
            with self.subTest(target=target):
                self.assertEqual(package_native.artifact_names(target, "v2.0.0"), names)

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
            [path.name for path in linux], list(package_native.helper_bins("x86_64-unknown-linux-gnu"))
        )
        self.assertEqual(
            [path.name for path in windows],
            [f"{name}.exe" for name in package_native.helper_bins("aarch64-pc-windows-msvc")],
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
                sorted(package_native.helper_bins(target)),
            )
            for path in copied:
                self.assertTrue(path.is_file())
                self.assertEqual(path.parent, destination)


    def test_linux_desktop_registers_stable_protocols(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "zzclawterm.desktop"
            package_native.write_desktop_file(path, "/opt/zzclawterm/zzclawterm")
            desktop = path.read_text(encoding="utf-8")
        self.assertIn("Exec=/opt/zzclawterm/zzclawterm %U\n", desktop)
        self.assertIn("MimeType=x-scheme-handler/zzclawterm;x-scheme-handler/ssh;x-scheme-handler/telnet;\n", desktop)

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
        self.assertEqual(package_native.windows_numeric_version("2.4.6-beta.1"), "2.4.6.1")
        self.assertEqual(package_native.linux_rpm_version("2.4.6"), ("2.4.6", "1"))
        self.assertEqual(
            package_native.linux_rpm_version("2.4.6-beta.1"),
            ("2.4.6", "0.beta.1"),
        )

    def test_windows_numeric_version_preserves_prerelease_revision(self) -> None:
        cases = {
            "2.0.0": "2.0.0.0",
            "2.0.0-preview.9": "2.0.0.9",
            "2.0.0-preview.10+build.123": "2.0.0.10",
            "2.0.0+build.123": "2.0.0.0",
            "2.0.0-beta": "2.0.0.0",
            "2.0.0-rc.4.extra": "2.0.0.0",
            "2.0.0-9": "2.0.0.9",
            "65535.65535.65535-preview.65535": "65535.65535.65535.65535",
        }
        for version, numeric in cases.items():
            with self.subTest(version=version):
                self.assertEqual(package_native.windows_numeric_version(version), numeric)

    def test_windows_numeric_version_rejects_word_overflow(self) -> None:
        for version in ("65536.0.0", "2.65536.0", "2.0.65536", "2.0.0-preview.65536"):
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, "65535"):
                package_native.windows_numeric_version(version)

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

    def test_inno_setup_7_compiler_is_found_in_program_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            compiler = Path(temporary) / "Inno Setup 7" / "ISCC.exe"
            compiler.parent.mkdir()
            compiler.touch()
            with (
                mock.patch.object(package_native.shutil, "which", return_value=None),
                mock.patch.dict(os.environ, {"ProgramFiles": temporary}),
            ):
                self.assertEqual(package_native.find_iscc(), str(compiler))

    def test_inno_setup_6_does_not_satisfy_the_packaging_requirement(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            compiler = Path(temporary) / "Inno Setup 6" / "ISCC.exe"
            compiler.parent.mkdir()
            compiler.touch()
            with (
                mock.patch.object(package_native.shutil, "which", return_value=None),
                mock.patch.dict(
                    os.environ,
                    {"ProgramFiles": temporary, "ProgramFiles(x86)": temporary},
                ),
            ):
                with self.assertRaisesRegex(RuntimeError, "Inno Setup 7"):
                    package_native.find_iscc()

    def test_windows_installer_invokes_iscc_with_release_parameters(self) -> None:
        target = "x86_64-pc-windows-msvc"
        info = package_native.target_info(target)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helpers = []
            for name in package_native.HELPER_BINS:
                fake = root / "build" / f"{name}.exe"
                fake.parent.mkdir(parents=True, exist_ok=True)
                fake.write_bytes(b"MZ")
                helpers.append(fake)
            for relative in package_native.conpty_files(target):
                path = root / "staging/resources/windows/conpty" / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"MZ")
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "DIST_DIR", root / "dist"),
                mock.patch.object(package_native.prepare_conpty, "STAGING", root / "staging"),
                mock.patch.object(package_native, "run") as run_mock,
                mock.patch.object(
                    package_native, "find_iscc", return_value="iscc"
                ),
                mock.patch.object(
                    package_native, "helper_binary_paths", return_value=helpers
                ),
            ):
                package_native.WORK_DIR.mkdir(parents=True)
                package_native.DIST_DIR.mkdir(parents=True)
                application = root / "zzclawterm.exe"
                application.write_bytes(b"MZ")
                package_native.create_windows_packages(
                    application, info, "0.0.1", "0.0.1"
                )
        args = run_mock.call_args.args[0]
        self.assertEqual(args[0], "iscc")
        defines = {arg for arg in args if arg.startswith("/D")}
        self.assertIn("/DVersion=0.0.1", defines)
        self.assertIn("/DNumericVersion=0.0.1.0", defines)
        self.assertIn("/DArch=x64", defines)
        self.assertTrue(any("windows-installer" in arg for arg in defines))
        self.assertTrue(
            any("ZzClawTerm_0.0.1_windows_x64-setup" in arg for arg in defines)
        )
        self.assertTrue(args[-1].endswith("windows-installer.iss"))

    def test_windows_installer_script_registers_only_the_zzclawterm_scheme(self) -> None:
        script = (
            package_native.ROOT_DIR / "scripts" / "release" / "windows-installer.iss"
        ).read_text(encoding="utf-8")
        self.assertIn('Source: "{#SourceDir}\\*.exe"', script)
        self.assertIn('Source: "{#SourceDir}\\conpty\\*"', script)
        self.assertIn('Source: "{#SourceDir}\\vcxsrv\\*"', script)
        self.assertIn(
            'Subkey: "Software\\Classes\\zzclawterm"; ValueType: string; '
            'ValueName: "URL Protocol"',
            script,
        )
        self.assertIn(
            'Subkey: "Software\\Classes\\zzclawterm\\shell\\open\\command"; '
            'ValueType: string; ValueData: """{app}\\ZzClawTerm.exe"" ""%1"""',
            script,
        )
        self.assertIn('uninsdeletekey', script)
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

    def test_stage_vcxsrv_fails_when_the_release_requires_a_distribution(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "package"
            destination.mkdir()
            with (
                mock.patch.dict(
                    "os.environ", {package_native.VCXSRV_REQUIRED_ENV_VAR: "1"}, clear=True
                ),
                mock.patch.object(package_native, "ROOT_DIR", root),
            ):
                with self.assertRaisesRegex(RuntimeError, "no VcXsrv distribution"):
                    package_native.stage_vcxsrv(destination)

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
            for relative in package_native.conpty_files(target):
                path = root / "staging/resources/windows/conpty" / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"MZ")
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "DIST_DIR", root / "dist"),
                mock.patch.object(package_native.prepare_conpty, "STAGING", root / "staging"),
                mock.patch.object(package_native, "run"),
                mock.patch.object(
                    package_native, "find_iscc", return_value="iscc"
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
                staged = package_native.WORK_DIR / "windows-installer" / "vcxsrv"
                staged_exe = (staged / package_native.VCXSRV_EXE).is_file()
                staged_fonts = (staged / "fonts" / "fonts.dir").is_file()
                portable = next(package_native.DIST_DIR.glob("*_portable.zip"))
                with zipfile.ZipFile(portable) as archive:
                    names = set(archive.namelist())
        # The installer script picks up the staged tree; its wildcard and
        # conditional vcxsrv entry are covered by the .iss content test.
        self.assertTrue(staged_exe)
        self.assertTrue(staged_fonts)
        self.assertIn("ZzClawTerm-portable/vcxsrv/vcxsrv.exe", names)
        self.assertIn("ZzClawTerm-portable/vcxsrv/NOTICE.txt", names)
        self.assertIn("ZzClawTerm-portable/vcxsrv/fonts/fonts.dir", names)

    def test_zip_entries_keep_the_executable_bit_without_host_state(self) -> None:
        self.assertEqual(package_native.archive_file_mode(0o100755), 0o755)
        self.assertEqual(package_native.archive_file_mode(0o100600), 0o644)

        executable = package_native.zip_member_info(
            "ZzClawTerm-portable/zzclawterm", 0o100755
        )
        self.assertEqual(executable.date_time, (1980, 1, 1, 0, 0, 0))
        # Pinned so the same tree hashes identically on Windows and on macOS.
        self.assertEqual(executable.create_system, 3)
        self.assertEqual(executable.external_attr >> 16, 0o100755)
        self.assertEqual(executable.compress_type, zipfile.ZIP_DEFLATED)

        plain = package_native.zip_member_info(
            "ZzClawTerm-portable/zzclawterm-portable", 0o100644
        )
        self.assertEqual(plain.external_attr >> 16, 0o100644)

        directory = package_native.zip_member_info("ZzClawTerm-portable/data/", 0o40755)
        self.assertTrue(directory.is_dir())
        self.assertEqual(directory.external_attr >> 16, 0o40755)
        self.assertEqual(directory.external_attr & 0x10, 0x10)
        self.assertEqual(directory.compress_type, zipfile.ZIP_STORED)

    def test_portable_zip_is_byte_reproducible(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = make_portable_package(root)
            restamp_tree(package, 1_000_000_000)
            first = root / "first-portable.zip"
            package_native.archive_zip(package, first)
            # A rerun unpacks a fresh artifact whose mtimes differ; the pinned
            # timestamps have to absorb that or latest.json's sha256 changes.
            restamp_tree(package, 1_600_000_000)
            second = root / "second-portable.zip"
            package_native.archive_zip(package, second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                names = archive.namelist()
                infos = archive.infolist()
                payload = archive.read("ZzClawTerm-portable/ZzClawTerm.exe")
        self.assertEqual(
            names,
            [
                "ZzClawTerm-portable/LICENSE",
                "ZzClawTerm-portable/VERSION",
                "ZzClawTerm-portable/ZzClawTerm.exe",
                "ZzClawTerm-portable/data/",
                "ZzClawTerm-portable/data/.keep",
                "ZzClawTerm-portable/vcxsrv/",
                "ZzClawTerm-portable/vcxsrv/fonts/",
                "ZzClawTerm-portable/vcxsrv/fonts/fonts.dir",
                "ZzClawTerm-portable/vcxsrv/vcxsrv.exe",
                "ZzClawTerm-portable/zzclawterm-portable",
            ],
        )
        self.assertEqual(payload, (b"MZ" + b"payload" * 128))
        for info in infos:
            with self.subTest(name=info.filename):
                self.assertEqual(info.date_time, package_native.ARCHIVE_DATE_TIME)
                self.assertEqual(info.create_system, 3)
                self.assertEqual(info.extra, b"")
                if info.is_dir():
                    self.assertEqual(info.external_attr >> 16, 0o40755)
                else:
                    self.assertIn(info.external_attr >> 16, (0o100644, 0o100755))
        # make_executable() cannot set a bit on Windows, where stat() reports
        # one from the .exe extension instead.
        executable = next(
            info for info in infos if info.filename.endswith("ZzClawTerm.exe")
        )
        self.assertEqual(executable.external_attr >> 16, 0o100755)

    def test_tar_entries_lose_host_state_but_keep_their_type(self) -> None:
        cases = [
            (tarfile.REGTYPE, 0o100755, 0o755),
            (tarfile.REGTYPE, 0o100644, 0o644),
            (tarfile.DIRTYPE, 0o40700, 0o755),
        ]
        for type_, mode, expected in cases:
            with self.subTest(type=type_, mode=oct(mode)):
                info = tarfile.TarInfo("ZzClawTerm.app/entry")
                info.type = type_
                info.mode = mode
                info.uid, info.gid = 501, 20
                info.uname, info.gname = "runner", "staff"
                info.mtime = 1_600_000_000
                normalized = package_native.tar_info_without_host_state(info)
                self.assertIs(normalized, info)
                self.assertEqual(normalized.mode, expected)
                self.assertEqual((normalized.uid, normalized.gid), (0, 0))
                self.assertEqual((normalized.uname, normalized.gname), ("", ""))
                self.assertEqual(normalized.mtime, package_native.ARCHIVE_MTIME)

        link = tarfile.TarInfo("ZzClawTerm.app/Contents/Frameworks/Current")
        link.type = tarfile.SYMTYPE
        link.linkname = "Versions/A"
        link.mode = 0o755
        link.uid, link.gid = 501, 20
        link.mtime = 1_600_000_000
        normalized_link = package_native.tar_info_without_host_state(link)
        self.assertTrue(normalized_link.issym())
        self.assertEqual(normalized_link.linkname, "Versions/A")
        self.assertEqual(normalized_link.mode, 0o777)
        self.assertEqual(normalized_link.mtime, package_native.ARCHIVE_MTIME)

    def test_macos_info_plist_uses_the_release_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "zzclawterm"
            binary.write_bytes(b"application")
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "copy_helpers", return_value=[]),
                mock.patch.object(package_native, "copy_release_documents"),
                mock.patch.object(
                    package_native,
                    "require_tool",
                    side_effect=RuntimeError("stop after writing Info.plist"),
                ),
            ):
                with self.assertRaisesRegex(RuntimeError, "stop after writing"):
                    package_native.create_macos_packages(
                        binary,
                        package_native.TARGETS["aarch64-apple-darwin"],
                        "0.2.1",
                        "0.2.1",
                    )
            plist_path = root / "work" / "ZzClawTerm.app" / "Contents" / "Info.plist"
            with plist_path.open("rb") as handle:
                plist = package_native.plistlib.load(handle)
        identity = package_native.release_identity("0.2.1")
        url_type = plist["CFBundleURLTypes"][0]
        self.assertEqual(url_type["CFBundleURLName"], identity.macos_identifier)
        self.assertEqual(url_type["CFBundleURLSchemes"], list(identity.url_schemes))

    def test_macos_tar_gz_is_byte_reproducible(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = make_app_bundle(root)
            restamp_tree(bundle, 1_000_000_000)
            first = root / "first.app.tar.gz"
            package_native.archive_tar_gz(bundle, first)
            restamp_tree(bundle, 1_600_000_000)
            second = root / "second.app.tar.gz"
            package_native.archive_tar_gz(bundle, second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            compressed = first.read_bytes()
            with tarfile.open(first, "r:gz") as archive:
                members = archive.getmembers()
                names = [member.name for member in members]
                binary = archive.extractfile(
                    f"{bundle.name}/Contents/MacOS/ZzClawTerm"
                ).read()
            expected_binary = (bundle / "Contents" / "MacOS" / "ZzClawTerm").read_bytes()
        # gzip stores an FNAME flag and an MTIME of its own next to the tar
        # headers; both must be absent or zero for the bytes to be reusable.
        self.assertEqual(compressed[3], 0)
        self.assertEqual(compressed[4:8], b"\0\0\0\0")
        self.assertEqual(binary, expected_binary)
        self.assertEqual(
            names,
            [
                "ZzClawTerm.app",
                "ZzClawTerm.app/Contents",
                "ZzClawTerm.app/Contents/Info.plist",
                "ZzClawTerm.app/Contents/MacOS",
                "ZzClawTerm.app/Contents/MacOS/ZzClawTerm",
                "ZzClawTerm.app/Contents/Resources",
                "ZzClawTerm.app/Contents/Resources/VERSION",
            ],
        )
        for member in members:
            with self.subTest(name=member.name):
                self.assertEqual(member.mtime, package_native.ARCHIVE_MTIME)
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertEqual((member.uname, member.gname), ("", ""))
                self.assertIn(member.mode, (0o644, 0o755))

    def test_macos_tar_gz_keeps_symlinks_and_their_targets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = make_app_bundle(root)
            current = bundle / "Contents" / "Frameworks" / "Current"
            current.parent.mkdir(parents=True)
            try:
                current.symlink_to("Versions/A")
            except (OSError, NotImplementedError) as error:
                self.skipTest(f"cannot create a symlink here: {error}")
            archive_path = root / "bundle.app.tar.gz"
            package_native.archive_tar_gz(bundle, archive_path)
            with tarfile.open(archive_path, "r:gz") as archive:
                members = {member.name: member for member in archive.getmembers()}
        link = members[f"{bundle.name}/Contents/Frameworks/Current"]
        self.assertEqual(link.type, tarfile.SYMTYPE)
        self.assertTrue(link.issym())
        self.assertEqual(link.linkname, "Versions/A")
        self.assertEqual(link.size, 0)

    def test_linux_desktop_registers_application_and_shell_url_schemes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "zzclawterm.desktop"
            package_native.write_desktop_file(path, "/opt/zzclawterm/zzclawterm")
            desktop = path.read_text(encoding="utf-8")
        self.assertIn("Exec=/opt/zzclawterm/zzclawterm %U\n", desktop)
        self.assertIn(
            "MimeType=x-scheme-handler/zzclawterm;x-scheme-handler/ssh;"
            "x-scheme-handler/telnet;\n",
            desktop,
        )
