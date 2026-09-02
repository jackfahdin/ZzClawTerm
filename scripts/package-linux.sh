#!/usr/bin/env bash
# ZzClawTerm Linux 打包：Release 构建 → linuxdeploy + qt 插件 → AppImage
# 依赖：linuxdeploy-x86_64.AppImage 与 linuxdeploy-plugin-qt-x86_64.AppImage
#       已放入 PATH（自行从 GitHub 发布页下载并 chmod +x）
# 用法：QT_ROOT=<Qt前缀，如 ~/Qt/6.11.1/gcc_64> bash scripts/package-linux.sh
# 注意：Qt 官方 gcc_64 的 imageformats/libqtiff.so 依赖 libtiff.so.5；较新发行版
#       （如 Ubuntu 24.04+ 仅提供 libtiff.so.6）需自行准备 libtiff5 并加入
#       LD_LIBRARY_PATH，否则 linuxdeploy-plugin-qt 会以 "Could not find dependency" 失败
set -euo pipefail
: "${QT_ROOT:?请通过 QT_ROOT 环境变量提供 Qt 前缀}"
# 版本号：默认 v0.1（手工用法不变）；CI 经 VERSION 环境变量注入
VERSION="${VERSION:-v0.1}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cmake --preset linux-gcc-release
cmake --build --preset linux-gcc-release

APPDIR="$ROOT/dist/linux/ZzClawTerm.AppDir"
rm -rf "$ROOT/dist/linux"
mkdir -p "$APPDIR/usr/bin"
cp "$ROOT/build/linux-gcc-release/src/ZzClawTerm" "$APPDIR/usr/bin/"

# 桌面入口与正式图标（由 scripts/generate-icons.py 从源图生成的多尺寸 PNG）
mkdir -p "$APPDIR/usr/share/applications"
for size in 16 24 32 48 64 128 256 512; do
    mkdir -p "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps"
    cp "$ROOT/resources/icons/generated/png/${size}.png" \
       "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/zzclawterm.png"
done
cat > "$APPDIR/usr/share/applications/zzclawterm.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=ZzClawTerm
Comment=Cross-platform SSH terminal
Exec=ZzClawTerm
Icon=zzclawterm
Categories=System;TerminalEmulator;
EOF

export QMAKE="$QT_ROOT/bin/qmake6"
export LD_LIBRARY_PATH="$QT_ROOT/lib:${LD_LIBRARY_PATH:-}"
linuxdeploy-x86_64.AppImage \
    --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/ZzClawTerm" \
    --desktop-file "$APPDIR/usr/share/applications/zzclawterm.desktop" \
    --plugin qt \
    --output appimage
mv ZzClawTerm-*.AppImage "$ROOT/dist/ZzClawTerm-${VERSION}-linux-x86_64.AppImage"
echo "产出：dist/ZzClawTerm-${VERSION}-linux-x86_64.AppImage"
