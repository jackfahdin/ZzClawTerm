from __future__ import annotations

import os
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


RELEASE_SCRIPTS = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE_SCRIPTS))

import package_native  # noqa: E402


def restamp_tree(root: Path, timestamp: float) -> None:
    """Give every path in root one mtime, as a fresh checkout would."""
    for path in [root, *root.rglob("*")]:
        os.utime(path, (timestamp, timestamp))


def make_portable_package(root: Path) -> Path:
    package = root / "ZzClawTerm-portable"
    (package / "data").mkdir(parents=True)
    (package / "vcxsrv" / "fonts").mkdir(parents=True)
    executable = package / "ZzClawTerm.exe"
    executable.write_bytes(b"MZ" + b"payload" * 128)
    package_native.make_executable(executable)
    (package / "LICENSE").write_bytes(b"Apache-2.0\n")
    (package / "VERSION").write_text("0.0.1\n", encoding="utf-8")
    (package / "data" / ".keep").touch()
    (package / "zzclawterm-portable").touch()
    (package / "vcxsrv" / "vcxsrv.exe").write_bytes(b"MZ")
    (package / "vcxsrv" / "fonts" / "fonts.dir").write_text("1\n", encoding="utf-8")
    return package


def make_app_bundle(root: Path) -> Path:
    bundle = root / "ZzClawTerm.app"
    macos_dir = bundle / "Contents" / "MacOS"
    resources_dir = bundle / "Contents" / "Resources"
    macos_dir.mkdir(parents=True)
    resources_dir.mkdir(parents=True)
    binary = macos_dir / "ZzClawTerm"
    binary.write_bytes(b"\xcf\xfa\xed\xfe" + b"code" * 256)
    package_native.make_executable(binary)
    (bundle / "Contents" / "Info.plist").write_bytes(b"<plist/>")
    (resources_dir / "VERSION").write_text("0.0.1\n", encoding="utf-8")
    return bundle


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
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "DIST_DIR", root / "dist"),
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
            with (
                mock.patch.object(package_native, "WORK_DIR", root / "work"),
                mock.patch.object(package_native, "DIST_DIR", root / "dist"),
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
