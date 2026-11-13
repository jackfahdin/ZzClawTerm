#!/usr/bin/env python3
"""Build and package the native ZzClawTerm application for one Rust target."""

from __future__ import annotations

import os
import hashlib
import json
import plistlib
import re
import shutil
import subprocess
import sys
import tarfile
import textwrap
import time
import tomllib
import zipfile
from dataclasses import dataclass
from pathlib import Path

import prepare_conpty
import linux_icons
import windows_portable


ROOT_DIR = Path(__file__).resolve().parents[2]
DIST_DIR = ROOT_DIR / "dist"
WORK_DIR = ROOT_DIR / "target" / "native-package"
RESOURCE_DIR = ROOT_DIR / "crates" / "zzclawterm-app" / "resources"
ICON_DIR = RESOURCE_DIR / "icons"
LICENSE_PATH = ROOT_DIR / "LICENSE"
APP_NAME = "ZzClawTerm"
APP_BIN = "zzclawterm"
# Helper processes the application spawns at runtime. `resolve_helper_path()`
# in zzclawterm-remote-desktop only looks beside the running executable, so each
# supported helper must be packaged next to the application binary.
HELPER_BINS = (
    "zzclawterm-android-helper",
    "zzclawterm-rdp-helper",
    "zzclawterm-vnc-helper",
    "zzclawterm-mcp",
)
CONPTY_LICENSE = Path(__file__).with_name("conpty-license.txt")


def helper_bins(target: str) -> tuple[str, ...]:
    return HELPER_BINS


def conpty_files(target: str) -> tuple[Path, ...]:
    if target not in windows_portable.LAYOUT["conpty_files"]:
        raise ValueError(f"ConPTY is only packaged for Windows targets: {target}")
    return tuple(
        Path(name).relative_to("conpty")
        for name in windows_portable.LAYOUT["conpty_files"][target]
    )


def copy_conpty(destination: Path, target: str) -> list[Path]:
    source_root = prepare_conpty.STAGING / "resources" / "windows" / "conpty"
    copied = []
    for relative in conpty_files(target):
        source = source_root / relative
        if not source.is_file():
            raise FileNotFoundError(f"prepared ConPTY file not found: {source}")
        output = destination / "conpty" / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, output)
        copied.append(output)
    license_output = destination / "conpty" / "LICENSE.txt"
    shutil.copy2(CONPTY_LICENSE, license_output)
    copied.append(license_output)
    return copied


PORTABLE_MARKER = windows_portable.LAYOUT["marker"]
VCXSRV_ENV_VAR = "ZZCLAWTERM_VCXSRV_DIST"
VCXSRV_DIRNAME = "vcxsrv"
VCXSRV_EXE = "vcxsrv.exe"
# Releases that promise an X server set this: without it a missing distribution
# only warns, which would silently publish a package whose X11 forwarding cannot
# start. CI sets it for the Windows x64 leg.
VCXSRV_REQUIRED_ENV_VAR = "ZZCLAWTERM_REQUIRE_VCXSRV"
VCXSRV_NOTICE = textwrap.dedent(
    """
    VcXsrv Windows X Server
    =======================

    This directory contains VcXsrv, an X server for Windows licensed under the
    GNU General Public License version 3 (GPLv3). ZzClawTerm launches
    vcxsrv.exe as a separate, standalone process to display remote X11
    applications for SSH sessions with X11 forwarding; the ZzClawTerm
    executable does not link against or derive from VcXsrv.

    VcXsrv source code and license:
      https://sourceforge.net/projects/vcxsrv/
    """
).lstrip()



@dataclass(frozen=True)
class PackageIdentity:
    display_name: str
    macos_identifier: str
    windows_registry_name: str
    desktop_id: str

    @property
    def url_schemes(self) -> tuple[str, ...]:
        return (self.desktop_id, "ssh", "telnet") if self.desktop_id == "zzclawterm" else (self.desktop_id,)

    @property
    def macos_bundle_name(self) -> str:
        return f"{self.display_name}.app"

    @property
    def windows_registry_key(self) -> str:
        return rf"Software\{self.windows_registry_name}"

    @property
    def windows_uninstall_key(self) -> str:
        return rf"Software\Microsoft\Windows\CurrentVersion\Uninstall\{self.display_name}"

    @property
    def linux_desktop_file(self) -> str:
        return f"{self.desktop_id}.desktop"


