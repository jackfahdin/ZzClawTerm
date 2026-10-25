from __future__ import annotations

import io
import plistlib
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

import verify_native_package  # noqa: E402
import package_native  # noqa: E402


def fake_pe(
    machine: int, *, subsystem: int = 3, resource_ids: tuple[int, ...] = ()
) -> bytes:
    data = bytearray(1024)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x84, machine)
    struct.pack_into("<H", data, 0x86, 1)
    struct.pack_into("<H", data, 0x94, 0xF0)
    optional = 0x98
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<H", data, optional + 68, subsystem)
    struct.pack_into("<I", data, optional + 108, 16)
    if resource_ids:
        struct.pack_into("<II", data, optional + 112 + 2 * 8, 0x1000, 0x100)
    section = optional + 0xF0
    data[section : section + 8] = b".rsrc\0\0\0"
    struct.pack_into("<IIII", data, section + 8, 0x100, 0x1000, 0x200, 0x200)
    if resource_ids:
        struct.pack_into("<HH", data, 0x200 + 12, 0, len(resource_ids))
        for index, resource_id in enumerate(resource_ids):
            struct.pack_into("<II", data, 0x210 + index * 8, resource_id, 0)
    return bytes(data)


def fake_macho(cpu_type: int) -> bytes:
    return b"\xcf\xfa\xed\xfe" + cpu_type.to_bytes(4, "little") + bytes(504)


def newc_entry(name: str, content: bytes) -> bytes:
    encoded_name = name.encode("utf-8") + b"\0"
    fields = [0, 0, 0, 0, 1, 0, len(content), 0, 0, 0, 0, len(encoded_name), 0]
    entry = b"070701" + b"".join(f"{value:08x}".encode() for value in fields)
    entry += encoded_name
    entry += bytes((-len(entry)) % 4)
    entry += content
    return entry + bytes((-len(entry)) % 4)


def write_portable(
    path: Path,
    machine: int,
    *,
    helper_machine: int | None = None,
    target: str = "x86_64-pc-windows-msvc",
    application_resources: tuple[int, ...] = (3, 14, 16, 24),
) -> None:
    """Build a portable zip whose layout matches package_native's output."""
    root = "ZzClawTerm-portable"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(
            f"{root}/ZzClawTerm.exe",
            fake_pe(machine, subsystem=2, resource_ids=application_resources),
        )
        if helper_machine is not None:
            for name in verify_native_package.helper_filenames(
                "x86_64-pc-windows-msvc"
            ):
                archive.writestr(f"{root}/{name}", fake_pe(helper_machine))
        archive.writestr(f"{root}/zzclawterm-portable", b"")
        archive.writestr(f"{root}/LICENSE", b"license")
        archive.writestr(f"{root}/VERSION", b"2.0.0\n")
        archive.writestr(f"{root}/data/.keep", b"")


