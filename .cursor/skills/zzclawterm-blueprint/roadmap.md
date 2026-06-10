# ZzClawTerm 开发路线图

总工期约 14-16 个月。每个 Phase 必须可独立发布。

## Phase 1：高性能终端引擎（12 周）

| 周 | 模块 | 任务 |
|----|------|------|
| 1 | 基础设施 | 项目初始化、CMake、CI（三端） |
| 2 | 平台层 | PTY 适配（ConPTY/forkpty），三端启动 Shell |
| 3-4 | 核心 | VT 解析器（状态机 → 颜色/光标/滚动），正确显示 ls/htop |
| 5-6 | 渲染 | RHI 渲染引擎 + 纹理图集 + 实例化渲染，60fps |
| 7 | 日志 | 热层 Ring Buffer（1万行） |
| 8 | 日志 | 温层 mmap + LZ4（100万行无卡顿） |
| 9 | 日志 | 冷层 SQLite + ZSTD + FTS5（无限存储） |
| 10 | UI | 主窗口 + 标签页 + 分屏 |
| 11 | 性能 | SIMD + 零拷贝 + 多线程，达标 |
| 12 | 测试 | 性能基准 + Bug 修复 |

里程碑：高性能本地终端 + 无限日志，性能对标 WindTerm。

## Phase 2：SSH + SFTP（12 周）

libssh 协程封装 → 密钥管理/多认证 → Agent 转发/ProxyJump → SFTP 引擎 + 断点续传/并发 → SFTP 双面板 UI → 会话管理/连接池 → 端口转发（本地/远程/动态）→ 智能会话恢复 → 集成测试。

里程碑：替代 PuTTY + WinSCP + SecureCRT。

## Phase 3：X11 转发（16 周）

X11 协议研究/架构设计 → 集成 VcXsrv/XQuartz（进程外）→ X11 代理层（协议解析 + 安全过滤 + LZ4 压缩）→ 窗口嵌入 Qt 标签页 → 自研内嵌 X Server（kdrive 架构）→ X11 扩展（RENDER/SHM/XInput2）→ 剪贴板同步/多窗口 → 兼容性测试（GTK/Qt/Tk）。

里程碑：X11 转发对齐 MobaXterm，三端统一。

## Phase 4：差异化特性（16 周）

llama.cpp 集成 → 命令补全/错误诊断 → 自然语言转命令/云端 API → 插件系统框架 + API + 内置插件 → 宏录制/Fleet 模式 → 协作（WebRTC）→ 串口/Telnet + 审计日志。

里程碑：功能全面超越 MobaXterm + WindTerm。

## 立即行动项（第 1-2 周）

```
第1周：
□ 初始化仓库；配置 CMake；搭建 CI（GitHub Actions 三端）
□ 集成第三方库（Qt/libssh/FreeType/LZ4/ZSTD/SQLite）
□ 实现 ZzPtyInterface + Windows ConPTY 适配
□ 验证数据流：PTY → 读取 → 显示原始字节
第2周：
□ VT 解析器状态机框架 + 基础转义序列（颜色/光标）
□ RHI 渲染器原型（可先 QPainter 验证，后替换 GPU）
□ 验证：正确显示 ls --color 输出
```

## 测试与质量门禁

- 单元测试覆盖：VT 解析器/Ring Buffer/行偏移索引/无锁队列 ≥95%；mmap/Cold Storage/搜索/SSH ≥85-90%。
- 性能基准纳入 CI，回退 >5% 构建失败：VT 解析（ASCII ≥10GB/s，混合 ≥2GB/s）、整屏渲染 ≤2ms、日志写入 ≥1M 行/s、搜索 1000万行 ≤100ms、温层滚动 ≤5ms。
- 编译器：MSVC 2022(17.4+) / GCC 12+ / Clang 15+ / Apple Clang 15+。

## 打包

- Windows：NSIS 安装包 + 便携 ZIP。
- Linux：AppImage + deb + rpm + Flatpak。
- macOS：DMG + Homebrew Cask。
- 包体积：不含 AI/X11 ~45MB，含 X11 ~60MB，含 AI ~250MB-2GB（X Server / AI 模型按需下载）。