STABLE_IDENTITY = PackageIdentity("ZzClawTerm", "com.kang.zzclawterm", "ZzClawTerm", "zzclawterm")
PREVIEW_IDENTITY = PackageIdentity(
    "ZzClawTerm Preview", "com.kang.zzclawterm.preview", "ZzClawTermPreview", "zzclawterm-preview"
)

# SemVer 2.0: prerelease numeric identifiers cannot have leading zeroes.
SEMVER = re.compile(
    r"(?P<major>0|[1-9][0-9]*)\.(?P<minor>0|[1-9][0-9]*)\.(?P<patch>0|[1-9][0-9]*)"
    r"(?:-(?P<pre>(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)


def parse_version(raw: str) -> re.Match[str]:
    match = SEMVER.fullmatch(normalize_version(raw))
    if match is None:
        raise ValueError(f"invalid release version: {raw}")
    return match


def release_identity(version: str) -> PackageIdentity:
    return PREVIEW_IDENTITY if parse_version(version).group("pre") else STABLE_IDENTITY


@dataclass(frozen=True)
class TargetInfo:
    target: str
    os_name: str
    arch: str

    @property
    def label(self) -> str:
        return f"{self.os_name}_{self.arch}"


TARGETS = {
    "aarch64-apple-darwin": TargetInfo("aarch64-apple-darwin", "macos", "arm64"),
    "x86_64-apple-darwin": TargetInfo("x86_64-apple-darwin", "macos", "x64"),
    "aarch64-unknown-linux-gnu": TargetInfo(
        "aarch64-unknown-linux-gnu", "linux", "arm64"
    ),
    "x86_64-unknown-linux-gnu": TargetInfo(
        "x86_64-unknown-linux-gnu", "linux", "x64"
    ),
    "aarch64-pc-windows-msvc": TargetInfo(
        "aarch64-pc-windows-msvc", "windows", "arm64"
    ),
    "x86_64-pc-windows-msvc": TargetInfo(
        "x86_64-pc-windows-msvc", "windows", "x64"
    ),
}


def run(
    args: list[str],
    *,
    cwd: Path = ROOT_DIR,
    env: dict[str, str] | None = None,
) -> None:
    print("+", " ".join(args), flush=True)
    subprocess.run(args, cwd=cwd, env=env, check=True)


def run_with_retry(args: list[str], *, attempts: int = 3, delay_seconds: float = 2.0) -> None:
    if attempts < 1:
        raise ValueError("attempts must be at least 1")
    for attempt in range(1, attempts + 1):
        try:
            run(args)
            return
        except subprocess.CalledProcessError:
            if attempt == attempts:
                raise
            time.sleep(delay_seconds)


def require_tool(name: str) -> str:
    path = shutil.which(name)
    if not path:
        raise RuntimeError(f"required packaging tool not found: {name}")
    return path


def workspace_version() -> str:
    with (ROOT_DIR / "Cargo.toml").open("rb") as handle:
        return tomllib.load(handle)["workspace"]["package"]["version"]


def normalize_version(raw: str) -> str:
    return raw.strip().removeprefix("v").removeprefix("V")


def validate_version(raw: str, expected: str | None = None) -> str:
    version = normalize_version(raw)
    parse_version(version)
    if expected is not None and version != expected:
        raise ValueError(
            f"release version {version} does not match Cargo workspace version {expected}"
        )
    return version


def validate_artifact_version(raw: str) -> str:
    """Validate a public filename label without changing package metadata."""
    value = raw.strip()
    if value == "continuous-build":
        return value
    return validate_version(value)


def target_info(target: str) -> TargetInfo:
    try:
        return TARGETS[target]
    except KeyError as error:
        raise ValueError(f"unsupported release target: {target}") from error


def artifact_names(target: str, version: str) -> set[str]:
    info = target_info(target)
    prefix = f"{APP_NAME}_{validate_artifact_version(version)}_{info.label}"
    if info.os_name == "macos":
        return {f"{prefix}.dmg", f"{prefix}.app.tar.gz"}
    if info.os_name == "linux":
        return {f"{prefix}.AppImage", f"{prefix}.deb", f"{prefix}.rpm"}
    return {f"{prefix}_portable.zip", f"{prefix}-setup.exe"}


def cargo_target_dir() -> Path:
    configured = os.environ.get("CARGO_TARGET_DIR")
    if not configured:
        return ROOT_DIR / "target"
    path = Path(configured)
    return path if path.is_absolute() else ROOT_DIR / path


def release_binary_path(target: str, name: str = APP_BIN) -> Path:
    suffix = ".exe" if "windows" in target else ""
    return cargo_target_dir() / target / "release" / f"{name}{suffix}"


def helper_binary_paths(target: str) -> list[Path]:
    return [release_binary_path(target, name) for name in helper_bins(target)]


def build_binary(package: str, name: str, target: str) -> Path:
    command = [
        "cargo", "build", "-p", package, "--bin", name,
        "--release", "--target", target, "--locked",
    ]
    environment = None
    if target.endswith("-windows-msvc"):
        # Keep Rust and every C/C++ dependency on the same static CRT. The ATL
        # shim must not make portable packages require a VC++ redistributable.
        environment = os.environ.copy()
        flags = environment.get("CARGO_ENCODED_RUSTFLAGS")
        if flags is None:
            flags = "\x1f".join(environment.get("RUSTFLAGS", "").split())
        environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(
            filter(None, [flags, "-C", "target-feature=+crt-static"])
        )
    run(command, env=environment)
    binary = release_binary_path(target, name)
    if not binary.is_file():
        raise FileNotFoundError(f"release executable not found: {binary}")
    return binary


def build_application(target: str) -> Path:
    import prepare_android
    _, prefix = prepare_android.prepare(target)
    if prefix is None:
        raise RuntimeError("Android decoder dependencies were not prepared")
    os.environ["FFMPEG_DIR"] = str(prefix)
    binary = build_binary("zzclawterm-app", APP_BIN, target)
    for name in helper_bins(target):
        build_binary(name, name, target)
    return binary


def make_executable(path: Path) -> None:
    path.chmod(path.stat().st_mode | 0o111)


def copy_helpers(destination: Path, target: str) -> list[Path]:
    """Copy helper binaries beside the application and return the copies."""
    copied = []
    for source in helper_binary_paths(target):
        binary = destination / source.name
        shutil.copy2(source, binary)
        if "windows" not in target:
            make_executable(binary)
        copied.append(binary)
    copy_android_resources(destination, target)
    return copied


def copy_android_resources(destination: Path, target: str) -> list[Path]:
    """Stage the same pinned resources used by development and CI."""
    import prepare_android
    adb = prepare_android.STAGING / "adb" / target
    ffmpeg = prepare_android.STAGING / "ffmpeg" / target
    if not adb.is_dir() or not ffmpeg.is_dir():
        raise FileNotFoundError("Run prepare_android.py before packaging")
    prepare_android.stage_runtime(adb, ffmpeg, destination)
    return prepare_android.write_resource_manifest(destination, target)


def resolve_vcxsrv_source() -> Path | None:
    """Locate the VcXsrv distribution tree to bundle with Windows packages.

    The ZZCLAWTERM_VCXSRV_DIST environment variable wins over the in-repo
    vendor/vcxsrv directory. A candidate that exists without vcxsrv.exe is a
    configuration error; no candidate at all means the package ships without
    an X server.
    """
    candidates: list[tuple[str, Path]] = []
    configured = os.environ.get(VCXSRV_ENV_VAR)
    if configured:
        candidates.append((f"${VCXSRV_ENV_VAR}", Path(configured)))
    candidates.append(("vendor directory", ROOT_DIR / "vendor" / VCXSRV_DIRNAME))
    for origin, candidate in candidates:
        if not candidate.is_dir():
            if origin.startswith("$"):
                raise RuntimeError(
                    f"{VCXSRV_ENV_VAR} does not point at a directory: {candidate}"
                )
            continue
        if not (candidate / VCXSRV_EXE).is_file():
            raise RuntimeError(
                f"VcXsrv source from {origin} is missing {VCXSRV_EXE}: {candidate}"
            )
        return candidate
    return None


def stage_vcxsrv(destination: Path) -> Path | None:
    """Copy the VcXsrv tree into destination/vcxsrv with a GPL NOTICE.txt.

    Returns the staged directory, or None (with a prominent warning) when no
    VcXsrv source is available, since packages without an X server are valid.
    """
    source = resolve_vcxsrv_source()
    if source is None:
        if os.environ.get(VCXSRV_REQUIRED_ENV_VAR):
            raise RuntimeError(
                f"{VCXSRV_REQUIRED_ENV_VAR} is set but no VcXsrv distribution "
                f"was found; set {VCXSRV_ENV_VAR} or populate "
                f"vendor/{VCXSRV_DIRNAME}/"
            )
        print(
            f"WARNING: no VcXsrv distribution found (set {VCXSRV_ENV_VAR} or "
            f"populate vendor/{VCXSRV_DIRNAME}/); packaging without an X server",
            flush=True,
        )
        return None
    staged = destination / VCXSRV_DIRNAME
    shutil.copytree(source, staged)
    (staged / "NOTICE.txt").write_text(VCXSRV_NOTICE, encoding="utf-8")
    print(f"==> Bundled VcXsrv from {source}", flush=True)
    return staged


def copy_release_documents(destination: Path, version: str) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    completion_licenses = ROOT_DIR / "crates/zzclawterm-core/assets/completion"
    android_licenses = ROOT_DIR / "crates" / "zzclawterm-android" / "assets"
    documents = [LICENSE_PATH, completion_licenses / "NOTICE.txt", completion_licenses / "LICENSE.carapace", completion_licenses / "LICENSE.nucleo",
                 android_licenses / "NOTICE.md", android_licenses / "LICENSE.scrcpy"]
    (destination / "LICENSE").write_text("\n\n".join(path.read_text(encoding="utf-8") for path in documents), encoding="utf-8")
    (destination / "VERSION").write_text(f"{version}\n", encoding="utf-8")


def reset_output() -> None:
    if DIST_DIR.exists():
        shutil.rmtree(DIST_DIR)
    DIST_DIR.mkdir()
    if WORK_DIR.exists():
        shutil.rmtree(WORK_DIR)
    WORK_DIR.mkdir(parents=True)


def archive_zip(source: Path, destination: Path) -> None:
    with zipfile.ZipFile(
        destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in sorted(source.rglob("*")):
            archive.write(path, path.relative_to(source.parent))


def windows_numeric_version(version: str) -> str:
    parsed = parse_version(version)
    # Keep in step with zzclawterm-app/build.rs. Build metadata never contributes;
    # a final numeric prerelease identifier distinguishes preview releases.
    prerelease = parsed.group("pre")
    revision = prerelease.rsplit(".", 1)[-1] if prerelease else ""
    numeric = [int(parsed.group(key)) for key in ("major", "minor", "patch")]
    numeric.append(int(revision) if revision.isascii() and revision.isdecimal() else 0)
    if any(part > 65535 for part in numeric):
        raise ValueError(f"Windows version component exceeds 65535: {version}")
    return ".".join(str(part) for part in numeric)


def innosetup_arch(target: str) -> str:
    if target.startswith("aarch64-"):
        return "arm64"
    return "x64"


def find_iscc() -> str:
    found = shutil.which("iscc") or shutil.which("iscc.exe") or shutil.which("ISCC.exe")
    if found:
        return found
    candidates = (
        Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
        / "Inno Setup 7"
        / "ISCC.exe",
        Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
        / "Inno Setup 7"
        / "ISCC.exe",
    )
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    raise RuntimeError("iscc not found; install Inno Setup 7 before packaging Windows")


def create_windows_packages(
    binary: Path, info: TargetInfo, version: str, artifact_version: str
) -> None:
    portable_root = WORK_DIR / "ZzClawTerm-portable"
    portable_root.mkdir()
    shutil.copy2(binary, portable_root / "ZzClawTerm.exe")
    copy_helpers(portable_root, info.target)
    copy_conpty(portable_root, info.target)
    stage_vcxsrv(portable_root)
    (portable_root / PORTABLE_MARKER).touch()
    (portable_root / "data").mkdir()
    (portable_root / "data" / ".keep").touch()
    copy_release_documents(portable_root, version)
    portable_output = (
        DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}_portable.zip"
    )
    archive_zip(portable_root, portable_output)

    installer_root = WORK_DIR / "windows-installer"
    installer_root.mkdir()
    shutil.copy2(binary, installer_root / "ZzClawTerm.exe")
    copy_helpers(installer_root, info.target)
    copy_conpty(installer_root, info.target)
    stage_vcxsrv(installer_root)
    copy_release_documents(installer_root, version)
    shutil.copy2(ICON_DIR / "icon.ico", installer_root / "icon.ico")

    output = DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}-setup.exe"
    installer_script = Path(__file__).resolve().with_name("windows-installer.iss")
    run(
        [
            find_iscc(),
            f"/DSourceDir={installer_root.resolve()}",
            f"/DVersion={version}",
            f"/DNumericVersion={windows_numeric_version(version)}",
            f"/DOutputDir={DIST_DIR.resolve()}",
            f"/DOutputName={output.stem}",
            f"/DArch={innosetup_arch(info.target)}",
            str(installer_script),
        ]
    )


