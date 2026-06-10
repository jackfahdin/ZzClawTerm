# ZzClawTerm 架构详解

## 五层职责

| 层 | 主要类 | 职责 |
|----|--------|------|
| UI (Presentation) | ZzMainWindow, ZzTerminalWidget, ZzX11Viewport, ZzSFTPPanel, ZzSessionTree, ZzSettingsDialog, ZzSearchBar, ZzTabBar, ZzSplitView, ZzStatusBar | 纯展示与交互，零业务逻辑 |
| 业务逻辑 (Business) | ZzSessionManager, ZzConnectionPool, ZzMacroEngine, ZzPluginManager, ZzAIAssistant, ZzConfigManager, ZzThemeManager, ZzKeyBindingManager, ZzFleetManager, ZzAuditLogger | 流程编排，不直接操作底层资源 |
| 核心引擎 (Core) | ZzTermCore(VT解析/文本缓冲/GPU渲染), ZzXServer(X11), ZzSFTPEngine, ZzLogEngine(三层存储), ZzSearchEngine(FTS5), ZzScriptEngine | 核心算法与数据处理 |
| 传输 (Transport) | ZzSSHTransport, ZzTelnetTransport, ZzSerialTransport, ZzPtyBackend, ZzRawTCPTransport | 协议实现与网络通信 |
| 平台适配 (PAL) | ZzPtyInterface, ZzAsyncIO, ZzFileWatcher, ZzNotification | 屏蔽平台差异，统一接口 |

## 平台适配矩阵

| 能力 | Windows | Linux | macOS |
|------|---------|-------|-------|
| PTY | ConPTY (Win10 1809+) / winpty 降级 | posix_openpt+grantpt | forkpty (util.h) |
| 异步 I/O | IOCP | io_uring(5.1+)/epoll | kqueue |
| 渲染后端 | Vulkan/D3D11 | Vulkan/OpenGL | Metal |
| X11 | VcXsrv 核心 | 系统 Xorg | XQuartz 核心 |
| 文件监控 | ReadDirectoryChangesW | inotify/fanotify | FSEvents/kqueue |
| 系统通知 | WinToast | libnotify/D-Bus | NSUserNotificationCenter |
| 密钥链 | Credential Manager | Secret Service(libsecret) | Keychain Services |

## 线程模型

- **主线程 (UI)**：Qt 事件循环、RHI 渲染、输入处理、信号分发。帧时间 ≤16ms，绝不阻塞。
- **I/O 线程池 (2-4)**：PTY/SSH 读取、VT 解析、写热缓冲区。→ 主线程用无锁队列。
- **归档线程池 (2-4)**：LZ4/ZSTD 压缩、mmap 写入、SQLite 批量插入、行偏移索引构建。任务队列，低优先级。
- **搜索线程 (1-2)**：全文索引更新、异步/并行分块搜索。Future/Promise 回调主线程。
- **X11 线程 (1)**：X11 协议处理、窗口事件、图像合成。共享纹理 + 信号通知。

## 数据流

```
输入：键盘 → 快捷键过滤 → 会话路由 → 传输层(SSH/PTY)
输出：传输层 → VT解析器(状态机) → 日志引擎(三层) → GPU渲染(RHI)
                                        └→ 搜索引擎(FTS5)
```

## 目录结构（核心部分）

```
ZzClawTerm/
├── CMakeLists.txt
├── cmake/                  # Find*.cmake, CompilerWarnings, PlatformSetup
├── src/
│   ├── main.cpp
│   ├── core/
│   │   ├── terminal/       # ZzTerminalCore, ZzVTParser, ZzTextBuffer
│   │   ├── log/            # ZzLogEngine, ZzRingBuffer, ZzMmapBuffer,
│   │   │                   #   ZzColdStorage, ZzLineIndex
│   │   ├── search/         # ZzSearchEngine
│   │   ├── render/         # ZzGpuRenderer, ZzGlyphCache, ZzTextureAtlas
│   │   └── x11/            # ZzXServer, ZzX11Protocol, ZzX11Compositor
│   ├── transport/          # ZzSSHTransport, ZzTelnetTransport,
│   │                       #   ZzSerialTransport, ZzSFTPEngine, ZzTransportInterface.h
│   ├── session/            # ZzSessionManager, ZzConnectionPool, ZzSessionConfig
│   ├── platform/           # ZzPtyInterface.h + Win/Unix 实现, ZzAsyncIO*,
│   │                       #   ZzFileWatcher, ZzNotification
│   ├── ui/                 # ZzMainWindow, ZzTerminalWidget, ZzTabBar, ZzSplitView,
│   │                       #   ZzSessionTree, ZzSFTPPanel, ZzX11Viewport, ZzSearchBar,
│   │                       #   ZzSettingsDialog, ZzStatusBar
│   ├── business/           # ZzConfigManager, ZzThemeManager, ZzKeyBindingManager,
│   │                       #   ZzMacroEngine, ZzFleetManager, ZzAuditLogger
│   ├── ai/                 # ZzAIAssistant, ZzLocalInference, ZzCloudInference,
│   │                       #   ZzCommandPredictor
│   ├── plugin/             # ZzPluginManager, ZzPluginInterface.h, ZzPluginContext
│   ├── collaboration/      # ZzCollabServer, ZzCollabClient
│   └── utils/              # ZzLockFreeQueue.h, ZzMemoryPool.h, ZzLRUCache.h,
│                           #   ZzThreadPool, ZzCrypto, ZzPath
├── resources/              # fonts/ themes/ shaders/ icons/ i18n/ config/
├── plugins/                # system-monitor, git-integration, docker-integration
├── tests/                  # unit/ integration/ benchmark/ fixtures/
├── docs/  scripts/  third_party/  .github/workflows/
├── .clang-format  .clang-tidy
```

每个自定义类 = 四文件：`ZzX.h`（公开声明）/ `ZzX.cpp`（公开实现）/ `ZzXPrivate.h`（Pimpl 私有声明）/ `ZzXPrivate.cpp`（私有实现）。抽象接口（如 `ZzPtyInterface.h`、`ZzTransportInterface.h`）只需头文件。

## 关键接口（签名要点）

- `ZzTransportInterface`：`QCoro::Task<bool> connectAsync()`、`write()`、`resize()`、能力查询 `supportsX11Forwarding/SFTP/PortForwarding`，信号 `dataReceived/stateChanged/errorOccurred/disconnected`。
- `ZzLogEngine`：`appendLine()`、`getLine()/getLines()`、`totalLines/totalBytes/diskUsage`、`QCoro::Task<...> searchAsync()`、`exportAsync()`、`cleanup()`。
- `ZzRendererInterface`：`initialize/shutdown`、`beginFrame/renderLine/renderCursor/renderSelection/endFrame`、`setFont/setColorScheme`、`markDirty/markAllDirty`。

## 核心数据结构

- `ZzCell`（~16B 对齐）：codepoint + 前景/背景色(Default/Indexed/RGB) + 属性位标志(Bold/Italic/Underline/Blink/Inverse/...) + width(1半角/2全角)。
- `ZzLogLine`：timestamp_ns + session_id + line_number + length + flags + 变长数据。
- `ZzSessionConfig`：name/group、protocol(LocalShell/SSH/Telnet/Serial/RawTCP)、host/port/username、authMethod、SSH 选项(X11/Agent/压缩/端口转发/ProxyJump)、终端选项、外观、高级(startupCommand/keepAlive/timeout)。
