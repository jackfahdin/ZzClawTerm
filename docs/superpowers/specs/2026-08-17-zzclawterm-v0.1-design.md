# ZzClawTerm v0.1 设计规格

## 文档信息

| 项目 | 内容 |
| ---- | ---- |
| 项目名称 | ZzClawTerm |
| 文档版本 | v0.1 设计规格 |
| 日期 | 2026-08-17 |
| 定位 | 跨平台终端工具（对标并超越 MobaXterm / WindTerm） |
| 分发模式 | 先自用 / 闭源 |
| 目标平台 | Windows / Linux / macOS，三平台同等投入 |

---

## 一、项目定位与目标

ZzClawTerm 是基于 Qt 6 与 C++20 的跨平台终端工具，复用已有的 ZzTermWidget（终端组件）、ZzPureTools（应用框架）与 libssh2（CMake 移植版），目标是在功能与体验上对标并超越 MobaXterm 和 WindTerm。

差异化方向：

- 三平台（Windows / Linux / macOS）完全一致的 UI/UX
- 终端滚动历史无上限存储且滚动不卡顿（现有终端的共同痛点）
- X11 forwarding 三端统一体验（v0.3，Windows 魔改 vcxsrv，Linux/macOS 用系统 X server）
- WindTerm 风格的 Dock 面板界面

## 二、v0.1 范围

### 2.1 v0.1 包含

- 多标签 SSH 终端（密码 / 公钥 / agent 认证）
- 会话管理：保存、分组（树形）、编辑、删除、双击连接
- 凭据存储：AES-256 加密 + 主密码
- 本地 shell 会话（复用 ZzTermWidget 内置 ptyqt，验证终端层与协议层解耦）
- 断线标签保留与手动重连
- 主机密钥验证（known_hosts 风格）
- 超大滚动历史（ZzLogEngine 热层 + 温层，100 万行级不卡顿、不丢失；真正无上限待 v0.2 冷层落地）
- 全局设置页（终端类型、编码、字号、配色）
- WindTerm 风格 Dock 布局壳层
- 三平台可执行包

### 2.2 v0.1 不包含（后续里程碑）

| 里程碑 | 内容 |
| ------ | ---- |
| v0.2 | SFTP 侧边栏面板、端口转发、终端分屏、ZzLogEngine 冷层（SQLite + FTS5）、系统密钥环凭据后端 |
| v0.3 | X11 forwarding 打通 + vcxsrv 魔改（独立子项目，届时单独走设计流程） |
| v0.4+ | 插件动态加载框架、串口、Telnet、宏 / 批量执行 |

### 2.3 插件系统策略：接口先行，框架后置

v0.1 不实现 QPluginLoader 动态加载，但所有可扩展点（传输协议、Dock 面板）都通过注册接口接入壳层：

- `ZzTransportInterface`：传输协议抽象（SSH / 本地 PTY 都实现它）
- `ZzPanelInterface`：Dock 面板抽象（会话面板、未来的 SFTP 面板）

内部模块与未来第三方插件走同一条注册路径，v0.4+ 引入插件框架时只是新增一种模块来源，不改架构。

## 三、总体架构

```text
┌─────────────────────────────────────────────┐
│  ZzClawTerm（本仓库，只做装配）               │
│  ├─ 会话模型（ZzSessionModel）               │
│  ├─ 凭据存储（ZzCredentialStore）            │
│  ├─ 日志引擎（ZzLogEngine）                  │
│  ├─ 多标签 / Dock UI（WindTerm 风格）        │
│  └─ 胶水：ZzSshCore channel ↔ ZzTermWidget  │
├─────────────────────────────────────────────┤
│  ZzPureTools（已有）                         │
│  无边框窗口、模块路由、导航、Fluent 主题       │
├──────────────────┬──────────────────────────┤
│  ZzTermWidget    │  ZzSshCore（新建仓库）     │
│  终端解析 + 绘制   │  libssh2 的 Qt 异步封装    │
├──────────────────┴──────────────────────────┤
│  libssh2（CMake 移植版，add_subdirectory）   │
└─────────────────────────────────────────────┘
```

