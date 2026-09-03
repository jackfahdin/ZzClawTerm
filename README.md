# ZzClawTerm

[![ci](https://github.com/jackfahdin/ZzClawTerm/actions/workflows/ci.yml/badge.svg)](https://github.com/jackfahdin/ZzClawTerm/actions/workflows/ci.yml)
[![perf](https://github.com/jackfahdin/ZzClawTerm/actions/workflows/perf.yml/badge.svg)](https://github.com/jackfahdin/ZzClawTerm/actions/workflows/perf.yml)
[![release](https://github.com/jackfahdin/ZzClawTerm/actions/workflows/release.yml/badge.svg)](https://github.com/jackfahdin/ZzClawTerm/actions/workflows/release.yml)

基于 Qt 6 + C++20 的跨平台终端工具，目标是在功能与体验上对标并超越 MobaXterm / WindTerm。

差异化方向：

- 三平台（Windows / Linux / macOS）完全一致的 UI/UX
- 终端滚动历史超大容量存储且滚动不卡顿（现有终端的共同痛点）
- WindTerm 风格的 Dock 面板界面
- 接口先行的可扩展架构：传输协议与 Dock 面板均通过注册接口接入壳层，为后续插件体系铺路

## 功能特性

### 已实现

- 多标签终端（ZzTabManager），断线标签保留与手动重连
- SSH 会话：密码 / 公钥 / agent 认证（基于 ZzSshCore，libssh2 的 Qt 异步封装）
- SSH 端口转发：本地 -L / 远程 -R / 动态 -D（SOCKS5），规则绑定会话 profile、
  连接成功自动启动、断线销毁重连重建、单规则失败隔离（状态栏「隧道 N」指示 +
  失败瞬时提示），规则在会话编辑对话框的端口转发规则表中维护
- X11 forwarding：SSH 会话可转发 X11 图形程序，三端统一体验——Linux 直连系统 X server，
  macOS 使用 XQuartz；Windows 端内建 ZzXsrv（vcxsrv 21.1.16.1 + xtrans 回环绑定 patch，
  默认仅监听 127.0.0.1/::1）按需下载安装；每会话独立 X server 实例与 cookie
  （MIT-MAGIC-COOKIE-1 自动生成与清理），开关在会话编辑对话框的 X11 转发选项中维护
- 本地 shell 会话（本地 PTY 传输，验证终端层与协议层解耦）
- 会话管理：保存、树形分组、编辑、删除、双击连接（ZzSessionModel + ZzSessionPanel）
- 凭据存储：AES-256-GCM 加密 + 主密码（PBKDF2-HMAC-SHA256 60 万次迭代派生密钥，GCM tag 提供完整性认证）；
  可选系统密钥环后端（Linux libsecret 已实测，Windows Credential Manager / macOS Keychain 按平台 API 实现），
  支持 AES 文件到密钥环的迁移
- 终端分屏：标签内水平 / 垂直分屏（ZzSplitContainer），Ctrl+Shift+E/O 分屏、Ctrl+Shift+W 关窗格、
  Ctrl+Shift+方向键移焦点；多窗格断线重连覆盖全部断线窗格
- SFTP 侧边栏面板（ZzSftpPanel）：远程目录浏览、上传 / 下载（多选）、新建目录 / 删除 / 重命名，
  传输队列带进度与取消；库侧流水线传输实测上下行均超过系统 OpenSSH sftp
- 主机密钥验证（known_hosts 风格确认对话框）
- 三层滚动历史日志引擎（ZzLogEngine）：
  - 热层：内存环形缓冲
  - 温层：mmap 文件 + LZ4 压缩，I/O 失败自动降级为纯内存模式
  - 冷层：SQLite + ZSTD 持久化 + FTS5 全文搜索，跨会话保留历史
- 全局设置页（终端类型、编码、字号、配色）
- 状态栏四要素（连接状态 / 编码 / 终端尺寸 / 活动隧道数）+ 瞬时错误提示（错误不弹窗）
- WindTerm 风格 Dock 布局壳层（无边框窗口、Fluent 导航）
- 三平台打包脚本（Linux AppImage 已产出 v0.1 包）

### 路线图

| 里程碑 | 内容 |
| ------ | ---- |
| v0.2（已完成） | SFTP 侧边栏面板、终端分屏、系统密钥环凭据后端 |
| v0.3 | X11 forwarding 三端统一体验（Windows 魔改 vcxsrv，Linux/macOS 用系统 X server） |
| v0.4+ | 插件动态加载框架、串口、Telnet、宏 / 批量执行 |

## 技术栈

- **语言 / 构建**：C++20，CMake ≥ 3.25 + CMakePresets（Ninja / VS2022 生成器）
- **UI**：Qt 6（≥ 6.8，Core/Gui/Widgets/Network/Test/Xml/Multimedia）
- **终端组件**：ZzTermWidget（终端解析 + 绘制，含本地 pty）
- **应用框架**：ZzPureTools（无边框窗口、模块路由、导航、Fluent 主题）
- **SSH**：ZzSshCore（自研，libssh2 的 Qt 异步封装，worker 线程泵模型；libssh2 由其嵌套子模块提供，CMake 移植版）+ OpenSSL（vendored 预编译 bundle，缺失时回退系统 OpenSSL）
- **日志引擎**：LZ4（温层压缩）、ZSTD（冷层压缩）、SQLite amalgamation（冷层存储 + FTS5，vendored，Public Domain）

## 架构与目录结构

本仓库只做装配：复用 ZzTermWidget（终端）、ZzPureTools（框架）、ZzSshCore（SSH）三大件，自身承载会话模型、凭据存储、日志引擎与 Dock UI 胶水层。可扩展点走注册接口：

- `ZzTransportInterface`：传输协议抽象（SSH / 本地 PTY 已实现，未来协议走同一注册路径）
- `ZzPanelInterface`：Dock 面板抽象（会话面板、SFTP 面板）

```text
src/
  main.cpp            入口：框架 bootstrap → 注册传输协议 → 装配页面/导航
  ZzAppShell.*        组合根：持有会话模型/凭据库，装配窗口 Dock 与状态栏
  transport/          传输抽象与注册表、SSH 传输、本地 PTY 传输
  session/            会话模型（sessions.json）、凭据存储（AES-256-GCM）
  log/                ZzLogEngine 三层日志引擎（环形缓冲 / mmap+LZ4 / SQLite+ZSTD）
  terminal/           终端视图与滚动历史桥接（终端 ↔ 日志引擎）
  tab/                多标签管理（连接生命周期、重连）
  panel/              Dock 面板抽象与会话面板、会话编辑对话框
  dialog/             主机密钥确认、主密码对话框
  settings/           全局设置与设置页
tests/
  unit/               单元测试（Qt Test，离屏）
  session/            会话/凭据测试与性能测试
  log/                日志引擎测试与性能测试
  perf/               性能基线测试与历史记录（records/）
  mocks/              测试桩（ZzMockTransport）
docs/
  superpowers/specs/  设计规格（v0.1 总体设计、日志冷层、SSH 端口转发）
  superpowers/plans/  实现计划
  acceptance/         人工验收文档
scripts/              三平台打包脚本
third_party/          第三方依赖（见下文"子模块"）
```

## 构建

前置条件：

- Qt ≥ 6.8，通过环境变量 `QT_ROOT` 指向 Qt 前缀（如 `~/Qt/6.8.2/gcc_64`）
- CMake ≥ 3.25；Linux/macOS 用 Ninja，Windows 用 Visual Studio 2022
- 初始化子模块：`git submodule update --init --recursive`
- `third_party/sqlite` 为 vendored amalgamation（非 git 子模块），缺失时需按冷层实现计划任务 1 的步骤下载（配置阶段会给出明确报错指引）

以 Linux Debug 为例：

```bash
export QT_ROOT=~/Qt/6.8.2/gcc_64   # 按实际 Qt 安装路径调整
git submodule update --init --recursive
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
```

全部可用 preset（`CMakePresets.json`）：

| 平台 | Configure / Build / Test preset |
| ---- | ---- |
| Linux | `linux-gcc-debug`、`linux-gcc-release` |
| Windows | `windows-msvc2022-debug`、`windows-msvc2022-release` |
| macOS | `macos-clang-debug`、`macos-clang-release`（部署目标 macOS 13.3） |

构建产物在 `build/<preset>/src/ZzClawTerm`。

## 测试

测试默认随构建开启（`ZZCLAWTERM_BUILD_TESTS=ON`），基于 Qt Test + CTest：

```bash
ctest --preset linux-gcc-debug
```

test preset 与 configure preset 同名；失败时自动输出详细日志（`outputOnFailure`）。性能基线测试位于 `tests/perf/`，历史记录存于 `tests/perf/records/`。

## 打包

| 平台 | 脚本 | 产物 |
| ---- | ---- | ---- |
| Linux | `QT_ROOT=<Qt前缀> bash scripts/package-linux.sh` | `dist/ZzClawTerm-v0.1-linux-x86_64.AppImage`（依赖 linuxdeploy + qt 插件在 PATH 中） |
| macOS | `QT_ROOT=<Qt前缀> bash scripts/package-macos.sh` | 自包含 `.app` + DMG（macdeployqt） |
| Windows | `pwsh scripts/package-windows.ps1 [-QtRoot <Qt前缀>]` | zip 绿色包（windeployqt） |

注意：macOS / Windows 打包脚本当前未实机验证；macOS 打包前需先补齐 OpenSSL 的 macOS 构建产物。

## 子模块

`third_party/` 下的依赖（详见 `.gitmodules`）：

| 路径 | 说明 |
| ---- | ---- |
| `ZzTermWidget` | 终端组件（解析 + 绘制 + 本地 pty） |
| `ZzPureTools` | 应用框架（无边框窗口、路由、导航、Fluent 主题） |
| `ZzSshCore` | libssh2 的 Qt 异步封装（自研；libssh2 CMake 移植版为其嵌套子模块，静态构建，OpenSSL 加密后端） |
| `openssl` | vendored 预编译 bundle（缺失时回退系统 OpenSSL） |
| `lz4` / `zstd` | 日志引擎温层 / 冷层压缩（静态构建） |
| `sqlite` | SQLite amalgamation（vendored，非子模块，Public Domain） |

## 文档

- 设计规格：`docs/superpowers/specs/`（v0.1 总体设计、日志引擎冷层、SSH 端口转发）
- 实现计划：`docs/superpowers/plans/`
- 人工验收：`docs/acceptance/v0.1-manual-acceptance.md`

## 许可证

[MIT](LICENSE) — Copyright (c) 2026 jackfahdin

第三方依赖各自保留其原始许可证（SQLite 为 Public Domain）。
