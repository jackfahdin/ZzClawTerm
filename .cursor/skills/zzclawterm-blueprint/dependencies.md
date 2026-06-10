# ZzClawTerm 开源依赖清单

所有依赖均允许闭源商业使用，只需动态链接 LGPL 库（Qt/libssh）并保留各库版权声明。

## 核心依赖

| 库 | 版本 | 用途 | 协议 | 地址 |
|----|------|------|------|------|
| Qt | 6.10+ | UI 框架、RHI 渲染、网络、文件系统、SQL | LGPL v3 / Commercial | https://www.qt.io |
| libssh | 0.10+ | SSH/SFTP 协议实现 | LGPL v2.1 | https://www.libssh.org |
| FreeType | 2.13+ | 字体光栅化 | FreeType License (BSD-like) | https://freetype.org |
| HarfBuzz | 8.0+ | 文本整形（复杂脚本） | MIT | https://harfbuzz.github.io |
| SQLite | 3.44+ | 冷数据存储、全文索引(FTS5) | Public Domain | https://sqlite.org |
| LZ4 | 1.9+ | 实时压缩（温数据层） | BSD-2-Clause | https://github.com/lz4/lz4 |
| ZSTD | 1.5+ | 高压缩比（冷数据层） | BSD-3-Clause | https://github.com/facebook/zstd |
| QCoro | 0.10+ | C++20 协程与 Qt 集成 | MIT | https://github.com/danvratil/qcoro |
| spdlog | 1.12+ | 高性能应用日志 | MIT | https://github.com/gabime/spdlog |

Qt 所需模块：`Core` `Gui` `Widgets` `OpenGL`(RHI 后端) `Network` `Sql`(SQLite 驱动) `Qml`(可选，未来 QML UI)。

## 平台特定依赖

| 库 | 平台 | 用途 | 协议 | 地址 |
|----|------|------|------|------|
| VcXsrv | Windows | X11 Server | MIT | https://github.com/ArcticaProject/vcxsrv |
| XQuartz | macOS | X11 Server | MIT | https://www.xquartz.org |
| winpty | Windows | ConPTY 降级方案(Win7/8) | MIT | https://github.com/rprichard/winpty |
| liburing | Linux | io_uring 封装 | LGPL / MIT | https://github.com/axboe/liburing |

## 可选依赖

| 库 | 用途 | 协议 | 地址 |
|----|------|------|------|
| llama.cpp | 本地 AI 推理引擎 | MIT | https://github.com/ggerganov/llama.cpp |
| nlohmann/json | JSON 解析（配置/主题） | MIT | https://github.com/nlohmann/json |
| fmt | 高性能字符串格式化 | MIT | https://github.com/fmtlib/fmt |
| Catch2 | 单元测试框架 | BSL-1.0 | https://github.com/catchorg/Catch2 |
| Google Benchmark | 性能基准测试 | Apache-2.0 | https://github.com/google/benchmark |
| libvterm | VT 解析参考/直接使用 | MIT | https://www.leonerd.org.uk/code/libvterm |
| Xephyr / kdrive | 自研 X Server 架构参考 | MIT | https://www.x.org |
| protobuf | 协作通信序列化 | BSD-3-Clause | https://github.com/protocolbuffers/protobuf |
| libwebrtc | 实时协作 P2P | BSD-3-Clause | https://webrtc.org |
| ICU | 复杂 Unicode 处理（备选） | Unicode License | https://icu.unicode.org |

## AI 本地模型选型

推理引擎统一用 **llama.cpp**（C++，CPU/GPU，跨平台）。

| 模型 | 参数量 | 内存 | 适用场景 |
|------|--------|------|----------|
| Phi-3 Mini | 3.8B | ~2GB | 命令补全、简单诊断 |
| Llama 3.2 3B | 3B | ~2GB | 通用对话 |
| CodeGemma 2B | 2B | ~1.5GB | 代码/命令专精 |

隐私：默认全部本地处理；云端 API 为可选、需用户明确开启。

## 引入新依赖的检查项

1. 协议是否允许闭源商业使用（LGPL 须动态链接）。
2. 是否三端可用。
3. 社区是否活跃、是否长期维护。
4. 是否与现有依赖功能重叠（避免重复）。
5. 对包体积的影响（目标：不含 AI/X11 ~45MB，含 X11 ~60MB）。
