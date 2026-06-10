# 构建指南

## 先决条件

- CMake ≥ 3.25
- C++20 编译器: MSVC 2022 / GCC 12+ / Clang 15+ / Apple Clang 15+
- Qt ≥ 6.5 (组件: Core, Gui, Widgets, Sql)

可选 (按需开启功能):
- vcpkg (提供 lz4 / zstd / freetype / harfbuzz / catch2 / benchmark)

## 基本构建

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
```

可执行文件位于 `build/`(Windows 为 `build/Release/`)。

## 功能开关

| 选项 | 默认 | 说明 |
|------|------|------|
| `ZZ_ENABLE_LZ4` | OFF | 温层 LZ4 压缩 |
| `ZZ_ENABLE_ZSTD` | OFF | 冷层 ZSTD 压缩 |
| `ZZ_ENABLE_FREETYPE` | OFF | FreeType 字形光栅化 (GPU 渲染路径) |
| `ZZ_ENABLE_TESTS` | OFF | 构建 Catch2 单元测试 |

示例 (启用全部, 经 vcpkg 提供依赖):

```bash
cmake -B build \
  -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake \
  -DVCPKG_MANIFEST_FEATURES="compression;freetype;tests" \
  -DZZ_ENABLE_LZ4=ON -DZZ_ENABLE_ZSTD=ON -DZZ_ENABLE_FREETYPE=ON -DZZ_ENABLE_TESTS=ON
cmake --build build --parallel
ctest --test-dir build
```

## 运行

```bash
./build/ZzClawTerm        # Windows: build\Release\ZzClawTerm.exe
```

启动后默认打开一个本地 Shell 标签页。常用快捷键:

- `Ctrl+T` 新建标签页
- `Ctrl+Shift+D` 水平分屏
- `Ctrl+Shift+F` 搜索日志

## 数据目录

会话日志数据库与配置位于系统应用数据目录 (Windows: `%APPDATA%/ZzClawTerm`,
Linux: `~/.local/share/ZzClawTerm`, macOS: `~/Library/Application Support/ZzClawTerm`)。