def verify_installer_fixture(target: str, *, omit: tuple[str, ...] = ()) -> None:
    """Exercise verify_windows_installer with a simulated Inno silent install."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        installer = root / "setup.exe"
        installer.write_bytes(fake_pe(0x8664))

        def run(command, **kwargs):
            if Path(str(command[0])).name == "unins000.exe":
                return  # silent uninstall
            output = Path(command[4].removeprefix("/DIR="))
            output.mkdir(parents=True, exist_ok=True)
            entries = {
                "ZzClawTerm.exe": fake_pe(0x8664, subsystem=2, resource_ids=(3, 14, 16)),
                "LICENSE": b"license",
                "VERSION": b"2.0.0\n",
                "unins000.exe": fake_pe(0x8664),
                **{name: fake_pe(0x8664)
                   for name in verify_native_package.helper_filenames(target)},
            }
            for name, data in entries.items():
                if name in omit:
                    continue
                (output / name).write_bytes(data)

        with (
            mock.patch.object(verify_native_package.sys, "platform", "win32"),
            mock.patch.object(
                verify_native_package.platform, "machine", return_value="AMD64"
            ),
            mock.patch.object(
                verify_native_package.subprocess, "run", side_effect=run
            ),
        ):
            verify_native_package.verify_windows_installer(installer, target, "2.0.0")


class VerifyNativePackageTests(unittest.TestCase):
    def setUp(self) -> None:
        # Synthetic PE fixtures only contain directory headers. Test metadata
        # comparisons separately; the Windows resource build test uses real PEs.
        patcher = mock.patch.object(verify_native_package, "windows_version_info")
        self.version_info = patcher.start()
        self.addCleanup(patcher.stop)
        self.version_info.return_value = self.application_metadata("2.0.0")

    @staticmethod
    def application_metadata(version: str) -> dict[str, object]:
        metadata: dict[str, object] = {
            "FileDescription": "ZzClawTerm", "ProductName": "ZzClawTerm",
            "FileVersion": package_native.windows_numeric_version(version),
            "ProductVersion": version,
        }
        for prefix in ("File", "Product"):
            for part, value in zip(("Major", "Minor", "Build", "Private"),
                                   package_native.windows_numeric_version(version).split(".")):
                metadata[f"{prefix}{part}Part"] = int(value)
        return metadata

    def test_archive_paths_reject_parent_traversal_and_absolute_paths(self) -> None:
        for path in ("../secret", "dir/../../secret", "/absolute/file"):
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                verify_native_package.require_safe_archive_path(path)
        verify_native_package.require_safe_archive_path("ZzClawTerm/dir/file")

    def test_windows_portable_has_required_entries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664, helper_machine=0x8664)
            verify_native_package.verify_windows_portable(
                path, "x86_64-pc-windows-msvc", "2.0.0"
            )

    def test_windows_portable_rejects_application_without_version_resource(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664, helper_machine=0x8664, application_resources=(3, 14))
            with self.assertRaisesRegex(RuntimeError, "version resource"):
                verify_native_package.verify_windows_portable(path, "x86_64-pc-windows-msvc", "2.0.0")

    def test_windows_portable_rejects_application_version_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664, helper_machine=0x8664)
            self.version_info.return_value = self.application_metadata("2.0.1")
            with (
                mock.patch.object(verify_native_package.sys, "platform", "win32"),
                self.assertRaisesRegex(RuntimeError, "FileVersion"),
            ):
                verify_native_package.verify_windows_portable(path, "x86_64-pc-windows-msvc", "2.0.0")

    def test_windows_portable_rejects_wrong_architecture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0xAA64, helper_machine=0x8664)
            with self.assertRaisesRegex(RuntimeError, "PE machine"):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_windows_portable_requires_every_helper_binary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664)
            with self.assertRaisesRegex(RuntimeError, "zzclawterm-rdp-helper.exe"):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_windows_installer_silent_installs_and_verifies_payload(self) -> None:
        verify_installer_fixture("x86_64-pc-windows-msvc")

    def test_windows_installer_rejects_an_incomplete_install(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "missing installed files"):
            verify_installer_fixture(
                "x86_64-pc-windows-msvc", omit=("zzclawterm-rdp-helper.exe",)
            )

    def test_windows_installer_requires_a_windows_host(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            installer = Path(directory) / "setup.exe"
            installer.write_bytes(fake_pe(0x8664))
            with (
                mock.patch.object(verify_native_package.sys, "platform", "linux"),
                self.assertRaisesRegex(RuntimeError, "can only be verified on Windows"),
            ):
                verify_native_package.verify_windows_installer(
                    installer, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_windows_installer_skips_a_foreign_architecture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            installer = Path(directory) / "setup.exe"
            installer.write_bytes(fake_pe(0xAA64))
            with (
                mock.patch.object(verify_native_package.sys, "platform", "win32"),
                mock.patch.object(
                    verify_native_package.platform, "machine", return_value="AMD64"
                ),
                mock.patch.object(verify_native_package.subprocess, "run") as run_mock,
            ):
                verify_native_package.verify_windows_installer(
                    installer, "aarch64-pc-windows-msvc", "2.0.0"
                )
            run_mock.assert_not_called()

    def test_windows_portable_rejects_helper_architecture_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664, helper_machine=0xAA64)
            with self.assertRaisesRegex(
                RuntimeError, "PE machine 0xaa64 for zzclawterm-rdp-helper.exe"
            ):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_windows_application_requires_gui_subsystem_and_icon_resources(self) -> None:
        valid = fake_pe(0x8664, subsystem=2, resource_ids=(3, 14, 16, 24))
        verify_native_package.verify_windows_application_binary(valid, "test.exe")

        with self.assertRaisesRegex(RuntimeError, "GUI subsystem"):
            verify_native_package.verify_windows_application_binary(
                fake_pe(0x8664, subsystem=3, resource_ids=(3, 14)), "test.exe"
            )
        for resources in ((14,), (3,), (24,)):
            with self.subTest(resources=resources), self.assertRaisesRegex(
                RuntimeError, "icon resources"
            ):
                verify_native_package.verify_windows_application_binary(
                    fake_pe(0x8664, subsystem=2, resource_ids=resources),
                    "test.exe",
                )

    def test_windows_application_requires_version_resource_on_both_architectures(self) -> None:
        for machine in (0x8664, 0xAA64):
            with self.subTest(machine=machine), self.assertRaisesRegex(RuntimeError, "version resource"):
                verify_native_package.verify_windows_application_binary(
                    fake_pe(machine, subsystem=2, resource_ids=(3, 14)), "test.exe",
                )

    def test_windows_application_rejects_incorrect_version_strings_and_fixed_versions(self) -> None:
        with mock.patch.object(verify_native_package.sys, "platform", "win32"):
            for version in ("2.0.0", "2.0.0-preview.9"):
                expected = self.application_metadata(version)
                self.version_info.return_value = expected
                verify_native_package.verify_windows_application_version(Path("test.exe"), version)
                for key in expected:
                    with self.subTest(version=version, field=key):
                        self.version_info.return_value = {**expected, key: "wrong"}
                        with self.assertRaisesRegex(RuntimeError, key):
                            verify_native_package.verify_windows_application_version(Path("test.exe"), version)

    def test_macos_archive_validates_both_identities_and_rejects_mismatch(self) -> None:
        for version in ("2.0.0", "2.0.0-preview.1"):
            identity = verify_native_package.package_native.release_identity(version)
            bundle = identity.macos_bundle_name
            with self.subTest(version=version), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "ZzClawTerm.app.tar.gz"
                plist = {
                    "CFBundleIdentifier": identity.macos_identifier,
                    "CFBundleDisplayName": identity.display_name,
                    "CFBundleName": identity.display_name,
                    "CFBundleExecutable": "ZzClawTerm",
                    "CFBundleShortVersionString": version,
                    "CFBundleVersion": version,
                    "CFBundleURLTypes": [{"CFBundleURLSchemes": [identity.desktop_id]}],
                }
                entries = {
                    f"{bundle}/Contents/MacOS/ZzClawTerm": fake_macho(0x0100000C),
                    **{f"{bundle}/Contents/MacOS/{name}": fake_macho(0x0100000C)
                       for name in verify_native_package.helper_filenames("aarch64-apple-darwin")},
                    f"{bundle}/Contents/Info.plist": plistlib.dumps(plist),
                    f"{bundle}/Contents/Resources/VERSION": f"{version}\n".encode(),
                    f"{bundle}/Contents/Resources/LICENSE": b"license",
                    f"{bundle}/Contents/Resources/icon.icns": b"icon",
                }

    def test_windows_portable_rejects_a_vcxsrv_tree_without_executable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            root = "ZzClawTerm-portable"
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr(f"{root}/ZzClawTerm.exe", fake_pe(0x8664))
                for name in verify_native_package.helper_filenames(
                    "x86_64-pc-windows-msvc"
                ):
                    archive.writestr(f"{root}/{name}", fake_pe(0x8664))
                archive.writestr(f"{root}/zzclawterm-portable", b"")
                archive.writestr(f"{root}/LICENSE", b"license")
                archive.writestr(f"{root}/VERSION", b"2.0.0\n")
                archive.writestr(f"{root}/data/.keep", b"")
                archive.writestr(f"{root}/vcxsrv/NOTICE.txt", b"notice")
            with self.assertRaisesRegex(RuntimeError, "vcxsrv/vcxsrv.exe"):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "2.0.0"
                )

    def test_macos_scheme_validation_rejects_extra_protocols(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "must register only"):
            verify_native_package.verify_macos_url_scheme(
                {
                    "CFBundleURLTypes": [
                        {"CFBundleURLSchemes": ["zzclawterm", "ssh", "telnet"]}
                    ]
                },
                "Info.plist",
            )

    def test_linux_desktop_validation_requires_zzclawterm_scheme_and_percent_u(
        self,
    ) -> None:
        desktop = "\n".join(
            (
                "[Desktop Entry]",
                "Type=Application",
                "Name=ZzClawTerm",
                "Icon=zzclawterm",
                "StartupWMClass=zzclawterm",
                "Exec=/opt/zzclawterm/zzclawterm %U",
                "MimeType=x-scheme-handler/zzclawterm;",
            )
        )
        verify_native_package.verify_linux_desktop(
            desktop, "/opt/zzclawterm/zzclawterm", "zzclawterm.desktop"
        )
        with self.assertRaisesRegex(RuntimeError, "MimeType"):
            verify_native_package.verify_linux_desktop(
                desktop.replace(
                    "x-scheme-handler/zzclawterm;",
                    "x-scheme-handler/zzclawterm;x-scheme-handler/ssh;",
                ),
                "/opt/zzclawterm/zzclawterm",
                "zzclawterm.desktop",
            )
    def test_rpm_member_reader_extracts_desktop_from_newc_payload(self) -> None:
        desktop = b"MimeType=x-scheme-handler/zzclawterm;\n"
        payload = newc_entry(
            "./usr/share/applications/zzclawterm.desktop", desktop
        ) + newc_entry("TRAILER!!!", b"")
        with (
            mock.patch.object(
                verify_native_package.shutil, "which", return_value="rpm2cpio"
            ),
            mock.patch.object(
                verify_native_package.subprocess,
                "check_output",
                return_value=payload,
            ),
        ):
            actual = verify_native_package.read_rpm_member(
                Path("zzclawterm.rpm"), "/usr/share/applications/zzclawterm.desktop"
            )
        self.assertEqual(actual, desktop)

    def test_release_verification_fails_before_platform_tools_when_asset_missing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "missing release artifacts"):
                verify_native_package.verify_release(
                    Path(directory), "x86_64-unknown-linux-gnu", "0.0.1"
                )

    def test_binary_header_helpers_reject_invalid_formats(self) -> None:
        with self.assertRaises(RuntimeError):
            verify_native_package.pe_machine(b"not-pe")
        with self.assertRaises(RuntimeError):
            verify_native_package.elf_machine(b"not-elf")
        with self.assertRaises(RuntimeError):
            verify_native_package.macho_cpu_type(b"not-macho")


if __name__ == "__main__":
    unittest.main()