仓库组织：

- 新建 `ZzSshCore` 独立仓库（远端 `gitcode.com/JackfahdinQt/ZzSshCore`）：CMake `add_subdirectory` 引入（与 ZzTermWidget 的引入方式一致），只依赖 Qt Core/Network，不依赖 Widgets，可在无 GUI 环境（CI、Docker）中做集成测试
- 本仓库 `ZzClawTerm`：应用仓库，通过 `add_subdirectory`（git submodule）引入 ZzSshCore、ZzTermWidget、ZzPureTools 与 libssh2
- 依赖方向严格单向：应用 → ZzSshCore → libssh2；应用 → ZzTermWidget。SSH 层与终端层互相无感知，靠应用层做字节流转发——换协议层（libssh）或换终端组件都不影响另一边

前后端分离：ZzSshCore、ZzLogEngine、会话模型均为纯 Qt Core 后端库；UI 层只通过信号槽 / QCoro 包装消费它们，不直接触碰领域逻辑。

## 四、ZzSshCore 设计

### 4.1 IO 模型：每连接一个工作线程，libssh2 阻塞模式

- 每个 SSH 连接独占一个 `QThread`，握手、认证、channel 读写在该线程内串行执行
- GUI 线程与 SSH 线程之间只通过 Qt 信号槽（queued connection）通信
- 满足 libssh2 的线程安全边界（同一 session 不可并发调用）
- 选择阻塞模式的理由：非阻塞模式下几乎每个 API 都可能返回 `LIBSSH2_ERROR_EAGAIN`，每个操作都要写成可重入状态机，代码量与调试成本数倍于线程方案
- 取消与断开：阻塞在读上时，通过关闭底层 socket 或 `libssh2_session_disconnect` 中断

备选方案已评估并否决：非阻塞 + QSocketNotifier（EAGAIN 状态机复杂度过高）、单 IO 线程多路复用（等于手写半个 libevent）。

### 4.2 API 形态

QObject + 信号槽，GUI 线程创建，内部投递到工作线程：

```cpp
/**
 * @brief 建立到远程主机的 SSH 连接（异步）。
 * @param host 主机地址（IP 或域名）。
 * @param port 端口号（1-65535）。
 * @param user 登录用户名。
 *
 * 完成后发射 connected() 或 errorOccurred()。
 */
void connectToHost(const QString &host, quint16 port, const QString &user);
```

- `ZzSshConnection`：连接生命周期、认证、keepalive；`connected` / `errorOccurred(code, message)` / `disconnected(reason)` 三种结局信号
- `ZzSshShellChannel`：`openShell(term, cols, rows)`、`write()`、`dataReceived()`、`resize()`
- 认证策略：按顺序尝试 agent → 公钥（配置路径）→ 密码（通过信号回调向上层索取，上层不感知细节）
- 主机密钥验证：首次连接发射指纹确认信号，上层确认后存入 `known_hosts.json`；密钥变更时发射明确警告信号
- 全部 C 资源（session、channel、socket）以 C++20 RAII 封装，析构即释放，禁止裸指针泄漏出封装层

### 4.3 QCoro 定位

QCoro 协程化的是 Qt 异步原语，无法直接协程化 libssh2（同步 C API），工作线程模型不变。ZzSshCore 核心只暴露信号槽 API，不硬依赖 QCoro；应用层可选用 QCoro 对信号做 `co_await` 包装，以协程风格编排连接流程。

### 4.4 已知局限与后路

- libssh2 算法支持取决于后端（OpenSSL / mbedtls），遇到服务器连不上先怀疑算法协商
- FIDO/U2F 硬件密钥、链式 ProxyJump 等能力不如 libssh；若后续成为硬需求，协议层可替换为 libssh，架构已为此隔离

## 五、ZzLogEngine 设计（无限滚动历史）

