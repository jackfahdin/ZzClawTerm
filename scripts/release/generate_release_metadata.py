#!/usr/bin/env python3
"""Generate deterministic release checksums and download/update manifests."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

import package_native


DOWNLOAD_ARTIFACTS = {
    "windows-x86_64": "NyaTerm_{version}_windows_x64-setup.exe",
    "windows-aarch64": "NyaTerm_{version}_windows_arm64-setup.exe",
    "windows-x86_64-portable": "NyaTerm_{version}_windows_x64_portable.zip",
    "windows-aarch64-portable": "NyaTerm_{version}_windows_arm64_portable.zip",
    "linux-x86_64": "NyaTerm_{version}_linux_x64.AppImage",
    "linux-aarch64": "NyaTerm_{version}_linux_arm64.AppImage",
    "darwin-x86_64": "NyaTerm_{version}_macos_x64.dmg",
    "darwin-aarch64": "NyaTerm_{version}_macos_arm64.dmg",
}

UPDATER_ARTIFACTS = {
    "darwin-x86_64": "NyaTerm_{version}_macos_x64.app.tar.gz",
    "darwin-aarch64": "NyaTerm_{version}_macos_arm64.app.tar.gz",
    "linux-x86_64": "NyaTerm_{version}_linux_x64.AppImage",
    "linux-aarch64": "NyaTerm_{version}_linux_arm64.AppImage",
    "windows-x86_64": "NyaTerm_{version}_windows_x64-setup.exe",
    "windows-x86_64-nsis": "NyaTerm_{version}_windows_x64-setup.exe",
    "windows-aarch64": "NyaTerm_{version}_windows_arm64-setup.exe",
    "windows-aarch64-nsis": "NyaTerm_{version}_windows_arm64-setup.exe",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_artifacts(version: str) -> set[str]:
    names: set[str] = set()
    for target in package_native.TARGETS:
        names.update(package_native.artifact_names(target, version))
    return names


def artifact_url(base_url: str, tag: str, filename: str) -> str:
    return f"{base_url.rstrip('/')}/releases/{tag}/{filename}"


def generate(
    directory: Path,
    *,
    version: str,
    tag: str,
    base_url: str,
    notes: str,
    pub_date: str,
) -> tuple[dict[str, object], dict[str, object]]:
    version = package_native.validate_version(version)
    if tag != f"v{version}":
        raise ValueError(f"release tag {tag} does not match version {version}")

    expected = expected_artifacts(version)
    missing = sorted(name for name in expected if not (directory / name).is_file())
    if missing:
        raise RuntimeError(f"missing release artifacts: {', '.join(missing)}")

    hashes = {name: sha256(directory / name) for name in sorted(expected)}
    checksum_text = "".join(
        f"{digest}  {name}\n" for name, digest in hashes.items()
    )
    (directory / "SHA256SUMS").write_text(checksum_text, encoding="utf-8")

    downloads: dict[str, dict[str, str]] = {}
    for platform, template in DOWNLOAD_ARTIFACTS.items():
        filename = template.format(version=version)
        downloads[platform] = {
            "url": artifact_url(base_url, tag, filename),
            "sha256": hashes[filename],
        }

    updater: dict[str, dict[str, str]] = {}
    signature_cache: dict[str, str] = {}
    for platform, template in UPDATER_ARTIFACTS.items():
        filename = template.format(version=version)
        if filename not in signature_cache:
            signature_path = directory / f"{filename}.sig"
            if not signature_path.is_file():
                raise RuntimeError(f"missing updater signature: {signature_path.name}")
            signature = signature_path.read_text(encoding="utf-8").strip()
            if not signature:
                raise RuntimeError(f"empty updater signature: {signature_path.name}")
            signature_cache[filename] = signature
        updater[platform] = {
            "url": artifact_url(base_url, tag, filename),
            "signature": signature_cache[filename],
        }

    downloads_manifest: dict[str, object] = {
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": downloads,
    }
    updater_manifest: dict[str, object] = {
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": updater,
    }
    for name, manifest in (
        ("downloads.json", downloads_manifest),
        ("latest.json", updater_manifest),
    ):
        (directory / name).write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    return downloads_manifest, updater_manifest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=Path("dist-release"))
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--notes-file", type=Path, required=True)
    parser.add_argument("--pub-date", required=True)
    args = parser.parse_args(argv)

    directory = args.dist.resolve()
    if not directory.is_dir():
        print(f"release directory not found: {directory}", file=sys.stderr)
        return 1
    generate(
        directory,
        version=args.version,
        tag=args.tag,
        base_url=args.base_url,
        notes=args.notes_file.read_text(encoding="utf-8"),
        pub_date=args.pub_date,
    )
    print(f"Generated SHA256SUMS, downloads.json and latest.json in {directory}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
