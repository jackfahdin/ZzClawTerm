# ZzClawTerm X11 Forwarding 应用侧（M1+M3）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 主仓库应用侧 X11 forwarding：cookie 生成与 xauthority 写入、Windows 官方 vcxsrv 二进制按需下载与进程管理、profile 开关、传输链路接线、标签页生命周期绑定。完成后三端可用（Windows 官方二进制独立窗口 / Linux 系统 X server / macOS XQuartz）。

**架构：** 新增 `src/x11/` 模块（ZzXAuthority / ZzXServerDownloader / ZzXServerManager），传输层 `ZzSshTransport` 在 shell 打开后按 profile 开关发起 X11 forwarding（调用 ZzSshCore 新 API），本地端点按平台选择（Windows: 127.0.0.1:6000+N；Linux/macOS: $DISPLAY 的 Unix socket）。Windows 官方二进制首次使用时按需下载（SHA256 校验），不进安装包。

**技术栈：** C++20、Qt6.8+、Qt Test；依赖计划 `2026-08-22-x11-forwarding-01-zzsshcore.md` 的库侧 API（仅任务 6 有依赖，其余可并行）。

**关键约束（全程遵守）：**
- 类名 Zz 前缀；文件名=类名；注释 Doxygen 风格简体中文
- commit message：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详细说明；**不要 push**
- 构建测试：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release`（基线 40 项全绿不能变红；若无 configure 先 `cmake --preset linux-gcc-release`，Qt 路径优先读 CMakeUserPresets.json）
- 全量 ctest 会覆写 tests/perf/records/ 下已入库记录（已知噪音），跑完 `git checkout -- tests/perf/records/` 恢复；新增记录文件保留
- 状态栏瞬时提示、错误不弹窗（现有规范）

**库侧 API 契约（计划 01 提供，本计划按此编码）：**

```cpp
// ZzSshShellChannel
void requestX11Forwarding(const QString &cookieHex);   // cookieHex = 32 字符 hex
signals: void x11ForwardingReady();  void x11ForwardingFailed(int code, const QString &message);

// ZzSshConnection
signals: void x11ChannelReceived(quint32 channelId, const QString &originHost, int originPort);