def create_macos_packages(
    binary: Path, info: TargetInfo, version: str, artifact_version: str
) -> None:
    identity = release_identity(version)
    bundle = WORK_DIR / identity.macos_bundle_name
    macos_dir = bundle / "Contents" / "MacOS"
    resources_dir = bundle / "Contents" / "Resources"
    macos_dir.mkdir(parents=True)
    resources_dir.mkdir(parents=True)
    app_binary = macos_dir / "ZzClawTerm"
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    helper_binaries = copy_helpers(macos_dir, info.target)
    # The macOS artwork includes the transparent margins used in Dock and Launchpad.
    shutil.copy2(RESOURCE_DIR / "icons-macos" / "icon.icns", resources_dir / "icon.icns")
    copy_release_documents(resources_dir, version)

    plist = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": identity.display_name,
        "CFBundleExecutable": "ZzClawTerm",
        "CFBundleIconFile": "icon.icns",
        "CFBundleIdentifier": identity.macos_identifier,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": identity.display_name,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleURLTypes": [
            {
                "CFBundleTypeRole": "Viewer",
                "CFBundleURLName": identity.macos_identifier,
                "CFBundleURLSchemes": list(identity.url_schemes),
            }
        ],
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": "12.0",
        "NSHighResolutionCapable": True,
        "NSHumanReadableCopyright": "Copyright Kang",
    }
    with (bundle / "Contents" / "Info.plist").open("wb") as handle:
        plistlib.dump(plist, handle, sort_keys=True)

    codesign = require_tool("codesign")
    # Sign inside out. A bundle-only signature seals extra Mach-O files in
    # Contents/MacOS as resources instead of signing them as code, which makes
    # the helper fail to launch under Gatekeeper.
    android_code = [*macos_dir.glob("android-libs/*.dylib"), macos_dir / "adb" / "adb"]
    for helper in [*android_code, *helper_binaries]:
        run([codesign, "--force", "--sign", "-", "--timestamp=none", str(helper)])
    manifest_path = macos_dir / "android-resources.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["files"] = {name: hashlib.sha256((macos_dir / name).read_bytes()).hexdigest() for name in manifest["files"]}
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    run([codesign, "--force", "--deep", "--sign", "-", "--timestamp=none", str(bundle)])

    tar_output = DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}.app.tar.gz"
    with tarfile.open(tar_output, "w:gz", compresslevel=9) as archive:
        archive.add(bundle, arcname=identity.macos_bundle_name)

    dmg_root = WORK_DIR / "dmg"
    dmg_root.mkdir()
    shutil.copytree(bundle, dmg_root / identity.macos_bundle_name, symlinks=True)
    (dmg_root / "Applications").symlink_to("/Applications")
    dmg_output = DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}.dmg"
    run_with_retry(
        [
            require_tool("hdiutil"),
            "create",
            "-volname",
            identity.display_name,
            "-srcfolder",
            str(dmg_root),
            "-ov",
            "-format",
            "UDZO",
            str(dmg_output),
        ]
    )


