#!/usr/bin/env python3
"""Prepare the pinned Microsoft ConPTY package for Windows releases."""

from __future__ import annotations

import hashlib
import json
import struct
import urllib.request
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
METADATA = Path(__file__).with_name("conpty-package.json")
PACKAGE_ID = "microsoft.windows.console.conpty"
STAGING = ROOT / "target" / "conpty-package"

FILES = (
    ("runtimes/win-x64/native/conpty.dll", "x64/conpty.dll", 0x8664),
    ("build/native/runtimes/x64/OpenConsole.exe", "x64/x64/OpenConsole.exe", 0x8664),
    ("build/native/runtimes/arm64/OpenConsole.exe", "x64/arm64/OpenConsole.exe", 0xAA64),
    ("runtimes/win-arm64/native/conpty.dll", "arm64/conpty.dll", 0xAA64),
    ("build/native/runtimes/arm64/OpenConsole.exe", "arm64/arm64/OpenConsole.exe", 0xAA64),
)


def pe_machine(data: bytes) -> int:
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError("missing PE DOS header")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if offset + 6 > len(data) or data[offset : offset + 4] != b"PE\0\0":
        raise ValueError("missing PE header")
    return struct.unpack_from("<H", data, offset + 4)[0]


def extract_verified(package: Path, output: Path) -> None:
    metadata = json.loads(METADATA.read_text(encoding="utf-8"))
    actual_hash = hashlib.sha256(package.read_bytes()).hexdigest()
    if actual_hash != metadata["sha256"]:
        raise ValueError(f"ConPTY package SHA-256 mismatch: {actual_hash}")
    with zipfile.ZipFile(package) as archive:
        extracted = {}
        for source, destination, machine in FILES:
            try:
                data = archive.read(source)
            except KeyError as error:
                raise ValueError(f"ConPTY package is missing {source}") from error
            actual_machine = pe_machine(data)
            if actual_machine != machine:
                raise ValueError(
                    f"ConPTY PE architecture mismatch for {source}: 0x{actual_machine:04x}"
                )
            extracted[destination] = data
    for destination, data in extracted.items():
        path = output / destination
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


def prepare() -> Path:
    metadata = json.loads(METADATA.read_text(encoding="utf-8"))
    version = metadata["version"]
    package = STAGING / f"{PACKAGE_ID}.{version}.nupkg"
    package.parent.mkdir(parents=True, exist_ok=True)
    if not package.is_file():
        url = f"https://api.nuget.org/v3-flatcontainer/{PACKAGE_ID}/{version}/{package.name}"
        with urllib.request.urlopen(url, timeout=60) as response:
            package.write_bytes(response.read())
    output = STAGING / "resources" / "windows" / "conpty"
    extract_verified(package, output)
    return output


if __name__ == "__main__":
    print(f"Prepared Microsoft.Windows.Console.ConPTY at {prepare()}")
