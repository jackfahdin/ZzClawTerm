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


def fake_pe(machine: int) -> bytes:
    data = bytearray(512)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x84, machine)
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


def write_portable(path: Path, machine: int, *, helper_machine: int | None = None) -> None:
    """Build a portable zip whose layout matches package_native's output."""
    root = "ZzClawTerm-portable"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(f"{root}/ZzClawTerm.exe", fake_pe(machine))
        if helper_machine is not None:
            for name in verify_native_package.helper_filenames(
                "x86_64-pc-windows-msvc"
            ):
                archive.writestr(f"{root}/{name}", fake_pe(helper_machine))
        archive.writestr(f"{root}/zzclawterm-portable", b"")
        archive.writestr(f"{root}/LICENSE", b"license")
        archive.writestr(f"{root}/VERSION", b"0.0.1\n")
        archive.writestr(f"{root}/data/.keep", b"")


class VerifyNativePackageTests(unittest.TestCase):
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
                path, "x86_64-pc-windows-msvc", "0.0.1"
            )

    def test_windows_portable_rejects_wrong_architecture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0xAA64, helper_machine=0x8664)
            with self.assertRaisesRegex(RuntimeError, "PE machine"):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "0.0.1"
                )

    def test_windows_portable_requires_every_helper_binary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664)
            with self.assertRaisesRegex(RuntimeError, "zzclawterm-rdp-helper.exe"):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "0.0.1"
                )

    def test_windows_portable_rejects_helper_architecture_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "portable.zip"
            write_portable(path, 0x8664, helper_machine=0xAA64)
            with self.assertRaisesRegex(
                RuntimeError, "PE machine 0xaa64 for zzclawterm-rdp-helper.exe"
            ):
                verify_native_package.verify_windows_portable(
                    path, "x86_64-pc-windows-msvc", "0.0.1"
                )

    def test_macos_archive_validates_bundle_metadata_and_architecture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ZzClawTerm.app.tar.gz"
            entries = {
                "ZzClawTerm.app/Contents/MacOS/ZzClawTerm": fake_macho(0x0100000C),
                **{
                    f"ZzClawTerm.app/Contents/MacOS/{name}": fake_macho(0x0100000C)
                    for name in verify_native_package.helper_filenames(
                        "aarch64-apple-darwin"
                    )
                },
                "ZzClawTerm.app/Contents/Info.plist": plistlib.dumps(
                    {
                        "CFBundleIdentifier": "com.jackfahdin.zzclawterm",
                        "CFBundleShortVersionString": "0.0.1",
                        "CFBundleURLTypes": [
                            {"CFBundleURLSchemes": ["zzclawterm"]}
                        ],
                    }
                ),
                "ZzClawTerm.app/Contents/Resources/VERSION": b"0.0.1\n",
                "ZzClawTerm.app/Contents/Resources/LICENSE": b"license",
                "ZzClawTerm.app/Contents/Resources/icon.icns": b"icon",
            }
            with tarfile.open(path, "w:gz") as archive:
                for name, data in entries.items():
                    item = tarfile.TarInfo(name)
                    item.size = len(data)
                    archive.addfile(item, io.BytesIO(data))
            verify_native_package.verify_macos_archive(
                path, "aarch64-apple-darwin", "0.0.1"
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
        with self.assertRaisesRegex(RuntimeError, "Exec"):
            verify_native_package.verify_linux_desktop(
                desktop.replace(" %U", ""),
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