def linux_deb_arch(target: str) -> str:
    return {
        "x86_64-unknown-linux-gnu": "amd64",
        "aarch64-unknown-linux-gnu": "arm64",
    }[target]


def linux_rpm_arch(target: str) -> str:
    return {
        "x86_64-unknown-linux-gnu": "x86_64",
        "aarch64-unknown-linux-gnu": "aarch64",
    }[target]


def linux_appimage_arch(target: str) -> str:
    return {
        "x86_64-unknown-linux-gnu": "x86_64",
        "aarch64-unknown-linux-gnu": "aarch64",
    }[target]


def linux_rpm_version(version: str) -> tuple[str, str]:
    parsed = parse_version(version)
    upstream = ".".join(parsed.group(key) for key in ("major", "minor", "patch"))
    prerelease = parsed.group("pre")
    if prerelease is None:
        return upstream, "1"
    normalized = re.sub(r"[^0-9A-Za-z]+", ".", prerelease).strip(".")
    return upstream, f"0.{normalized}"


def write_desktop_file(
    path: Path, executable: str, identity: PackageIdentity = STABLE_IDENTITY
) -> None:
    path.write_text(
        textwrap.dedent(
            f"""
            [Desktop Entry]
            Type=Application
            Name={identity.display_name}
            Comment=Native GPUI terminal and SSH client
            Exec={executable} %U
            Icon={identity.desktop_id}
            StartupWMClass={identity.display_name}
            Terminal=false
            Categories=Development;TerminalEmulator;Network;
            MimeType={"".join(f"x-scheme-handler/{scheme};" for scheme in identity.url_schemes)}
            StartupNotify=true
            """
        ).lstrip(),
        encoding="utf-8",
    )
    path.chmod(0o644)