// ZzSshX11Bridge（字节透传，cookie 校验由本地 X server 完成）
struct LocalEndpoint { QString host; quint16 port = 0; QString localSocketPath; };
ZzSshX11Bridge(ZzSshConnection *connection, LocalEndpoint endpoint, QObject *parent = nullptr);
signals: void bridgeReady(quint32 channelId);  void bridgeFailed(quint32 channelId, int code, const QString &message);
```

---

## 文件结构

**创建：**
- `src/x11/ZzXAuthority.h/.cpp` — cookie 生成、xauthority 二进制文件写入、Linux/macOS `xauth add` 封装
- `src/x11/ZzXServerDownloader.h/.cpp` — Windows 官方 vcxsrv 发布物按需下载 + SHA256 校验 + 解压
- `src/x11/ZzXServerManager.h/.cpp` — server 进程生命周期、display 号探测分配、启动参数组装
- `tests/unit/tst_ZzXAuthority.cpp`、`tst_ZzXServerDownloader.cpp`、`tst_ZzXServerManager.cpp`

**修改：**
- `src/session/ZzSessionProfile.h/.cpp` — 加 `x11Forwarding` 布尔字段（JSON 序列化）
- `src/panel/ZzSessionEditDialog.h/.cpp` — X11 转发勾选项
- `src/transport/ZzSshTransport.h/.cpp` — shell 打开后按开关发起 X11 forwarding、按平台建桥、状态栏提示
- `src/ZzAppShell.cpp` — 无（生命周期经标签页关闭→传输析构自然回收；如 manager 为每会话实例则无需壳层改动）
- `src/CMakeLists.txt`、`tests/CMakeLists.txt` — 登记
- `README.md` — 功能特性补 X11 forwarding
- `docs/superpowers/specs/2026-08-22-x11-forwarding-design.md` — 无（规格不改）

---

### 任务 1：官方发布物调研与常量固化

**文件：**
- 创建：`src/x11/ZzXServerDownloader.h`（先只写常量与注释）

- [ ] **步骤 1：查实官方发布物**

运行（逐条执行并记录结果）：

```bash
curl -sI "https://sourceforge.net/projects/vcxsrv/files/latest/download" | head -5
curl -s "https://api.github.com/repos/marchaesen/vcxsrv/releases/latest" | grep -E '"(tag_name|name|browser_download_url)"'
```

确定：最新版本号、可用资产形式（installer.exe / zip / 7z）、稳定下载 URL、官方公布的 SHA256（若官方不给哈希，本步骤下载一次算出其 SHA256 并记录来源与日期）。

- [ ] **步骤 2：选定解压方案**

- 若资产为 zip：Qt 无内置 zip 读取，用 `QProcess` 调系统工具（Windows: `tar.exe -xf`（Win10+ 自带 bsdtar）；开发期 Linux: `unzip`/bsdtar）
- 若只有 NSIS installer.exe：bsdtar 可解 NSIS（`tar -xf installer.exe`），验证能解出 vcxsrv.exe + xkbcomp 数据再定案
- 决策与证据写进 `ZzXServerDownloader.h` 头注释（版本、URL、SHA256、解压方式、核实日期）

- [ ] **步骤 3：常量落码**

```cpp
namespace ZzXServerRelease {
inline constexpr char kVersion[] = "<查实版本>";
inline constexpr char kUrl[] = "<稳定下载 URL>";
inline constexpr char kSha256[] = "<哈希>";
}
```

- [ ] **步骤 4：Commit**

```bash
git add src/x11/ZzXServerDownloader.h
git commit -m "feat(x11): 固化 vcxsrv 官方发布物常量

- 版本/URL/SHA256/解压方式经实际下载核实（来源与日期见头注释）"
```

---

### 任务 2：ZzXAuthority——cookie 与 xauthority 文件

**文件：**
- 创建：`src/x11/ZzXAuthority.h/.cpp`
- 测试：`tests/unit/tst_ZzXAuthority.cpp`

xauthority 二进制格式（大端）：每条记录 = `family(u16)` + `addrlen(u16)+addr` + `numlen(u16)+num(显示号字符串)` + `namelen(u16)+name("MIT-MAGIC-COOKIE-1")` + `datalen(u16)+data(16 字节)`。family 用 256（FamilyWild，免主机名匹配）。

- [ ] **步骤 1：编写失败的测试**

```cpp
void cookieIs32HexChars();          // generateCookie() 两次不同、纯 hex、长度 32
void writtenFileAcceptedByXauth();  // 写入临时文件后系统 xauth 能读回
void filePermissions0600();         // 权限检查
```

`writtenFileAcceptedByXauth` 实现（Linux CI 有 xauth）：

```cpp
ZzXAuthority auth;
const QString cookie = auth.generateCookie();
const QString path = QDir::temp().filePath("zzxauth-test");
QVERIFY(auth.writeXauthorityFile(path, 99, cookie));
QProcess p;
p.start("xauth", {"-f", path, "list"});
QVERIFY(p.waitForFinished(5000));
const QString out = QString::fromUtf8(p.readAllStandardOutput());
QVERIFY(out.contains("MIT-MAGIC-COOKIE-1"));
QVERIFY(out.contains(cookie));
```

（系统无 xauth 时该用例 QSKIP：`QProcess::start` 失败检测。）

- [ ] **步骤 2：运行验证失败**（编译失败，类不存在）

- [ ] **步骤 3：实现**

```cpp
class ZzXAuthority
{
public:
    /** @brief 生成 32 字符十六进制 cookie（16 字节加密随机）。 */
    QString generateCookie() const;          // QRandomGenerator::system()，逐字节 %02x
    /** @brief 以 xauth 二进制格式写入授权记录（family=FamilyWild，0600 权限，QSaveFile 原子落盘）。 */
    bool writeXauthorityFile(const QString &path, int display, const QString &cookieHex) const;
    /** @brief Linux/macOS：对系统 X server 执行 xauth add（$XAUTHORITY 或默认 ~/.Xauthority）。 */
    bool addToSystemAuthority(int display, const QString &cookieHex, QString *errorOut = nullptr) const;
};
```

`addToSystemAuthority`：`QProcess` 执行 `xauth add :<display> . <cookie>`，非零退出或超时（5s）返回 false 并填 errorOut。

- [ ] **步骤 4：运行验证通过**

```bash
cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R ZzXAuthority --output-on-failure
```

- [ ] **步骤 5：Commit**

```bash
git add src/x11/ZzXAuthority.h src/x11/ZzXAuthority.cpp tests/unit/tst_ZzXAuthority.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat(x11): 新增 ZzXAuthority 授权管理