### 5.1 问题与目标

现有终端的滚动历史要么有固定上限（超出即丢弃），要么全量驻留内存（百万行后内存爆炸、滚动卡顿）。ZzClawTerm 的目标：**行数无上限，滚动帧时间 ≤16ms，内存占用与行数无关（有界）**。

### 5.2 分层存储架构（v0.1 实现热层 + 温层，冷层 v0.2）

```text
终端输出
   │
   ▼
┌────────────────────────────────────────────┐
│ 热层：内存环形缓冲区（Ring Buffer）          │
│  · 默认 10,000 行，可配置                   │
│  · 固定内存占用，O(1) 读写                  │
│  · 保留完整字符属性（颜色 / 粗体等）          │
│  · 溢出时最老的数据块归档到温层              │
├────────────────────────────────────────────┤
│ 温层：mmap 内存映射文件 + LZ4 分块压缩       │
│  · 默认 1,000,000 行，可配置                │
│  · 分块压缩（64KB 一块），按需解压           │
│  · 分块行偏移索引：每 1024 行记录一个偏移，  │
│    块内小范围扫描定位（借鉴 klogg 行索引     │
│    思路，1000 万行索引仅 ~80KB）            │
│  · 超限时最老的块丢弃（v0.2 改为归档冷层）   │
├────────────────────────────────────────────┤
│ 冷层：SQLite + ZSTD + FTS5（v0.2）          │
│  · 无限容量，全文搜索，按会话 / 时间查询     │
└────────────────────────────────────────────┘
```

### 5.3 滚动不卡顿的关键手段

- 虚拟滚动：只渲染可见行，与总行数无关（ZzTermWidget 已有脏区 / 增量绘制基建，对接热层读取）
- 预加载：检测滚动方向与速度，后台线程提前解压滚动方向上的相邻块
- 分层读取：热层命中零延迟；温层命中按需解压（单块 64KB，微秒级）；用户无感
- 归档在后台线程执行，绝不阻塞 I/O 线程与 UI 线程

### 5.4 与 ZzTermWidget 的关系

ZzTermWidget 保留当前屏幕缓冲与渲染职责；ZzLogEngine 接管滚动历史的持久化与回溯读取。v0.1 的集成方式：ZzTermWidget 滚动出的行追加到 ZzLogEngine 热层；向上滚动超出 ZzTermWidget 内存历史时，从 ZzLogEngine 读回。

## 六、会话模型与凭据存储

### 6.1 会话数据模型

- `ZzSessionProfile`：名称、分组路径、主机、端口、用户名、认证方式（agent / 密钥路径 / 密码引用）、终端类型、编码、配色方案名、keepalive 间隔
- 分组用路径字符串（如 `生产环境/Web 服务器`），重命名分组即改字符串前缀，模型简单
- 存储格式：JSON 文件（Linux `~/.config/ZzClawTerm/sessions.json`，Windows `%APPDATA%`，macOS `~/Library/Application Support`），Qt 自带 QJsonDocument，零新增依赖；v0.1 会话量级下不需要 SQLite

### 6.2 凭据存储

- v0.1 实现：**AES-256-GCM 加密 + 主密码**。凭据存 `credentials.dat`，首次启动设主密码，解锁一次驻留内存
- 加密实现使用 libssh2 已依赖的 OpenSSL EVP 接口，不引入新依赖
- 凭据访问收敛到 `ZzCredentialStore` 接口；v0.2 可加系统密钥环（Windows Credential Manager / macOS Keychain / libsecret）作为同一接口的另一实现，应用层无感
- 不做明文 / obscure 编码存储

## 七、应用壳层与 UI（WindTerm 风格）

基于 ZzPureTools 现有能力（无边框窗口、模块路由、Fluent 主题、Dock）：

