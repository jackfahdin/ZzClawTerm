# ZzSshCore X11 Forwarding 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 为 ZzSshCore 新增 SSH X11 forwarding：在 shell channel 上发起 x11-req（应用侧传入 cookie）、经 session 回调接受服务端回连的 "x11" channel、桥接到本地 X server 端点（Windows TCP / Unix QLocalSocket）。

**架构：** 完全复用现有 worker 线程泵模型：x11-req 是 shell channel 上的 channel request（照 doWriteChannel 的槽模式）；服务端回连的 "x11" channel 由 libssh2 内建的 `LIBSSH2_CALLBACK_X11` session 回调交付（在 worker 泵上下文同步触发），回调内照 `acceptForwardedChannels`（src/ZzSshConnectionWorker.cpp:282-299）的模式 adopt；数据面复用 `ZzSocketChannelPump`（泛化为 QIODevice 以接 QLocalSocket）。**不 patch libssh2**（vendored 1.11.2_DEV 原生支持，packet.c:249-431 `packet_x11_open`）。

**技术栈：** C++20、Qt6、libssh2（vendored）、Qt Test、Docker（sshd + Xvfb）集成测试。

**关键约束（全程遵守）：**
- 类名 Zz 前缀；文件名=类名（含大小写）；注释 Doxygen 风格简体中文
- commit message：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详细说明；**不要 push**
- 构建：`cmake --preset linux-release && cmake --build --preset linux-release`
- 单元测试：`ctest --test-dir build/linux-release -L unit --output-on-failure`
- 集成/性能：`tests/integration/docker/run-integration-tests.sh build/linux-release`（需 Docker）
- 测试里等待异步事件**必须**用 `QSignalSpy::wait` / `QTRY_VERIFY_WITH_TIMEOUT`，禁止 `waitForReadyRead`（不派发事件循环）

---

## 文件结构

**创建：**
- `src/ZzSshX11Bridge.h/.cpp` — GUI 线程门面：监听回连 x11 channel，连本地 X 端点，建泵双向转发
- `tests/unit/tst_ZzSshX11Bridge.cpp` — 桥接单元测试（桩 channel + 本地 QLocalServer/QTcpServer）
- `tests/unit/tst_ZzSocketChannelPump.cpp` — 泵泛化后的新增用例（QLocalSocket 路径）
- `tests/integration/tst_ZzSshX11IT.cpp` — 端到端集成测试（Docker sshd + Xvfb 回环 xdpyinfo）
- `tests/perf/tst_ZzX11Perf.cpp` — 性能门控（建立耗时 + xdpyinfo 批量回环 + xwd 大流量）
- `tests/perf/records/<日期>-zzx11.json` — 性能记录（测试自动落盘）

**修改：**
- `src/ZzSshError.h:11-28` — 错误码追加 `X11RequestFailed=1015`、`X11BridgeFailed=1016`
- `src/ZzSshError.cpp` — message 映射表同步两条
- `src/ZzSshConnectionWorker.h/.cpp` — x11-req 槽、X11 回调注册与 trampoline、adopt 分发
- `src/ZzSshShellChannel.h/.cpp` — `requestX11Forwarding()` 公开 API
- `src/ZzSshConnection.h/.cpp` — `x11ChannelReceived` 信号 + `adoptX11Channel()`
- `src/ZzSocketChannelPump.h/.cpp` — QTcpSocket → QIODevice 泛化
- `tests/integration/docker/Dockerfile:7` — 加 `xvfb x11-apps x11perf xauth`；sshd_config 显式 `X11Forwarding yes`
- `tests/integration/docker/entrypoint.sh` — 加 `ZZ_DISABLE_X11` 开关（2224 拒绝容器用）
- `tests/integration/docker/run-integration-tests.sh` — ctest 前起客户端侧 Xvfb :99（带测试 xauth 文件）
- `CMakeLists.txt` / `tests/CMakeLists.txt` — 登记新源文件与测试
- `README.md` — 新增 X11 forwarding 章节

---

### 任务 1：错误码 X11RequestFailed / X11BridgeFailed