- 加密随机 cookie 生成（32 字符 hex）
- xauthority 二进制格式手写写入（FamilyWild + 0600 + QSaveFile 原子落盘），
  测试用系统 xauth 回读验证格式正确性
- Linux/macOS xauth add 封装"
```

---

### 任务 3：ZzXServerDownloader——按需下载与校验

**文件：**
- 创建：`src/x11/ZzXServerDownloader.h/.cpp`（补全实现）
- 测试：`tests/unit/tst_ZzXServerDownloader.cpp`

- [ ] **步骤 1：编写失败的测试**

网络层注入 `QNetworkAccessManager*`（测试用本地 QTcpServer/文件 URL 桩）：

```cpp
void downloadsAndVerifies();        // 桩返回合法包体 → 校验通过 → 解压出目标文件清单
void rejectsOnSha256Mismatch();     // 篡改一字节 → downloadFailed，不落盘
void rejectsOnHttpError();          // 404 → downloadFailed 含状态码
void resumesExistingInstall();      // 本地已有同版本且校验通过 → 直接 ready 不发请求
```

- [ ] **步骤 2：运行验证失败** → **步骤 3：实现**

```cpp
class ZzXServerDownloader : public QObject
{
    Q_OBJECT
public:
    explicit ZzXServerDownloader(QObject *parent = nullptr);
    /** @brief 本地已装版本（无则空串）。安装根目录 = AppDataLocation/xserver/。 */
    QString installedVersion() const;
    /** @brief 确保可用：已装且校验通过直接发 ready；否则下载+校验+解压。 */
    void ensureAvailable();
    /** @brief 安装后 vcxsrv.exe 的完整路径（Windows）；其他平台返回空。 */
    QString serverExecutablePath() const;
signals:
    void ready(const QString &executablePath);
    void downloadFailed(const QString &message);
    void progressChanged(int percent);   // 状态栏显示用
};
```

实现要点：下载到临时文件 → QCryptographicHash::Sha256 流式校验 → 校验通过才解压（任务 1 定案的方式）→ 版本标记文件（`xserver/VERSION` 写 kVersion）；任何失败清理半成品。仅 Windows 真用；其他平台 `ensureAvailable()` 直接 `ready(QString())`（编译期 `#ifdef Q_OS_WIN` 分支，非 Windows 路径也要有单测）。

- [ ] **步骤 4：运行验证通过** → **步骤 5：Commit**

```bash
git add src/x11/ZzXServerDownloader.* tests/unit/tst_ZzXServerDownloader.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat(x11): 新增 ZzXServerDownloader 按需下载

- 官方发布物下载 → SHA256 流式校验 → 解压到用户数据目录，失败清理半成品
- 版本标记幂等：已装同版本直接 ready；非 Windows 平台编译期直通"
```

---

### 任务 4：ZzXServerManager——进程生命周期与 display 分配

**文件：**
- 创建：`src/x11/ZzXServerManager.h/.cpp`
- 测试：`tests/unit/tst_ZzXServerManager.cpp`

