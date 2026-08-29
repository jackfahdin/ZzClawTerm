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

# 桌面入口与图标（图标沿用占位 PNG，正式图标随 UI 资源任务补充）
mkdir -p "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
# 占位图标（base64 内嵌 256x256 PNG，正式图标随 UI 资源任务补充后替换此处）
cat <<'B64' | base64 -d > "$APPDIR/usr/share/icons/hicolor/256x256/apps/zzclawterm.png"
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
B64
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