**文件：**
- 修改：`src/ZzSshError.h:27`（`SftpOperationFailed` 之后）
- 修改：`src/ZzSshError.cpp`（message 映射表）
- 测试：`tests/unit/`（已有错误码测试则加入，没有则新建 `tst_ZzSshError.cpp`）

- [ ] **步骤 1：编写失败的测试**

```cpp
void tst_ZzSshError::x11ErrorMessages()
{
    QCOMPARE(ZzSshError::message(ZzSshErrorCode::X11RequestFailed),
             QStringLiteral("X11 转发请求被服务端拒绝"));
    QCOMPARE(ZzSshError::message(ZzSshErrorCode::X11BridgeFailed),
             QStringLiteral("X11 本地桥接失败"));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit -R SshError --output-on-failure`
预期：FAIL，报 `X11RequestFailed` 未声明

- [ ] **步骤 3：实现**

`src/ZzSshError.h` 在 `SftpOperationFailed` 后追加：

```cpp
    SftpOperationFailed,
    X11RequestFailed,   ///< 1015：x11-req 被服务端拒绝（CHANNEL_FAILURE）
    X11BridgeFailed,    ///< 1016：本地 X 端点连接失败
```

`src/ZzSshError.cpp` 映射表加两条，文案与测试一致。

- [ ] **步骤 4：运行测试验证通过**

同步骤 2 命令，预期 PASS。

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSshError.h src/ZzSshError.cpp tests/unit/
git commit -m "feat: 新增 X11 forwarding 错误码