def copy_linux_icons(root: Path, identity: PackageIdentity = STABLE_IDENTITY) -> None:
    linux_icons.copy_icons(root, ICON_DIR, identity.desktop_id)


def parse_dpkg_dependencies(output: str) -> str:
    prefix = "shlibs:Depends="
    for line in output.splitlines():
        if line.startswith(prefix) and line[len(prefix) :].strip():
            return line[len(prefix) :].strip()
    raise RuntimeError("dpkg-shlibdeps did not report shlibs:Depends")


def linux_deb_dependencies(binaries: list[Path]) -> str:
    scratch = WORK_DIR / "shlibdeps"
    debian = scratch / "debian"
    debian.mkdir(parents=True)
    (debian / "control").write_text(
        "Source: zzclawterm\nPackage: zzclawterm\nArchitecture: any\nDescription: ZzClawTerm\n",
        encoding="utf-8",
    )
    # Helpers can pull shared libraries the application itself does not need.
    command = [require_tool("dpkg-shlibdeps"), "-O"]
    for binary in binaries:
        command += ["-e", str(binary)]
    output = subprocess.check_output(command, cwd=scratch, text=True)
    return parse_dpkg_dependencies(output)


def create_linux_appimage(
    binary: Path, info: TargetInfo, version: str, artifact_version: str
) -> None:
    identity = release_identity(version)
    appdir = WORK_DIR / "ZzClawTerm.AppDir"
    usr_bin = appdir / "usr" / "bin"
    usr_bin.mkdir(parents=True)
    app_binary = usr_bin / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    copy_helpers(usr_bin, info.target)
    copy_release_documents(appdir / "usr" / "share" / "doc" / identity.desktop_id, version)

    applications = appdir / "usr" / "share" / "applications"
    applications.mkdir(parents=True)
    write_desktop_file(applications / identity.linux_desktop_file, APP_BIN, identity)
    shutil.copy2(applications / identity.linux_desktop_file, appdir / identity.linux_desktop_file)
    shutil.copy2(ICON_DIR / "128x128.png", appdir / f"{identity.desktop_id}.png")
    (appdir / f"{identity.desktop_id}.png").chmod(0o644)
    copy_linux_icons(appdir, identity)

    apprun = appdir / "AppRun"
    apprun.write_text(
        '#!/bin/sh\nAPPDIR="${APPDIR:-$(dirname "$(readlink -f "$0")")}"\n'
        'exec "$APPDIR/usr/bin/zzclawterm" "$@"\n',
        encoding="utf-8",
    )
    make_executable(apprun)
    linux_icons.normalize_permissions(appdir)

    output = DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}.AppImage"
    env = os.environ.copy()
    env["ARCH"] = linux_appimage_arch(info.target)
    env.setdefault("APPIMAGE_EXTRACT_AND_RUN", "1")
    run([require_tool("appimagetool"), str(appdir), str(output)], env=env)


