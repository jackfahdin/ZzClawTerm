#!/usr/bin/env bash
# 构建 ZzClawTerm (Linux / macOS)
# 用法: scripts/build_unix.sh [Release|Debug]
set -euo pipefail

BUILD_TYPE="${1:-Release}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cmake -S "$ROOT" -B "$ROOT/build" -DCMAKE_BUILD_TYPE="$BUILD_TYPE"
cmake --build "$ROOT/build" --parallel

echo "构建完成: $ROOT/build"