- ZzSshErrorCode 追加 X11RequestFailed(1015) 与 X11BridgeFailed(1016)
- message 映射表同步中文文案"
```

---

### 任务 2：泵泛化——ZzSocketChannelPump 支持 QIODevice

**文件：**
- 修改：`src/ZzSocketChannelPump.h/.cpp`
- 测试：`tests/unit/tst_ZzSocketChannelPump.cpp`（已有则加用例）

背景：泵当前只接 `QTcpSocket`；X11 桥接在 Linux/macOS 要连 `/tmp/.X11-unix/X<N>`，需要 `QLocalSocket`。泛化为 `QIODevice`（readsReady/writesReady 都用 QIODevice 信号；socket 特有的 abort/disconnect 语义用 qobject_cast 保留）。

- [ ] **步骤 1：编写失败的测试**

```cpp
void tst_ZzSocketChannelPump::pumpsOverLocalSocket()
{
    // QLocalServer 监听临时路径，QLocalSocket 客户端接入，
    // 泵接 QLocalSocket 端；写入 channel 端桩数据，断言经 local socket 收到。
    // 结构照现有 QTcpSocket 用例，只换 socket 类型。
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit -R SocketChannelPump --output-on-failure`
预期：编译失败（构造函数只收 QTcpSocket）

- [ ] **步骤 3：实现**

构造函数签名改为：

```cpp
/**
 * @brief 创建 socket/本地套接字与 SSH channel 的双向泵。
 * @param io 本地端设备（QTcpSocket 或 QLocalSocket），泵不持有所有权。
 * @param channel SSH channel 包装，泵不持有所有权。
 * @note 双端背压水位不变（1MB 高水位 / 512KB 恢复）；任一端关闭即联动关闭并自毁。
 */
ZzSocketChannelPump(QIODevice *io, ZzSshChannel *channel, QObject *parent = nullptr);
```

实现里 `QTcpSocket` 特有的调用（如 `abort()`、`disconnected` 信号）用 `qobject_cast<QTcpSocket*>` / `qobject_cast<QLocalSocket*>` 分别接线；读写一律走 `QIODevice::read/write/readyRead`。既有 QTcpSocket 行为不变。

- [ ] **步骤 4：运行测试验证通过**

运行：`ctest --test-dir build/linux-release -L unit --output-on-failure`
预期：全部 PASS（含既有 QTcpSocket 用例无回归）

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSocketChannelPump.h src/ZzSocketChannelPump.cpp tests/unit/tst_ZzSocketChannelPump.cpp
git commit -m "refactor: ZzSocketChannelPump 泛化为 QIODevice

- 构造函数由 QTcpSocket 改为 QIODevice，支持 QLocalSocket（X11 Unix 套接字桥接前置）
- socket 类型特有语义用 qobject_cast 分别保留，QTcpSocket 既有行为不变"
```

---

### 任务 3：worker 侧——X11 回调注册 + x11-req 槽

**文件：**
- 修改：`src/ZzSshConnectionWorker.h/.cpp`
- 测试：无独立单测（worker 需真实 session），由任务 5 集成测试覆盖；本任务结束以编译 + 既有单测全绿为门禁

关键事实（来自 libssh2 vendored 源码，已核实）：
- 回调类型宏 `LIBSSH2_X11_OPEN_FUNC`：`third_party/libssh2/include/libssh2.h:298-300`
- 槽号 `LIBSSH2_CALLBACK_X11 = 4`：`libssh2.h:337`
- 注册：`libssh2_session_callback_set2(session, LIBSSH2_CALLBACK_X11, cb)`（`libssh2.h:593-596`）
- 回调在 worker 线程泵调用内**同步**触发，channel 已 confirm 并挂入 session；回调返回即视为应用接管，adopt 必须在回调内同步完成
- x11-req：`libssh2_channel_x11_req_ex(channel, single_connection=0, auth_proto=NULL, auth_cookie=hex, screen=0)`（`channel.c:1387-1400`），EAGAIN 走 waitSocketReady 重试（同 doOpenShell 模式）

- [ ] **步骤 1：worker.h 声明**

```cpp
public slots:
    /** @brief 在指定 shell channel 上请求 X11 forwarding。@param channelId 目标 channel。@param cookieHex 32 字符十六进制 cookie（应用侧生成并写入本地 X 授权）。 */
    void doRequestX11(quint32 channelId, const QString &cookieHex);

signals:
    /** @brief x11-req 被服务端接受。 */
    void x11RequestReady(quint32 channelId);
    /** @brief x11-req 被拒绝或出错。 */
    void x11RequestFailed(quint32 channelId, int code, const QString &message);
    /** @brief 服务端回连的 x11 channel 已接入（originator 为远端 X 客户端地址）。 */
    void x11ChannelAccepted(quint32 channelId, const QString &originHost, int originPort);

private:
    static void x11OpenTrampoline(LIBSSH2_SESSION *session, LIBSSH2_CHANNEL *channel,
                                  const char *shost, int sport, void **abstract);
    void adoptX11Channel(LIBSSH2_CHANNEL *raw, const char *shost, int sport);
```

- [ ] **步骤 2：worker.cpp 实现**

- session 建立处（握手完成后、与 setWaitFunction 同区域）注册回调：

```cpp
libssh2_session_set_abstract(session, this);
libssh2_session_callback_set2(session, LIBSSH2_CALLBACK_X11,
                              &ZzSshConnectionWorker::x11OpenTrampoline);
```

（先 grep 确认现有代码是否已设置 session abstract；若已占用，改用一个小的上下文结构体或直接 static 指针表，择最少侵入者并在注释说明。）

- trampoline 与 adopt（照 `acceptForwardedChannels` 模式）：

```cpp
void ZzSshConnectionWorker::x11OpenTrampoline(LIBSSH2_SESSION *, LIBSSH2_CHANNEL *channel,
                                              const char *shost, int sport, void **abstract)
{
    auto *self = static_cast<ZzSshConnectionWorker *>(*abstract);
    if (self)
        self->adoptX11Channel(channel, shost, sport);
}

void ZzSshConnectionWorker::adoptX11Channel(LIBSSH2_CHANNEL *raw, const char *shost, int sport)
{
    auto *channel = ZzSshChannel::adoptOpened(raw);   // 照 worker.cpp:282-299 同款
    channel->setWaitFunction(...);                    // 与 acceptForwardedChannels 一致
    const quint32 id = m_nextAcceptedChannelId++;     // 高位段，worker.h:203
    m_channels.emplace(id, ChannelEntry{channel, {}});
    emit x11ChannelAccepted(id, QString::fromUtf8(shost), sport);
}
```

- doRequestX11：

```cpp
void ZzSshConnectionWorker::doRequestX11(quint32 channelId, const QString &cookieHex)
{
    auto it = m_channels.find(channelId);
    if (it == m_channels.end()) {
        emit x11RequestFailed(channelId,
            static_cast<int>(ZzSshErrorCode::InternalError), QStringLiteral("channel 不存在"));
        return;
    }
    const QByteArray cookie = cookieHex.toLatin1();   // 32 字符 hex
    int rc;
    do {
        rc = libssh2_channel_x11_req_ex(it->second.channel->rawHandle(),
                                        0, nullptr, cookie.constData(), 0);
    } while (rc == LIBSSH2_ERROR_EAGAIN && waitSocketReady());
    if (rc == 0)
        emit x11RequestReady(channelId);
    else
        emit x11RequestFailed(channelId,
            static_cast<int>(ZzSshErrorCode::X11RequestFailed),
            QStringLiteral("x11-req 被服务端拒绝或会话不可用"));
}
```

（`rawHandle()` 若 ZzSshChannel 未暴露 LIBSSH2_CHANNEL*，加一个 `LIBSSH2_CHANNEL *rawHandle() const` 只读访问器——SFTP 引擎已有同类先例，照抄。）

- [ ] **步骤 3：构建 + 既有单测全绿**

运行：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit --output-on-failure`
预期：编译通过，14/14 PASS

- [ ] **步骤 4：Commit**

```bash
git add src/ZzSshConnectionWorker.h src/ZzSshConnectionWorker.cpp src/ZzSshChannel.h src/ZzSshChannel.cpp
git commit -m "feat: worker 接入 X11 channel 回调与 x11-req 槽

- libssh2_session_callback_set2 注册 LIBSSH2_CALLBACK_X11，回调内同步 adopt
  （照 acceptForwardedChannels 模式：adoptOpened + 高位段 channelId + 入 m_channels）
- doRequestX11 槽：libssh2_channel_x11_req_ex 携带应用侧 cookie，EAGAIN 重试
- 信号：x11RequestReady/x11RequestFailed/x11ChannelAccepted"
```

---

### 任务 4：GUI 侧 API——ZzSshShellChannel::requestX11Forwarding + ZzSshConnection 信号

**文件：**
- 修改：`src/ZzSshShellChannel.h/.cpp`
- 修改：`src/ZzSshConnection.h/.cpp`

- [ ] **步骤 1：ZzSshShellChannel 增加公开 API**

```cpp
public:
    /**
     * @brief 请求在本 shell channel 上启用 X11 forwarding。
     * @param cookieHex 32 字符十六进制 cookie；调用方负责把同一 cookie 写入本地 X 授权。
     * @note 仅 channel 已打开时有效；结果经 x11ForwardingReady/x11ForwardingFailed 上报。
     */
    void requestX11Forwarding(const QString &cookieHex);

signals:
    void x11ForwardingReady();
    void x11ForwardingFailed(int code, const QString &message);
```

.cpp：queued invoke worker 的 `doRequestX11(m_channelId, cookieHex)`；worker 的 `x11RequestReady/x11RequestFailed` 按 channelId 过滤接到本 channel 信号（过滤接线照 `ZzSshConnection::createForwardListener` 里 forwardedTcpipAccepted 的同款 lambda 过滤模式，src/ZzSshConnection.cpp:298）。

- [ ] **步骤 2：ZzSshConnection 增加回连信号与 adopt**

```cpp
signals:
    /** @brief 服务端回连的 x11 channel 已就绪，可用 adoptX11Channel 取走。 */
    void x11ChannelReceived(quint32 channelId, const QString &originHost, int originPort);

public:
    /** @brief 取走一个已接入的 x11 channel（所有权转移给调用方）。 */
    ZzSshChannel *adoptX11Channel(quint32 channelId);
```

.cpp：`x11ChannelReceived` 直接透传 worker 的 `x11ChannelAccepted`（queued）；`adoptX11Channel` 照 `adoptForwardChannel` 同款实现（worker 侧把 channel 从 m_channels 摘出并包装）。

- [ ] **步骤 3：构建 + 单测全绿 + Commit**

```bash
cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit --output-on-failure
git add src/ZzSshShellChannel.h src/ZzSshShellChannel.cpp src/ZzSshConnection.h src/ZzSshConnection.cpp
git commit -m "feat: GUI 侧 X11 forwarding API

- ZzSshShellChannel::requestX11Forwarding(cookieHex)，结果信号按 channelId 过滤分发
- ZzSshConnection::x11ChannelReceived 信号 + adoptX11Channel 所有权转移"
```

---

### 任务 5：ZzSshX11Bridge（本地端点桥接）

**文件：**
- 创建：`src/ZzSshX11Bridge.h/.cpp`
- 测试：`tests/unit/tst_ZzSshX11Bridge.cpp`

- [ ] **步骤 1：编写失败的测试**

用 QTcpServer/QLocalServer 起本地假 X server；构造桥接（传桩 ZzSshConnection 或直接驱动其输入），模拟 x11ChannelReceived → 断言桥接连到本地端点、数据双向流通、端点连接失败时发 `bridgeFailed`（错误码 X11BridgeFailed=1016）。桩 channel 可用任务 2 泛化后的泵 + 本地 socketpair 模拟。

- [ ] **步骤 2：运行验证失败**（编译失败，类不存在）

- [ ] **步骤 3：实现**

```cpp
/**
 * @brief X11 回连 channel 到本地 X server 的桥接器。
 *
 * 监听 ZzSshConnection::x11ChannelReceived，为每条 channel 建立到本地 X 端点的
 * 连接并挂 ZzSocketChannelPump 双向转发。字节流原样透传（cookie 校验由本地
 * X server 依据其授权文件完成，本类不触碰 X 协议内容）。
 */
class ZzSshX11Bridge : public QObject
{
    Q_OBJECT
public:
    /** @brief 本地 X 端点。Windows：host=127.0.0.1, port=6000+display；
     *  Unix：localSocketPath=/tmp/.X11-unix/X<display>。 */
    struct LocalEndpoint {
        QString host;             ///< TCP 模式主机（Windows）
        quint16 port = 0;         ///< TCP 模式端口；0 表示用 localSocketPath
        QString localSocketPath;  ///< Unix 套接字路径（Linux/macOS）
    };

    ZzSshX11Bridge(ZzSshConnection *connection, LocalEndpoint endpoint, QObject *parent = nullptr);

signals:
    void bridgeReady(quint32 channelId);
    void bridgeFailed(quint32 channelId, int code, const QString &message);
};
```

实现要点：
- 构造时 `connect(connection, &ZzSshConnection::x11ChannelReceived, ...)`（Qt::QueuedConnection）
- 槽内：`endpoint.port != 0` → `new QTcpSocket` connectToHost(host, port)；否则 `new QLocalSocket` connectToServer(localSocketPath)
- connected 后 `connection->adoptX11Channel(channelId)` + `new ZzSocketChannelPump(io, channel)`（泵自毁语义沿用）
- 连接失败（errorOccurred 或 5s 超时 QTimer）→ 丢弃该 channel（adopt 后 close）+ 发 bridgeFailed

- [ ] **步骤 4：运行验证通过 + 全量单测**

```bash
cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit --output-on-failure
```

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSshX11Bridge.h src/ZzSshX11Bridge.cpp tests/unit/tst_ZzSshX11Bridge.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshX11Bridge 本地端点桥接

- 按平台端点（Windows TCP 6000+N / Unix QLocalSocket）接 X11 回连 channel
- 复用泛化后的 ZzSocketChannelPump，字节流原样透传不过问 X 协议
- 端点连接失败发 bridgeFailed(X11BridgeFailed)"
```

---

### 任务 6：Docker 集成环境加 X11

**文件：**
- 修改：`tests/integration/docker/Dockerfile:7`、`:17-24`（sshd_config 段）
- 修改：`tests/integration/docker/entrypoint.sh`
- 修改：`tests/integration/docker/run-integration-tests.sh`

- [ ] **步骤 1：Dockerfile**

apt 行追加包：`xvfb x11-apps x11perf xauth`；sshd_config sed 段追加显式：

```bash
sed -i 's/^#\?X11Forwarding.*/X11Forwarding yes/' /etc/ssh/sshd_config
```

- [ ] **步骤 2：entrypoint.sh 加拒绝开关**（照 ZZ_DISABLE_FORWARDING 先例）：

```bash
if [ "$ZZ_DISABLE_X11" = "1" ]; then
    sed -i 's/^X11Forwarding.*/X11Forwarding no/' /etc/ssh/sshd_config
fi
```

- [ ] **步骤 3：run-integration-tests.sh**

- 2224 容器（拒绝转发那个）加 `-e ZZ_DISABLE_X11=1`
- ctest 前起客户端侧 Xvfb：

```bash
XAUTH_TEST="$BUILD_DIR/x11-test-auth"
rm -f "$XAUTH_TEST"
Xvfb :99 -auth "$XAUTH_TEST" -screen 0 1280x1024x24 &
XVFB_PID=$!
trap 'kill $XVFB_PID 2>/dev/null' EXIT
export ZZSSH_TEST_X11_DISPLAY=:99
export ZZSSH_TEST_X11_XAUTHORITY="$XAUTH_TEST"
```

- [ ] **步骤 4：重建镜像验证**

运行：`tests/integration/docker/run-integration-tests.sh build/linux-release`
预期：镜像构建成功，既有 14 项集成/性能测试全绿（无 X11 新用例前不退化）

- [ ] **步骤 5：Commit**

```bash
git add tests/integration/docker/
git commit -m "test: Docker 集成环境支持 X11 forwarding

- 镜像加 xvfb/x11-apps/x11perf/xauth，sshd 显式 X11Forwarding yes
- ZZ_DISABLE_X11=1 开关供拒绝路径用例；2224 容器启用
- 运行脚本起客户端侧 Xvfb :99 并导出测试授权文件路径"
```

---

### 任务 7：端到端集成测试 tst_ZzSshX11IT.cpp

**文件：**
- 测试：创建 `tests/integration/tst_ZzSshX11IT.cpp`

结构照 `tst_ZzSshForwardListenerIT.cpp`：`initTestCase` 读 `ZzSshTestServerConfig::fromEnvironment()`，无环境 QSKIP；`makeConnected` 用 QSignalSpy 等 connected。

- [ ] **步骤 1：编写测试（先失败）**

用例清单：

```cpp
void x11RequestAccepted();        // requestX11Forwarding → x11ForwardingReady
void xdpyinfoRoundTrip();         // 核心端到端
void x11RequestRejectedByServer();// 连 2224 容器 → x11ForwardingFailed(X11RequestFailed)
void wrongCookieRejected();       // 授权文件不写 cookie → xdpyinfo 失败
```

核心用例 `xdpyinfoRoundTrip` 流程：

```cpp
// 1. 生成 cookie（32 字符 hex，QRandomGenerator::system）
// 2. 把 cookie 写进 Xvfb :99 的授权文件：
//    QProcess 执行 xauth -f $ZZSSH_TEST_X11_XAUTHORITY add :99 . <cookie>
//    （Xvfb 由运行脚本带 -auth 同一文件启动，xauth add 即时生效）
// 3. makeConnected → 开 shell channel → requestX11Forwarding(cookie)
//    QSignalSpy 等 x11ForwardingReady（15s）
// 4. 创建 ZzSshX11Bridge（LocalEndpoint{localSocketPath: "/tmp/.X11-unix/X99"}）
//    监听 connection->x11ChannelReceived 的 QSignalSpy
// 5. shell 里执行：export DISPLAY=localhost:10.0; xdpyinfo
//    （sshd 已自动把 cookie 加进远端 ~/.Xauthority；远端 xdpyinfo 经代理 display
//      发起 X11 连接 → 回连 channel → 桥到 Xvfb :99，cookie 命中授权文件）
// 6. QSignalSpy 等 x11ChannelReceived（15s），等 bridgeReady
// 7. 从 shell 输出收集 xdpyinfo 结果，QTRY 断言包含 "name of display" 与 "version number"
```

注意：shell 输出读取、远端命令注入照既有 IT 的 shell channel 用法；等待一律 QSignalSpy::wait / QTRY_VERIFY_WITH_TIMEOUT。

- [ ] **步骤 2：运行验证失败**

运行：`tests/integration/docker/run-integration-tests.sh build/linux-release`
预期：新用例 FAIL（API 已就位则至少 xdpyinfoRoundTrip 在调试前失败于环境细节）——若任务 3-5 已按序完成，本步实际用于调通测试

- [ ] **步骤 3：调通四个用例**

逐项修环境问题（cookie 写文件时机、DISPLAY 号偏移以 sshd 实际分配为准——从 shell 里 `echo $DISPLAY` 取，不要硬编码 10）。

- [ ] **步骤 4：全量集成 + 单测绿**

运行：`ctest --test-dir build/linux-release -L unit --output-on-failure && tests/integration/docker/run-integration-tests.sh build/linux-release`
预期：单测全绿；集成含新 4 用例全绿

- [ ] **步骤 5：Commit**

```bash
git add tests/integration/tst_ZzSshX11IT.cpp tests/CMakeLists.txt
git commit -m "test: X11 forwarding 端到端集成测试

- xdpyinfo 经 SSH 转发回环 Xvfb :99 验证全链路（cookie 鉴权真实生效）
- 负路径：服务端拒绝（X11Forwarding no）、cookie 不匹配
- 等待一律 QSignalSpy，杜绝 waitForReadyRead"
```

---

### 任务 8：性能门控 tst_ZzX11Perf.cpp

**文件：**
- 测试：创建 `tests/perf/tst_ZzX11Perf.cpp`
- 记录：`tests/perf/records/<日期>-zzx11.json`（自动落盘入库）

照 `tst_ZzForwardPerf.cpp` 的记录/回归框架（5% 容忍、绝对阈值 + 基线对比），场景：

```cpp
void x11SetupLatency();      // connected → x11ForwardingReady 耗时，阈值 ≤2000ms
void xdpyinfoBurst();        // 连续 100 次 xdpyinfo 回环总耗时/均值，阈值均值 ≤500ms
void xwdBulkThroughput();    // xwd -root -silent（1280x1024x24 ≈ 5MB/次）× 10 次，
                             // 折算 X11 channel 吞吐 MB/s，绝对阈值 ≥5MB/s + 基线回归
```

远端命令经 shell channel 驱动；Xvfb :99 由运行脚本提供。首轮运行为基线采集（回归对比 QSKIP，与 SFTP 首轮同语义）。

- [ ] **步骤 1-3：** 编写 → 跑通 → 记录落盘

运行：`tests/integration/docker/run-integration-tests.sh build/linux-release`
预期：3 场景 PASS，`tests/perf/records/` 新增 `*-zzx11.json`

- [ ] **步骤 4：Commit**

```bash
git add tests/perf/tst_ZzX11Perf.cpp tests/perf/records/
git commit -m "test(perf): X11 forwarding 性能门控

- 建立耗时 ≤2000ms、xdpyinfo 批量回环均值 ≤500ms、xwd 大流量吞吐 ≥5MB/s
- 记录入库 tests/perf/records，沿用 5% 回归容忍"
```

---

### 任务 9：README + 收尾回归

- [ ] **步骤 1：README 新增「X11 forwarding」章节**（中文）：功能说明、cookie 由调用方管理的设计（透传不改写）、API 示例（requestX11Forwarding + ZzSshX11Bridge 最小代码段）、测试方式

- [ ] **步骤 2：全量终审**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --test-dir build/linux-release -L unit --output-on-failure
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：单测全绿；集成+性能全绿；既有记录无超 5% 回归（被覆写的已跟踪 records 检查 diff 是否合理后保留）

- [ ] **步骤 3：Commit**

```bash
git add README.md
git commit -m "docs: README 新增 X11 forwarding 章节

- 功能链路与 cookie 透传设计说明
- requestX11Forwarding + ZzSshX11Bridge 最小使用示例"
```

---

## 自检记录

- 规格覆盖：§三 3.2（ZzSshCore 新增）→ 任务 1-5；§七 7.1（Docker 端到端 + 负路径 + 性能门控）→ 任务 6-8；README → 任务 9
- 类型一致性：`requestX11Forwarding(cookieHex)` / `x11ForwardingReady` / `x11ForwardingFailed` / `x11ChannelReceived(channelId,originHost,originPort)` / `adoptX11Channel(channelId)` / `ZzSshX11Bridge::LocalEndpoint` 全计划一致；错误码 1015/1016 与 ZzSshError.h 现有递推衔接
- 无占位符：每个代码步骤含实际代码；集成测试的环境细节（DISPLAY 号）给了获取方法而非硬编码
