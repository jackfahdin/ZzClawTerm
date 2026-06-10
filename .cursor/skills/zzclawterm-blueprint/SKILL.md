---
name: zzclawterm-blueprint
description: ZzClawTerm 项目（高性能跨平台终端工具，替代并超越 MobaXterm/WindTerm）的技术蓝图与开发参考。当在本项目中进行架构设计、新增模块、选择开源依赖库、实现终端引擎/无限日志存储/X11转发/SSH-SFTP/GPU渲染/AI助手、规划开发阶段，或需要确认性能目标与目录结构时使用。
---

# ZzClawTerm 技术蓝图

> 本 skill 是 ZzClawTerm 的开发蓝本。任何架构决策、模块新增、依赖引入都应遵循本文档。代码风格（Zz 前缀、四文件 Pimpl、C++20、Doxygen 中文注释）遵循 `zz-qt6-cpp20-architect` skill。

## 一、项目定位

ZzClawTerm 是面向开发者/运维的下一代终端工具。五大核心竞争力：

1. **性能极致** — GPU 加速渲染（Qt RHI）+ 零拷贝 + SIMD，对标 WindTerm。
2. **无限日志** — 三层存储架构，日志永不丢失、永不卡顿（核心卖点）。
3. **X11 转发** — 三端统一的图形转发，对标 MobaXterm。
4. **AI 集成** — 命令补全、错误诊断、自然语言转命令。
5. **三端一致** — Windows / Linux / macOS 完全相同的 UI/UX。

技术栈：**C++20 / Qt 6.10+ / Qt RHI / libssh / FreeType + HarfBuzz / SQLite(FTS5) / LZ4 + ZSTD / QCoro / llama.cpp**。

目标平台：Windows 10+、Linux（X11/Wayland）、macOS 12+。

## 二、五层架构（必须遵守的分层）

```
UI 层 (Presentation)      纯展示与交互，零业务逻辑
   │ 信号/槽
业务逻辑层 (Business)      流程编排，不直接操作底层资源
   │
核心引擎层 (Core)         VT解析 / 日志 / 搜索 / 渲染 / X11Server
   │
传输层 (Transport)        SSH / Telnet / Serial / PTY / RawTCP
   │
平台适配层 (PAL)          屏蔽 Win/Linux/macOS 差异，统一接口
```

**铁律**：UI 层绝不含业务逻辑或数据模型访问；反之亦然。跨层只通过接口与信号/槽通信。

详细模块清单、目录结构、线程模型见 [architecture.md](architecture.md)。

## 三、关键设计决策

### 3.1 无限日志：三层存储（项目核心创新）

| 层 | 介质 | 容量 | 占用 | 特征 |
|----|------|------|------|------|
| 热 (Hot) | 内存 Ring Buffer | 1万行 | ~10MB 固定 | 完整 VT 属性，O(1) |
| 温 (Warm) | mmap + LZ4 | 100万行 | ~100MB | 分块压缩 + 行偏移索引 |
| 冷 (Cold) | SQLite + ZSTD + FTS5 | 无限 | 原始10-20% | 全文索引，按会话/时间分区 |

- **行偏移索引**（借鉴 klogg）：每 1024 行一个块级偏移，1000万行仅 ~80KB 索引内存。
- **虚拟滚动**：无论多少行，只渲染可见的 24-50 行。
- **三层并行搜索**：热层内存搜索最先返回，温层多线程分块，冷层 FTS5 毫秒级。
- 详见 [log-engine.md](log-engine.md)。

### 3.2 X11 转发：分阶段

1. **Phase 1**：进程外集成 VcXsrv(Win)/XQuartz(macOS)/系统 Xorg(Linux)。
2. **Phase 2**：X11 代理层（协议拦截 + 安全过滤 + LZ4 压缩）+ 窗口嵌入标签页。
3. **Phase 3**：自研内嵌 X Server（基于 kdrive/Xephyr 架构，后端用 Qt RHI 替代 DDX 驱动）。

### 3.3 性能目标（量化，CI 回归检测）

| 指标 | 目标 |
|------|------|
| 冷启动 / 热启动 | ≤500ms / ≤300ms |
| 数据吞吐 | ≥800MB/s |
| 渲染帧率 / 延迟 | ≥60fps / ≤16ms |
| 内存（空闲 / 1000万行） | ≤40MB / ≤100MB |
| 搜索（1000万行） | ≤100ms |

