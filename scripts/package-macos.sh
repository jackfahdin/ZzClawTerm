#!/usr/bin/env bash
# ZzClawTerm macOS 打包：Release 构建 → macdeployqt 生成自包含 .app → DMG
# 用法：QT_ROOT=~/Qt/6.8.2/macos bash scripts/package-macos.sh
# 注意：本脚本在 macOS 平台人工验收时执行，当前未实机验证
# 已知限制：third_party/openssl（gitcode.com/ZzThirdParty/openssl）尚无 macOS 构建产物，
#           macOS 打包前需先补齐 OpenSSL macOS 构建，否则 Release 配置阶段即失败
set -euo pipefail
: "${QT_ROOT:?请通过 QT_ROOT 环境变量提供 Qt 前缀}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cmake --preset macos-clang-release
cmake --build --preset macos-clang-release

DIST="$ROOT/dist/macos"
rm -rf "$DIST"
mkdir -p "$DIST"
cp -R "$ROOT/build/macos-clang-release/src/ZzClawTerm.app" "$DIST/"

"$QT_ROOT/bin/macdeployqt" "$DIST/ZzClawTerm.app" -dmg \
    -always-overwrite -verbose=1
mv "$DIST/ZzClawTerm.dmg" "$ROOT/dist/ZzClawTerm-v0.1-macos.dmg"
echo "产出：dist/ZzClawTerm-v0.1-macos.dmg"