def create_linux_deb(
    binary: Path, info: TargetInfo, version: str, artifact_version: str
) -> None:
    identity = release_identity(version)
    root = WORK_DIR / "deb"
    app_root = root / "opt" / identity.desktop_id
    app_root.mkdir(parents=True)
    app_binary = app_root / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    helper_binaries = copy_helpers(app_root, info.target)
    copy_release_documents(app_root, version)
    copy_release_documents(root / "usr" / "share" / "doc" / identity.desktop_id, version)

    applications = root / "usr" / "share" / "applications"
    applications.mkdir(parents=True)
    write_desktop_file(applications / identity.linux_desktop_file, f"/opt/{identity.desktop_id}/zzclawterm", identity)
    copy_linux_icons(root, identity)

    control_dir = root / "DEBIAN"
    control_dir.mkdir()
    linux_icons.write_postinst(control_dir / "postinst", identity.desktop_id)
    dependencies = linux_deb_dependencies([app_binary, *helper_binaries])
    (control_dir / "control").write_text(
        textwrap.dedent(
            f"""
            Package: {identity.desktop_id}
            Version: {version.replace('-', '~')}
            Section: utils
            Priority: optional
            Architecture: {linux_deb_arch(info.target)}
            Maintainer: Kang <noreply@zzclawterm.app>
            Depends: {dependencies}
            Recommends: libvulkan1 | mesa-vulkan-drivers
            Description: ZzClawTerm native GPUI terminal and SSH client
             Native terminal workspace with SSH, SFTP and remote operations.
            """
        ).lstrip(),
        encoding="utf-8",
    )
    output = DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}.deb"
    linux_icons.normalize_permissions(root)
    run([require_tool("dpkg-deb"), "--build", "--root-owner-group", str(root), str(output)])