关键技术：零拷贝数据管道、SIMD 加速 VT 解析（AVX2/NEON）、增量渲染+脏区域追踪、纹理图集+实例化渲染（一次 draw call）、内存池、无锁队列、QCoro 协程异步 I/O。

### 3.4 渲染：Qt RHI（不直接用 OpenGLWidget）

自动适配 Vulkan(Linux/Win) / Metal(macOS) / D3D11(Win)，避免维护多套渲染代码。降级方案：QPainter 软件渲染。

## 四、开源依赖（引入前必读）

核心依赖（均允许闭源商业使用，仅需动态链接 LGPL 库并保留版权声明）：

| 库 | 版本 | 用途 | 协议 |
|----|------|------|------|
| Qt | 6.10+ | UI / RHI / 网络 / SQL | LGPL v3 |
| libssh | 0.10+ | SSH/SFTP | LGPL v2.1 |
| FreeType | 2.13+ | 字体光栅化 | FreeType License |
| HarfBuzz | 8.0+ | 文本整形 | MIT |
| SQLite | 3.44+ | 冷存储 + FTS5 | Public Domain |
| LZ4 | 1.9+ | 实时压缩（温层） | BSD-2 |
| ZSTD | 1.5+ | 归档压缩（冷层） | BSD-3 |
| QCoro | 0.10+ | C++20 协程 | MIT |
| spdlog | 1.12+ | 应用日志 | MIT |

平台/可选依赖（VcXsrv、XQuartz、winpty、liburing、llama.cpp、nlohmann/json、fmt、Catch2、Google Benchmark 等）及完整版本/协议/地址见 [dependencies.md](dependencies.md)。

**原则**：新增依赖必须确认协议允许闭源商业使用；优先选用社区活跃的库；避免引入功能重叠的多个库。

## 五、开发路线（四阶段，约 14-16 个月）

- **Phase 1（12周）高性能终端引擎**：PTY 适配 → VT 解析 → RHI 渲染 → 三层日志 → 基础 UI → 性能优化。里程碑：本地终端 + 无限日志，性能对标 WindTerm。
- **Phase 2（12周）SSH + SFTP**：libssh 协程封装 → 认证/Agent/ProxyJump → SFTP 双面板 → 连接池 → 端口转发 → 会话恢复。里程碑：替代 PuTTY+WinSCP。
- **Phase 3（16周）X11 转发**：进程外集成 → 代理层 → 窗口嵌入 → 自研内嵌 X Server。里程碑：对齐 MobaXterm。
- **Phase 4（16周）差异化**：AI 助手 → 插件系统 → 宏/Fleet → 协作 → 串口/Telnet/审计。里程碑：超越竞品。

每个 Phase 必须可独立发布。详细周计划见 [roadmap.md](roadmap.md)。

## 六、工作约定

- **新增类**：四文件结构 `ZzX.h / ZzX.cpp / ZzXPrivate.h / ZzXPrivate.cpp`，Pimpl 模式，放入对应分层目录（见 architecture.md 目录树）。
- **命名空间**：禁止链式 `a::b::c`，用传统嵌套，按功能分类（ZzCore/ZzLog/ZzWidgets 等）。
- **注释**：Doxygen 风格 + 简体中文。
- **测试**：核心算法单元测试覆盖率 ≥90%；关键路径性能基准测试纳入 CI，回退 >5% 则构建失败。
- **构建**：CMake 3.25+，C++20（MSVC 2022 / GCC 12+ / Clang 15+）。
- **安全**：凭证存入系统密钥链（Credential Manager / Keychain / Secret Service），AES-256 加密；X11 流量始终经 SSH 通道。

## 七、回应任务的格式

延续 `zz-qt6-cpp20-architect` 规范，按三段式回应：
1. **【架构思考与优化建议】**：先预判循环依赖、拷贝、线程安全、生命周期等问题。
2. **【文件与类设计清单】**：列出将创建/修改的文件及职责。
3. **【代码实现】**：标明完整路径 + 内容，严格遵循本蓝图与 Zz 规范。

> 口头禅：“遵循 Zz 规范，采用 Pimpl 与 C++20，为您构建清晰、稳固的 Qt6 应用。”