- [ ] **步骤 1：编写失败的测试**

用桩可执行（测试里生成 shell 脚本假 server：sleep 并报告参数到文件）注入 `serverProgramOverride`：

```cpp
void allocatesFreeDisplay();        // 6000+N 被占（测试先占 6000）→ 分配到 N+1
void startsWithExpectedArgs();      // 参数含 :N -multiwindow -clipboard -listen tcp -auth <path>
void reportsCrashAndAllowsRestart();// 桩退出码非 0 → serverCrashed；restart() 再拉起
void stopsCleanly();                // stop() 后进程退出、display 释放
```

- [ ] **步骤 2：运行验证失败** → **步骤 3：实现**

```cpp
class ZzXServerManager : public QObject
{
    Q_OBJECT
public:
    explicit ZzXServerManager(QObject *parent = nullptr);
    /** @brief 分配空闲 display：从 0 递增探测 6000+N 端口可绑定。 */
    static int allocateDisplay();
    /** @brief 启动 server（Windows：vcxsrv.exe；Unix 系统 X server 已在运行则只记录端点）。 */
    void start(const QString &executablePath, const QString &xauthorityPath, int display);
    void stop();
    bool isRunning() const;
    int display() const;
    /** @brief 本地端点（供 ZzSshX11Bridge）：Windows {127.0.0.1, 6000+display}；Unix {/tmp/.X11-unix/X<display>}。 */
    ZzSshX11Bridge::LocalEndpoint localEndpoint() const;
signals:
    void started(int display);
    void crashed(const QString &message);   // 非预期退出（含退出码）
    void stopped();
};
```

实现要点：`QProcess` 持有；`crashed` 与主动 `stop()` 区分（内部 stopping 标志）；Windows 启动参数：`:N -multiwindow -clipboard -listen tcp -auth <path>`；Unix 平台 `start()` 从 `$DISPLAY` 解析 display 号并直接发 `started`（系统 X server 不由我们拉起）；测试注入点 `setServerProgramForTesting(const QString&)`。

- [ ] **步骤 4：运行验证通过** → **步骤 5：Commit**

```bash
git add src/x11/ZzXServerManager.* tests/unit/tst_ZzXServerManager.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat(x11): 新增 ZzXServerManager 进程管理

- display 号端口探测分配；Windows 参数组装（-multiwindow -clipboard -listen tcp -auth）
- 崩溃/主动停止区分，支持重启；Unix 平台复用系统 X server 只解析端点"
```

---

### 任务 5：profile 开关 + 编辑对话框

**文件：**
- 修改：`src/session/ZzSessionProfile.h/.cpp`
- 修改：`src/panel/ZzSessionEditDialog.h/.cpp`
- 测试：`tests/unit/tst_ZzSessionEditDialog.cpp`、profile 序列化测试所在文件

- [ ] **步骤 1：编写失败的测试**

```cpp
void x11ForwardingRoundTrip();      // profile JSON 序列化/反序列化保持
void dialogExposesX11Checkbox();    // objectName=x11CheckBox，加载/保存与 profile 一致
void x11DefaultsOff();              // 旧 profile JSON 无该字段 → false（向后兼容）
```

- [ ] **步骤 2：运行验证失败** → **步骤 3：实现**

- `ZzSessionProfile` 加 `bool x11Forwarding = false;`，toJson/fromJson 双向（fromJson 缺省 false）
- 对话框加「X11 转发（实验性）」勾选项，tooltip：「Windows 端首次使用将下载内建 X server；Linux/macOS 需本机 X server / XQuartz」；布局照现有行模式

- [ ] **步骤 4：运行验证通过** → **步骤 5：Commit**

```bash
git add src/session/ZzSessionProfile.* src/panel/ZzSessionEditDialog.* tests/unit/
git commit -m "feat(session): profile 新增 X11 转发开关

- x11Forwarding 布尔字段，JSON 序列化向后兼容（缺省 false）
- 会话编辑对话框加勾选项与平台提示 tooltip"
```