def create_linux_rpm(
    binary: Path, info: TargetInfo, version: str, artifact_version: str
) -> None:
    identity = release_identity(version)
    rpm_root = WORK_DIR / "rpm"
    top_dir = rpm_root / "rpmbuild"
    payload = rpm_root / "payload"
    for directory in ("BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS"):
        (top_dir / directory).mkdir(parents=True)

    app_root = payload / "opt" / identity.desktop_id
    app_root.mkdir(parents=True)
    app_binary = app_root / APP_BIN
    shutil.copy2(binary, app_binary)
    make_executable(app_binary)
    copy_helpers(app_root, info.target)
    copy_release_documents(app_root, version)
    copy_release_documents(payload / "usr" / "share" / "doc" / identity.desktop_id, version)
    applications = payload / "usr" / "share" / "applications"
    applications.mkdir(parents=True)
    write_desktop_file(applications / identity.linux_desktop_file, f"/opt/{identity.desktop_id}/zzclawterm", identity)
    copy_linux_icons(payload, identity)
    linux_icons.normalize_permissions(payload)

    rpm_version, rpm_release = linux_rpm_version(version)
    payload_path = str(payload.resolve()).replace("%", "%%")
    spec = textwrap.dedent(
        f"""
        Name: {identity.desktop_id}
        Version: {rpm_version}
        Release: {rpm_release}
        Summary: ZzClawTerm native GPUI terminal and SSH client
        License: Apache-2.0
        URL: https://zzclawterm.app
        BuildArch: {linux_rpm_arch(info.target)}

        %description
        Native terminal workspace with SSH, SFTP and remote operations.

        %install
        rm -rf %{{buildroot}}
        mkdir -p %{{buildroot}}
        cp -a "{payload_path}/." %{{buildroot}}/

        %files
        /opt/{identity.desktop_id}
        /usr/share/applications/{identity.linux_desktop_file}
        /usr/share/icons/hicolor/*/apps/{identity.desktop_id}.png
        /usr/share/doc/{identity.desktop_id}
        """
    ).lstrip()
    spec_path = top_dir / "SPECS" / "zzclawterm.spec"
    spec_path.write_text(spec, encoding="utf-8")
    run(
        [
            require_tool("rpmbuild"),
            "--define",
            f"_topdir {top_dir.resolve()}",
            "--define",
            "_build_id_links none",
            "--define",
            "debug_package %{nil}",
            # Resource hashes cover the exact staged binaries. RPM's default
            # strip hooks otherwise rewrite ADB and FFmpeg after hashing.
            "--define",
            "__strip /bin/true",
            "--define",
            "__brp_strip %{nil}",
            "--define",
            "__brp_strip_static_archive %{nil}",
            "--define",
            "__brp_strip_comment_note %{nil}",
            "-bb",
            str(spec_path),
        ]
    )
    built = list((top_dir / "RPMS" / linux_rpm_arch(info.target)).glob("*.rpm"))
    if len(built) != 1:
        raise RuntimeError(f"expected one RPM artifact, found {len(built)}")
    shutil.copy2(
        built[0], DIST_DIR / f"{APP_NAME}_{artifact_version}_{info.label}.rpm"
    )


