# ZzClawTerm

高性能跨平台终端工具，目标替代并超越 MobaXterm / WindTerm。Windows / Linux / macOS 三端统一。

## 核心特性 (MVP / Phase 1)

- 本地 Shell（Windows ConPTY、Linux/macOS forkpty）
- VT100 / xterm-256color / TrueColor 解析
- 高性能渲染（QPainter 原型，后续切 Qt RHI GPU 加速）
- 无限日志存储（热 RingBuffer → 温 mmap+LZ4 → 冷 SQLite+ZSTD+FTS5）+ 虚拟滚动
- 标签页、分屏、主题、配置持久化、基础搜索

后续阶段：SSH/SFTP（Phase 2）、X11 转发（Phase 3）、AI/插件/协作（Phase 4）。

## 技术栈

C++20 / Qt 6 / SQLite(FTS5)。可选：LZ4、ZSTD、FreeType、HarfBuzz。

## 构建

需要 Qt 6.5+ 与 CMake 3.25+。

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
```

可选功能开关：

```bash
cmake -B build -DZZ_ENABLE_LZ4=ON -DZZ_ENABLE_ZSTD=ON -DZZ_ENABLE_FREETYPE=ON -DZZ_ENABLE_TESTS=ON
```

使用 vcpkg 提供可选依赖时，加 `-DCMAKE_TOOLCHAIN_FILE=<vcpkg>/scripts/buildsystems/vcpkg.cmake`，
并按需启用 manifest feature（compression / freetype / tests）。

## 目录结构

```
src/
├── core/       终端引擎、渲染、日志、搜索
├── platform/   PTY 等平台适配
├── transport/  传输 (Phase 1 为本地 PTY)
├── session/    会话管理
├── ui/         界面层
├── business/   配置/主题
└── utils/      通用工具
```

代码规范：类名 `Zz` 前缀、四文件 Pimpl 结构、Doxygen 简体中文注释。详见 `.cursor/skills/`。

## 许可证

见 LICENSE。