```text
┌──────────────────────────────────────────────┐
│  标题栏（无边框，ZzWindowKit）                 │
├──────────┬───────────────────────────────────┤
│          │  标签栏（拖动排序）                  │
│  会话面板  ├───────────────────────────────────┤
│  (Dock)   │                                   │
│  · 分组树  │      ZzTerminalView               │
│  · 双击连  │      (QTermWidget + channel)      │
│  · 右键    │                                   │
│    菜单    │                                   │
├──────────┴───────────────────────────────────┤
│  状态栏：连接状态 | 编码 | 行列                 │
└──────────────────────────────────────────────┘
```

组件职责：

- `ZzSessionPanel`（QDockWidget，实现 `ZzPanelInterface`）：树形分组视图，双击连接，右键新建 / 编辑 / 删除 / 复制会话；可折叠、可停靠左右
- `ZzTabManager`：每标签持有一个 `ZzTerminalView`（组合 QTermWidget + `ZzSshShellChannel` 或本地 PTY）；支持关闭、拖拽排序；断线标签变灰保留，右键重连，不自动关标签
- 本地 shell：特殊会话类型，不经 SSH，直接 ptyqt 起本地 shell，复用同一 `ZzTerminalView`
- 设置页（v0.1 仅全局）：默认终端类型、编码、字号、配色；会话级覆盖留待后续
- 终端分屏 v0.1 不做

连接流程：双击会话 → `ZzTabManager` 新建标签 → `ZzSshConnection` 在工作线程连接 → 认证（密码经主密码解锁后从 `ZzCredentialStore` 取，缺主密码弹解锁框）→ 开 shell channel → 字节流双向接到 QTermWidget。

## 八、错误处理

- ZzSshCore 异步操作统一三种结局信号：`connected` / `errorOccurred(code, message)` / `disconnected(reason)`；错误码透传 libssh2 错误码 + 封装层自定义码（认证失败、主机密钥变更等）
- 主机密钥首次确认、变更警告为安全底线，不可省略
- GUI 层错误走状态栏 + 标签内提示，不用弹窗轰炸；连接失败保留在标签内显示错误与"重试"按钮
- 日志引擎 I/O 失败（磁盘满等）降级为纯内存模式并提示用户，不影响终端交互

## 九、测试策略

- 测试框架：Qt 官方 QTest（`Qt6::Test`），与 ZzTermWidget / ZzPureTools 一致
- ZzSshCore：单元测试 mock socket 层；集成测试用 Docker 起 openssh-server 容器，覆盖密码 / 密钥认证、shell 收发、断线、主机密钥校验；SFTP 测试基建 v0.1 顺带铺设（v0.2 复用）
- ZzLogEngine：环形缓冲区溢出 / 归档往返、压缩解压一致性、行偏移索引定位正确性、滚动读取等价性测试
- 应用层：QTest 覆盖会话模型序列化、凭据加解密往返、ZzTabManager 生命周期
- 三平台人工验收清单（参照 ZzPureTools 的验收清单模式）
- 沿用既有项目规矩：核心逻辑（解析、存储、加密）必须附带回归测试

### 9.1 性能测试门控（硬性要求）

- 每完成一项功能必须附带性能测试，**不达标不允许验收通过**
- 性能指标以第二章验收标准与各模块性能目标为准（如滚动帧时间 ≤16ms、凭据加解密、解析吞吐等）
- 每次性能测试的结果必须持久化记录到仓库 `tests/perf/records/YYYY-MM-DD-<功能名>.json`，内容包含：
  - 测试项名称与通过阈值、实测值、是否通过
  - 环境信息：CPU / 内存 / OS / Qt 版本 / 编译器 / 构建类型（Release）
  - 代码版本：git commit hash
  - 测试时间
- 历史记录全部保留在仓库中，后续性能回退时可按功能与时间轴对比排查
- 性能测试进 ctest，Release 构建下的数字才有效；阈值失败即测试失败

## 十、编码与构建规范

