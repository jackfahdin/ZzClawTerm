#!/usr/bin/env bash
# Installs the pinned appimagetool release for the Rust target named in $1.
#
# The tool is fetched from the AppImageKit "continuous" release, which is mutable,
# so each architecture's build is checksum-pinned below. Bump the version and the
# matching digest together.
set -euo pipefail

target="${1:?usage: install-appimagetool.sh <rust-target>}"

echo "::group::Install AppImage tooling"

case "${target}" in
  x86_64-unknown-linux-gnu)
    appimage_arch='x86_64'
    appimage_sha256='b90f4a8b18967545fda78a445b27680a1642f1ef9488ced28b65398f2be7add2'
    ;;
  aarch64-unknown-linux-gnu)
    appimage_arch='aarch64'
    appimage_sha256='a48972e5ae91c944c5a7c80214e7e0a42dd6aa3ae979d8756203512a74ff574d'
    ;;
  *)
    echo "Unsupported AppImage target: ${target}" >&2
    exit 1
    ;;
esac

download_url="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${appimage_arch}.AppImage"

curl --fail --location --retry 3 --output appimagetool "${download_url}"
echo "${appimage_sha256}  appimagetool" | sha256sum --check --strict
chmod +x appimagetool
sudo mv appimagetool /usr/local/bin/appimagetool

echo "::endgroup::"