def create_linux_packages(
    binary: Path, info: TargetInfo, version: str, artifact_version: str
) -> None:
    create_linux_appimage(binary, info, version, artifact_version)
    create_linux_deb(binary, info, version, artifact_version)
    create_linux_rpm(binary, info, version, artifact_version)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: package_native.py <rust-target>")
    info = target_info(sys.argv[1])
    expected_version = workspace_version()
    version = validate_version(
        os.environ.get("ZZCLAWTERM_VERSION", expected_version), expected_version
    )
    artifact_version = validate_artifact_version(
        os.environ.get("ZZCLAWTERM_ARTIFACT_VERSION", version)
    )
    reset_output()
    print(f"==> Packaging {APP_NAME} {version} for {info.target}", flush=True)
    binary = build_application(info.target)

    if info.os_name == "macos":
        create_macos_packages(binary, info, version, artifact_version)
    elif info.os_name == "linux":
        create_linux_packages(binary, info, version, artifact_version)
    else:
        create_windows_packages(binary, info, version, artifact_version)

    actual = {path.name for path in DIST_DIR.iterdir() if path.is_file()}
    expected = artifact_names(info.target, artifact_version)
    if actual != expected:
        raise RuntimeError(
            f"package output mismatch: expected {sorted(expected)}, got {sorted(actual)}"
        )
    for path in sorted(DIST_DIR.iterdir()):
        print(f"==> {path.name} ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