- 语言 C++20，框架 Qt 6.8+，构建 CMake 3.25+
- 构建配置使用 **CMakeLists.txt + CMakePresets.json**（与 ZzPureTools 一致）：
  - 共享构建矩阵（三平台 × Debug/Release）固化在仓库的 `CMakePresets.json`
  - 开发者本机路径（Qt SDK、编译器）通过环境变量或未跟踪的 `CMakeUserPresets.json` 提供，附 `CMakeUserPresets.json.example` 模板
  - 禁止把本机绝对路径提交进仓库
- 所有类加 `Zz` 前缀，大小写按 Qt 惯例：`ZzPushButton`、`ZzPureTitleBar`、`ZzSshConnection`
- 除 `main.cpp` 外，文件名与类名严格一致（含大小写）：`ZzSshConnection.h` / `ZzSshConnection.cpp`
- 注释强制 Doxygen 风格、简体中文：

```cpp
/**
 * @brief 异步连接到远程服务器。
 * @param host 主机地址（IP或域名）。
 * @param port 端口号（1-65535）。
 * @return 无。完成后发射 connected() 或 errorOccurred()。
 * @note 同一连接对象不可重复调用，需等待上一次调用完成。
 *
 * @code
 * auto *conn = new ZzSshConnection(this);
 * connect(conn, &ZzSshConnection::connected, this, []{ ... });
 * conn->connectToHost("example.com", 22, "root");
 * @endcode
 */
void connectToHost(const QString &host, quint16 port, const QString &user);
```

- 核心库不依赖 Qt Widgets；UI 层不直接访问传输与存储实现

## 十一、依赖清单

| 库 | 版本 | 用途 | 协议 | 引入方式 |
| -- | ---- | ---- | ---- | -------- |
| Qt | 6.8+ | UI 框架、Core/Network/Widgets | LGPL v3 / 商业 | 系统 / 官方安装 |
| libssh2 | CMake 移植版（gitcode.com/JackfahdinImport/libssh2） | SSH 协议 | BSD | add_subdirectory |
| OpenSSL | 3.x（gitcode.com/ZzThirdParty/openssl，当前缺 macOS 构建，macOS 打包前补齐） | libssh2 后端 + 凭据 AES 加密 | Apache 2.0 | add_subdirectory |
| ZzTermWidget | 现有仓库 | 终端解析与绘制 | 遵循 qtermwidget 上游（LGPL） | add_subdirectory |
| ZzPureTools | 现有仓库 | 应用壳层框架 | MIT | add_subdirectory |
| LZ4 | 1.9+ | 温层分块压缩 | BSD 2-Clause | add_subdirectory（vendored） |
| QCoro | 0.10+ | 应用层协程包装（可选） | MIT | add_subdirectory（vendored） |
| SQLite / ZSTD | — | 冷层（v0.2 引入） | Public Domain / BSD | 暂缓 |

许可证提示：自用闭源无问题；若将来对外分发，ZzTermWidget（qtermwidget 上游为 LGPL）需动态链接或开源对应修改，届时再处理。

## 十二、里程碑验收标准

### v0.1 验收

- 能保存 / 分组 / 编辑会话，双击连接真实 SSH 服务器
- vim、top、中文、256 色正常交互
- 滚动历史 100 万行无卡顿、无丢失，内存占用有界
- 断线标签保留并可重连；主机密钥变更弹出警告
- 主密码加解密凭据往返正确
- 本地 shell 会话可用
- 三平台出可执行包，人工验收清单通过
- 所有功能的性能测试通过且结果已记录到 `tests/perf/records/`（见 9.1，不达标不验收）

### v0.2 / v0.3 概要

- v0.2：SFTP 侧边栏（复用同一 `ZzSshConnection` 开 SFTP channel）、端口转发、终端分屏、ZzLogEngine 冷层、系统密钥环凭据后端
- v0.3：X11 forwarding（`libssh2_channel_x11_req_ex` + cookie 管理）；Windows 魔改 vcxsrv，Linux/macOS 直连系统 X server；vcxsrv 魔改为独立子项目，单独设计