---

### 任务 6：传输链路接线（依赖计划 01 完成）

**文件：**
- 修改：`src/transport/ZzSshTransport.h/.cpp`
- 测试：`tests/`（mock 层允许则补用例；否则以全量回归 + 手工验证记录兜底）

- [ ] **步骤 1：实现**

`ZzSshTransport` 在 shell channel 打开后：

```cpp
// 伪码骨架（按现有 open() 流程插入）
if (m_profile.x11Forwarding) {
    auto *authority = new ZzXAuthority;              // 成员持有
    const QString cookie = authority->generateCookie();
#if defined(Q_OS_WIN)
    // downloader->ensureAvailable() → ready 后 manager->start(exe, xauthPath, display)
    // xauthorityPath = AppDataLocation/xserver/xauth-<display>（writeXauthorityFile 写入）
#else
    // display 从 $DISPLAY 解析；authority->addToSystemAuthority(display, cookie)
    // 失败 → statusMessage("X11 授权写入失败：...")，不阻断会话
#endif
    m_x11Bridge = new ZzSshX11Bridge(connection, m_x11Manager->localEndpoint(), this);
    connect(m_x11Bridge, &ZzSshX11Bridge::bridgeFailed, this, /* statusMessage 瞬时提示 */);
    shellChannel->requestX11Forwarding(cookie);
    connect(shellChannel, &ZzSshShellChannel::x11ForwardingFailed, this,
            /* statusMessage("X11 转发被服务端拒绝") */);
}
```

要点：X11 失败一律瞬时提示不弹窗、不阻断 SSH 会话；`ZzXServerManager`/桥/authority 以 transport 为父对象，标签关闭随传输析构回收（server 为每会话实例，规格 §5.3）；Linux 无 `$DISPLAY`、macOS 无 XQuartz socket 时提前提示并跳过。

- [ ] **步骤 2：Linux 本机端到端手工验证并记录**（写进 commit message）：

```bash
# 本机有 X server 的前提下（无则起 Xvfb :0 代替）
# 用一个开了 X11 的 SSH 会话连测试服务器，跑 xeyes/xclock，确认窗口出现在本机
```

- [ ] **步骤 3：全量回归**

```bash
cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release
git checkout -- tests/perf/records/   # 恢复噪音覆写
```

预期：40+ 项全绿

- [ ] **步骤 4：Commit**

```bash
git add src/transport/ZzSshTransport.*
git commit -m "feat(transport): SSH 会话接线 X11 forwarding

- shell 打开后按 profile 开关生成 cookie、备妥本地端点、发起 x11-req
- Windows 按需下载拉起内建 server；Linux/macOS 写系统 xauth 用系统 X server
- 失败全部瞬时提示不阻断会话；Linux 本机 xeyes 端到端实测通过（见提交说明）"
```

---

### 任务 7：README + 收尾

- [ ] **步骤 1：README 功能特性补 X11 forwarding 条目**（三端形态、Windows 首次使用按需下载内建 X server 的说明）

- [ ] **步骤 2：终审全量回归**（命令同任务 6 步骤 3）

- [ ] **步骤 3：Commit**

```bash
git add README.md
git commit -m "docs: README 补 X11 forwarding 特性说明"
```

---

## 自检记录

- 规格覆盖：§5.2 下载器 → 任务 1/3；§5.3 display/cookie → 任务 2/4；§三 3.3 profile/对话框 → 任务 5；§四数据流应用侧 → 任务 6；README → 任务 7；§7.2 单测（cookie/权限/下载失败/display 冲突/生命周期桩）→ 各任务测试
- 类型一致性：库侧 API 名与计划 01 逐字一致；`LocalEndpoint` 字段名一致
- 依赖说明：任务 1-5 不依赖计划 01（可与库侧计划并行）；任务 6 需计划 01 合并后进行
- 无占位符：调研步骤给了具体命令与判定标准；代码步骤含实际签名与实现要点
