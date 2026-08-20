# SSH 端口转发 ZzSshCore 侧 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在既有 ZzSshCore 仓库（libssh2 的 C++20/Qt 6 异步封装库，基线 HEAD 39dd5f5）上新增 SSH 端口转发能力：worker 读取泵改造为多 channel 调度（按 libssh2 block directions），新增 `ZzSshChannel::openDirectTcpip`、统一双向搬运工 `ZzSocketChannelPump`、本地/动态转发入口 `ZzSshTunnel`、SOCKS5 解析器 `ZzSocks5Handshake`、远程转发 `ZzSshForwardListener`，并落地错误处理矩阵、资源上限与性能门控。应用侧（ZzClawTerm 的 ZzForwardRule/ZzTunnelManager/对话框/状态栏）不在本计划。

**规格：** `ZzClawTerm/docs/superpowers/specs/2026-08-21-ssh-port-forwarding-design.md`（已与用户逐节确认；§二决策表为准）。

**架构：** 线程模型不变——每 SSH 连接一个 worker QThread，libssh2 调用全部在该线程串行执行。连接建立（阻塞模式完成 TCP/握手/认证）后切换 session 为非阻塞模式：channel read 空转返回 EAGAIN（本封装映射为 0），write 用单次写 `writeSome` + 每 channel 待发队列（`ZzChannelWriteQueue`，1MB 高水位 / 512KB 恢复水位迟滞），worker 泵周期（5ms QTimer）逐 channel 轮询读写，互不阻塞。GUI 线程侧：本地监听 QTcpServer（`ZzSshTunnel`）接受连接 → `ZzSshConnection::createForwardChannel()` queued 到 worker 开 direct-tcpip channel → `ZzSocketChannelPump` 在 QTcpSocket 与 channel 门面（`ZzSshForwardChannel`）间搬运数据；远程转发（`ZzSshForwardListener`）由 worker 在泵周期内 `channel_forward_accept` 接入 channel，GUI 侧 QTcpSocket 连本地目标后建泵。三种转发复用同一泵，仅入口与 channel 打开方式不同。

**技术栈：** C++20 / Qt 6.8+（Core + Network + Test）/ CMake 3.25+（CMakePresets.json）/ libssh2（CMake 移植版，OpenSSL 后端）/ QTest / Docker（openssh-server 集成测试，镜像新增 socat 作回显目标）。

**前置条件：**

- 本机已安装 Qt 6.8+（本机路径在未跟踪的 `CMakeUserPresets.json`）、CMake 3.25+、Ninja、Docker。
- 所有命令默认在 `/home/zz/Jackfahdin/github/ZzSshCore` 下执行；示例命令统一使用 `linux-release` preset，Windows/macOS 替换为 `windows-release` / `macos-release`。
- **Qt6::Network 链接现状（已确认）**：根 `CMakeLists.txt` 第 16 行 `find_package(Qt6 6.8 REQUIRED COMPONENTS Core Network)`、第 57 行 `target_link_libraries(zzsshcore PUBLIC Qt6::Core Qt6::Network ...)`。QTcpServer/QTcpSocket 可直接使用，**无需修改链接配置**。
- 既有测试基线（任务 1 开始前必须先跑通作回归锚）：

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`；integration + perf 全 `Passed`（生成当日 `tests/perf/records/YYYY-MM-DD-zzsshcore.json` 基线，供任务 9 回归对比）。

**性能门控（规格 §八，硬性）：** 任务 9 的性能测试进 ctest（perf 标签），仅 Release 构建有效；阈值不达标即测试失败；结果写入 `tests/perf/records/YYYY-MM-DD-zzforward.json`（含阈值、实测值、环境信息、git commit hash）并提交仓库。shell 回归口径 = 既有 perf 基线的 `connect-password-local` 与 `shell-echo-throughput` 两项，回退超 5% 即失败。

**ZzSshCore 仓库纪律：** 全部改动在 ZzSshCore 仓库内 commit；推送远端前先经用户确认（规格 §十）。主仓库 gitlink 更新不在本计划。

---

## 文件结构

| 文件 | 职责 | 动作 |
| ---- | ---- | ---- |
| `src/ZzSshError.h/.cpp` | 追加 `TunnelListenFailed` / `ForwardListenFailed` 错误码 | 修改（任务 1） |
| `src/ZzSshTransport.h` | 追加 `ZzSshWaitResult::Writable` 与纯虚 `waitWritable` | 修改（任务 1） |
| `src/ZzTcpTransport.h/.cpp` | 生产传输层实现 `waitWritable`（poll POLLOUT） | 修改（任务 1） |
| `tests/helpers/ZzMockTransport.h` | mock 实现 `waitWritable`（脚本队列，默认 Writable） | 修改（任务 1） |
| `src/ZzSshSession.h/.cpp` | 追加 `setBlocking` / `blockDirections`；`sendKeepalive` 容忍 EAGAIN | 修改（任务 1、3） |
| `src/ZzSshChannel.h/.cpp` | EAGAIN 重试基建（`WaitFn`）、`writeSome`、`openDirectTcpip`、`adoptOpened` | 修改（任务 2） |
| `src/ZzChannelWriteQueue.h` | 每 channel 待发写队列（1MB 水位迟滞），header-only 可单测 | 创建（任务 3） |
| `src/ZzSshConnectionWorker.h/.cpp` | 多 channel 泵改造、写队列冲刷、read 暂停、direct-tcpip 打开、远程转发 listen/accept/cancel | 修改（任务 3） |
| `src/ZzSshForwardChannel.h/.cpp` | direct-tcpip/forwarded-tcpip channel 的 GUI 线程门面 | 创建（任务 4） |
| `src/ZzSshConnection.h/.cpp` | 追加 `createForwardChannel` / `createTunnel` / `createForwardListener` / `adoptForwardChannel` | 修改（任务 4、7、8） |
| `src/ZzSocks5Handshake.h/.cpp` | RFC1928 无认证子集纯函数式解析器 | 创建（任务 5） |
| `src/ZzSocketChannelPump.h/.cpp` | QTcpSocket ↔ ZzSshForwardChannel 双向搬运工（1MB 背压） | 创建（任务 6） |
| `src/ZzSshTunnel.h/.cpp` | 本地/动态转发运行时实体（QTcpServer 入口，256 并发上限） | 创建（任务 7） |
| `src/ZzSshForwardListener.h/.cpp` | 远程转发运行时实体 | 创建（任务 8） |
| `tests/unit/tst_ZzChannelWriteQueue.cpp` | 写队列水位迟滞单测 | 创建（任务 3） |
| `tests/unit/tst_ZzSshForwardChannel.cpp` | 门面创建前置条件单测 | 创建（任务 4） |
| `tests/unit/tst_ZzSocks5Handshake.cpp` | 解析器合法/畸形/截断单测 | 创建（任务 5） |
| `tests/unit/tst_ZzSocketChannelPump.cpp` | 泵双向/背压/联动关闭单测 | 创建（任务 6） |
| `tests/unit/tst_ZzSshTunnel.cpp` | 隧道监听/端口占用/上限/单连接错误单测 | 创建（任务 7） |
| `tests/integration/tst_ZzSshForwardChannelIT.cpp` | direct-tcpip 端到端集成测试 | 创建（任务 4） |
| `tests/integration/tst_ZzSshTunnelIT.cpp` | -L/-D 端到端与断线失效集成测试 | 创建（任务 7） |
| `tests/integration/tst_ZzSshForwardListenerIT.cpp` | -R 端到端/服务端拒绝/cancel 集成测试 | 创建（任务 8） |
| `tests/integration/ZzSshTestServerConfig.h/.cpp` | 追加 `noForwardPort`（ZZSSH_TEST_NOFWD_PORT） | 修改（任务 8） |
| `tests/integration/docker/Dockerfile` | 显式 AllowTcpForwarding yes；安装 socat | 修改（任务 8） |
| `tests/integration/docker/entrypoint.sh` | 支持 `ZZ_DISABLE_FORWARDING=1`（AllowTcpForwarding no） | 修改（任务 8） |
| `tests/integration/docker/run-integration-tests.sh` | 追加第三容器（拒绝转发，端口 2224） | 修改（任务 8） |
| `tests/perf/tst_ZzForwardPerf.cpp` | 转发性能门控（吞吐/建立延迟/256 并发/shell 回归） | 创建（任务 9） |
| `tests/perf/records/YYYY-MM-DD-zzforward.json` | 转发性能记录 | 创建（任务 9 生成） |
| `tests/unit/tst_ZzSshError.cpp` / `tst_ZzTcpTransport.cpp` / `tst_ZzMockTransport.cpp` / `tst_ZzSshSession.cpp` / `tst_ZzSshConnectionWorker.cpp` | 只追加用例，不改既有用例 | 修改（各任务） |
| `CMakeLists.txt` / `tests/CMakeLists.txt` | target_sources / zz_add_test 追加 | 修改（各任务） |

依赖方向：`ZzSshTunnel` / `ZzSshForwardListener` → `ZzSocketChannelPump` → `ZzSshForwardChannel` → `ZzSshConnection` → `ZzSshConnectionWorker` →（`ZzSshSession` / `ZzSshChannel` / `ZzChannelWriteQueue` / `ZzSshTransport`）→ libssh2。`ZzSocks5Handshake` 无依赖（纯函数），仅被 `ZzSshTunnel` 使用。

---

### 任务 1：错误码扩展 + 传输层 waitWritable + 会话非阻塞基建

为后续任务打底：`ZzSshErrorCode` 追加两个转发错误码；`ZzSshTransport` 追加 `waitWritable`（非阻塞写 EAGAIN 时等待 socket 可写）；`ZzSshSession` 追加 `setBlocking` / `blockDirections`。本任务不改任何既有行为，既有测试必须保持全绿。

**文件：**
- 修改：`src/ZzSshError.h`、`src/ZzSshError.cpp`
- 修改：`src/ZzSshTransport.h`
- 修改：`src/ZzTcpTransport.h`、`src/ZzTcpTransport.cpp`
- 修改：`tests/helpers/ZzMockTransport.h`
- 修改：`src/ZzSshSession.h`、`src/ZzSshSession.cpp`
- 测试：修改 `tests/unit/tst_ZzSshError.cpp`、`tests/unit/tst_ZzTcpTransport.cpp`、`tests/unit/tst_ZzMockTransport.cpp`、`tests/unit/tst_ZzSshSession.cpp`（均只追加用例）

- [ ] **步骤 1：编写失败的测试（四处追加用例）**

`tests/unit/tst_ZzSshError.cpp`：在类的 private slots 声明区追加 `void forwardErrorCodesHaveMessages();`，在文件末尾（`QTEST_GUILESS_MAIN` 之前）追加：

```cpp
void tst_ZzSshError::forwardErrorCodesHaveMessages()
{
    // 新增错误码必须有非空中文描述，且数值紧跟既有枚举（尾部追加，不改既有值）
    QCOMPARE(static_cast<int>(ZzSshErrorCode::TunnelListenFailed),
             static_cast<int>(ZzSshErrorCode::KnownHostsCorrupted) + 1);
    QCOMPARE(static_cast<int>(ZzSshErrorCode::ForwardListenFailed),
             static_cast<int>(ZzSshErrorCode::TunnelListenFailed) + 1);
    QVERIFY(!ZzSshError::message(static_cast<int>(ZzSshErrorCode::TunnelListenFailed)).isEmpty());
    QVERIFY(!ZzSshError::message(static_cast<int>(ZzSshErrorCode::ForwardListenFailed)).isEmpty());
}
```

`tests/unit/tst_ZzMockTransport.cpp`：在类的 private slots 声明区追加 `void waitWritableDefaultsAndScripted();`，文件末尾追加：

```cpp
void tst_ZzMockTransport::waitWritableDefaultsAndScripted()
{
    ZzMockTransport t;
    // 默认返回 Writable（可写是常态）
    QCOMPARE(t.waitWritable(0), ZzSshWaitResult::Writable);
    t.enqueueWaitWritableResult(ZzSshWaitResult::Timeout);
    t.enqueueWaitWritableResult(ZzSshWaitResult::Error);
    QCOMPARE(t.waitWritable(0), ZzSshWaitResult::Timeout);
    QCOMPARE(t.waitWritable(0), ZzSshWaitResult::Error);
    QCOMPARE(t.waitWritable(0), ZzSshWaitResult::Writable); // 队空回落默认
}
```

`tests/unit/tst_ZzTcpTransport.cpp`：在类的 private slots 声明区追加 `void waitWritableOnConnectedSocket();`，文件末尾追加：

```cpp
void tst_ZzTcpTransport::waitWritableOnConnectedSocket()
{
    // loopback 真实连接：可写应立即可达；关闭后返回 Error
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    ZzTcpTransport t;
    QString err;
    QVERIFY(t.open(QStringLiteral("127.0.0.1"), server.serverPort(), 3000, &err));
    QVERIFY(server.waitForNewConnection(1000));
    QCOMPARE(t.waitWritable(1000), ZzSshWaitResult::Writable);
    t.close();
    QCOMPARE(t.waitWritable(0), ZzSshWaitResult::Error);
}
```

`tests/unit/tst_ZzSshSession.cpp`：在类的 private slots 声明区追加 `void setBlockingAndBlockDirectionsAreSafe();`，文件末尾追加：

```cpp
void tst_ZzSshSession::setBlockingAndBlockDirectionsAreSafe()
{
    ZzSshSession s;
    s.setBlocking(false); // 未握手也可安全调用
    s.setBlocking(true);
    QCOMPARE(s.blockDirections(), 0); // 无进行中的阻塞操作时为 0
    ZzSshSession empty; // 构造即有效；再验证无效 session 的防御路径
    // blockDirections 对有效 session 返回 0；析构安全由既有用例覆盖
    QCOMPARE(empty.blockDirections(), 0);
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——`ZzSshErrorCode::TunnelListenFailed`、`ZzSshWaitResult::Writable`、`waitWritable`、`setBlocking`、`blockDirections` 均不存在。

- [ ] **步骤 3：修改 `src/ZzSshError.h` 与 `src/ZzSshError.cpp`**

`src/ZzSshError.h`：在枚举末尾（`KnownHostsCorrupted` 之后）追加两个成员：

```cpp
    KnownHostsCorrupted,        ///< known_hosts 文件损坏或不可读
    TunnelListenFailed,         ///< 本地/动态转发监听绑定失败（端口占用等）
    ForwardListenFailed         ///< 服务端拒绝远程转发监听（AllowTcpForwarding off 等）
```

`src/ZzSshError.cpp`：在 `message()` 的 switch 末尾（`KnownHostsCorrupted` case 之后）追加：

```cpp
    case ZzSshErrorCode::TunnelListenFailed:  return QStringLiteral("转发监听端口绑定失败");
    case ZzSshErrorCode::ForwardListenFailed: return QStringLiteral("服务端拒绝远程转发监听");
```

- [ ] **步骤 4：修改 `src/ZzSshTransport.h`、`src/ZzTcpTransport.h/.cpp`、`tests/helpers/ZzMockTransport.h`**

`src/ZzSshTransport.h`：`ZzSshWaitResult` 追加 `Writable` 成员；接口追加纯虚 `waitWritable`：

```cpp
enum class ZzSshWaitResult {
    Readable,   ///< socket 可读（含对端关闭，读取方会看到 EOF）
    Writable,   ///< socket 可写
    Timeout,    ///< 超时
    Error       ///< 出错（含被 abortBlocking() 中止）
};
```

在 `waitReadable` 声明之后追加：

```cpp
    /**
     * @brief 等待 socket 可写（阻塞，带超时）。
     * @param timeoutMs 超时毫秒数；0 表示立即返回。
     * @return 等待结果（Writable / Timeout / Error）。
     */
    virtual ZzSshWaitResult waitWritable(int timeoutMs) = 0;
```

`src/ZzTcpTransport.h`：在 `waitReadable` 声明后追加：

```cpp
    ZzSshWaitResult waitWritable(int timeoutMs) override;
```

`src/ZzTcpTransport.cpp`：在 `waitReadable` 实现之后追加（结构与 waitReadable 相同，事件换为 POLLOUT）：

```cpp
ZzSshWaitResult ZzTcpTransport::waitWritable(int timeoutMs)
{
    const qintptr fd = m_socket.load();
    if (fd < 0)
        return ZzSshWaitResult::Error;
    QElapsedTimer timer;
    timer.start();
    int rc = -1;
#ifdef Q_OS_WIN
    pollfd pfd{static_cast<SOCKET>(fd), POLLOUT, 0};
    // 与 waitReadable 相同的中断重试策略：用剩余超时预算重试 poll
    while (true) {
        rc = WSAPoll(&pfd, 1, qMax(0, timeoutMs - static_cast<int>(timer.elapsed())));
        if (rc >= 0 || WSAGetLastError() != WSAEINTR)
            break;
        if (timer.elapsed() >= timeoutMs) {
            rc = 0;
            break;
        }
    }
#else
    pollfd pfd{static_cast<int>(fd), POLLOUT, 0};
    while (true) {
        rc = ::poll(&pfd, 1, qMax(0, timeoutMs - static_cast<int>(timer.elapsed())));
        if (rc >= 0 || errno != EINTR)
            break;
        if (timer.elapsed() >= timeoutMs) {
            rc = 0;
            break;
        }
    }
#endif
    if (rc > 0) {
        if (pfd.revents & POLLOUT)
            return ZzSshWaitResult::Writable;
        return ZzSshWaitResult::Error;
    }
    return rc == 0 ? ZzSshWaitResult::Timeout : ZzSshWaitResult::Error;
}
```

`tests/helpers/ZzMockTransport.h`：追加脚本队列与实现（public 区）：

```cpp
    /** @brief 追加一次 waitWritable 的返回值。 */
    void enqueueWaitWritableResult(ZzSshWaitResult r) { m_waitWritableResults.enqueue(r); }

    ZzSshWaitResult waitWritable(int) override
    {
        if (!m_waitWritableResults.isEmpty())
            return m_waitWritableResults.dequeue();
        return ZzSshWaitResult::Writable; // 默认可写（可写是常态）
    }
```

private 区追加成员：

```cpp
    QQueue<ZzSshWaitResult> m_waitWritableResults;
```

- [ ] **步骤 5：修改 `src/ZzSshSession.h` 与 `src/ZzSshSession.cpp`**

`src/ZzSshSession.h`：在 `isTransportBroken()` 声明之前追加：

```cpp
    /**
     * @brief 切换会话阻塞模式。
     *
     * 连接建立（握手+认证）完成后由工作线程切换为非阻塞模式，
     * 使多 channel 调度中单个 channel 的空读/满写不再阻塞其他 channel。
     */
    void setBlocking(bool blocking);

    /**
     * @brief 会话当前的阻塞方向（LIBSSH2_SESSION_BLOCK_INBOUND/OUTBOUND 位掩码）。
     * @return 位掩码；会话无效或无阻塞操作时为 0。
     */
    int blockDirections() const;
```

`src/ZzSshSession.cpp`：在文件末尾追加：

```cpp
void ZzSshSession::setBlocking(bool blocking)
{
    if (m_session)
        libssh2_session_set_blocking(m_session, blocking ? 1 : 0);
}

int ZzSshSession::blockDirections() const
{
    return m_session ? libssh2_session_block_directions(m_session) : 0;
}
```

- [ ] **步骤 6：运行测试验证通过（含全量回归锚）**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`（含 4 个新用例）；integration + perf 全 `Passed`（既有行为零回归）。

- [ ] **步骤 7：Commit**

```bash
git add src/ZzSshError.h src/ZzSshError.cpp src/ZzSshTransport.h src/ZzTcpTransport.h src/ZzTcpTransport.cpp \
    tests/helpers/ZzMockTransport.h src/ZzSshSession.h src/ZzSshSession.cpp \
    tests/unit/tst_ZzSshError.cpp tests/unit/tst_ZzTcpTransport.cpp tests/unit/tst_ZzMockTransport.cpp tests/unit/tst_ZzSshSession.cpp
git commit -m "feat: 追加转发错误码、waitWritable 与会话非阻塞基建

为端口转发多 channel 调度打底：
- ZzSshErrorCode 尾部追加 TunnelListenFailed / ForwardListenFailed 及中文描述
- ZzSshTransport 追加 ZzSshWaitResult::Writable 与纯虚 waitWritable，
  ZzTcpTransport 以 poll(POLLOUT) 实现，ZzMockTransport 支持脚本化返回值
- ZzSshSession 追加 setBlocking / blockDirections，供连接建立后切换
  非阻塞模式与 EAGAIN 重试等待使用
既有行为不变，既有测试全部保持通过。"
```

---

### 任务 2：ZzSshChannel 非阻塞适配（WaitFn 重试基建 + writeSome）

session 切非阻塞后，`ZzSshChannel` 的所有 libssh2 调用都可能返回 `LIBSSH2_ERROR_EAGAIN`。本任务为 channel 加装 `WaitFn`（由 worker 注入的"等待 socket 就绪"回调），`openShell`/`write`/`resize`/`close` 改为 EAGAIN 重试语义；`read` 把 EAGAIN 映射为 0（既有注释本就声明"0 表示对端 EOF 或暂无数据"，语义兼容）；新增单次写 `writeSome` 供 worker 写队列冲刷使用。阻塞模式下 EAGAIN 不会发生，既有行为完全不变。

**文件：**
- 修改：`src/ZzSshChannel.h`、`src/ZzSshChannel.cpp`
- 测试：修改 `tests/unit/tst_ZzSshSession.cpp`（追加 `writeSome` 防御用例）
- 修改：`CMakeLists.txt`（无需变动，源文件已登记；仅确认）

- [ ] **步骤 1：编写失败的测试（追加用例）**

`tests/unit/tst_ZzSshSession.cpp`：在类的 private slots 声明区追加 `void writeSomeWithoutChannelFails();`，文件末尾追加：

```cpp
void tst_ZzSshSession::writeSomeWithoutChannelFails()
{
    ZzSshChannel ch;
    QString err;
    QCOMPARE(ch.writeSome(QByteArray("abc"), &err), static_cast<qint64>(-1));
    QVERIFY(!err.isEmpty());
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错 `writeSome` 不是 `ZzSshChannel` 的成员。

- [ ] **步骤 3：重写 `src/ZzSshChannel.h`（完整替换）**

```cpp
#pragma once

#include <QByteArray>
#include <QString>

#include <functional>
#include <memory>

#include <libssh2.h>

class ZzSshSession;

/**
 * @brief LIBSSH2_CHANNEL 的 RAII 封装。
 *
 * 析构即 close + free；禁止拷贝与移动。
 * 会话处于非阻塞模式时，所有可能 EAGAIN 的操作通过 WaitFn 等待 socket
 * 就绪后重试（read/writeSome 除外：它们把 EAGAIN 映射为 0，由调度方重试）。
 * @note 非线程安全：同一实例的所有方法必须在持有它的工作线程内串行调用。
 */
class ZzSshChannel
{
public:
    /**
     * @brief EAGAIN 重试时等待 socket 再次就绪的回调（由工作线程注入）。
     * @return true 表示 socket 已就绪可重试；false 表示应中止（取消/断线/超时）。
     */
    using WaitFn = std::function<bool()>;

    ZzSshChannel() = default;
    ~ZzSshChannel();

    ZzSshChannel(const ZzSshChannel &) = delete;
    ZzSshChannel &operator=(const ZzSshChannel &) = delete;
    ZzSshChannel(ZzSshChannel &&) = delete;
    ZzSshChannel &operator=(ZzSshChannel &&) = delete;

    /** @brief 包装一个已打开的裸 channel 句柄（forwarded-tcpip accept 用），所有权转移。 */
    static std::unique_ptr<ZzSshChannel> adoptOpened(LIBSSH2_CHANNEL *handle);

    /** @brief 安装 EAGAIN 重试等待回调（打开 channel 之前调用）。 */
    void setWaitFunction(WaitFn fn) { m_waitFn = std::move(fn); }

    /** @brief channel 是否处于打开状态。 */
    bool isOpen() const { return m_channel != nullptr; }

    /**
     * @brief 打开交互式 shell channel（session channel + PTY + shell 进程，EAGAIN 自动重试）。
     * @param session 已完成握手与认证的会话。
     * @param term 终端类型（如 "xterm-256color"）。
     * @param cols 终端列数。
     * @param rows 终端行数。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    bool openShell(ZzSshSession &session, const QString &term, int cols, int rows, QString *errorString);

    /**
     * @brief 打开 direct-tcpip channel（本地/动态转发的数据通道，EAGAIN 自动重试）。
     * @param session 已完成握手与认证的会话。
     * @param targetHost 转发目标主机（由 SSH 服务端发起连接）。
     * @param targetPort 转发目标端口。
     * @param originatorHost 原始连接来源地址（协议字段，可填本地对端地址）。
     * @param originatorPort 原始连接来源端口。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    bool openDirectTcpip(ZzSshSession &session, const QString &targetHost, quint16 targetPort,
                         const QString &originatorHost, quint16 originatorPort, QString *errorString);

    /**
     * @brief 读取 channel 数据（返回本次到达的数据）。
     * @param out 输出缓冲区。
     * @param maxBytes 单次最多读取字节数。
     * @return 读取的字节数；0 表示对端 EOF 或暂无数据（非阻塞会话的 EAGAIN）；负数表示错误。
     */
    qint64 read(QByteArray *out, int maxBytes);

    /**
     * @brief 单次写入（不循环）：非阻塞会话写缓冲满时返回 0，由调度方稍后重试。
     * @param data 待写数据。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 实际写入的字节数；0 表示暂不写（EAGAIN）；负数表示错误。
     */
    qint64 writeSome(const QByteArray &data, QString *errorString);

    /**
     * @brief 写入数据（循环写直至全部发出；非阻塞会话遇 EAGAIN 经 WaitFn 等待重试）。
     * @param data 待写数据。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 全部写入成功返回 true。
     */
    bool write(const QByteArray &data, QString *errorString);

    /**
     * @brief 调整 PTY 尺寸（EAGAIN 自动重试）。
     * @param cols 列数。
     * @param rows 行数。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    bool resize(int cols, int rows, QString *errorString);

    /** @brief 对端是否已发送 EOF。 */
    bool isEof() const;

    /**
     * @brief 关闭并释放 channel（幂等）。
     *
     * 非阻塞会话且装有 WaitFn 时 close 的 EAGAIN 会重试；
     * 无 WaitFn（阻塞会话或 teardown 路径）时单次尝试即释放，保证析构安全。
     */
    void close();

private:
    /** @brief EAGAIN 时等待重试；无 WaitFn 或 WaitFn 返回 false 时放弃。 */
    bool waitRetry() const { return m_waitFn && m_waitFn(); }

    LIBSSH2_CHANNEL *m_channel = nullptr;
    WaitFn m_waitFn;
};
```

- [ ] **步骤 4：重写 `src/ZzSshChannel.cpp`（完整替换）**

```cpp
#include "ZzSshChannel.h"

#include "ZzSshError.h"
#include "ZzSshSession.h"

namespace {

/**
 * @brief 对返回 int 的 libssh2 调用做 EAGAIN 重试。
 * @return 非 EAGAIN 的最终返回码；放弃重试时返回 LIBSSH2_ERROR_EAGAIN。
 */
template <typename Fn>
int retryEagain(Fn &&fn, const ZzSshChannel::WaitFn &waitFn)
{
    while (true) {
        const int rc = fn();
        if (rc != LIBSSH2_ERROR_EAGAIN)
            return rc;
        if (!waitFn || !waitFn())
            return LIBSSH2_ERROR_EAGAIN;
    }
}

} // namespace

ZzSshChannel::~ZzSshChannel()
{
    close();
}

std::unique_ptr<ZzSshChannel> ZzSshChannel::adoptOpened(LIBSSH2_CHANNEL *handle)
{
    if (!handle)
        return nullptr;
    auto ch = std::make_unique<ZzSshChannel>();
    ch->m_channel = handle;
    return ch;
}

bool ZzSshChannel::openShell(ZzSshSession &session, const QString &term, int cols, int rows, QString *errorString)
{
    close();
    if (!session.isValid()) {
        if (errorString) *errorString = QStringLiteral("SSH 会话无效");
        return false;
    }
    // 非阻塞会话：channel_open_session 可能 EAGAIN，经 WaitFn 等待 socket 就绪重试
    while (true) {
        m_channel = libssh2_channel_open_session(session.handle());
        if (m_channel)
            break;
        if (libssh2_session_last_errno(session.handle()) != LIBSSH2_ERROR_EAGAIN || !waitRetry()) {
            if (errorString) *errorString = QStringLiteral("打开 session channel 失败");
            return false;
        }
    }
    const QByteArray t = term.toUtf8();
    if (retryEagain([&] {
            return libssh2_channel_request_pty_ex(m_channel, t.constData(),
                                                  static_cast<unsigned int>(t.size()),
                                                  nullptr, 0, cols, rows, 0, 0);
        }, m_waitFn) != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = QStringLiteral("PTY 请求失败");
        close();
        return false;
    }
    if (retryEagain([&] {
            return libssh2_channel_process_startup(m_channel, "shell", 5, nullptr, 0);
        }, m_waitFn) != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = QStringLiteral("启动 shell 进程失败");
        close();
        return false;
    }
    return true;
}

bool ZzSshChannel::openDirectTcpip(ZzSshSession &session, const QString &targetHost, quint16 targetPort,
                                   const QString &originatorHost, quint16 originatorPort, QString *errorString)
{
    close();
    if (!session.isValid()) {
        if (errorString) *errorString = QStringLiteral("SSH 会话无效");
        return false;
    }
    const QByteArray th = targetHost.toUtf8();
    const QByteArray oh = originatorHost.toUtf8();
    while (true) {
        m_channel = libssh2_channel_direct_tcpip_ex(session.handle(),
                                                    th.constData(), targetPort,
                                                    oh.constData(), originatorPort);
        if (m_channel)
            break;
        if (libssh2_session_last_errno(session.handle()) != LIBSSH2_ERROR_EAGAIN || !waitRetry()) {
            if (errorString)
                *errorString = QStringLiteral("打开 direct-tcpip channel 失败（目标 %1:%2）")
                                   .arg(targetHost)
                                   .arg(targetPort);
            return false;
        }
    }
    return true;
}

qint64 ZzSshChannel::read(QByteArray *out, int maxBytes)
{
    if (!m_channel || !out || maxBytes <= 0)
        return -1;
    QByteArray buf(maxBytes, Qt::Uninitialized);
    const ssize_t rc = libssh2_channel_read_ex(m_channel, 0, buf.data(), static_cast<size_t>(buf.size()));
    if (rc == LIBSSH2_ERROR_EAGAIN)
        return 0; // 非阻塞会话暂无数据：语义同"暂无数据"，由调度方下周期重试
    if (rc > 0) {
        buf.truncate(static_cast<qsizetype>(rc));
        *out = buf;
    }
    return static_cast<qint64>(rc);
}

qint64 ZzSshChannel::writeSome(const QByteArray &data, QString *errorString)
{
    if (!m_channel) {
        if (errorString) *errorString = QStringLiteral("channel 未打开");
        return -1;
    }
    if (data.isEmpty())
        return 0;
    const ssize_t rc = libssh2_channel_write_ex(m_channel, 0, data.constData(),
                                                static_cast<size_t>(data.size()));
    if (rc == LIBSSH2_ERROR_EAGAIN)
        return 0; // 写缓冲满：由调度方稍后重试
    if (rc < 0) {
        if (errorString) *errorString = ZzSshError::messageForLibssh2(static_cast<int>(rc));
        return -1;
    }
    return static_cast<qint64>(rc);
}

bool ZzSshChannel::write(const QByteArray &data, QString *errorString)
{
    if (!m_channel) {
        if (errorString) *errorString = QStringLiteral("channel 未打开");
        return false;
    }
    qsizetype sent = 0;
    while (sent < data.size()) {
        const ssize_t rc = libssh2_channel_write_ex(m_channel, 0, data.constData() + sent,
                                                    static_cast<size_t>(data.size() - sent));
        if (rc == LIBSSH2_ERROR_EAGAIN) {
            if (!waitRetry()) {
                if (errorString) *errorString = QStringLiteral("channel 写入等待被中止");
                return false;
            }
            continue;
        }
        if (rc < 0) {
            if (errorString) *errorString = ZzSshError::messageForLibssh2(static_cast<int>(rc));
            return false;
        }
        sent += rc;
    }
    return true;
}

bool ZzSshChannel::resize(int cols, int rows, QString *errorString)
{
    if (!m_channel) {
        if (errorString) *errorString = QStringLiteral("channel 未打开");
        return false;
    }
    if (retryEagain([&] {
            return libssh2_channel_request_pty_size_ex(m_channel, cols, rows, 0, 0);
        }, m_waitFn) != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = QStringLiteral("调整 PTY 尺寸失败");
        return false;
    }
    return true;
}

bool ZzSshChannel::isEof() const
{
    return m_channel && libssh2_channel_eof(m_channel) == 1;
}

void ZzSshChannel::close()
{
    if (!m_channel)
        return;
    // 无 WaitFn（阻塞会话 / teardown 路径 socket 已死）时单次尝试即释放，保证析构不阻塞
    retryEagain([&] { return libssh2_channel_close(m_channel); }, m_waitFn);
    libssh2_channel_free(m_channel);
    m_channel = nullptr;
}
```

- [ ] **步骤 5：运行测试验证通过（含全量回归锚）**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`（含新用例）；integration + perf 全 `Passed`。shell echo/resize/断开等既有集成测试（本功能的回归锚）一个不许失败。

- [ ] **步骤 6：Commit**

```bash
git add src/ZzSshChannel.h src/ZzSshChannel.cpp tests/unit/tst_ZzSshSession.cpp
git commit -m "feat: ZzSshChannel 适配非阻塞会话并新增 writeSome/openDirectTcpip

- 新增 WaitFn 回调：session 非阻塞后 EAGAIN 调用经其等待 socket 就绪重试
- read 将 EAGAIN 映射为 0（语义兼容既有注释\"0 表示暂无数据\"）
- 新增 writeSome 单次写（EAGAIN 返回 0），供工作线程写队列冲刷使用
- 新增 openDirectTcpip（libssh2_channel_direct_tcpip_ex 封装）
- 新增 adoptOpened 包装已打开的裸 channel（后续 forward_accept 用）
- openShell/resize/close 改为 EAGAIN 重试语义，阻塞模式行为不变"
```

---

### 任务 3：worker 读取泵多 channel 调度改造（替换 worker.h:24 的"唯一 channel"假设）

核心改造（规格 §三方案 A）：

1. `doConnect` 成功、即将发射 `connected()` 之前调用 `m_session->setBlocking(false)`；
2. worker 创建 channel 时安装 `WaitFn`（按 `blockDirections` 等 socket 读/写就绪，带中止检查与 30s 上限）；
3. `m_channels` 的值从 `unique_ptr<ZzSshChannel>` 升级为 `ChannelEntry`（channel + 写队列 + 读暂停标志）；
4. `onReadTimer()` 重写：非阻塞下逐 channel 读至 EAGAIN（单 channel 单轮上限 8×64KiB 防霸凌）、冲刷各 channel 写队列（单周期上限 8×64KiB）；错误处理保持既有语义（`isTransportBroken()` → 连接级断线结局；其余 → channel 级错误）；
5. `doWriteChannel` 改为入队 + 立即冲刷；队列水位经 `channelWritePaused(id, bool)` 信号通知 GUI 节流；
6. 新增 `doSetChannelReadPaused`（GUI 侧 socket 写缓冲超水位时暂停该 channel 读取）；
7. `ZzSshSession::sendKeepalive` 容忍 EAGAIN（非阻塞下 keepalive 暂不发送不等于断线）。

**文件：**
- 创建：`src/ZzChannelWriteQueue.h`
- 修改：`src/ZzSshSession.h`、`src/ZzSshSession.cpp`（sendKeepalive 注释与实现）
- 修改：`src/ZzSshConnectionWorker.h`、`src/ZzSshConnectionWorker.cpp`
- 修改：`CMakeLists.txt`
- 测试：创建 `tests/unit/tst_ZzChannelWriteQueue.cpp`；修改 `tests/unit/tst_ZzSshConnectionWorker.cpp`（追加用例）
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzChannelWriteQueue.cpp`**

```cpp
#include <QtTest>

#include "ZzChannelWriteQueue.h"

/**
 * @brief ZzChannelWriteQueue 水位迟滞与出入队的单元测试（无网络）。
 */
class tst_ZzChannelWriteQueue : public QObject
{
    Q_OBJECT

private slots:
    void appendAndConsume();
    void partialConsumeKeepsRemainder();
    void congestionHysteresis();
    void clearResets();
};

void tst_ZzChannelWriteQueue::appendAndConsume()
{
    ZzChannelWriteQueue q;
    QCOMPARE(q.pendingBytes(), 0);
    q.append(QByteArray(1000, 'a'));
    q.append(QByteArray(500, 'b'));
    QCOMPARE(q.pendingBytes(), 1500);
    QCOMPARE(q.head(100).size(), 100);
    q.removeFirst(100);
    QCOMPARE(q.pendingBytes(), 1400);
    q.removeFirst(1400);
    QCOMPARE(q.pendingBytes(), 0);
}

void tst_ZzChannelWriteQueue::partialConsumeKeepsRemainder()
{
    ZzChannelWriteQueue q;
    q.append(QByteArray(100, 'x'));
    const QByteArray chunk = q.head(65536);
    QCOMPARE(chunk.size(), 100);
    q.removeFirst(30); // writeSome 只写了 30 字节
    QCOMPARE(q.pendingBytes(), 70);
    QCOMPARE(q.head(65536), QByteArray(70, 'x'));
}

void tst_ZzChannelWriteQueue::congestionHysteresis()
{
    ZzChannelWriteQueue q;
    QVERIFY(!q.congested());
    q.append(QByteArray(static_cast<int>(ZzChannelWriteQueue::HighWatermark) + 1, 'a'));
    QVERIFY(q.congested()); // 超过 1MB 高水位
    // 降到 512KB 恢复水位之上仍保持拥塞（迟滞）
    q.removeFirst(ZzChannelWriteQueue::HighWatermark + 1 - ZzChannelWriteQueue::ResumeWatermark - 1);
    QVERIFY(q.pendingBytes() > ZzChannelWriteQueue::ResumeWatermark);
    QVERIFY(q.congested());
    q.removeFirst(q.pendingBytes() - ZzChannelWriteQueue::ResumeWatermark);
    QVERIFY(!q.congested()); // 降到恢复水位及以下
}

void tst_ZzChannelWriteQueue::clearResets()
{
    ZzChannelWriteQueue q;
    q.append(QByteArray(static_cast<int>(ZzChannelWriteQueue::HighWatermark) + 1, 'a'));
    QVERIFY(q.congested());
    q.clear();
    QCOMPARE(q.pendingBytes(), 0);
    QVERIFY(!q.congested());
}

QTEST_GUILESS_MAIN(tst_ZzChannelWriteQueue)
#include "tst_ZzChannelWriteQueue.moc"
```

`tests/unit/tst_ZzSshConnectionWorker.cpp`：在类的 private slots 声明区追加：

```cpp
    void writeToUnknownChannelIsIgnored();
    void readPauseOnUnknownChannelIsIgnored();
```

文件末尾追加：

```cpp
void tst_ZzSshConnectionWorker::writeToUnknownChannelIsIgnored()
{
    auto shared = std::make_shared<ZzSshConnectionShared>();
    ZzSshConnectionWorker worker(shared, [] { return std::make_unique<ZzMockTransport>(); });
    QSignalSpy errSpy(&worker, &ZzSshConnectionWorker::channelErrorOccurred);
    worker.doWriteChannel(999, QByteArray("abc"));   // 未连接、无此 channel：静默
    worker.doWriteChannel(0x80000001, QByteArray("x"));
    QCOMPARE(errSpy.count(), 0);
}

void tst_ZzSshConnectionWorker::readPauseOnUnknownChannelIsIgnored()
{
    auto shared = std::make_shared<ZzSshConnectionShared>();
    ZzSshConnectionWorker worker(shared, [] { return std::make_unique<ZzMockTransport>(); });
    worker.doSetChannelReadPaused(999, true);  // 无此 channel：静默不崩溃
    worker.doSetChannelReadPaused(999, false);
    QVERIFY(true);
}
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzChannelWriteQueue unit/tst_ZzChannelWriteQueue.cpp)
set_tests_properties(tst_ZzChannelWriteQueue PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——找不到 `ZzChannelWriteQueue.h`；`doSetChannelReadPaused` 不存在。

- [ ] **步骤 3：创建 `src/ZzChannelWriteQueue.h`**

```cpp
#pragma once

#include <QByteArray>

/**
 * @brief 单 channel 待发写队列（工作线程内使用），带高/低水位迟滞。
 *
 * GUI 侧数据经 queued 调用抵达工作线程后先入队，再由读取泵周期冲刷。
 * 队列超过 1MB 高水位时进入拥塞态（通知 GUI 暂停 socket 读取），
 * 降到 512KB 恢复水位及以下时解除（迟滞避免水位线附近抖动）。
 */
class ZzChannelWriteQueue
{
public:
    static constexpr qsizetype HighWatermark = 1024 * 1024;   ///< 1MB 拥塞水位
    static constexpr qsizetype ResumeWatermark = 512 * 1024;  ///< 512KB 恢复水位

    /** @brief 追加待发数据。 */
    void append(const QByteArray &data)
    {
        m_buffer += data;
        if (!m_congested && m_buffer.size() > HighWatermark)
            m_congested = true;
    }

    /** @brief 查看队首最多 maxBytes 字节（不移除）。 */
    QByteArray head(qsizetype maxBytes) const
    {
        return m_buffer.left(qMin(maxBytes, m_buffer.size()));
    }

    /** @brief 移除队首 n 字节（writeSome 实际写入后调用）。 */
    void removeFirst(qsizetype n)
    {
        m_buffer.remove(0, static_cast<qsizetype>(qMin(n, m_buffer.size())));
        if (m_congested && m_buffer.size() <= ResumeWatermark)
            m_congested = false;
    }

    /** @brief 清空并复位拥塞态。 */
    void clear()
    {
        m_buffer.clear();
        m_congested = false;
    }

    /** @brief 当前待发字节数。 */
    qsizetype pendingBytes() const { return m_buffer.size(); }

    /** @brief 是否处于拥塞态（迟滞：超 1MB 进入，降到 512KB 及以下退出）。 */
    bool congested() const { return m_congested; }

private:
    QByteArray m_buffer;
    bool m_congested = false;
};
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzChannelWriteQueue.h)
```

- [ ] **步骤 4：修改 `src/ZzSshSession.h/.cpp`（sendKeepalive 容忍 EAGAIN）**

`src/ZzSshSession.h`：将 `sendKeepalive` 的注释替换为：

```cpp
    /**
     * @brief 立即发送一次 keepalive。
     * @return 发送成功或暂不发送（非阻塞会话 EAGAIN，下周期再试）返回 true；
     *         仅确定性的传输层错误返回 false（说明连接已断开）。
     */
    bool sendKeepalive();
```

`src/ZzSshSession.cpp`：将 `sendKeepalive` 实现替换为：

```cpp
bool ZzSshSession::sendKeepalive()
{
    if (!m_session)
        return false;
    int secondsToNext = 0;
    const int rc = libssh2_keepalive_send(m_session, &secondsToNext);
    // 非阻塞会话下 EAGAIN 仅表示本次发不出去，连接未必断开：下周期重试
    return rc == LIBSSH2_ERROR_NONE || rc == LIBSSH2_ERROR_EAGAIN;
}
```

- [ ] **步骤 5：修改 `src/ZzSshConnectionWorker.h`（完整替换）**

```cpp
#pragma once

#include <QMutex>
#include <QObject>
#include <QTimer>

#include <functional>
#include <memory>
#include <unordered_map>

#include "ZzChannelWriteQueue.h"
#include "ZzSshChannel.h"
#include "ZzSshConnectParams.h"
#include "ZzSshSession.h"

#include <libssh2.h>

class ZzSshTransport;
class ZzSshConnectionShared;

/**
 * @brief SSH 连接的工作线程执行体（规格 §4.1：每连接一个 QThread）。
 *
 * 本对象 moveToThread 到专属 QThread，所有槽函数在该线程内串行执行。
 * 握手、认证在阻塞模式下完成；连接建立后 session 切换为非阻塞模式，
 * 读取泵（5ms QTimer）逐 channel 轮询读写（EAGAIN 即跳过），单个
 * channel 的空读/满写不再阻塞其他 channel（替换 v0.1"唯一 shell channel"假设）。
 */
class ZzSshConnectionWorker : public QObject
{
    Q_OBJECT

public:
    /** @brief 传输层工厂（测试注入 mock 用；默认创建 ZzTcpTransport）。 */
    using TransportFactory = std::function<std::unique_ptr<ZzSshTransport>()>;

    explicit ZzSshConnectionWorker(std::shared_ptr<ZzSshConnectionShared> shared, QObject *parent = nullptr);
    ZzSshConnectionWorker(std::shared_ptr<ZzSshConnectionShared> shared, TransportFactory factory, QObject *parent = nullptr);
    ~ZzSshConnectionWorker() override;

    /** @brief 线程安全：中止进行中的阻塞操作（从 GUI 线程调用）。 */
    void interrupt();

public slots:
    /** @brief 执行完整连接流程：TCP → 握手 → 主机密钥验证 → 认证 → keepalive。 */
    void doConnect(const ZzSshConnectParams &params);

    /** @brief 主动断开并清理（发射 disconnected）。 */
    void doDisconnect();

    /** @brief 打开 shell channel。 */
    void doOpenShell(quint32 channelId, const QString &term, int cols, int rows);

    /** @brief 打开 direct-tcpip channel（本地/动态转发数据通道）。 */
    void doOpenDirectTcpip(quint32 channelId, const QString &targetHost, quint16 targetPort,
                           const QString &originatorHost, quint16 originatorPort);

    /** @brief 向 channel 写入数据（入队 + 立即冲刷一次）。 */
    void doWriteChannel(quint32 channelId, const QByteArray &data);

    /** @brief 调整 channel 的 PTY 尺寸。 */
    void doResizeChannel(quint32 channelId, int cols, int rows);

    /** @brief 关闭 channel。 */
    void doCloseChannel(quint32 channelId);

    /** @brief 暂停/恢复 channel 读取（GUI 侧 socket 写缓冲背压用）。 */
    void doSetChannelReadPaused(quint32 channelId, bool paused);

    /** @brief 请求服务端监听远程转发端口。 */
    void doForwardListen(quint32 listenerId, const QString &listenHost, quint16 listenPort);

    /** @brief 取消远程转发监听并释放 listener。 */
    void doForwardCancel(quint32 listenerId);

    /** @brief 发送一次 keepalive，失败则判定断线。 */
    void doSendKeepalive();

signals:
    void connected();
    void errorOccurred(int code, const QString &message);
    void disconnected(const QString &reason);
    void passwordRequested();
    void hostKeyUnknown(const QString &host, quint16 port, const QString &keyType,
                        const QString &fingerprint);
    void hostKeyChanged(const QString &host, quint16 port, const QString &keyType,
                        const QString &oldFingerprint, const QString &newFingerprint);
    void shellOpened(quint32 channelId);
    void directTcpipOpened(quint32 channelId);
    void channelDataReceived(quint32 channelId, const QByteArray &data);
    void channelClosed(quint32 channelId);
    void channelErrorOccurred(quint32 channelId, int code, const QString &message);
    /** @brief channel 写队列拥塞态变化（true=超 1MB 高水位，false=回落到 512KB 恢复水位）。 */
    void channelWritePaused(quint32 channelId, bool paused);
    void forwardListening(quint32 listenerId, quint16 boundPort);
    void forwardListenFailed(quint32 listenerId, int code, const QString &message);
    /** @brief 远程转发接入新连接（libssh2 当前 API 不暴露 originator，参数填空字符串与 0）。 */
    void forwardedTcpipAccepted(quint32 listenerId, quint32 channelId,
                                const QString &originatorHost, quint16 originatorPort);

private slots:
    void onReadTimer();

private:
    /** @brief 单 channel 运行时条目。 */
    struct ChannelEntry {
        std::unique_ptr<ZzSshChannel> channel;
        ZzChannelWriteQueue writeQueue;   ///< 待发写队列（1MB 水位迟滞）
        bool readPaused = false;          ///< GUI 侧 socket 写缓冲背压：暂停读该 channel
        bool writePausedNotified = false; ///< 拥塞信号去抖（仅状态翻转时发射）
    };

    bool verifyHostKey(const QString &host, quint16 port, const QString &storePath,
                       int *codeOut, QString *msgOut);
    bool authenticate(const QString &user, const ZzSshAuthConfig &config, int *codeOut, QString *msgOut);
    bool tryAgentAuth(const QString &user);
    bool tryPublicKeyAuth(const QString &user, const ZzSshAuthConfig &config);
    bool tryPasswordAuth(const QString &user, int *codeOut, QString *msgOut);

    /** @brief EAGAIN 重试等待：按会话阻塞方向等 socket 就绪（30s 上限，可查中止）。 */
    bool waitSocketReady();

    /** @brief 冲刷单 channel 写队列（单轮上限 8×64KiB）；返回是否有实际写入。 */
    bool flushChannelWrite(quint32 channelId, ChannelEntry &entry);

    /** @brief 泵周期内接入所有 listener 的待接入 forwarded-tcpip channel。 */
    void acceptForwardedChannels();

    /** @brief 清理 listener/channel/session/transport（不发射信号）。 */
    void teardown();

    // 与 ZzSshConnection 共享所有权：连接析构等待超时后线程会被摘除，
    // worker 可能比连接对象活得更久，必须持有副本保证共享状态有效
    std::shared_ptr<ZzSshConnectionShared> m_shared;
    TransportFactory m_factory;
    QMutex m_transportMutex;         // 保护 m_transport（interrupt 跨线程访问）
    std::unique_ptr<ZzSshTransport> m_transport;
    std::unique_ptr<ZzSshSession> m_session;
    // QHash 的隐式共享拷贝不支持 move-only value（erase/take 会触发拷贝），故用 std::unordered_map
    std::unordered_map<quint32, ChannelEntry> m_channels;
    std::unordered_map<quint32, LIBSSH2_LISTENER *> m_forwardListeners;
    quint32 m_nextAcceptedChannelId = 0x80000000; // forward_accept 自发 channelId 高位段，避开 GUI 分配段
    QTimer *m_readTimer = nullptr;
    QTimer *m_keepaliveTimer = nullptr;
};
```

- [ ] **步骤 6：修改 `src/ZzSshConnectionWorker.cpp`（替换以下函数；其余函数保持原样）**

在文件头部 include 区追加：

```cpp
#include <QElapsedTimer>
```

`doConnect`：在 `emit connected();` 之前插入（keepalive 配置之后）：

```cpp
    // 连接建立完成：切换非阻塞模式，读取泵改为逐 channel 轮询调度（EAGAIN 即跳过），
    // 多 channel（shell + 若干转发 channel）互不阻塞
    m_session->setBlocking(false);

    emit connected();
```

`doOpenShell`：替换为（安装 WaitFn + ChannelEntry 结构）：

```cpp
void ZzSshConnectionWorker::doOpenShell(quint32 channelId, const QString &term, int cols, int rows)
{
    if (!m_session || !m_session->isValid()) {
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::ChannelOpenFailed),
                                  QStringLiteral("会话未连接"));
        return;
    }
    auto channel = std::make_unique<ZzSshChannel>();
    channel->setWaitFunction([this] { return waitSocketReady(); });
    QString err;
    if (!channel->openShell(*m_session, term, cols, rows, &err)) {
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::ChannelOpenFailed), err);
        return;
    }
    ChannelEntry entry;
    entry.channel = std::move(channel);
    m_channels.emplace(channelId, std::move(entry));
    if (!m_readTimer->isActive())
        m_readTimer->start();
    emit shellOpened(channelId);
}
```

新增 `doOpenDirectTcpip`、`doSetChannelReadPaused`、`doForwardListen`、`doForwardCancel`、`waitSocketReady`、`flushChannelWrite`、`acceptForwardedChannels`（接在 `doOpenShell` 之后）：

```cpp
void ZzSshConnectionWorker::doOpenDirectTcpip(quint32 channelId, const QString &targetHost, quint16 targetPort,
                                              const QString &originatorHost, quint16 originatorPort)
{
    if (!m_session || !m_session->isValid()) {
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::ChannelOpenFailed),
                                  QStringLiteral("会话未连接"));
        return;
    }
    auto channel = std::make_unique<ZzSshChannel>();
    channel->setWaitFunction([this] { return waitSocketReady(); });
    QString err;
    if (!channel->openDirectTcpip(*m_session, targetHost, targetPort, originatorHost, originatorPort, &err)) {
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::ChannelOpenFailed), err);
        return;
    }
    ChannelEntry entry;
    entry.channel = std::move(channel);
    m_channels.emplace(channelId, std::move(entry));
    if (!m_readTimer->isActive())
        m_readTimer->start();
    emit directTcpipOpened(channelId);
}

void ZzSshConnectionWorker::doSetChannelReadPaused(quint32 channelId, bool paused)
{
    const auto it = m_channels.find(channelId);
    if (it == m_channels.end())
        return;
    it->second.readPaused = paused;
}

void ZzSshConnectionWorker::doForwardListen(quint32 listenerId, const QString &listenHost, quint16 listenPort)
{
    if (!m_session || !m_session->isValid()) {
        emit forwardListenFailed(listenerId, static_cast<int>(ZzSshErrorCode::ForwardListenFailed),
                                 QStringLiteral("会话未连接"));
        return;
    }
    const QByteArray host = listenHost.toUtf8();
    int boundPort = 0;
    LIBSSH2_LISTENER *listener = nullptr;
    while (true) {
        listener = libssh2_channel_forward_listen_ex(m_session->handle(),
                                                     host.isEmpty() ? nullptr : host.constData(),
                                                     listenPort, &boundPort, 1);
        if (listener)
            break;
        if (libssh2_session_last_errno(m_session->handle()) != LIBSSH2_ERROR_EAGAIN || !waitSocketReady()) {
            // 典型：服务端 AllowTcpForwarding off——规则级失败，会话保留（规格 §六）
            emit forwardListenFailed(listenerId, static_cast<int>(ZzSshErrorCode::ForwardListenFailed),
                                     QStringLiteral("服务端拒绝远程转发监听（%1:%2）")
                                         .arg(listenHost)
                                         .arg(listenPort));
            return;
        }
    }
    m_forwardListeners.emplace(listenerId, listener);
    if (!m_readTimer->isActive())
        m_readTimer->start();
    emit forwardListening(listenerId, static_cast<quint16>(boundPort));
}

void ZzSshConnectionWorker::doForwardCancel(quint32 listenerId)
{
    const auto it = m_forwardListeners.find(listenerId);
    if (it == m_forwardListeners.end())
        return;
    libssh2_channel_forward_cancel(it->second); // cancel 即释放 listener
    m_forwardListeners.erase(it);
}

bool ZzSshConnectionWorker::waitSocketReady()
{
    // ZzSshChannel 的 EAGAIN 重试等待：按会话阻塞方向等 socket 读/写就绪。
    // 每次最多阻塞 100ms，循环检查中止标志，总上限 30s。
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < 30000) {
        if (m_shared->abortRequested.load())
            return false;
        const int dirs = m_session ? m_session->blockDirections() : LIBSSH2_SESSION_BLOCK_INBOUND;
        QMutexLocker locker(&m_transportMutex);
        if (!m_transport || !m_transport->isOpen())
            return false;
        const ZzSshWaitResult r = (dirs & LIBSSH2_SESSION_BLOCK_OUTBOUND)
                                      ? m_transport->waitWritable(100)
                                      : m_transport->waitReadable(100);
        if (r == ZzSshWaitResult::Readable || r == ZzSshWaitResult::Writable)
            return true;
        if (r == ZzSshWaitResult::Error)
            return false;
    }
    return false;
}

bool ZzSshConnectionWorker::flushChannelWrite(quint32 channelId, ChannelEntry &entry)
{
    bool progress = false;
    for (int i = 0; i < 8 && entry.writeQueue.pendingBytes() > 0; ++i) {
        const QByteArray chunk = entry.writeQueue.head(65536);
        QString err;
        const qint64 n = entry.channel->writeSome(chunk, &err);
        if (n < 0) {
            // 写失败按 channel 级错误上报（连接级断线由读路径 isTransportBroken 统一判定）
            emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::InternalError), err);
            entry.writeQueue.clear();
            break;
        }
        if (n == 0)
            break; // EAGAIN：写缓冲满，下周期继续
        entry.writeQueue.removeFirst(n);
        progress = true;
    }
    const bool congested = entry.writeQueue.congested();
    if (congested != entry.writePausedNotified) {
        entry.writePausedNotified = congested;
        emit channelWritePaused(channelId, congested);
    }
    return progress;
}

void ZzSshConnectionWorker::acceptForwardedChannels()
{
    for (const auto &kv : m_forwardListeners) {
        const quint32 listenerId = kv.first;
        while (true) {
            LIBSSH2_CHANNEL *raw = libssh2_channel_forward_accept(kv.second);
            if (!raw)
                break; // 非阻塞会话：EAGAIN 表示无更多待接入连接
            auto channel = ZzSshChannel::adoptOpened(raw);
            channel->setWaitFunction([this] { return waitSocketReady(); });
            const quint32 channelId = m_nextAcceptedChannelId++;
            ChannelEntry entry;
            entry.channel = std::move(channel);
            m_channels.emplace(channelId, std::move(entry));
            emit forwardedTcpipAccepted(listenerId, channelId, QString(), 0);
        }
    }
}
```

`doWriteChannel`：替换为（入队 + 立即冲刷）：

```cpp
void ZzSshConnectionWorker::doWriteChannel(quint32 channelId, const QByteArray &data)
{
    const auto it = m_channels.find(channelId);
    if (it == m_channels.end())
        return;
    it->second.writeQueue.append(data);
    flushChannelWrite(channelId, it->second);
}
```

`doResizeChannel`：替换为（`m_channels` 值类型升级为 `ChannelEntry`，仅解引用方式变化）：

```cpp
void ZzSshConnectionWorker::doResizeChannel(quint32 channelId, int cols, int rows)
{
    const auto it = m_channels.find(channelId);
    if (it == m_channels.end())
        return;
    QString err;
    if (!it->second.channel->resize(cols, rows, &err))
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::InternalError), err);
}
```

`onReadTimer`：完整替换为：

```cpp
void ZzSshConnectionWorker::onReadTimer()
{
    if (m_channels.empty() && m_forwardListeners.empty()) {
        m_readTimer->stop();
        return;
    }
    QMutexLocker locker(&m_transportMutex);
    if (!m_transport || !m_transport->isOpen())
        return;

    const ZzSshWaitResult wr = m_transport->waitReadable(0);
    if (wr == ZzSshWaitResult::Error) {
        locker.unlock();
        teardown();
        emit disconnected(QStringLiteral("连接中断（传输层错误）"));
        return;
    }

    // 远程转发：泵周期内接入待接入的 forwarded-tcpip channel
    acceptForwardedChannels();

    // 非阻塞会话：read 返回 0 即暂无数据（EAGAIN），writeSome 返回 0 即写缓冲满，
    // 逐 channel 轮询不会阻塞；单 channel 单轮上限 8×64KiB 防止高速通道霸凌低速通道
    for (int round = 0; round < 8 && !m_channels.empty(); ++round) {
        bool anyProgress = false;
        std::vector<quint32> ids;
        ids.reserve(m_channels.size());
        for (const auto &kv : m_channels)
            ids.push_back(kv.first);
        for (const quint32 id : ids) {
            const auto it = m_channels.find(id);
            if (it == m_channels.end())
                continue;
            ChannelEntry &entry = it->second;
            if (!entry.readPaused) {
                for (int i = 0; i < 8; ++i) {
                    QByteArray data;
                    const qint64 n = entry.channel->read(&data, 65536);
                    if (n > 0) {
                        emit channelDataReceived(id, data);
                        anyProgress = true;
                        continue;
                    }
                    if (n == 0)
                        break; // EAGAIN 或暂无数据
                    // n < 0：硬错误（既有语义）
                    if (m_session && m_session->isTransportBroken()) {
                        // 传输层已断（如对端 sshd 会话被杀）：连接级结局而非 channel 错误
                        locker.unlock();
                        for (const auto &kv2 : m_channels)
                            emit channelClosed(kv2.first);
                        teardown();
                        emit disconnected(QStringLiteral("连接中断（对端关闭连接）"));
                        return;
                    }
                    emit channelErrorOccurred(id, static_cast<int>(ZzSshErrorCode::InternalError),
                                              QStringLiteral("channel 读取失败"));
                    break;
                }
            }
            if (entry.channel->isEof()) {
                m_channels.erase(it);
                emit channelClosed(id);
                continue;
            }
            if (flushChannelWrite(id, entry))
                anyProgress = true;
        }
        if (!anyProgress)
            break;
        if (m_transport->waitReadable(0) != ZzSshWaitResult::Readable)
            break;
    }
}
```

`teardown`：完整替换为（增加 listener 清理）：

```cpp
void ZzSshConnectionWorker::teardown()
{
    m_keepaliveTimer->stop();
    m_readTimer->stop();
    for (const auto &kv : m_forwardListeners)
        libssh2_channel_forward_cancel(kv.second);
    m_forwardListeners.clear();
    m_channels.clear();
    m_session.reset();
    QMutexLocker locker(&m_transportMutex);
    if (m_transport) {
        m_transport->close();
        m_transport.reset();
    }
}
```

- [ ] **步骤 7：运行测试验证通过（回归锚：既有测试一个不许失败）**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`（含 4 个新用例）；integration + perf 全 `Passed`。特别关注 `tst_ZzSshShellChannelIT`（openShellAndEcho / resizeChangesPtySize / remoteExitClosesChannel）与 `tst_ZzSshDisconnectIT`——非阻塞切换后 shell 路径行为必须与 v0.1 一致；perf 的 shell-echo-throughput 不低于既有基线的 95%（记录文件对比，正式门控在任务 9）。

- [ ] **步骤 8：Commit**

```bash
git add src/ZzChannelWriteQueue.h src/ZzSshSession.h src/ZzSshSession.cpp \
    src/ZzSshConnectionWorker.h src/ZzSshConnectionWorker.cpp CMakeLists.txt \
    tests/unit/tst_ZzChannelWriteQueue.cpp tests/unit/tst_ZzSshConnectionWorker.cpp tests/CMakeLists.txt
git commit -m "feat: worker 读取泵改造为非阻塞多 channel 调度

替换 v0.1\"每连接只开一个 shell channel\"的假设（worker.h:24）：
- doConnect 成功后 session 切非阻塞，channel 安装 WaitFn（按 block
  directions 等 socket 读/写就绪，100ms 步进、30s 上限、可查中止）
- m_channels 升级为 ChannelEntry（channel + ZzChannelWriteQueue 写队列
  + 读暂停标志），读泵逐 channel 读至 EAGAIN，单 channel 单轮上限
  8×64KiB 防霸凌
- doWriteChannel 改为入队 + 立即冲刷；队列 1MB/512KB 水位迟滞经
  channelWritePaused 信号通知 GUI 节流
- 新增 doSetChannelReadPaused / doOpenDirectTcpip / doForwardListen /
  doForwardCancel / acceptForwardedChannels（accept 的 channelId 使用
  0x80000000 起的高位段，避开 GUI 分配段）
- sendKeepalive 容忍 EAGAIN（暂不发送不等于断线）
- 断线/EOF/channel 错误语义与 v0.1 保持一致（既有测试全绿作回归锚）"
```

---

### 任务 4：ZzSshForwardChannel 门面与 ZzSshConnection::createForwardChannel

GUI 线程侧的 direct-tcpip channel 门面（与 `ZzSshShellChannel` 平级），以及 `ZzSshConnection` 上的创建入口。worker 侧 `doOpenDirectTcpip` 已在任务 3 就绪；本任务补门面对象、信号分发与端到端集成验证。

**文件：**
- 创建：`src/ZzSshForwardChannel.h`、`src/ZzSshForwardChannel.cpp`
- 修改：`src/ZzSshConnection.h`、`src/ZzSshConnection.cpp`
- 修改：`CMakeLists.txt`
- 测试：创建 `tests/unit/tst_ZzSshForwardChannel.cpp`、`tests/integration/tst_ZzSshForwardChannelIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSshForwardChannel.cpp`：

```cpp
#include <QtTest>

#include "ZzSshConnection.h"
#include "ZzSshForwardChannel.h"

/**
 * @brief ZzSshForwardChannel 门面创建前置条件的单元测试（无网络）。
 */
class tst_ZzSshForwardChannel : public QObject
{
    Q_OBJECT

private slots:
    void createBeforeConnectReturnsNull();
};

void tst_ZzSshForwardChannel::createBeforeConnectReturnsNull()
{
    ZzSshConnection conn;
    QVERIFY(conn.createForwardChannel(QStringLiteral("127.0.0.1"), 80,
                                      QStringLiteral("127.0.0.1"), 12345) == nullptr);
}

QTEST_GUILESS_MAIN(tst_ZzSshForwardChannel)
#include "tst_ZzSshForwardChannel.moc"
```

创建 `tests/integration/tst_ZzSshForwardChannelIT.cpp`：

```cpp
#include <QtTest>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshForwardChannel.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief direct-tcpip channel 的集成测试（Docker openssh-server）。
 *
 * 目标固定为容器内 127.0.0.1:22（sshd 自身）：读 SSH banner 验证
 * channel→本地方向；写入客户端 banner 触发 sshd 回应验证本地→channel 方向。
 */
class tst_ZzSshForwardChannelIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void directTcpipReadsBanner();
    void directTcpipWriteDirection();
    void directTcpipTargetUnreachableFailsChannelOnly();

private:
    /** @brief 建立已认证连接（自动信任主机密钥、自动提供密码）。 */
    std::unique_ptr<ZzSshConnection> makeConnected(const QString &storePath);
    /** @brief 累积通道数据直到包含 needle 或超时。 */
    bool waitForData(ZzSshForwardChannel *channel, const QByteArray &needle, int timeoutMs);

    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshForwardChannelIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshForwardChannelIT::makeConnected(const QString &storePath)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    return conn;
}

bool tst_ZzSshForwardChannelIT::waitForData(ZzSshForwardChannel *channel, const QByteArray &needle,
                                            int timeoutMs)
{
    QSignalSpy spy(channel, &ZzSshForwardChannel::dataReceived);
    QByteArray accum;
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < timeoutMs) {
        for (const QList<QVariant> &item : spy)
            accum += item.at(0).toByteArray();
        spy.clear();
        if (accum.contains(needle))
            return true;
        spy.wait(500);
    }
    qWarning() << "waitForData 超时，已收到:" << accum.left(256);
    return false;
}

void tst_ZzSshForwardChannelIT::directTcpipReadsBanner()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    ZzSshForwardChannel *channel =
        conn->createForwardChannel(QStringLiteral("127.0.0.1"), 22,
                                   QStringLiteral("127.0.0.1"), 41000);
    QVERIFY(channel != nullptr);
    QSignalSpy openSpy(channel, &ZzSshForwardChannel::opened);
    QVERIFY(openSpy.wait(10000));
    QVERIFY(waitForData(channel, QByteArray("SSH-"), 10000)); // sshd banner
}

void tst_ZzSshForwardChannelIT::directTcpipWriteDirection()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    ZzSshForwardChannel *channel =
        conn->createForwardChannel(QStringLiteral("127.0.0.1"), 22,
                                   QStringLiteral("127.0.0.1"), 41001);
    QVERIFY(channel != nullptr);
    QSignalSpy openSpy(channel, &ZzSshForwardChannel::opened);
    QVERIFY(openSpy.wait(10000));
    QVERIFY(waitForData(channel, QByteArray("SSH-"), 10000)); // 先读到服务端 banner

    // 写入客户端 banner：sshd 收到后会回应 KEXINIT（非空数据），验证本端→channel 方向
    QSignalSpy dataSpy(channel, &ZzSshForwardChannel::dataReceived);
    channel->write("SSH-2.0-zzprobe_0.1\r\n");
    QElapsedTimer timer;
    timer.start();
    qint64 received = 0;
    while (timer.elapsed() < 10000 && received == 0) {
        for (const QList<QVariant> &item : dataSpy)
            received += item.at(0).toByteArray().size();
        dataSpy.clear();
        if (received > 0)
            break;
        dataSpy.wait(500);
    }
    QVERIFY2(received > 0, "写入客户端 banner 后未收到 sshd 回应");
}

void tst_ZzSshForwardChannelIT::directTcpipTargetUnreachableFailsChannelOnly()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    // 容器内 127.0.0.1:1 无人监听：服务端打开失败 → 仅该 channel 报错（规格 §六）
    ZzSshForwardChannel *channel =
        conn->createForwardChannel(QStringLiteral("127.0.0.1"), 1,
                                   QStringLiteral("127.0.0.1"), 41002);
    QVERIFY(channel != nullptr);
    QSignalSpy errSpy(channel, &ZzSshForwardChannel::errorOccurred);
    QVERIFY(errSpy.wait(10000));
    QCOMPARE(errSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::ChannelOpenFailed));
    QCOMPARE(conn->state(), ZzSshConnection::State::Connected); // 会话不受影响
}

QTEST_GUILESS_MAIN(tst_ZzSshForwardChannelIT)
#include "tst_ZzSshForwardChannelIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshForwardChannel unit/tst_ZzSshForwardChannel.cpp)
set_tests_properties(tst_ZzSshForwardChannel PROPERTIES LABELS "unit")

zz_add_test(tst_ZzSshForwardChannelIT integration/tst_ZzSshForwardChannelIT.cpp)
target_link_libraries(tst_ZzSshForwardChannelIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshForwardChannelIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——找不到 `ZzSshForwardChannel.h`，`createForwardChannel` 不存在。

- [ ] **步骤 3：创建 `src/ZzSshForwardChannel.h` 与 `src/ZzSshForwardChannel.cpp`**

`src/ZzSshForwardChannel.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QObject>

class ZzSshConnection;

/**
 * @brief 转发数据通道（direct-tcpip / forwarded-tcpip）的 GUI 线程门面。
 *
 * 本地/动态转发由 ZzSshConnection::createForwardChannel() 创建（创建即自动
 * 发起 direct-tcpip 打开，结局为 opened() 或 errorOccurred()）；
 * 远程转发由 ZzSshForwardListener 经 ZzSshConnection::adoptForwardChannel()
 * 包装 worker 已接入的 channel（创建即视为已打开，queued 发射 opened()）。
 * 所有操作经 queued 调用转发到连接的工作线程执行。
 */
class ZzSshForwardChannel : public QObject
{
    Q_OBJECT

public:
    ~ZzSshForwardChannel() override;

    /** @brief 通道 ID（连接内唯一）。 */
    quint32 channelId() const { return m_channelId; }

public slots:
    /** @brief 写入数据（异步；经 worker 写队列冲刷，拥塞时经 writeCongestionChanged 反压）。 */
    void write(const QByteArray &data);

    /** @brief 暂停/恢复 worker 侧该 channel 的读取（socket 写缓冲背压用）。 */
    void setReadPaused(bool paused);

    /** @brief 关闭通道（异步）。完成后发射 closed()。 */
    void closeChannel();

signals:
    /** @brief 通道已打开（direct-tcpip 就绪或 forwarded-tcpip 已接入）。 */
    void opened();

    /** @brief 收到对端数据。 */
    void dataReceived(const QByteArray &data);

    /** @brief worker 写队列拥塞态变化（true=超 1MB 高水位，false=回落恢复）。 */
    void writeCongestionChanged(bool congested);

    /** @brief 通道操作失败（如服务端拒绝打开 direct-tcpip）。 */
    void errorOccurred(int code, const QString &message);

    /** @brief 通道已关闭（主动或对端 EOF）。 */
    void closed();

private:
    friend class ZzSshConnection;
    friend class tst_ZzSocketChannelPump; // 单元测试直接构造用

    /** @brief 私有构造：仅 ZzSshConnection（及测试）可创建。 */
    ZzSshForwardChannel(quint32 channelId, ZzSshConnection *connection, QObject *parent);

    ZzSshConnection *m_connection; // 非拥有
    quint32 m_channelId;
};
```

`src/ZzSshForwardChannel.cpp`：

```cpp
#include "ZzSshForwardChannel.h"

#include "ZzSshConnection.h"
#include "ZzSshConnectionWorker.h"

ZzSshForwardChannel::ZzSshForwardChannel(quint32 channelId, ZzSshConnection *connection, QObject *parent)
    : QObject(parent)
    , m_connection(connection)
    , m_channelId(channelId)
{
}

ZzSshForwardChannel::~ZzSshForwardChannel()
{
    closeChannel();
}

void ZzSshForwardChannel::write(const QByteArray &data)
{
    if (data.isEmpty())
        return;
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return; // 连接已销毁
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id, data] { w->doWriteChannel(id, data); }, Qt::QueuedConnection);
}

void ZzSshForwardChannel::setReadPaused(bool paused)
{
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return;
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id, paused] { w->doSetChannelReadPaused(id, paused); },
                              Qt::QueuedConnection);
}

void ZzSshForwardChannel::closeChannel()
{
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return;
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id] { w->doCloseChannel(id); }, Qt::QueuedConnection);
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshForwardChannel.h src/ZzSshForwardChannel.cpp)
```

- [ ] **步骤 4：修改 `src/ZzSshConnection.h`**

前置声明区追加：

```cpp
class ZzSshForwardChannel;
```

public 区（`createShellChannel()` 声明之后）追加：

```cpp
    /**
     * @brief 创建一个 direct-tcpip 转发通道（仅 Connected 状态可用）。
     * @param targetHost 转发目标主机（由 SSH 服务端发起连接）。
     * @param targetPort 转发目标端口。
     * @param originatorHost 原始来源地址（协议字段）。
     * @param originatorPort 原始来源端口。
     * @return 通道门面（parent 为本连接），创建即自动发起打开，
     *         结局为 opened() 或 errorOccurred()；未连接时返回 nullptr。
     */
    ZzSshForwardChannel *createForwardChannel(const QString &targetHost, quint16 targetPort,
                                              const QString &originatorHost, quint16 originatorPort);
```

private 区追加（`worker()` 声明之后）：

```cpp
    /** @brief 包装 worker 已接入的 forwarded-tcpip channel（远程转发用，创建即视为已打开）。 */
    ZzSshForwardChannel *adoptForwardChannel(quint32 channelId);

    /** @brief 为转发通道门面接线 worker 信号（按 channelId 过滤分发）。 */
    void wireForwardChannel(ZzSshForwardChannel *channel);
```

`friend class ZzSshShellChannel;` 之后追加：

```cpp
    friend class ZzSshForwardListener;
```

- [ ] **步骤 5：修改 `src/ZzSshConnection.cpp`**

include 区追加：

```cpp
#include "ZzSshForwardChannel.h"
```

文件末尾追加：

```cpp
void ZzSshConnection::wireForwardChannel(ZzSshForwardChannel *channel)
{
    connect(m_worker, &ZzSshConnectionWorker::directTcpipOpened, channel,
            [channel](quint32 id) {
                if (id == channel->channelId())
                    emit channel->opened();
            });
    connect(m_worker, &ZzSshConnectionWorker::channelDataReceived, channel,
            [channel](quint32 id, const QByteArray &data) {
                if (id == channel->channelId())
                    emit channel->dataReceived(data);
            });
    connect(m_worker, &ZzSshConnectionWorker::channelClosed, channel,
            [channel](quint32 id) {
                if (id == channel->channelId())
                    emit channel->closed();
            });
    connect(m_worker, &ZzSshConnectionWorker::channelErrorOccurred, channel,
            [channel](quint32 id, int code, const QString &message) {
                if (id == channel->channelId())
                    emit channel->errorOccurred(code, message);
            });
    connect(m_worker, &ZzSshConnectionWorker::channelWritePaused, channel,
            [channel](quint32 id, bool paused) {
                if (id == channel->channelId())
                    emit channel->writeCongestionChanged(paused);
            });
}

ZzSshForwardChannel *ZzSshConnection::createForwardChannel(const QString &targetHost, quint16 targetPort,
                                                           const QString &originatorHost,
                                                           quint16 originatorPort)
{
    if (m_state != State::Connected)
        return nullptr;
    auto *channel = new ZzSshForwardChannel(m_nextChannelId++, this, this);
    wireForwardChannel(channel);
    ZzSshConnectionWorker *w = m_worker;
    const quint32 id = channel->channelId();
    QMetaObject::invokeMethod(w, [w, id, targetHost, targetPort, originatorHost, originatorPort] {
        w->doOpenDirectTcpip(id, targetHost, targetPort, originatorHost, originatorPort);
    }, Qt::QueuedConnection);
    return channel;
}

ZzSshForwardChannel *ZzSshConnection::adoptForwardChannel(quint32 channelId)
{
    if (m_state != State::Connected)
        return nullptr;
    auto *channel = new ZzSshForwardChannel(channelId, this, this);
    wireForwardChannel(channel);
    // channel 已在 worker 侧打开：queued 发射 opened()，保证调用方先完成信号接线
    QMetaObject::invokeMethod(channel, [channel] { emit channel->opened(); }, Qt::QueuedConnection);
    return channel;
}
```

- [ ] **步骤 6：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`；integration 全 `Passed`，含 `tst_ZzSshForwardChannelIT` 三个用例（banner 读取、写入方向、目标不可达仅 channel 失败）。

- [ ] **步骤 7：Commit**

```bash
git add src/ZzSshForwardChannel.h src/ZzSshForwardChannel.cpp src/ZzSshConnection.h src/ZzSshConnection.cpp \
    CMakeLists.txt tests/unit/tst_ZzSshForwardChannel.cpp tests/integration/tst_ZzSshForwardChannelIT.cpp tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshForwardChannel 门面与 createForwardChannel 入口

- ZzSshForwardChannel：direct-tcpip/forwarded-tcpip channel 的 GUI 线程
  门面（write/setReadPaused/closeChannel queued 到 worker；opened/
  dataReceived/writeCongestionChanged/errorOccurred/closed 信号）
- ZzSshConnection::createForwardChannel 创建即自动打开 direct-tcpip；
  adoptForwardChannel 包装 worker 已接入的 channel（远程转发用）；
  wireForwardChannel 按 channelId 过滤分发 worker 信号
- 集成测试：容器内 127.0.0.1:22 读 SSH banner、写客户端 banner 验证
  双向贯通、目标不可达仅 channel 失败且会话保留"
```

---

### 任务 5：ZzSocks5Handshake（RFC1928 无认证子集纯函数解析器）

动态转发（-D）的协议解析器：无状态、无 QObject、纯静态函数，合法/畸形/截断输入全部可单测。两个阶段分离：`parseGreeting`（方法协商，只接受 0x00 无认证）与 `parseRequest`（CONNECT 请求，支持 IPv4/域名/IPv6）。

**文件：**
- 创建：`src/ZzSocks5Handshake.h`、`src/ZzSocks5Handshake.cpp`
- 修改：`CMakeLists.txt`
- 测试：创建 `tests/unit/tst_ZzSocks5Handshake.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSocks5Handshake.cpp`**

```cpp
#include <QtTest>

#include "ZzSocks5Handshake.h"

/**
 * @brief ZzSocks5Handshake 纯函数解析器的单元测试（合法/畸形/截断全覆盖）。
 */
class tst_ZzSocks5Handshake : public QObject
{
    Q_OBJECT

private slots:
    // greeting 阶段
    void greetingValid();
    void greetingTruncated();
    void greetingBadVersion();
    void greetingNoAcceptableMethod();
    void greetingExtraBytesPreserved();
    // request 阶段
    void requestIPv4();
    void requestDomain();
    void requestIPv6();
    void requestTruncated();
    void requestBadVersion();
    void requestUnsupportedCommand();
    void requestUnsupportedAddressType();
    // 应答构造
    void buildMethodSelectionBytes();
    void buildReplyBytes();
};

// ---- greeting ----

void tst_ZzSocks5Handshake::greetingValid()
{
    // VER=5 NMETHODS=2 METHODS={0x00, 0x02}
    const auto r = ZzSocks5Handshake::parseGreeting(QByteArray("\x05\x02\x00\x02", 4));
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Ready);
    QCOMPARE(r.method, 0x00);
    QCOMPARE(r.consumedBytes, 4);
}

void tst_ZzSocks5Handshake::greetingTruncated()
{
    QCOMPARE(ZzSocks5Handshake::parseGreeting(QByteArray()).result,
             ZzSocks5Handshake::Result::NeedMoreData);
    QCOMPARE(ZzSocks5Handshake::parseGreeting(QByteArray("\x05", 1)).result,
             ZzSocks5Handshake::Result::NeedMoreData);
    // NMETHODS=3 但只到了 2 个方法字节
    QCOMPARE(ZzSocks5Handshake::parseGreeting(QByteArray("\x05\x03\x00\x01", 4)).result,
             ZzSocks5Handshake::Result::NeedMoreData);
}

void tst_ZzSocks5Handshake::greetingBadVersion()
{
    const auto r = ZzSocks5Handshake::parseGreeting(QByteArray("\x04\x01\x00", 3));
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Error);
    QCOMPARE(r.method, 0xFF);
}

void tst_ZzSocks5Handshake::greetingNoAcceptableMethod()
{
    // 只提供 GSSAPI(0x01) 与用户名密码(0x02)：无可接受方法
    const auto r = ZzSocks5Handshake::parseGreeting(QByteArray("\x05\x02\x01\x02", 4));
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Error);
    QCOMPARE(r.method, 0xFF);
}

void tst_ZzSocks5Handshake::greetingExtraBytesPreserved()
{
    // greeting 后紧跟 request 开头：consumedBytes 只含 greeting 部分
    const QByteArray buf("\x05\x01\x00" "\x05\x01\x00", 6);
    const auto r = ZzSocks5Handshake::parseGreeting(buf);
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Ready);
    QCOMPARE(r.consumedBytes, 3);
}

// ---- request ----

void tst_ZzSocks5Handshake::requestIPv4()
{
    // VER=5 CMD=CONNECT RSV=0 ATYP=IPv4 127.0.0.1 PORT=8080
    const QByteArray buf("\x05\x01\x00\x01\x7f\x00\x00\x01\x1f\x90", 10);
    const auto r = ZzSocks5Handshake::parseRequest(buf);
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Ready);
    QCOMPARE(r.targetHost, QStringLiteral("127.0.0.1"));
    QCOMPARE(r.targetPort, 8080);
    QCOMPARE(r.consumedBytes, 10);
}

void tst_ZzSocks5Handshake::requestDomain()
{
    // ATYP=DOMAIN(3) len=11 "example.com" PORT=443
    QByteArray buf("\x05\x01\x00\x03\x0b", 5);
    buf += "example.com";
    buf += QByteArray("\x01\xbb", 2);
    const auto r = ZzSocks5Handshake::parseRequest(buf);
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Ready);
    QCOMPARE(r.targetHost, QStringLiteral("example.com"));
    QCOMPARE(r.targetPort, 443);
    QCOMPARE(r.consumedBytes, buf.size());
}

void tst_ZzSocks5Handshake::requestIPv6()
{
    // ATYP=IPv6(4) ::1 PORT=22
    QByteArray buf("\x05\x01\x00\x04", 4);
    buf += QByteArray(15, '\x00') + '\x01';
    buf += QByteArray("\x00\x16", 2);
    const auto r = ZzSocks5Handshake::parseRequest(buf);
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Ready);
    QCOMPARE(r.targetHost, QStringLiteral("::1"));
    QCOMPARE(r.targetPort, 22);
}

void tst_ZzSocks5Handshake::requestTruncated()
{
    QCOMPARE(ZzSocks5Handshake::parseRequest(QByteArray()).result,
             ZzSocks5Handshake::Result::NeedMoreData);
    QCOMPARE(ZzSocks5Handshake::parseRequest(QByteArray("\x05\x01\x00", 3)).result,
             ZzSocks5Handshake::Result::NeedMoreData);
    // IPv4 地址不完整
    QCOMPARE(ZzSocks5Handshake::parseRequest(QByteArray("\x05\x01\x00\x01\x7f\x00", 6)).result,
             ZzSocks5Handshake::Result::NeedMoreData);
    // 域名长度字节到了但域名本身不完整
    QCOMPARE(ZzSocks5Handshake::parseRequest(QByteArray("\x05\x01\x00\x03\x0bex", 7)).result,
             ZzSocks5Handshake::Result::NeedMoreData);
    // 地址完整但缺端口
    QCOMPARE(ZzSocks5Handshake::parseRequest(QByteArray("\x05\x01\x00\x01\x7f\x00\x00\x01\x1f", 9)).result,
             ZzSocks5Handshake::Result::NeedMoreData);
}

void tst_ZzSocks5Handshake::requestBadVersion()
{
    const auto r = ZzSocks5Handshake::parseRequest(QByteArray("\x04\x01\x00\x01\x7f\x00\x00\x01\x00\x50", 10));
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Error);
    QCOMPARE(r.replyCode, ZzSocks5Handshake::ReplyGeneralFailure);
}

void tst_ZzSocks5Handshake::requestUnsupportedCommand()
{
    // CMD=BIND(2) 不支持
    const auto r = ZzSocks5Handshake::parseRequest(QByteArray("\x05\x02\x00\x01\x7f\x00\x00\x01\x00\x50", 10));
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Error);
    QCOMPARE(r.replyCode, ZzSocks5Handshake::ReplyCommandNotSupported);
}

void tst_ZzSocks5Handshake::requestUnsupportedAddressType()
{
    // ATYP=0x09 非法
    const auto r = ZzSocks5Handshake::parseRequest(QByteArray("\x05\x01\x00\x09\x7f\x00\x00\x01\x00\x50", 10));
    QCOMPARE(r.result, ZzSocks5Handshake::Result::Error);
    QCOMPARE(r.replyCode, ZzSocks5Handshake::ReplyAddressTypeNotSupported);
}

// ---- 应答构造 ----

void tst_ZzSocks5Handshake::buildMethodSelectionBytes()
{
    QCOMPARE(ZzSocks5Handshake::buildMethodSelection(0x00), QByteArray("\x05\x00", 2));
    QCOMPARE(ZzSocks5Handshake::buildMethodSelection(0xFF), QByteArray("\x05\xff", 2));
}

void tst_ZzSocks5Handshake::buildReplyBytes()
{
    // VER=5 REP RSV=0 ATYP=IPv4 BND.ADDR=0.0.0.0 BND.PORT=0
    QCOMPARE(ZzSocks5Handshake::buildReply(ZzSocks5Handshake::ReplySucceeded),
             QByteArray("\x05\x00\x00\x01\x00\x00\x00\x00\x00\x00", 10));
    QCOMPARE(ZzSocks5Handshake::buildReply(0x05).at(1), 0x05);
}

QTEST_GUILESS_MAIN(tst_ZzSocks5Handshake)
#include "tst_ZzSocks5Handshake.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSocks5Handshake unit/tst_ZzSocks5Handshake.cpp)
set_tests_properties(tst_ZzSocks5Handshake PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——找不到 `ZzSocks5Handshake.h`。

- [ ] **步骤 3：创建 `src/ZzSocks5Handshake.h` 与 `src/ZzSocks5Handshake.cpp`**

`src/ZzSocks5Handshake.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QString>

/**
 * @brief SOCKS5（RFC1928 无认证子集）纯函数式解析器。
 *
 * 无状态、不持有 socket：调用方把从连接开始累计的字节流传入，
 * 解析器返回 NeedMoreData（截断）/ Ready（合法）/ Error（畸形，
 * 携带 RFC1928 REP 错误码，由调用方回写后关闭连接）。
 * 仅支持：无认证方法（0x00）、CONNECT 命令、IPv4/域名/IPv6 地址。
 */
class ZzSocks5Handshake
{
public:
    /** @brief 解析结果类别。 */
    enum class Result {
        NeedMoreData,   ///< 输入截断，需等待更多数据
        Ready,          ///< 解析成功
        Error           ///< 协议错误（按 RFC1928 回错误码后关闭连接）
    };

    /** @brief greeting（方法协商）解析结果。 */
    struct GreetingResult {
        Result result;
        quint8 method = 0xFF;   ///< 选定方法（0x00 无认证）；Error 时为 0xFF（无可接受方法）
        int consumedBytes = 0;  ///< greeting 消耗的字节数（之后的数据属于 request）
    };

    /** @brief CONNECT 请求解析结果。 */
    struct RequestResult {
        Result result;
        quint8 replyCode = ReplySucceeded;  ///< Error 时的 RFC1928 REP 码
        QString targetHost;                 ///< 目标主机（IPv4/IPv6 字面量或域名）
        quint16 targetPort = 0;             ///< 目标端口
        int consumedBytes = 0;              ///< 请求消耗的字节数（之后的数据属于应用流）
    };

    // RFC1928 REP 错误码
    static constexpr quint8 ReplySucceeded = 0x00;              ///< 成功
    static constexpr quint8 ReplyGeneralFailure = 0x01;         ///< 一般性失败
    static constexpr quint8 ReplyConnectionRefused = 0x05;      ///< 目标拒绝连接
    static constexpr quint8 ReplyCommandNotSupported = 0x07;    ///< 不支持的命令
    static constexpr quint8 ReplyAddressTypeNotSupported = 0x08;///< 不支持的地址类型

    /** @brief 解析 greeting（VER NMETHODS METHODS...），只接受无认证方法。 */
    static GreetingResult parseGreeting(const QByteArray &buffer);

    /** @brief 构造方法选择应答（VER METHOD）。 */
    static QByteArray buildMethodSelection(quint8 method);

    /** @brief 解析 CONNECT 请求（VER CMD RSV ATYP DST.ADDR DST.PORT）。 */
    static RequestResult parseRequest(const QByteArray &buffer);

    /** @brief 构造请求应答（BND.ADDR/BND.PORT 恒为 0.0.0.0:0，RFC1928 允许）。 */
    static QByteArray buildReply(quint8 replyCode);
};
```

`src/ZzSocks5Handshake.cpp`：

```cpp
#include "ZzSocks5Handshake.h"

#include <QHostAddress>

#include <cstring>

namespace {

constexpr quint8 kVersion = 0x05;
constexpr quint8 kCmdConnect = 0x01;
constexpr quint8 kAtypIPv4 = 0x01;
constexpr quint8 kAtypDomain = 0x03;
constexpr quint8 kAtypIPv6 = 0x04;
constexpr quint8 kMethodNoAuth = 0x00;
constexpr quint8 kMethodNoAcceptable = 0xFF;

} // namespace

ZzSocks5Handshake::GreetingResult ZzSocks5Handshake::parseGreeting(const QByteArray &buffer)
{
    GreetingResult r;
    if (buffer.size() < 2) {
        r.result = Result::NeedMoreData;
        return r;
    }
    if (static_cast<quint8>(buffer.at(0)) != kVersion) {
        r.result = Result::Error;
        return r;
    }
    const int nmethods = static_cast<quint8>(buffer.at(1));
    if (buffer.size() < 2 + nmethods) {
        r.result = Result::NeedMoreData;
        return r;
    }
    for (int i = 0; i < nmethods; ++i) {
        if (static_cast<quint8>(buffer.at(2 + i)) == kMethodNoAuth) {
            r.result = Result::Ready;
            r.method = kMethodNoAuth;
            r.consumedBytes = 2 + nmethods;
            return r;
        }
    }
    r.result = Result::Error; // 客户端未提供无认证方法
    r.method = kMethodNoAcceptable;
    return r;
}

QByteArray ZzSocks5Handshake::buildMethodSelection(quint8 method)
{
    QByteArray out(2, Qt::Uninitialized);
    out[0] = static_cast<char>(kVersion);
    out[1] = static_cast<char>(method);
    return out;
}

ZzSocks5Handshake::RequestResult ZzSocks5Handshake::parseRequest(const QByteArray &buffer)
{
    RequestResult r;
    if (buffer.size() < 4) {
        r.result = Result::NeedMoreData;
        return r;
    }
    if (static_cast<quint8>(buffer.at(0)) != kVersion) {
        r.result = Result::Error;
        r.replyCode = ReplyGeneralFailure;
        return r;
    }
    if (static_cast<quint8>(buffer.at(1)) != kCmdConnect) {
        r.result = Result::Error;
        r.replyCode = ReplyCommandNotSupported;
        return r;
    }
    const quint8 atyp = static_cast<quint8>(buffer.at(3));
    int hostEnd = 0; // 地址字段结束偏移（不含端口）
    switch (atyp) {
    case kAtypIPv4: {
        if (buffer.size() < 4 + 4 + 2) {
            r.result = Result::NeedMoreData;
            return r;
        }
        const auto *p = reinterpret_cast<const quint8 *>(buffer.constData() + 4);
        r.targetHost = QStringLiteral("%1.%2.%3.%4").arg(p[0]).arg(p[1]).arg(p[2]).arg(p[3]);
        hostEnd = 4 + 4;
        break;
    }
    case kAtypDomain: {
        const int len = static_cast<quint8>(buffer.at(4));
        if (buffer.size() < 5 + len + 2) {
            r.result = Result::NeedMoreData;
            return r;
        }
        r.targetHost = QString::fromUtf8(buffer.constData() + 5, len);
        hostEnd = 5 + len;
        break;
    }
    case kAtypIPv6: {
        if (buffer.size() < 4 + 16 + 2) {
            r.result = Result::NeedMoreData;
            return r;
        }
        Q_IPV6ADDR a6{};
        std::memcpy(a6.c, buffer.constData() + 4, 16);
        r.targetHost = QHostAddress(a6).toString();
        hostEnd = 4 + 16;
        break;
    }
    default:
        r.result = Result::Error;
        r.replyCode = ReplyAddressTypeNotSupported;
        return r;
    }
    const auto *pp = reinterpret_cast<const quint8 *>(buffer.constData() + hostEnd);
    r.targetPort = static_cast<quint16>((pp[0] << 8) | pp[1]);
    r.consumedBytes = hostEnd + 2;
    r.result = Result::Ready;
    r.replyCode = ReplySucceeded;
    return r;
}

QByteArray ZzSocks5Handshake::buildReply(quint8 replyCode)
{
    QByteArray out(10, Qt::Uninitialized);
    out[0] = static_cast<char>(kVersion);
    out[1] = static_cast<char>(replyCode);
    out[2] = 0x00;                    // RSV
    out[3] = static_cast<char>(kAtypIPv4);
    std::memset(out.data() + 4, 0, 6); // BND.ADDR 0.0.0.0 + BND.PORT 0
    return out;
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSocks5Handshake.h src/ZzSocks5Handshake.cpp)
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzSocks5Handshake
```

预期：`Passed`。

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSocks5Handshake.h src/ZzSocks5Handshake.cpp CMakeLists.txt tests/unit/tst_ZzSocks5Handshake.cpp tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSocks5Handshake 纯函数解析器

RFC1928 无认证子集：parseGreeting（只接受 0x00 无认证）与
parseRequest（CONNECT，IPv4/域名/IPv6）两阶段分离；截断返回
NeedMoreData、畸形返回 Error 并携带 RFC1928 REP 错误码；
buildMethodSelection/buildReply 构造应答（BND 恒为 0.0.0.0:0）。
单元测试覆盖合法/畸形/截断全部路径。"
```

---

### 任务 6：ZzSocketChannelPump（QTcpSocket ↔ channel 统一双向搬运工）

三种转发复用的数据泵（规格 §四）：`readyRead` → 写 channel；channel 数据 → 写 socket（写缓冲 1MB 水位背压，恢复水位 512KB）；worker 写队列拥塞 → `setReadEnabled(false)` 反向节流；任一端关闭/出错 → 两端联动关闭、泵自毁。单测用真实 loopback socket + 未经连接的 `ZzSshConnection`（其 worker 会静默丢弃未知 channel 的写入，正好充当 channel 侧黑洞），经 friend 直接构造 `ZzSshForwardChannel`。

**文件：**
- 创建：`src/ZzSocketChannelPump.h`、`src/ZzSocketChannelPump.cpp`
- 修改：`CMakeLists.txt`
- 测试：创建 `tests/unit/tst_ZzSocketChannelPump.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSocketChannelPump.cpp`**

```cpp
#include <QtTest>
#include <QPointer>
#include <QTcpServer>
#include <QTcpSocket>

#include "ZzSshConnection.h"
#include "ZzSshForwardChannel.h"
#include "ZzSocketChannelPump.h"

/**
 * @brief ZzSocketChannelPump 的单元测试（loopback socket + 未连接的 ZzSshConnection）。
 *
 * channel 侧数据用直接发射 dataReceived 信号的方式注入（Qt 信号是 public 成员函数）；
 * socket→channel 方向的端到端贯通由集成测试覆盖（worker 对未知 channel 的写入静默丢弃，
 * 正好充当本测试的黑洞）。
 */
class tst_ZzSocketChannelPump : public QObject
{
    Q_OBJECT

private slots:
    void channelDataReachesSocket();
    void backpressurePausesAndResumes();
    void peerCloseFinishesPump();
    void channelCloseFinishesPump();
    void congestionDisablesSocketRead();

private:
    /** @brief 建立 loopback socket 对（client 交给泵，serverSide 由测试驱动）。 */
    QTcpSocket *makeSocketPair(QTcpServer *server, QTcpSocket **serverSideOut);
};

QTcpSocket *tst_ZzSocketChannelPump::makeSocketPair(QTcpServer *server, QTcpSocket **serverSideOut)
{
    QVERIFY(server->listen(QHostAddress::LocalHost));
    auto *client = new QTcpSocket;
    client->connectToHost(QHostAddress::LocalHost, server->serverPort());
    QVERIFY(client->waitForConnected(3000));
    QVERIFY(server->waitForNewConnection(3000));
    *serverSideOut = server->nextPendingConnection();
    return client;
}

void tst_ZzSocketChannelPump::channelDataReachesSocket()
{
    ZzSshConnection conn; // 不连接：worker 对未知 channel 的写入静默丢弃
    QTcpServer server;
    QTcpSocket *serverSide = nullptr;
    QTcpSocket *client = makeSocketPair(&server, &serverSide);

    auto *channel = new ZzSshForwardChannel(1, &conn, nullptr); // friend 构造
    auto *pump = new ZzSocketChannelPump(client, channel);
    QCOMPARE(client->parent(), pump);   // 泵接管所有权
    QCOMPARE(channel->parent(), pump);

    emit channel->dataReceived(QByteArray("hello-pump"));
    QVERIFY(serverSide->waitForReadyRead(3000));
    QCOMPARE(serverSide->readAll(), QByteArray("hello-pump"));

    delete pump;
}

void tst_ZzSocketChannelPump::backpressurePausesAndResumes()
{
    ZzSshConnection conn;
    QTcpServer server;
    QTcpSocket *serverSide = nullptr;
    QTcpSocket *client = makeSocketPair(&server, &serverSide);

    auto *channel = new ZzSshForwardChannel(2, &conn, nullptr);
    auto *pump = new ZzSocketChannelPump(client, channel);
    QVERIFY(!pump->isChannelReadPaused());

    // serverSide 不读：channel 侧持续来数据，socket 写缓冲必超 1MB 高水位
    // （16MB 远超 loopback 内核缓冲，bytesToWrite 必然越线）
    const QByteArray chunk(65536, 'x');
    for (int i = 0; i < 256; ++i) { // 共 16MB
        emit channel->dataReceived(chunk);
        QCoreApplication::processEvents();
    }
    QTRY_VERIFY_WITH_TIMEOUT(pump->isChannelReadPaused(), 15000);

    // 对端开始读取：写缓冲降到恢复水位（512KB）以下后解除暂停
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < 15000 && pump->isChannelReadPaused()) {
        serverSide->waitForReadyRead(500);
        serverSide->readAll();
        QCoreApplication::processEvents();
    }
    QVERIFY(!pump->isChannelReadPaused());

    delete pump;
}

void tst_ZzSocketChannelPump::peerCloseFinishesPump()
{
    ZzSshConnection conn;
    QTcpServer server;
    QTcpSocket *serverSide = nullptr;
    QTcpSocket *client = makeSocketPair(&server, &serverSide);

    auto *channel = new ZzSshForwardChannel(3, &conn, nullptr);
    auto *pump = new ZzSocketChannelPump(client, channel);
    const QPointer<ZzSocketChannelPump> guard(pump);
    QSignalSpy finishSpy(pump, &ZzSocketChannelPump::finished);

    serverSide->disconnectFromHost(); // 对端关闭 → 泵联动关闭并自毁
    QVERIFY(finishSpy.wait(5000));
    QTRY_VERIFY_WITH_TIMEOUT(guard.isNull(), 3000); // finished 后泵 deleteLater 自毁
}

void tst_ZzSocketChannelPump::channelCloseFinishesPump()
{
    ZzSshConnection conn;
    QTcpServer server;
    QTcpSocket *serverSide = nullptr;
    QTcpSocket *client = makeSocketPair(&server, &serverSide);

    auto *channel = new ZzSshForwardChannel(4, &conn, nullptr);
    auto *pump = new ZzSocketChannelPump(client, channel);
    QSignalSpy finishSpy(pump, &ZzSocketChannelPump::finished);

    emit channel->closed(); // channel 关闭 → 泵关闭 socket 并自毁
    QCOMPARE(finishSpy.count(), 1);
    QTRY_VERIFY_WITH_TIMEOUT(client->state() != QAbstractSocket::ConnectedState, 3000);
    QCoreApplication::processEvents();
}

void tst_ZzSocketChannelPump::congestionDisablesSocketRead()
{
    ZzSshConnection conn;
    QTcpServer server;
    QTcpSocket *serverSide = nullptr;
    QTcpSocket *client = makeSocketPair(&server, &serverSide);

    auto *channel = new ZzSshForwardChannel(5, &conn, nullptr);
    auto *pump = new ZzSocketChannelPump(client, channel);

    // worker 写队列拥塞：泵停止读 socket，数据在 socket 缓冲中堆积
    emit channel->writeCongestionChanged(true);
    serverSide->write(QByteArray(4096, 'y'));
    QVERIFY(serverSide->waitForBytesWritten(3000));
    QTest::qWait(300); // 给 readyRead 一个发射机会（不应被消费）
    QVERIFY(client->bytesAvailable() > 0); // 未被泵读走

    // 拥塞解除：泵恢复读取
    emit channel->writeCongestionChanged(false);
    QTRY_VERIFY_WITH_TIMEOUT(client->bytesAvailable() == 0, 5000);

    delete pump;
}

QTEST_GUILESS_MAIN(tst_ZzSocketChannelPump)
#include "tst_ZzSocketChannelPump.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSocketChannelPump unit/tst_ZzSocketChannelPump.cpp)
set_tests_properties(tst_ZzSocketChannelPump PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——找不到 `ZzSocketChannelPump.h`。

- [ ] **步骤 3：创建 `src/ZzSocketChannelPump.h` 与 `src/ZzSocketChannelPump.cpp`**

`src/ZzSocketChannelPump.h`：

```cpp
#pragma once

#include <QObject>

class QTcpSocket;
class ZzSshForwardChannel;

/**
 * @brief QTcpSocket 与 ZzSshForwardChannel 之间的双向搬运工（三种转发复用）。
 *
 * - socket readyRead → 写 channel（worker 写队列拥塞时经 writeCongestionChanged
 *   反向暂停 socket 读取）；
 * - channel dataReceived → 写 socket；socket 写缓冲超 1MB 高水位时暂停 channel
 *   读取（setReadPaused），降到 512KB 恢复水位以下时恢复；
 * - 任一端关闭/出错 → 两端联动关闭，发射 finished() 后泵 deleteLater 自毁。
 *
 * 构造即接管 socket 与 channel 的所有权（reparent 到本对象）。
 */
class ZzSocketChannelPump : public QObject
{
    Q_OBJECT

public:
    static constexpr qint64 HighWatermark = 1024 * 1024; ///< socket 写缓冲 1MB 高水位

    ZzSocketChannelPump(QTcpSocket *socket, ZzSshForwardChannel *channel, QObject *parent = nullptr);

    /** @brief channel 读取当前是否被背压暂停（测试观测用）。 */
    bool isChannelReadPaused() const { return m_channelReadPaused; }

signals:
    /** @brief 任一端终结，两端已联动关闭；泵随后 deleteLater 自毁。 */
    void finished();

private slots:
    void onSocketReadyRead();
    void onChannelData(const QByteArray &data);
    void onSocketBytesWritten();
    void onSocketDisconnected();
    void onChannelClosed();
    void onChannelError(int code, const QString &message);
    void onCongestionChanged(bool congested);

private:
    /** @brief 联动关闭两端并自毁（幂等）。 */
    void shutdown();

    QTcpSocket *m_socket;         // 拥有（child）
    ZzSshForwardChannel *m_channel; // 拥有（child）
    bool m_channelReadPaused = false;
    bool m_finished = false;
};
```

`src/ZzSocketChannelPump.cpp`：

```cpp
#include "ZzSocketChannelPump.h"

#include <QTcpSocket>

#include "ZzSshForwardChannel.h"

ZzSocketChannelPump::ZzSocketChannelPump(QTcpSocket *socket, ZzSshForwardChannel *channel,
                                         QObject *parent)
    : QObject(parent)
    , m_socket(socket)
    , m_channel(channel)
{
    m_socket->setParent(this);
    m_channel->setParent(this);

    connect(m_socket, &QTcpSocket::readyRead, this, &ZzSocketChannelPump::onSocketReadyRead);
    connect(m_socket, &QTcpSocket::bytesWritten, this,
            [this](qint64) { onSocketBytesWritten(); });
    connect(m_socket, &QTcpSocket::disconnected, this, &ZzSocketChannelPump::onSocketDisconnected);
    connect(m_socket, &QTcpSocket::errorOccurred, this,
            [this](QAbstractSocket::SocketError) { shutdown(); });
    connect(m_channel, &ZzSshForwardChannel::dataReceived, this, &ZzSocketChannelPump::onChannelData);
    connect(m_channel, &ZzSshForwardChannel::closed, this, &ZzSocketChannelPump::onChannelClosed);
    connect(m_channel, &ZzSshForwardChannel::errorOccurred, this, &ZzSocketChannelPump::onChannelError);
    connect(m_channel, &ZzSshForwardChannel::writeCongestionChanged, this,
            &ZzSocketChannelPump::onCongestionChanged);
}

void ZzSocketChannelPump::onSocketReadyRead()
{
    if (m_finished)
        return;
    while (m_socket->bytesAvailable() > 0) {
        const QByteArray data = m_socket->read(65536);
        if (data.isEmpty())
            break;
        m_channel->write(data); // 经 worker 写队列冲刷；拥塞反压见 onCongestionChanged
    }
}

void ZzSocketChannelPump::onChannelData(const QByteArray &data)
{
    if (m_finished)
        return;
    m_socket->write(data);
    if (!m_channelReadPaused && m_socket->bytesToWrite() > HighWatermark) {
        // socket 写缓冲超 1MB：暂停 channel 读取，让对端 TCP 窗口自然节流（规格 §四）
        m_channelReadPaused = true;
        m_channel->setReadPaused(true);
    }
}

void ZzSocketChannelPump::onSocketBytesWritten()
{
    if (m_finished)
        return;
    if (m_channelReadPaused && m_socket->bytesToWrite() <= HighWatermark / 2) {
        m_channelReadPaused = false;
        m_channel->setReadPaused(false);
    }
}

void ZzSocketChannelPump::onSocketDisconnected()
{
    shutdown();
}

void ZzSocketChannelPump::onChannelClosed()
{
    shutdown();
}

void ZzSocketChannelPump::onChannelError(int, const QString &)
{
    // 单连接出错只关该连接（规格 §六）：泵自毁，隧道继续监听
    shutdown();
}

void ZzSocketChannelPump::onCongestionChanged(bool congested)
{
    if (m_finished)
        return;
    m_socket->setReadEnabled(!congested);
}

void ZzSocketChannelPump::shutdown()
{
    if (m_finished)
        return;
    m_finished = true;
    m_socket->disconnectFromHost();
    m_channel->closeChannel();
    emit finished();
    deleteLater();
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSocketChannelPump.h src/ZzSocketChannelPump.cpp)
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzSocketChannelPump
```

预期：`Passed`（5 个用例）。

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSocketChannelPump.h src/ZzSocketChannelPump.cpp CMakeLists.txt \
    tests/unit/tst_ZzSocketChannelPump.cpp tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSocketChannelPump 统一双向搬运工

QTcpSocket 与 ZzSshForwardChannel 之间的双向数据泵，本地/远程/动态
三种转发复用：
- socket readyRead 写 channel；worker 写队列拥塞（channelWritePaused）
  经 writeCongestionChanged 反向暂停 socket 读取
- channel 数据写 socket；写缓冲超 1MB 高水位暂停 channel 读取，
  降到 512KB 恢复水位以下恢复（迟滞防抖）
- 任一端关闭/出错联动关闭两端并自毁（单连接出错只关该连接）
单元测试以 loopback socket + 未连接的 ZzSshConnection 覆盖双向搬运、
背压暂停/恢复、两端联动关闭与拥塞节流。"
```

---

### 任务 7：ZzSshTunnel（本地 -L / 动态 -D 转发入口）

GUI 线程对象，一条本地/动态转发规则的运行时实体（规格 §三）：QTcpServer 监听 → 每个已接受连接开 direct-tcpip channel → 建泵。Dynamic 模式下每个连接先走 `ZzSocks5Handshake` 两阶段握手，解析出目标后才开 channel（RFC1928：reply 在目标连接建立后发送）。资源上限：单隧道最大并发连接 256（超限直接丢弃新连接）。断线时自动释放监听与全部泵并发射 `invalidated()`，重连后上层再调 `start()` 即重建（规格 §六的下层支撑）。

**文件：**
- 创建：`src/ZzSshTunnel.h`、`src/ZzSshTunnel.cpp`
- 修改：`src/ZzSshConnection.h`、`src/ZzSshConnection.cpp`（追加 `createTunnel`）
- 修改：`CMakeLists.txt`
- 测试：创建 `tests/unit/tst_ZzSshTunnel.cpp`、`tests/integration/tst_ZzSshTunnelIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSshTunnel.cpp`：

```cpp
#include <QtTest>
#include <QTcpServer>
#include <QTcpSocket>

#include "ZzSshConnection.h"
#include "ZzSshError.h"
#include "ZzSshTunnel.h"

/**
 * @brief ZzSshTunnel 的单元测试（loopback + 未连接的 ZzSshConnection）。
 */
class tst_ZzSshTunnel : public QObject
{
    Q_OBJECT

private slots:
    void localListenEmitsListening();
    void occupiedPortFailsOnlyThatTunnel();
    void invalidConnectionClosesOnlyItself();
    void dynamicConnectionLimit256();
};

void tst_ZzSshTunnel::localListenEmitsListening()
{
    ZzSshConnection conn;
    ZzSshTunnel *tunnel = conn.createTunnel(ZzSshTunnel::Type::Local,
                                            QStringLiteral("127.0.0.1"), 0,
                                            QStringLiteral("127.0.0.1"), 22);
    QVERIFY(tunnel != nullptr);
    QSignalSpy spy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(spy.count(), 1); // listen 同步完成
    QVERIFY(tunnel->listenPort() > 0);
}

void tst_ZzSshTunnel::occupiedPortFailsOnlyThatTunnel()
{
    ZzSshConnection conn;
    // 先占用一个端口
    QTcpServer blocker;
    QVERIFY(blocker.listen(QHostAddress::LocalHost));

    ZzSshTunnel *bad = conn.createTunnel(ZzSshTunnel::Type::Local,
                                         QStringLiteral("127.0.0.1"), blocker.serverPort(),
                                         QStringLiteral("127.0.0.1"), 22);
    QSignalSpy failSpy(bad, &ZzSshTunnel::failed);
    bad->start();
    QCOMPARE(failSpy.count(), 1);
    QCOMPARE(failSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::TunnelListenFailed));

    // 端口占用只影响该规则：另一条隧道正常监听（规格 §六）
    ZzSshTunnel *good = conn.createTunnel(ZzSshTunnel::Type::Local,
                                          QStringLiteral("127.0.0.1"), 0,
                                          QStringLiteral("127.0.0.1"), 22);
    QSignalSpy listenSpy(good, &ZzSshTunnel::listening);
    good->start();
    QCOMPARE(listenSpy.count(), 1);
}

void tst_ZzSshTunnel::invalidConnectionClosesOnlyItself()
{
    ZzSshConnection conn; // 未连接：createForwardChannel 返回 nullptr → 单连接失败
    ZzSshTunnel *tunnel = conn.createTunnel(ZzSshTunnel::Type::Local,
                                            QStringLiteral("127.0.0.1"), 0,
                                            QStringLiteral("127.0.0.1"), 22);
    QSignalSpy listenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(listenSpy.count(), 1);

    QSignalSpy errSpy(tunnel, &ZzSshTunnel::connectionError);
    QTcpSocket s1;
    s1.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(s1.waitForConnected(3000));
    QTRY_VERIFY_WITH_TIMEOUT(errSpy.count() == 1, 5000);
    QVERIFY(s1.waitForDisconnected(5000)); // 该连接被关闭

    // 隧道继续监听：再连一个仍触发同样的单连接错误（规格 §六）
    QTcpSocket s2;
    s2.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(s2.waitForConnected(3000));
    QTRY_VERIFY_WITH_TIMEOUT(errSpy.count() == 2, 5000);
}

void tst_ZzSshTunnel::dynamicConnectionLimit256()
{
    ZzSshConnection conn;
    ZzSshTunnel *tunnel = conn.createTunnel(ZzSshTunnel::Type::Dynamic,
                                            QStringLiteral("127.0.0.1"), 0,
                                            QString(), 0);
    QSignalSpy listenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(listenSpy.count(), 1);

    // 256 个连接各发半截 greeting（"\x05\x01"：NMETHODS 未到齐），全部停留在握手挂起态
    QList<QTcpSocket *> sockets;
    for (int i = 0; i < ZzSshTunnel::MaxConnections; ++i) {
        auto *s = new QTcpSocket;
        s->connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
        QVERIFY(s->waitForConnected(3000));
        QVERIFY(s->write("\x05\x01", 2) == 2);
        sockets.append(s);
    }
    QTRY_VERIFY_WITH_TIMEOUT(tunnel->activeConnectionCount() == ZzSshTunnel::MaxConnections,
                             30000);

    // 第 257 个连接被直接丢弃（资源上限，规格 §四）
    QTcpSocket extra;
    extra.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(extra.waitForConnected(3000));
    QVERIFY(extra.waitForDisconnected(5000));
    QCOMPARE(tunnel->activeConnectionCount(), ZzSshTunnel::MaxConnections);

    qDeleteAll(sockets);
}

QTEST_GUILESS_MAIN(tst_ZzSshTunnel)
#include "tst_ZzSshTunnel.moc"
```

创建 `tests/integration/tst_ZzSshTunnelIT.cpp`：

```cpp
#include <QtTest>
#include <QTcpSocket>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshTestServerConfig.h"
#include "ZzSshTunnel.h"

/**
 * @brief ZzSshTunnel（-L / -D）的集成测试（Docker openssh-server）。
 *
 * 转发目标固定为容器内 127.0.0.1:22（sshd banner 作为贯通标记）。
 */
class tst_ZzSshTunnelIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void localForwardEndToEnd();
    void dynamicForwardEndToEnd();
    void dynamicRejectsBadVersion();
    void disconnectInvalidatesAndReconnectRebuilds();

private:
    std::unique_ptr<ZzSshConnection> makeConnected(const QString &storePath);
    /** @brief 从 socket 精确读取 n 字节（超时返回已读部分）。 */
    QByteArray readN(QTcpSocket *socket, qsizetype n, int timeoutMs);

    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshTunnelIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshTunnelIT::makeConnected(const QString &storePath)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    return conn;
}

QByteArray tst_ZzSshTunnelIT::readN(QTcpSocket *socket, qsizetype n, int timeoutMs)
{
    QByteArray out;
    QElapsedTimer timer;
    timer.start();
    while (out.size() < n && timer.elapsed() < timeoutMs) {
        if (socket->bytesAvailable() > 0) {
            out += socket->read(n - out.size());
            continue;
        }
        socket->waitForReadyRead(200);
    }
    return out;
}

void tst_ZzSshTunnelIT::localForwardEndToEnd()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    ZzSshTunnel *tunnel = conn->createTunnel(ZzSshTunnel::Type::Local,
                                             QStringLiteral("127.0.0.1"), 0,
                                             QStringLiteral("127.0.0.1"), 22);
    QVERIFY(tunnel != nullptr);
    QSignalSpy listenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(listenSpy.count(), 1);

    QTcpSocket client;
    client.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(client.waitForConnected(5000));
    // channel→本地方向：容器 sshd banner
    QVERIFY2(readN(&client, 4, 10000).startsWith("SSH-"), "未收到 sshd banner");
    // 本地→channel 方向：写客户端 banner 触发 sshd 回应 KEXINIT
    QVERIFY(client.write("SSH-2.0-zzprobe_0.1\r\n") > 0);
    QVERIFY(client.waitForBytesWritten(5000));
    QVERIFY2(readN(&client, 8, 10000).size() >= 8, "写入后未收到 sshd 回应");
}

void tst_ZzSshTunnelIT::dynamicForwardEndToEnd()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    ZzSshTunnel *tunnel = conn->createTunnel(ZzSshTunnel::Type::Dynamic,
                                             QStringLiteral("127.0.0.1"), 0,
                                             QString(), 0);
    QVERIFY(tunnel != nullptr);
    QSignalSpy listenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(listenSpy.count(), 1);

    QTcpSocket client;
    client.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(client.waitForConnected(5000));

    // greeting：VER=5 NMETHODS=1 METHODS={0x00}
    QVERIFY(client.write("\x05\x01\x00", 3) == 3);
    QCOMPARE(readN(&client, 2, 5000), QByteArray("\x05\x00", 2)); // 选定无认证

    // request：CONNECT 127.0.0.1:22
    QVERIFY(client.write("\x05\x01\x00\x01\x7f\x00\x00\x01\x00\x16", 10) == 10);
    const QByteArray reply = readN(&client, 10, 10000);
    QCOMPARE(reply.size(), 10);
    QCOMPARE(reply.at(1), '\x00'); // REP=succeeded

    // 握手后即为应用流：容器 sshd banner
    QVERIFY2(readN(&client, 4, 10000).startsWith("SSH-"), "SOCKS5 握手后未收到 sshd banner");
}

void tst_ZzSshTunnelIT::dynamicRejectsBadVersion()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    ZzSshTunnel *tunnel = conn->createTunnel(ZzSshTunnel::Type::Dynamic,
                                             QStringLiteral("127.0.0.1"), 0,
                                             QString(), 0);
    QSignalSpy listenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(listenSpy.count(), 1);

    QTcpSocket client;
    client.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(client.waitForConnected(5000));
    QVERIFY(client.write("\x04\x01\x00", 3) == 3); // SOCKS4：协议错误
    QCOMPARE(readN(&client, 2, 5000), QByteArray("\x05\xff", 2)); // 无可接受方法
    QVERIFY(client.waitForDisconnected(5000)); // 回错误码后关闭该连接（规格 §六）
}

void tst_ZzSshTunnelIT::disconnectInvalidatesAndReconnectRebuilds()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);

    ZzSshTunnel *tunnel = conn->createTunnel(ZzSshTunnel::Type::Local,
                                             QStringLiteral("127.0.0.1"), 0,
                                             QStringLiteral("127.0.0.1"), 22);
    QSignalSpy listenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(listenSpy.count(), 1);
    const quint16 port = tunnel->listenPort();

    // 断线：监听释放、隧道失效（规格 §六）
    QSignalSpy invalidatedSpy(tunnel, &ZzSshTunnel::invalidated);
    QSignalSpy discSpy(conn.get(), &ZzSshConnection::disconnected);
    conn->disconnectFromHost();
    QVERIFY(discSpy.wait(10000));
    QTRY_VERIFY_WITH_TIMEOUT(invalidatedSpy.count() == 1, 5000);

    // 重连后重建：同一隧道对象再次 start，端到端仍贯通
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy reconnectedSpy(conn.get(), &ZzSshConnection::connected);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    QVERIFY(reconnectedSpy.wait(15000));
    QSignalSpy relistenSpy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    QCOMPARE(relistenSpy.count(), 1);
    Q_UNUSED(port); // listenPort 为 0 时重建后绑定新端口

    QTcpSocket client;
    client.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(client.waitForConnected(5000));
    QVERIFY2(readN(&client, 4, 10000).startsWith("SSH-"), "重建后未收到 sshd banner");
}

QTEST_GUILESS_MAIN(tst_ZzSshTunnelIT)
#include "tst_ZzSshTunnelIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshTunnel unit/tst_ZzSshTunnel.cpp)
set_tests_properties(tst_ZzSshTunnel PROPERTIES LABELS "unit")

zz_add_test(tst_ZzSshTunnelIT integration/tst_ZzSshTunnelIT.cpp)
target_link_libraries(tst_ZzSshTunnelIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshTunnelIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——找不到 `ZzSshTunnel.h`，`createTunnel` 不存在。

- [ ] **步骤 3：创建 `src/ZzSshTunnel.h` 与 `src/ZzSshTunnel.cpp`**

`src/ZzSshTunnel.h`：

```cpp
#pragma once

#include <QObject>

#include "ZzSocks5Handshake.h"

class QTcpServer;
class QTcpSocket;
class ZzSshConnection;
class ZzSshForwardChannel;

/**
 * @brief 本地（-L）/动态（-D）转发隧道：一条转发规则的运行时实体（GUI 线程）。
 *
 * QTcpServer 本地监听；每个已接受连接：
 * - Local：直接开 direct-tcpip channel（目标为配置的目标地址）；
 * - Dynamic：先跑 SOCKS5（RFC1928 无认证子集）两阶段握手，解析出目标后
 *   再开 direct-tcpip channel（reply 在目标连接建立后发送）；
 * 随后交给 ZzSocketChannelPump 双向搬运。
 *
 * 资源上限：单隧道最大并发连接 256（超限直接丢弃新连接，隧道继续服务）。
 * 连接断开时自动释放监听与全部泵并发射 invalidated()；重连后再次 start()
 * 即重建（断线销毁/重连重建语义的下层支撑，规格 §六）。
 */
class ZzSshTunnel : public QObject
{
    Q_OBJECT

public:
    /** @brief 隧道类型。 */
    enum class Type {
        Local,   ///< 本地转发（-L）：监听 → 固定目标
        Dynamic  ///< 动态转发（-D）：SOCKS5 入口，目标由握手解析
    };

    static constexpr int MaxConnections = 256; ///< 单隧道最大并发连接数（规格 §四）

    ~ZzSshTunnel() override;

    Type type() const { return m_type; }
    QString listenHost() const { return m_listenHost; }
    /** @brief 实际监听端口（start 成功后有效；listenPort=0 时为系统分配端口）。 */
    quint16 listenPort() const { return m_listenPort; }
    /** @brief 当前活动连接数（泵 + 打开中/握手中的挂起连接）。 */
    int activeConnectionCount() const;

public slots:
    /** @brief 开始监听（幂等）。失败发射 failed()，成功发射 listening()。 */
    void start();

    /** @brief 停止监听并关闭全部连接（幂等；不发射信号）。 */
    void stop();

signals:
    /** @brief 监听就绪。 */
    void listening(quint16 boundPort);

    /** @brief 规则级失败（监听绑定失败等）：仅该规则受影响（规格 §六）。 */
    void failed(int code, const QString &message);

    /** @brief 单连接级错误提示（目标不可达等）：该连接已关闭，隧道继续监听。 */
    void connectionError(const QString &message);

    /** @brief SSH 连接断开，隧道已自动停止；重连后可再次 start() 重建。 */
    void invalidated();

private:
    friend class ZzSshConnection;

    /** @brief 私有构造：仅 ZzSshConnection::createTunnel() 可创建。 */
    ZzSshTunnel(Type type, const QString &listenHost, quint16 listenPort,
                const QString &targetHost, quint16 targetPort,
                ZzSshConnection *connection, QObject *parent);

    /** @brief Local：等待 channel 打开的挂起连接。 */
    struct PendingConnect {
        QTcpSocket *socket = nullptr;
        ZzSshForwardChannel *channel = nullptr;
    };

    /** @brief Dynamic：SOCKS5 握手状态机（每连接一份）。 */
    struct Socks5Pending {
        QTcpSocket *socket = nullptr;
        QByteArray buffer;                    ///< 从连接开始累计的握手字节流
        bool greetingDone = false;            ///< 方法协商是否完成
        ZzSshForwardChannel *channel = nullptr; ///< 打开中的 channel（等待 opened）
    };

    void onNewConnection();
    void finishPendingConnect(ZzSshForwardChannel *channel);
    void failPendingConnect(ZzSshForwardChannel *channel, const QString &message);
    void startSocks5(QTcpSocket *socket);
    void onSocks5Data(Socks5Pending *pending);
    void dropSocks5Pending(Socks5Pending *pending);

    QTcpServer *m_server = nullptr;
    ZzSshConnection *m_connection; // 非拥有
    Type m_type;
    QString m_listenHost;
    quint16 m_listenPort;    // 当前实际端口
    quint16 m_requestedPort; // 原始请求端口（stop 后 start 重建用）
    QString m_targetHost;
    quint16 m_targetPort;
    int m_pumpCount = 0;
    QList<PendingConnect> m_pendingConnects;
    QList<Socks5Pending *> m_pendingSocks5;
};
```

`src/ZzSshTunnel.cpp`：

```cpp
#include "ZzSshTunnel.h"

#include <QTcpServer>
#include <QTcpSocket>

#include "ZzSshConnection.h"
#include "ZzSshError.h"
#include "ZzSshForwardChannel.h"
#include "ZzSocketChannelPump.h"

ZzSshTunnel::ZzSshTunnel(Type type, const QString &listenHost, quint16 listenPort,
                         const QString &targetHost, quint16 targetPort,
                         ZzSshConnection *connection, QObject *parent)
    : QObject(parent)
    , m_connection(connection)
    , m_type(type)
    , m_listenHost(listenHost)
    , m_listenPort(listenPort)
    , m_requestedPort(listenPort)
    , m_targetHost(targetHost)
    , m_targetPort(targetPort)
{
    // 断线销毁语义（规格 §六）：连接断开 → 释放监听与全部泵，发射 invalidated()；
    // 重连后上层再次 start() 即重建
    connect(m_connection, &ZzSshConnection::disconnected, this, [this](const QString &) {
        const bool hadState = m_server != nullptr || m_pumpCount > 0
                              || !m_pendingConnects.isEmpty() || !m_pendingSocks5.isEmpty();
        stop();
        if (hadState)
            emit invalidated();
    });
}

ZzSshTunnel::~ZzSshTunnel()
{
    stop();
}

int ZzSshTunnel::activeConnectionCount() const
{
    return m_pumpCount + static_cast<int>(m_pendingConnects.size())
           + static_cast<int>(m_pendingSocks5.size());
}

void ZzSshTunnel::start()
{
    if (m_server)
        return; // 幂等
    m_server = new QTcpServer(this);
    connect(m_server, &QTcpServer::newConnection, this, &ZzSshTunnel::onNewConnection);
    if (!m_server->listen(QHostAddress(m_listenHost), m_requestedPort)) {
        const QString msg = m_server->errorString();
        m_server->deleteLater();
        m_server = nullptr;
        // 端口占用等绑定失败：仅该规则失败（规格 §六），由上层标记状态
        emit failed(static_cast<int>(ZzSshErrorCode::TunnelListenFailed), msg);
        return;
    }
    m_listenPort = m_server->serverPort();
    emit listening(m_listenPort);
}

void ZzSshTunnel::stop()
{
    if (m_server) {
        m_server->close();
        m_server->deleteLater();
        m_server = nullptr;
    }
    for (ZzSocketChannelPump *pump : findChildren<ZzSocketChannelPump *>())
        delete pump; // 泵析构联动关闭 socket 与 channel
    m_pumpCount = 0;
    for (const PendingConnect &pc : m_pendingConnects) {
        pc.socket->disconnectFromHost();
        pc.socket->deleteLater();
        pc.channel->closeChannel();
        pc.channel->deleteLater();
    }
    m_pendingConnects.clear();
    for (Socks5Pending *p : m_pendingSocks5) {
        p->socket->disconnectFromHost();
        p->socket->deleteLater();
        if (p->channel) {
            p->channel->closeChannel();
            p->channel->deleteLater();
        }
        delete p;
    }
    m_pendingSocks5.clear();
    m_listenPort = m_requestedPort;
}

void ZzSshTunnel::onNewConnection()
{
    if (!m_server)
        return;
    while (m_server->hasPendingConnections()) {
        QTcpSocket *socket = m_server->nextPendingConnection();
        if (activeConnectionCount() >= MaxConnections) {
            // 资源上限（规格 §四）：超限直接丢弃，隧道继续服务既有连接
            socket->disconnectFromHost();
            socket->deleteLater();
            continue;
        }
        if (m_type == Type::Dynamic) {
            startSocks5(socket);
            continue;
        }
        // Local：channel 打开前挂起 socket 读取，避免泵建立前数据丢失
        socket->setParent(this);
        socket->setReadEnabled(false);
        ZzSshForwardChannel *channel = m_connection->createForwardChannel(
            m_targetHost, m_targetPort,
            socket->peerAddress().toString(), socket->peerPort());
        if (!channel) { // 会话未连接或已销毁
            emit connectionError(QStringLiteral("会话未连接，无法接受转发连接"));
            socket->disconnectFromHost();
            socket->deleteLater();
            continue;
        }
        PendingConnect pc;
        pc.socket = socket;
        pc.channel = channel;
        m_pendingConnects.append(pc);
        connect(channel, &ZzSshForwardChannel::opened, this,
                [this, channel] { finishPendingConnect(channel); });
        connect(channel, &ZzSshForwardChannel::errorOccurred, this,
                [this, channel](int, const QString &message) {
                    failPendingConnect(channel, message);
                });
    }
}

void ZzSshTunnel::finishPendingConnect(ZzSshForwardChannel *channel)
{
    for (qsizetype i = 0; i < m_pendingConnects.size(); ++i) {
        if (m_pendingConnects[i].channel != channel)
            continue;
        QTcpSocket *socket = m_pendingConnects[i].socket;
        m_pendingConnects.removeAt(i);
        socket->setReadEnabled(true);
        auto *pump = new ZzSocketChannelPump(socket, channel, this);
        ++m_pumpCount;
        connect(pump, &ZzSocketChannelPump::finished, this, [this] { --m_pumpCount; });
        return;
    }
}

void ZzSshTunnel::failPendingConnect(ZzSshForwardChannel *channel, const QString &message)
{
    for (qsizetype i = 0; i < m_pendingConnects.size(); ++i) {
        if (m_pendingConnects[i].channel != channel)
            continue;
        QTcpSocket *socket = m_pendingConnects[i].socket;
        m_pendingConnects.removeAt(i);
        // 单连接出错只关该连接，隧道继续监听（规格 §六）
        emit connectionError(message);
        socket->disconnectFromHost();
        socket->deleteLater();
        channel->closeChannel();
        channel->deleteLater();
        return;
    }
}

void ZzSshTunnel::startSocks5(QTcpSocket *socket)
{
    socket->setParent(this);
    auto *pending = new Socks5Pending;
    pending->socket = socket;
    m_pendingSocks5.append(pending);
    connect(socket, &QTcpSocket::readyRead, this, [this, pending] { onSocks5Data(pending); });
    connect(socket, &QTcpSocket::disconnected, this, [this, pending] { dropSocks5Pending(pending); });
}

void ZzSshTunnel::onSocks5Data(Socks5Pending *pending)
{
    if (!m_pendingSocks5.contains(pending))
        return; // 已在其他回调中销毁
    pending->buffer += pending->socket->readAll();

    if (!pending->greetingDone) {
        const ZzSocks5Handshake::GreetingResult gr = ZzSocks5Handshake::parseGreeting(pending->buffer);
        if (gr.result == ZzSocks5Handshake::Result::NeedMoreData)
            return;
        if (gr.result == ZzSocks5Handshake::Result::Error) {
            // 协议错误：回 0xFF（无可接受方法）后关闭该连接（规格 §六）
            pending->socket->write(ZzSocks5Handshake::buildMethodSelection(0xFF));
            pending->socket->disconnectFromHost();
            dropSocks5Pending(pending);
            return;
        }
        pending->socket->write(ZzSocks5Handshake::buildMethodSelection(gr.method));
        pending->buffer.remove(0, gr.consumedBytes);
        pending->greetingDone = true;
        if (pending->buffer.isEmpty())
            return;
    }

    const ZzSocks5Handshake::RequestResult rr = ZzSocks5Handshake::parseRequest(pending->buffer);
    if (rr.result == ZzSocks5Handshake::Result::NeedMoreData)
        return;
    if (rr.result == ZzSocks5Handshake::Result::Error) {
        pending->socket->write(ZzSocks5Handshake::buildReply(rr.replyCode));
        pending->socket->disconnectFromHost();
        dropSocks5Pending(pending);
        return;
    }
    pending->buffer.remove(0, rr.consumedBytes);

    // RFC1928：reply 在目标连接建立后发送——先开 direct-tcpip channel
    ZzSshForwardChannel *channel = m_connection->createForwardChannel(
        rr.targetHost, rr.targetPort,
        pending->socket->peerAddress().toString(), pending->socket->peerPort());
    if (!channel) {
        pending->socket->write(ZzSocks5Handshake::buildReply(ZzSocks5Handshake::ReplyGeneralFailure));
        pending->socket->disconnectFromHost();
        dropSocks5Pending(pending);
        return;
    }
    pending->channel = channel;
    connect(channel, &ZzSshForwardChannel::opened, this, [this, pending] {
        if (!m_pendingSocks5.contains(pending))
            return;
        pending->socket->write(ZzSocks5Handshake::buildReply(ZzSocks5Handshake::ReplySucceeded));
        if (!pending->buffer.isEmpty()) {
            pending->channel->write(pending->buffer); // 握手期间到达的应用数据
            pending->buffer.clear();
        }
        auto *pump = new ZzSocketChannelPump(pending->socket, pending->channel, this);
        ++m_pumpCount;
        connect(pump, &ZzSocketChannelPump::finished, this, [this] { --m_pumpCount; });
        m_pendingSocks5.removeAll(pending);
        delete pending;
    });
    connect(channel, &ZzSshForwardChannel::errorOccurred, this, [this, pending](int, const QString &) {
        if (!m_pendingSocks5.contains(pending))
            return;
        // 目标不可达：回 connection refused 后关闭该连接（规格 §六）
        pending->socket->write(ZzSocks5Handshake::buildReply(ZzSocks5Handshake::ReplyConnectionRefused));
        pending->socket->disconnectFromHost();
        pending->channel->closeChannel();
        pending->channel->deleteLater();
        m_pendingSocks5.removeAll(pending);
        delete pending;
    });
}

void ZzSshTunnel::dropSocks5Pending(Socks5Pending *pending)
{
    if (!m_pendingSocks5.removeAll(pending))
        return;
    if (pending->channel) {
        pending->channel->closeChannel();
        pending->channel->deleteLater();
    }
    pending->socket->deleteLater();
    delete pending;
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshTunnel.h src/ZzSshTunnel.cpp)
```

- [ ] **步骤 4：修改 `src/ZzSshConnection.h/.cpp`（追加 createTunnel）**

`src/ZzSshConnection.h`：前置声明区追加 `class ZzSshTunnel;`；include 区追加 `#include "ZzSshTunnel.h"`（`ZzSshTunnel::Type` 在参数中使用，需要完整类型）；public 区（`createForwardChannel` 声明之后）追加：

```cpp
    /**
     * @brief 创建本地/动态转发隧道（任意连接状态可创建；转发在 Connected 后真正可用）。
     * @param type Local（-L）或 Dynamic（-D SOCKS5）。
     * @param listenHost 本地监听地址。
     * @param listenPort 本地监听端口（0 = 系统分配，实际端口见 listening 信号）。
     * @param targetHost 转发目标主机（仅 Local 需要；Dynamic 忽略）。
     * @param targetPort 转发目标端口（仅 Local 需要；Dynamic 忽略）。
     * @return 隧道对象（parent 为本连接）；调用 start() 开始监听。
     */
    ZzSshTunnel *createTunnel(ZzSshTunnel::Type type, const QString &listenHost, quint16 listenPort,
                              const QString &targetHost = QString(), quint16 targetPort = 0);
```

`src/ZzSshConnection.cpp`：include 区追加 `#include "ZzSshTunnel.h"`；文件末尾追加：

```cpp
ZzSshTunnel *ZzSshConnection::createTunnel(ZzSshTunnel::Type type, const QString &listenHost,
                                           quint16 listenPort, const QString &targetHost,
                                           quint16 targetPort)
{
    return new ZzSshTunnel(type, listenHost, listenPort, targetHost, targetPort, this, this);
}
```

- [ ] **步骤 5：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`（含 `tst_ZzSshTunnel` 4 用例）；integration 全 `Passed`（含 `tst_ZzSshTunnelIT` 4 用例：-L 双向贯通、-D 端到端、SOCKS4 拒绝、断线失效+重连重建）。

- [ ] **步骤 6：Commit**

```bash
git add src/ZzSshTunnel.h src/ZzSshTunnel.cpp src/ZzSshConnection.h src/ZzSshConnection.cpp CMakeLists.txt \
    tests/unit/tst_ZzSshTunnel.cpp tests/integration/tst_ZzSshTunnelIT.cpp tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshTunnel 本地/动态转发入口

- QTcpServer 监听，Local 每连接开 direct-tcpip 建泵；Dynamic 先跑
  ZzSocks5Handshake 两阶段握手（RFC1928：reply 在目标连接建立后发送），
  协议错误按 RFC1928 回错误码后关闭该连接
- 单隧道最大并发连接 256，超限直接丢弃新连接
- 端口占用仅该规则 failed(TunnelListenFailed)；单连接出错只关该连接
- 断线自动释放监听与全部泵并发射 invalidated()，重连后 start() 重建
- 集成测试：-L/-D 端到端（容器 sshd banner）、SOCKS4 拒绝、
  断线失效与重连重建贯通"
```

---

### 任务 8：ZzSshForwardListener（远程 -R 转发）与 Docker 基建扩展

worker 侧的 `doForwardListen` / `doForwardCancel` / `acceptForwardedChannels` 已在任务 3 落地；本任务交付 GUI 线程实体 `ZzSshForwardListener`（`start` → 服务端监听；泵周期接入的 forwarded-tcpip channel → QTcpSocket 连本地目标 → 建泵；`stop` → cancel）、Docker 基建扩展（显式 AllowTcpForwarding、新增拒绝转发容器、安装 socat 作回显目标）与端到端集成测试。

**文件：**
- 创建：`src/ZzSshForwardListener.h`、`src/ZzSshForwardListener.cpp`
- 修改：`src/ZzSshConnection.h`、`src/ZzSshConnection.cpp`（追加 `createForwardListener`）
- 修改：`CMakeLists.txt`
- 修改：`tests/integration/ZzSshTestServerConfig.h`、`tests/integration/ZzSshTestServerConfig.cpp`
- 修改：`tests/integration/docker/Dockerfile`、`tests/integration/docker/entrypoint.sh`、`tests/integration/docker/run-integration-tests.sh`
- 测试：创建 `tests/integration/tst_ZzSshForwardListenerIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/integration/tst_ZzSshForwardListenerIT.cpp`**

```cpp
#include <QtTest>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshForwardChannel.h"
#include "ZzSshForwardListener.h"
#include "ZzSshShellChannel.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief ZzSshForwardListener（-R）的集成测试（Docker openssh-server）。
 *
 * 容器内通过 shell channel 用 socat 连接服务端监听端口，驱动端到端数据流；
 * 本地目标为测试进程内的 QTcpServer。
 */
class tst_ZzSshForwardListenerIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void remoteForwardEndToEnd();
    void remoteForwardRejectedByServer();
    void cancelStopsForwarding();

private:
    std::unique_ptr<ZzSshConnection> makeConnected(const QString &storePath, quint16 port);
    ZzSshShellChannel *openShellOrSkip(ZzSshConnection *conn);
    bool waitForShellData(ZzSshShellChannel *channel, const QByteArray &needle, int timeoutMs);

    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshForwardListenerIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshForwardListenerIT::makeConnected(const QString &storePath,
                                                                           quint16 port)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    conn->connectToHost(m_cfg.host, port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    return conn;
}

ZzSshShellChannel *tst_ZzSshForwardListenerIT::openShellOrSkip(ZzSshConnection *conn)
{
    ZzSshShellChannel *channel = conn->createShellChannel();
    if (!channel)
        return nullptr;
    QSignalSpy openSpy(channel, &ZzSshShellChannel::shellOpened);
    channel->openShell(QStringLiteral("xterm-256color"), 80, 24);
    if (!openSpy.wait(10000))
        return nullptr;
    return channel;
}

bool tst_ZzSshForwardListenerIT::waitForShellData(ZzSshShellChannel *channel,
                                                  const QByteArray &needle, int timeoutMs)
{
    QSignalSpy spy(channel, &ZzSshShellChannel::dataReceived);
    QByteArray accum;
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < timeoutMs) {
        for (const QList<QVariant> &item : spy)
            accum += item.at(0).toByteArray();
        spy.clear();
        if (accum.contains(needle))
            return true;
        spy.wait(500);
    }
    qWarning() << "waitForShellData 超时，已收到:" << accum.right(512);
    return false;
}

void tst_ZzSshForwardListenerIT::remoteForwardEndToEnd()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")), m_cfg.port);
    QVERIFY(conn != nullptr);

    // 本地目标：测试进程内 QTcpServer
    QTcpServer target;
    QVERIFY(target.listen(QHostAddress::LocalHost));

    ZzSshForwardListener *listener =
        conn->createForwardListener(QStringLiteral("127.0.0.1"), 0,
                                    QStringLiteral("127.0.0.1"), target.serverPort());
    QVERIFY(listener != nullptr);
    QSignalSpy listenSpy(listener, &ZzSshForwardListener::listening);
    QSignalSpy acceptedSpy(listener, &ZzSshForwardListener::connectionAccepted);
    listener->start();
    QVERIFY(listenSpy.wait(10000));
    const quint16 boundPort = listenSpy.first().at(0).value<quint16>();
    QVERIFY(boundPort > 0);

    // 容器内用 socat 连接服务端监听端口（经 shell channel 驱动）
    ZzSshShellChannel *shell = openShellOrSkip(conn.get());
    QVERIFY(shell != nullptr);
    shell->write(QStringLiteral("socat - TCP:127.0.0.1:%1\n").arg(boundPort).toUtf8());

    // 远端 → 本地目标方向
    QVERIFY2(target.waitForNewConnection(10000), "本地目标未收到转发连接");
    QTRY_VERIFY_WITH_TIMEOUT(acceptedSpy.count() == 1, 5000);
    QTcpSocket *targetSide = target.nextPendingConnection();
    QVERIFY(targetSide != nullptr);
    QVERIFY(targetSide->write("ZZ_R2L_MARK\n") > 0);
    QVERIFY(waitForShellData(shell, QByteArray("ZZ_R2L_MARK"), 10000)); // 经 socat 显示到 shell

    // 本地目标 → 远端方向：shell 键盘输入经 socat 到达 targetSide
    shell->write("ZZ_L2R_MARK\n");
    QVERIFY2(targetSide->waitForReadyRead(10000), "本地目标未收到远端数据");
    QVERIFY(targetSide->readAll().contains("ZZ_L2R_MARK"));
}

void tst_ZzSshForwardListenerIT::remoteForwardRejectedByServer()
{
    if (m_cfg.noForwardPort == 0)
        QSKIP("未设置 ZZSSH_TEST_NOFWD_PORT（使用 run-integration-tests.sh 运行）");
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts_nofwd.json")),
                              m_cfg.noForwardPort);
    QVERIFY(conn != nullptr);

    // AllowTcpForwarding no：规则级失败，会话保留（规格 §六）
    ZzSshForwardListener *listener =
        conn->createForwardListener(QStringLiteral("127.0.0.1"), 18080,
                                    QStringLiteral("127.0.0.1"), 3000);
    QVERIFY(listener != nullptr);
    QSignalSpy failSpy(listener, &ZzSshForwardListener::failed);
    listener->start();
    QVERIFY(failSpy.wait(10000));
    QCOMPARE(failSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::ForwardListenFailed));
    QCOMPARE(conn->state(), ZzSshConnection::State::Connected);
}

void tst_ZzSshForwardListenerIT::cancelStopsForwarding()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")), m_cfg.port);
    QVERIFY(conn != nullptr);

    QTcpServer target;
    QVERIFY(target.listen(QHostAddress::LocalHost));

    ZzSshForwardListener *listener =
        conn->createForwardListener(QStringLiteral("127.0.0.1"), 0,
                                    QStringLiteral("127.0.0.1"), target.serverPort());
    QVERIFY(listener != nullptr);
    QSignalSpy listenSpy(listener, &ZzSshForwardListener::listening);
    listener->start();
    QVERIFY(listenSpy.wait(10000));
    const quint16 boundPort = listenSpy.first().at(0).value<quint16>();

    listener->stop();
    QTest::qWait(500); // 等 cancel 经 queued 调用抵达工作线程并生效

    ZzSshShellChannel *shell = openShellOrSkip(conn.get());
    QVERIFY(shell != nullptr);
    shell->write(QStringLiteral("echo probe | socat -T 2 - TCP:127.0.0.1:%1\n")
                     .arg(boundPort)
                     .toUtf8());
    // cancel 后服务端端口已关闭：本地目标不应再收到连接
    QVERIFY2(!target.waitForNewConnection(4000), "cancel 后本地目标仍收到转发连接");
}

QTEST_GUILESS_MAIN(tst_ZzSshForwardListenerIT)
#include "tst_ZzSshForwardListenerIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshForwardListenerIT integration/tst_ZzSshForwardListenerIT.cpp)
target_link_libraries(tst_ZzSshForwardListenerIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshForwardListenerIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
```

预期：编译失败——找不到 `ZzSshForwardListener.h`，`createForwardListener` 不存在。

- [ ] **步骤 3：创建 `src/ZzSshForwardListener.h` 与 `src/ZzSshForwardListener.cpp`**

`src/ZzSshForwardListener.h`：

```cpp
#pragma once

#include <QObject>

class QTcpSocket;
class ZzSshConnection;
class ZzSshForwardChannel;

/**
 * @brief 远程转发（-R）：一条远程转发规则的运行时实体（GUI 线程）。
 *
 * start() 请求 SSH 服务端监听指定地址端口；服务端接受的每个连接由工作线程
 * 在泵周期内 forward_accept 接入为 forwarded-tcpip channel，本对象收到通知后
 * 用 QTcpSocket 连接本地目标并交给 ZzSocketChannelPump 双向搬运。
 * stop() 取消服务端监听（forward_cancel）并关闭全部连接。
 * 连接断开时自动停止并发射 invalidated()；重连后再次 start() 即重建。
 */
class ZzSshForwardListener : public QObject
{
    Q_OBJECT

public:
    ~ZzSshForwardListener() override;

    /** @brief 监听器 ID（连接内唯一，与 channelId 共用分配序列）。 */
    quint32 listenerId() const { return m_listenerId; }

    QString listenHost() const { return m_listenHost; }
    quint16 listenPort() const { return m_listenPort; }
    /** @brief 当前活动转发连接数。 */
    int activeConnectionCount() const { return m_pumpCount; }

public slots:
    /** @brief 请求服务端开始监听（幂等）。结局为 listening() 或 failed()。 */
    void start();

    /** @brief 取消服务端监听并关闭全部连接（幂等；不发射信号）。 */
    void stop();

    // 以下三个槽由 ZzSshConnection 按 listenerId 过滤后接线调用（库内部使用）
    /** @brief 服务端监听就绪。 */
    void onListening(quint16 boundPort);
    /** @brief 服务端拒绝监听（AllowTcpForwarding off 等）。 */
    void onListenFailed(int code, const QString &message);
    /** @brief 工作线程接入新 forwarded-tcpip channel。 */
    void onForwarded(quint32 channelId);

signals:
    /** @brief 服务端监听就绪（boundPort 为实际绑定端口，请求 0 时有用）。 */
    void listening(quint16 boundPort);

    /** @brief 规则级失败：仅该规则受影响，会话保留（规格 §六）。 */
    void failed(int code, const QString &message);

    /** @brief 单连接级错误提示（本地目标不可达等）：该连接已关闭，监听继续。 */
    void connectionError(const QString &message);

    /** @brief 新转发连接已建立（观测用；channel 所有权已归泵，勿 delete）。 */
    void connectionAccepted(ZzSshForwardChannel *channel);

    /** @brief SSH 连接断开，监听器已自动停止；重连后可再次 start() 重建。 */
    void invalidated();

private:
    friend class ZzSshConnection;

    /** @brief 私有构造：仅 ZzSshConnection::createForwardListener() 可创建。 */
    ZzSshForwardListener(quint32 listenerId, const QString &listenHost, quint16 listenPort,
                         const QString &targetHost, quint16 targetPort,
                         ZzSshConnection *connection, QObject *parent);

    quint32 m_listenerId;
    QString m_listenHost;
    quint16 m_listenPort;
    QString m_targetHost;
    quint16 m_targetPort;
    ZzSshConnection *m_connection; // 非拥有
    bool m_started = false;
    int m_pumpCount = 0;
};
```

`src/ZzSshForwardListener.cpp`：

```cpp
#include "ZzSshForwardListener.h"

#include <QTcpSocket>

#include "ZzSshConnection.h"
#include "ZzSshConnectionWorker.h"
#include "ZzSshForwardChannel.h"
#include "ZzSocketChannelPump.h"

ZzSshForwardListener::ZzSshForwardListener(quint32 listenerId, const QString &listenHost,
                                           quint16 listenPort, const QString &targetHost,
                                           quint16 targetPort, ZzSshConnection *connection,
                                           QObject *parent)
    : QObject(parent)
    , m_listenerId(listenerId)
    , m_listenHost(listenHost)
    , m_listenPort(listenPort)
    , m_targetHost(targetHost)
    , m_targetPort(targetPort)
    , m_connection(connection)
{
    // 断线销毁语义（规格 §六）：worker teardown 已 cancel 服务端监听，
    // 本对象释放本地泵并通知上层；重连后上层再次 start() 即重建
    connect(m_connection, &ZzSshConnection::disconnected, this, [this](const QString &) {
        const bool hadState = m_started || m_pumpCount > 0;
        stop();
        if (hadState)
            emit invalidated();
    });
}

ZzSshForwardListener::~ZzSshForwardListener()
{
    stop();
}

void ZzSshForwardListener::start()
{
    if (m_started)
        return;
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return; // 连接已销毁
    m_started = true;
    const quint32 id = m_listenerId;
    const QString host = m_listenHost;
    const quint16 port = m_listenPort;
    QMetaObject::invokeMethod(w, [w, id, host, port] { w->doForwardListen(id, host, port); },
                              Qt::QueuedConnection);
}

void ZzSshForwardListener::stop()
{
    if (m_started) {
        m_started = false;
        ZzSshConnectionWorker *w = m_connection->worker();
        if (w) {
            const quint32 id = m_listenerId;
            QMetaObject::invokeMethod(w, [w, id] { w->doForwardCancel(id); }, Qt::QueuedConnection);
        }
    }
    for (ZzSocketChannelPump *pump : findChildren<ZzSocketChannelPump *>())
        delete pump; // 泵析构联动关闭 socket 与 channel
    m_pumpCount = 0;
}

void ZzSshForwardListener::onListening(quint16 boundPort)
{
    emit listening(boundPort);
}

void ZzSshForwardListener::onListenFailed(int code, const QString &message)
{
    m_started = false;
    emit failed(code, message);
}

void ZzSshForwardListener::onForwarded(quint32 channelId)
{
    // 先连本地目标，成功后才包装 channel 建泵；失败只关该连接（规格 §六）
    auto *socket = new QTcpSocket(this);
    connect(socket, &QTcpSocket::connected, this, [this, socket, channelId] {
        ZzSshForwardChannel *channel = m_connection->adoptForwardChannel(channelId);
        if (!channel) { // 会话已断
            socket->disconnectFromHost();
            socket->deleteLater();
            return;
        }
        auto *pump = new ZzSocketChannelPump(socket, channel, this);
        ++m_pumpCount;
        connect(pump, &ZzSocketChannelPump::finished, this, [this] { --m_pumpCount; });
        emit connectionAccepted(channel);
    });
    connect(socket, &QTcpSocket::errorOccurred, this,
            [this, socket, channelId](QAbstractSocket::SocketError) {
                ZzSshForwardChannel *channel = m_connection->adoptForwardChannel(channelId);
                if (channel) { // 包装后立即关闭：回收 worker 侧 channel
                    channel->closeChannel();
                    channel->deleteLater();
                }
                emit connectionError(QStringLiteral("转发目标 %1:%2 不可达")
                                         .arg(m_targetHost)
                                         .arg(m_targetPort));
                socket->deleteLater();
            });
    socket->connectToHost(m_targetHost, m_targetPort);
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshForwardListener.h src/ZzSshForwardListener.cpp)
```

- [ ] **步骤 4：修改 `src/ZzSshConnection.h/.cpp`（追加 createForwardListener）**

`src/ZzSshConnection.h`：前置声明区追加 `class ZzSshForwardListener;`；public 区（`createTunnel` 声明之后）追加：

```cpp
    /**
     * @brief 创建远程转发监听器（-R，仅 Connected 状态可用）。
     * @param listenHost 服务端监听地址。
     * @param listenPort 服务端监听端口（0 = 服务端分配，实际端口见 listening 信号）。
     * @param targetHost 本地转发目标主机。
     * @param targetPort 本地转发目标端口。
     * @return 监听器对象（parent 为本连接）；未连接时返回 nullptr。调用 start() 开始。
     */
    ZzSshForwardListener *createForwardListener(const QString &listenHost, quint16 listenPort,
                                                const QString &targetHost, quint16 targetPort);
```

`src/ZzSshConnection.cpp`：include 区追加 `#include "ZzSshForwardListener.h"`；文件末尾追加：

```cpp
ZzSshForwardListener *ZzSshConnection::createForwardListener(const QString &listenHost,
                                                             quint16 listenPort,
                                                             const QString &targetHost,
                                                             quint16 targetPort)
{
    if (m_state != State::Connected)
        return nullptr;
    auto *listener = new ZzSshForwardListener(m_nextChannelId++, listenHost, listenPort,
                                              targetHost, targetPort, this, this);
    connect(m_worker, &ZzSshConnectionWorker::forwardListening, listener,
            [listener](quint32 id, quint16 boundPort) {
                if (id == listener->listenerId())
                    listener->onListening(boundPort);
            });
    connect(m_worker, &ZzSshConnectionWorker::forwardListenFailed, listener,
            [listener](quint32 id, int code, const QString &message) {
                if (id == listener->listenerId())
                    listener->onListenFailed(code, message);
            });
    connect(m_worker, &ZzSshConnectionWorker::forwardedTcpipAccepted, listener,
            [listener](quint32 id, quint32 channelId, const QString &, quint16) {
                if (id == listener->listenerId())
                    listener->onForwarded(channelId);
            });
    return listener;
}
```

- [ ] **步骤 5：修改 Docker 基建（Dockerfile / entrypoint.sh / run-integration-tests.sh / 测试配置）**

`tests/integration/docker/Dockerfile`：安装 socat（性能测试回显目标与 -R 测试驱动端），并显式开启转发：

```dockerfile
RUN apt-get update \
    && apt-get install -y --no-install-recommends openssh-server procps socat \
    && rm -rf /var/lib/apt/lists/*
```

在 sshd_config 调整段追加一行（与既有两行 sed 并列）：

```dockerfile
    && sed -i 's/^#\?AllowTcpForwarding.*/AllowTcpForwarding yes/' /etc/ssh/sshd_config
```

`tests/integration/docker/entrypoint.sh`：完整替换为：

```bash
#!/bin/bash
# 容器入口：
#   ZZ_REGEN_HOSTKEYS=1       启动前重新生成主机密钥（用于密钥变更测试）
#   ZZ_DISABLE_FORWARDING=1   关闭 TCP 转发（用于"服务端拒绝转发"错误路径测试）
set -e

if [ "${ZZ_REGEN_HOSTKEYS:-0}" = "1" ]; then
    rm -f /etc/ssh/ssh_host_*
    ssh-keygen -A
fi

if [ "${ZZ_DISABLE_FORWARDING:-0}" = "1" ]; then
    sed -i 's/^#\?AllowTcpForwarding.*/AllowTcpForwarding no/' /etc/ssh/sshd_config
fi

exec /usr/sbin/sshd -D -e
```

`tests/integration/docker/run-integration-tests.sh`：完整替换为：

```bash
#!/bin/bash
# 用法: tests/integration/docker/run-integration-tests.sh <build目录>
# 构建测试镜像、启动三个容器（主容器 + 密钥变更容器 + 拒绝转发容器）、
# 运行 integration/perf 标签的 ctest、清理容器。
set -uo pipefail

BUILD_DIR="${1:?用法: $0 <build目录>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

IMAGE="zzsshcore-test-sshd:latest"
CONTAINER="zzsshcore-sshd"
CHANGED_CONTAINER="zzsshcore-sshd-chg"
NOFWD_CONTAINER="zzsshcore-sshd-nofwd"
PORT=2222
CHANGED_PORT=2223
NOFWD_PORT=2224

# fresh clone 后私钥权限为 0644，统一收紧（libssh2 不检查，但保持一致性）
chmod 600 "${SCRIPT_DIR}/keys/id_ed25519" || true

echo "== 构建测试镜像 =="
docker build -t "${IMAGE}" "${SCRIPT_DIR}" || exit 1

docker rm -f "${CONTAINER}" "${CHANGED_CONTAINER}" "${NOFWD_CONTAINER}" >/dev/null 2>&1 || true

echo "== 启动测试容器（127.0.0.1:${PORT} / ${CHANGED_PORT} / ${NOFWD_PORT}）=="
docker run -d --name "${CONTAINER}" -p "127.0.0.1:${PORT}:22" "${IMAGE}" || exit 1
docker run -d --name "${CHANGED_CONTAINER}" -e ZZ_REGEN_HOSTKEYS=1 -p "127.0.0.1:${CHANGED_PORT}:22" "${IMAGE}" || exit 1
docker run -d --name "${NOFWD_CONTAINER}" -e ZZ_DISABLE_FORWARDING=1 -p "127.0.0.1:${NOFWD_PORT}:22" "${IMAGE}" || exit 1

# 等待三个容器的 sshd 就绪
READY=0
for _ in $(seq 1 30); do
    if (exec 3<>"/dev/tcp/127.0.0.1/${PORT}") 2>/dev/null \
       && (exec 4<>"/dev/tcp/127.0.0.1/${CHANGED_PORT}") 2>/dev/null \
       && (exec 5<>"/dev/tcp/127.0.0.1/${NOFWD_PORT}") 2>/dev/null; then READY=1; break; fi
    sleep 1
done
if [ "${READY}" != "1" ]; then
    echo "错误：测试容器 30 秒内未就绪"
    docker rm -f "${CONTAINER}" "${CHANGED_CONTAINER}" "${NOFWD_CONTAINER}" >/dev/null 2>&1
    exit 1
fi

export ZZSSH_TEST_HOST=127.0.0.1
export ZZSSH_TEST_PORT="${PORT}"
export ZZSSH_TEST_CHANGED_PORT="${CHANGED_PORT}"
export ZZSSH_TEST_NOFWD_PORT="${NOFWD_PORT}"
export ZZSSH_TEST_USER=zztest
export ZZSSH_TEST_PASSWORD=zzpass123
export ZZSSH_TEST_KEY_PATH="${SCRIPT_DIR}/keys/id_ed25519"
export ZZSSH_TEST_MAIN_CONTAINER="${CONTAINER}"
export ZZSSH_TEST_CHANGED_CONTAINER="${CHANGED_CONTAINER}"

echo "== 运行集成与性能测试 =="
ctest --test-dir "${BUILD_DIR}" -L 'integration|perf' --output-on-failure
STATUS=$?

echo "== 清理容器 =="
docker rm -f "${CONTAINER}" "${CHANGED_CONTAINER}" "${NOFWD_CONTAINER}" >/dev/null 2>&1 || true
exit ${STATUS}
```

`tests/integration/ZzSshTestServerConfig.h`：在 `changedPort` 成员之后追加：

```cpp
    quint16 noForwardPort = 0;  ///< ZZSSH_TEST_NOFWD_PORT（拒绝转发容器，0 表示未配置）
```

`tests/integration/ZzSshTestServerConfig.cpp`：在 `c.changedPort = ...` 行之后追加：

```cpp
    c.noForwardPort = static_cast<quint16>(qEnvironmentVariableIntValue("ZZSSH_TEST_NOFWD_PORT"));
```

- [ ] **步骤 6：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`；integration 全 `Passed`，含 `tst_ZzSshForwardListenerIT` 三用例（-R 双向贯通、AllowTcpForwarding off 拒绝且会话保留、cancel 后不再接入）。

- [ ] **步骤 7：Commit**

```bash
git add src/ZzSshForwardListener.h src/ZzSshForwardListener.cpp src/ZzSshConnection.h src/ZzSshConnection.cpp \
    CMakeLists.txt tests/integration/ZzSshTestServerConfig.h tests/integration/ZzSshTestServerConfig.cpp \
    tests/integration/docker/Dockerfile tests/integration/docker/entrypoint.sh \
    tests/integration/docker/run-integration-tests.sh \
    tests/integration/tst_ZzSshForwardListenerIT.cpp tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshForwardListener 远程转发与 Docker 拒绝转发容器

- ZzSshForwardListener（GUI 线程）：start 请求服务端监听，泵周期接入的
  forwarded-tcpip channel 经 QTcpSocket 连本地目标后建泵复用
  ZzSocketChannelPump；stop 即 forward_cancel；断线自动停止并发射
  invalidated()，重连后 start 重建
- 本地目标不可达只关该连接；服务端拒绝（AllowTcpForwarding off）经
  failed(ForwardListenFailed) 上报且会话保留
- Docker 基建：镜像安装 socat、显式 AllowTcpForwarding yes；entrypoint
  支持 ZZ_DISABLE_FORWARDING=1；run-integration-tests.sh 追加第三容器
  （127.0.0.1:2224）并导出 ZZSSH_TEST_NOFWD_PORT
- 集成测试：-R 双向贯通（容器内 socat 驱动）、服务端拒绝、cancel 生效"
```

---

### 任务 9：转发性能门控与记录（规格 §八）

四项门控（本地 Docker 回环环境下的 ZzSshCore 层指标，Release 构建才有效，不达标即测试失败）：

| 测试项 | 阈值 | 说明 |
| ------ | ---- | ---- |
| `forward-setup-latency` | 平均 ≤ 200 ms | 连接成功后 createTunnel→listening 的就绪时间，5 次取平均 |
| `forward-echo-throughput` | ≥ 50 MB/s | -L 隧道经容器内 socat 回显 64MB 的双向吞吐（口径同既有 shell-echo-throughput） |
| `forward-concurrent-256` | 256 全通、第 257 被拒、关闭后计数归零 | 资源上限路径：无泄漏、无错数据 |
| `shell-connect-regression` / `shell-throughput-regression` | ≤ 5% | 本进程复测 shell 两项指标，对比 records 目录中变更前基线（git_commit 不同于当前构建的最新 `*-zzsshcore.json`）；无基线则 QSKIP 该用例 |

结果写入 `tests/perf/records/YYYY-MM-DD-zzforward.json`（阈值、实测值、环境信息、git commit hash、时间）并提交。

**文件：**
- 创建：`tests/perf/tst_ZzForwardPerf.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：创建 `tests/perf/tst_ZzForwardPerf.cpp`**

```cpp
#include <QtTest>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QThread>

#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

#if defined(Q_OS_MACOS)
#  include <sys/sysctl.h>
#elif defined(Q_OS_WIN)
#  include <windows.h>
#endif

#include "ZzSshConnection.h"
#include "ZzSshShellChannel.h"
#include "ZzSshTestServerConfig.h"
#include "ZzSshTunnel.h"

#ifndef ZZ_PERF_RECORDS_DIR
#  define ZZ_PERF_RECORDS_DIR "."
#endif
#ifndef ZZ_GIT_COMMIT
#  define ZZ_GIT_COMMIT "unknown"
#endif
#ifndef ZZ_BUILD_TYPE
#  define ZZ_BUILD_TYPE "unknown"
#endif

namespace {

/** @brief 返回物理内存总量（MB），无法获取时返回 -1。 */
qint64 totalMemoryMB()
{
#if defined(Q_OS_LINUX)
    QFile f(QStringLiteral("/proc/meminfo"));
    if (f.open(QIODevice::ReadOnly)) {
        const QByteArray content = f.readAll();
        const qsizetype pos = content.indexOf("MemTotal:");
        if (pos >= 0) {
            const QByteArray line = content.mid(pos, content.indexOf('\n', pos) - pos);
            return line.split(' ').filter([](const QByteArray &p) { return !p.isEmpty(); })
                       .value(1)
                       .toLongLong()
                   / 1024;
        }
    }
    return -1;
#elif defined(Q_OS_MACOS)
    int mib[2] = {CTL_HW, HW_MEMSIZE};
    int64_t mem = 0;
    size_t len = sizeof(mem);
    if (sysctl(mib, 2, &mem, &len, nullptr, 0) == 0)
        return mem / 1048576;
    return -1;
#elif defined(Q_OS_WIN)
    MEMORYSTATUSEX status{sizeof(status)};
    if (GlobalMemoryStatusEx(&status))
        return static_cast<qint64>(status.ullTotalPhys / 1048576);
    return -1;
#else
    return -1;
#endif
}

/** @brief 返回编译器描述字符串。 */
QString compilerString()
{
#if defined(Q_CC_CLANG)
    return QStringLiteral("Clang %1.%2").arg(__clang_major__).arg(__clang_minor__);
#elif defined(Q_CC_GNU)
    return QStringLiteral("GCC %1.%2").arg(__GNUC__).arg(__GNUC_MINOR__);
#elif defined(Q_CC_MSVC)
    return QStringLiteral("MSVC %1").arg(_MSC_VER);
#else
    return QStringLiteral("unknown");
#endif
}

} // namespace

/**
 * @brief 端口转发性能门控测试（规格 §八：Release 构建、阈值失败即测试失败、结果落盘）。
 */
class tst_ZzForwardPerf : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void forwardSetupLatency();
    void forwardEchoThroughput();
    void concurrentConnections256();
    void shellRegressionVsBaseline();
    void cleanupTestCase();

private:
    std::unique_ptr<ZzSshConnection> connectOnce(const QString &storePath, qint64 *elapsedMs);
    /** @brief 经 shell channel 在容器内启动 socat 回显服务（127.0.0.1:18222）。 */
    bool startEchoServer(ZzSshConnection *conn);
    /** @brief 创建指向容器内回显服务的 -L 隧道（返回 nullptr 表示失败）。 */
    ZzSshTunnel *startEchoTunnel(ZzSshConnection *conn);
    /** @brief 追加一条性能记录。 */
    void addRecord(const QString &name, const QString &unit, double threshold,
                   double measured, bool passed);

    ZzSshTestServerConfig m_cfg;
    QJsonArray m_records;

    static constexpr double SETUP_LATENCY_THRESHOLD_MS = 200.0;  ///< 转发建立延迟阈值
    static constexpr double THROUGHPUT_THRESHOLD_MBPS = 50.0;    ///< 转发吞吐阈值
    static constexpr qint64 THROUGHPUT_BYTES = 64LL * 1024 * 1024; ///< 吞吐测量数据量：64MB
    static constexpr double REGRESSION_TOLERANCE = 0.05;         ///< shell 回归容忍 5%
};

void tst_ZzForwardPerf::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过性能测试（使用 run-integration-tests.sh 运行）");
    if (QStringLiteral(ZZ_BUILD_TYPE) != QLatin1String("Release"))
        QSKIP("性能测试仅在 Release 构建下有效（规格 §八）");
}

std::unique_ptr<ZzSshConnection> tst_ZzForwardPerf::connectOnce(const QString &storePath,
                                                                qint64 *elapsedMs)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    QElapsedTimer timer;
    timer.start();
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    *elapsedMs = timer.elapsed();
    return conn;
}

bool tst_ZzForwardPerf::startEchoServer(ZzSshConnection *conn)
{
    ZzSshShellChannel *shell = conn->createShellChannel(); // parent 为 conn，随连接存活
    if (!shell)
        return false;
    QSignalSpy openSpy(shell, &ZzSshShellChannel::shellOpened);
    shell->openShell(QStringLiteral("xterm-256color"), 80, 24);
    if (!openSpy.wait(10000))
        return false;
    shell->write("pkill -f 'socat TCP-LISTEN:18222' 2>/dev/null; "
                 "socat TCP-LISTEN:18222,bind=127.0.0.1,reuseaddr,fork EXEC:/bin/cat &\n");
    QTest::qWait(1000); // 等 socat 就绪
    return true;
}

ZzSshTunnel *tst_ZzForwardPerf::startEchoTunnel(ZzSshConnection *conn)
{
    ZzSshTunnel *tunnel = conn->createTunnel(ZzSshTunnel::Type::Local,
                                             QStringLiteral("127.0.0.1"), 0,
                                             QStringLiteral("127.0.0.1"), 18222);
    if (!tunnel)
        return nullptr;
    QSignalSpy spy(tunnel, &ZzSshTunnel::listening);
    tunnel->start();
    if (spy.count() != 1 && !spy.wait(5000))
        return nullptr;
    return tunnel;
}

void tst_ZzForwardPerf::addRecord(const QString &name, const QString &unit, double threshold,
                                  double measured, bool passed)
{
    QJsonObject r;
    r.insert(QStringLiteral("name"), name);
    r.insert(QStringLiteral("unit"), unit);
    r.insert(QStringLiteral("threshold"), threshold);
    r.insert(QStringLiteral("measured"), measured);
    r.insert(QStringLiteral("passed"), passed);
    m_records.append(r);
}

void tst_ZzForwardPerf::forwardSetupLatency()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qint64 elapsed = 0;
    auto conn = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &elapsed);
    QVERIFY(conn != nullptr);

    constexpr int samples = 5;
    qint64 totalMs = 0;
    for (int i = 0; i < samples; ++i) {
        QElapsedTimer timer;
        timer.start();
        ZzSshTunnel *tunnel = conn->createTunnel(ZzSshTunnel::Type::Local,
                                                 QStringLiteral("127.0.0.1"), 0,
                                                 QStringLiteral("127.0.0.1"), 22);
        QVERIFY(tunnel != nullptr);
        QSignalSpy spy(tunnel, &ZzSshTunnel::listening);
        tunnel->start();
        QVERIFY(spy.count() == 1 || spy.wait(5000));
        totalMs += timer.elapsed();
        delete tunnel;
    }
    const double avgMs = static_cast<double>(totalMs) / samples;
    addRecord(QStringLiteral("forward-setup-latency"), QStringLiteral("ms"),
              SETUP_LATENCY_THRESHOLD_MS, avgMs, avgMs <= SETUP_LATENCY_THRESHOLD_MS);
    QVERIFY2(avgMs <= SETUP_LATENCY_THRESHOLD_MS,
             qPrintable(QStringLiteral("转发建立延迟 %1 ms 超过阈值 %2 ms")
                            .arg(avgMs)
                            .arg(SETUP_LATENCY_THRESHOLD_MS)));
}

void tst_ZzForwardPerf::forwardEchoThroughput()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qint64 elapsed = 0;
    auto conn = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &elapsed);
    QVERIFY(conn != nullptr);
    QVERIFY(startEchoServer(conn.get()));
    ZzSshTunnel *tunnel = startEchoTunnel(conn.get());
    QVERIFY(tunnel != nullptr);

    QTcpSocket client;
    client.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(client.waitForConnected(5000));

    const QByteArray chunk(65536, 'T');
    qint64 sent = 0;
    qint64 received = 0;
    QElapsedTimer timer;
    timer.start();
    while (received < THROUGHPUT_BYTES && timer.elapsed() < 120000) {
        while (sent < THROUGHPUT_BYTES && client.bytesToWrite() < 4 * 1024 * 1024) {
            client.write(chunk);
            sent += chunk.size();
        }
        if (client.bytesAvailable() > 0)
            received += client.readAll().size();
        else
            client.waitForReadyRead(50);
        QCoreApplication::processEvents();
    }
    const double seconds = timer.elapsed() / 1000.0;
    QVERIFY2(received >= THROUGHPUT_BYTES, "回显数据量不足，隧道可能中断");
    // 双向回显口径（字节数 / 总耗时），与既有 shell-echo-throughput 一致
    const double mbps = (static_cast<double>(THROUGHPUT_BYTES) / 1048576.0) / seconds;
    addRecord(QStringLiteral("forward-echo-throughput"), QStringLiteral("MB/s"),
              THROUGHPUT_THRESHOLD_MBPS, mbps, mbps >= THROUGHPUT_THRESHOLD_MBPS);
    QVERIFY2(mbps >= THROUGHPUT_THRESHOLD_MBPS,
             qPrintable(QStringLiteral("转发吞吐 %1 MB/s 低于阈值 %2 MB/s")
                            .arg(mbps)
                            .arg(THROUGHPUT_THRESHOLD_MBPS)));
}

void tst_ZzForwardPerf::concurrentConnections256()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qint64 elapsed = 0;
    auto conn = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &elapsed);
    QVERIFY(conn != nullptr);
    QVERIFY(startEchoServer(conn.get()));
    ZzSshTunnel *tunnel = startEchoTunnel(conn.get());
    QVERIFY(tunnel != nullptr);

    // 256 路并发全部接入（资源上限路径）
    QList<QTcpSocket *> sockets;
    for (int i = 0; i < ZzSshTunnel::MaxConnections; ++i) {
        auto *s = new QTcpSocket;
        s->connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
        QVERIFY2(s->waitForConnected(10000), qPrintable(QStringLiteral("第 %1 路连接失败").arg(i)));
        sockets.append(s);
    }
    QTRY_VERIFY_WITH_TIMEOUT(tunnel->activeConnectionCount() == ZzSshTunnel::MaxConnections,
                             30000);

    // 第 257 路被丢弃
    QTcpSocket extra;
    extra.connectToHost(QHostAddress::LocalHost, tunnel->listenPort());
    QVERIFY(extra.waitForConnected(5000));
    QVERIFY2(extra.waitForDisconnected(10000), "第 257 路连接未被拒绝");

    // 每路唯一 pattern 回显校验：无错数据
    QList<QByteArray> expected;
    QList<QByteArray> received;
    for (int i = 0; i < ZzSshTunnel::MaxConnections; ++i) {
        QByteArray p(1000, static_cast<char>('A' + (i % 26)));
        p += QByteArray::number(i);
        expected.append(p);
        received.append(QByteArray());
        QVERIFY(sockets[i]->write(p) == p.size());
    }
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < 60000) {
        bool done = true;
        for (int i = 0; i < ZzSshTunnel::MaxConnections; ++i) {
            if (received[i].size() >= expected[i].size())
                continue;
            if (sockets[i]->bytesAvailable() > 0)
                received[i] += sockets[i]->read(expected[i].size() - received[i].size());
            if (received[i].size() < expected[i].size())
                done = false;
        }
        if (done)
            break;
        QCoreApplication::processEvents();
        QThread::msleep(5);
    }
    for (int i = 0; i < ZzSshTunnel::MaxConnections; ++i)
        QVERIFY2(received[i] == expected[i],
                 qPrintable(QStringLiteral("第 %1 路回显数据不符（无错数据门控）").arg(i)));

    // 全部关闭后活动计数归零：无泄漏
    qDeleteAll(sockets);
    sockets.clear();
    QTRY_VERIFY_WITH_TIMEOUT(tunnel->activeConnectionCount() == 0, 15000);
    addRecord(QStringLiteral("forward-concurrent-256"), QStringLiteral("count"),
              ZzSshTunnel::MaxConnections, ZzSshTunnel::MaxConnections, true);
}

void tst_ZzForwardPerf::shellRegressionVsBaseline()
{
    // 基线：records 目录中最新的、git_commit 不同于当前构建的 zzsshcore 记录（变更前基线）
    const QDir recordsDir(QStringLiteral(ZZ_PERF_RECORDS_DIR));
    const QStringList files = recordsDir.entryList(QStringList() << QStringLiteral("*-zzsshcore.json"),
                                                   QDir::Files, QDir::Name);
    double baseConnectMs = -1;
    double baseThroughput = -1;
    for (auto it = files.crbegin(); it != files.crend(); ++it) {
        QFile f(recordsDir.filePath(*it));
        if (!f.open(QIODevice::ReadOnly))
            continue;
        const QJsonObject root = QJsonDocument::fromJson(f.readAll()).object();
        if (root.value(QStringLiteral("git_commit")).toString() == QStringLiteral(ZZ_GIT_COMMIT))
            continue; // 跳过本提交本轮生成的记录
        const QJsonArray recs = root.value(QStringLiteral("records")).toArray();
        for (const auto &v : recs) {
            const QJsonObject r = v.toObject();
            if (r.value(QStringLiteral("name")).toString() == QStringLiteral("connect-password-local"))
                baseConnectMs = r.value(QStringLiteral("measured")).toDouble();
            if (r.value(QStringLiteral("name")).toString() == QStringLiteral("shell-echo-throughput"))
                baseThroughput = r.value(QStringLiteral("measured")).toDouble();
        }
        if (baseConnectMs > 0 && baseThroughput > 0)
            break;
    }
    if (baseConnectMs <= 0 || baseThroughput <= 0)
        QSKIP("records 目录无变更前性能基线，跳过回归对比");

    // 本次实测：连接耗时（3 次平均，口径同既有 connect-password-local）
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qint64 totalMs = 0;
    for (int i = 0; i < 3; ++i) {
        qint64 e = 0;
        auto c = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &e);
        QVERIFY(c != nullptr);
        totalMs += e;
    }
    const double connectAvgMs = static_cast<double>(totalMs) / 3;

    // 本次实测：shell 回显吞吐（1MB，口径同既有 shell-echo-throughput）
    qint64 e = 0;
    auto conn = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &e);
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = conn->createShellChannel();
    QVERIFY(channel != nullptr);
    QSignalSpy openSpy(channel, &ZzSshShellChannel::shellOpened);
    channel->openShell(QStringLiteral("xterm-256color"), 80, 24);
    QVERIFY(openSpy.wait(10000));
    QSignalSpy dataSpy(channel, &ZzSshShellChannel::dataReceived);
    channel->write("stty -echo; cat\n"); // 关闭终端回显并进入 cat，保证计量只统计 cat 的返回数据
    QTest::qWait(1000);
    const QByteArray line = QByteArray(2047, 'A') + '\n';
    const int lineCount = static_cast<int>((1024 * 1024) / line.size());
    qint64 receivedBytes = 0;
    QElapsedTimer timer;
    timer.start();
    for (int i = 0; i < lineCount; ++i)
        channel->write(line);
    while (receivedBytes < qint64(lineCount) * line.size() && timer.elapsed() < 60000) {
        for (const QList<QVariant> &item : dataSpy)
            receivedBytes += item.at(0).toByteArray().size();
        dataSpy.clear();
        dataSpy.wait(500);
    }
    QVERIFY2(receivedBytes >= qint64(lineCount) * line.size(), "回显数据量不足，连接可能中断");
    const double shellMbps =
        (static_cast<double>(lineCount) * line.size() / 1048576.0) / (timer.elapsed() / 1000.0);

    // 回归判定：连接耗时上涨 / 吞吐下跌均不得超 5%（规格 §八）
    const double connectReg = (connectAvgMs - baseConnectMs) / baseConnectMs;
    const double throughputReg = (baseThroughput - shellMbps) / baseThroughput;
    addRecord(QStringLiteral("shell-connect-regression"), QStringLiteral("ratio"),
              REGRESSION_TOLERANCE, connectReg, connectReg <= REGRESSION_TOLERANCE);
    addRecord(QStringLiteral("shell-throughput-regression"), QStringLiteral("ratio"),
              REGRESSION_TOLERANCE, throughputReg, throughputReg <= REGRESSION_TOLERANCE);
    QVERIFY2(connectReg <= REGRESSION_TOLERANCE,
             qPrintable(QStringLiteral("连接耗时回归 %1%%（基线 %2 ms → 本次 %3 ms）超过 5%%")
                            .arg(connectReg * 100.0)
                            .arg(baseConnectMs)
                            .arg(connectAvgMs)));
    QVERIFY2(throughputReg <= REGRESSION_TOLERANCE,
             qPrintable(QStringLiteral("shell 吞吐回归 %1%%（基线 %2 MB/s → 本次 %3 MB/s）超过 5%%")
                            .arg(throughputReg * 100.0)
                            .arg(baseThroughput)
                            .arg(shellMbps)));
}

void tst_ZzForwardPerf::cleanupTestCase()
{
    if (m_records.isEmpty())
        return;
    QJsonObject env;
    env.insert(QStringLiteral("os"), QSysInfo::prettyProductName());
    env.insert(QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture());
    env.insert(QStringLiteral("memory_mb"), static_cast<double>(totalMemoryMB()));
    env.insert(QStringLiteral("qt_version"), QString::fromLatin1(qVersion()));
    env.insert(QStringLiteral("compiler"), compilerString());
    env.insert(QStringLiteral("build_type"), QStringLiteral(ZZ_BUILD_TYPE));

    QJsonObject root;
    root.insert(QStringLiteral("feature"), QStringLiteral("zzforward"));
    root.insert(QStringLiteral("timestamp"),
                QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
    root.insert(QStringLiteral("git_commit"), QStringLiteral(ZZ_GIT_COMMIT));
    root.insert(QStringLiteral("environment"), env);
    root.insert(QStringLiteral("records"), m_records);

    const QString fileName = QDate::currentDate().toString(QStringLiteral("yyyy-MM-dd"))
                             + QStringLiteral("-zzforward.json");
    const QString path = QStringLiteral(ZZ_PERF_RECORDS_DIR) + QLatin1Char('/') + fileName;
    QFile f(path);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        qWarning() << "无法写入性能记录文件:" << path;
        return;
    }
    f.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    qInfo() << "性能记录已写入:" << path;
}

QTEST_GUILESS_MAIN(tst_ZzForwardPerf)
#include "tst_ZzForwardPerf.moc"
```

在 `tests/CMakeLists.txt` 末尾追加（复用既有 `execute_process(git rev-parse)` 注入的 `ZZ_GIT_COMMIT` 变量）：

```cmake
zz_add_test(tst_ZzForwardPerf perf/tst_ZzForwardPerf.cpp)
target_link_libraries(tst_ZzForwardPerf PRIVATE zzsshcore_itconfig)
target_compile_definitions(tst_ZzForwardPerf PRIVATE
    ZZ_PERF_RECORDS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/perf/records"
    ZZ_GIT_COMMIT="${ZZ_GIT_COMMIT}"
    ZZ_BUILD_TYPE="$<CONFIG>"
)
set_tests_properties(tst_ZzForwardPerf PROPERTIES LABELS "perf")
```

- [ ] **步骤 2：Release 构建并运行性能测试**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：全部 integration + perf 测试 `Passed`，含 `tst_ZzForwardPerf` 四用例；输出中出现 `性能记录已写入: .../tests/perf/records/<今天日期>-zzforward.json`。

**注意：** `shellRegressionVsBaseline` 依赖 records 目录中存在 git_commit 不同于当前构建的既有 `*-zzsshcore.json` 基线。执行本任务前应确保仓库中已有变更前基线（前置条件步骤已生成；若当天多次运行 tst_ZzSshPerf 覆盖了当日文件，手工 `git checkout -- tests/perf/records/` 恢复已提交基线后再跑）。

- [ ] **步骤 3：检查记录文件内容**

```bash
cat tests/perf/records/$(date +%F)-zzforward.json
```

预期：JSON 包含 `feature=zzforward`、`git_commit`（非 unknown）、`environment`（build_type=Release）、`records` 五项（forward-setup-latency / forward-echo-throughput / forward-concurrent-256 / shell-connect-regression / shell-throughput-regression）且 `passed` 均为 `true`。

- [ ] **步骤 4：Commit（含首条转发性能记录）**

```bash
git add tests/perf/tst_ZzForwardPerf.cpp tests/CMakeLists.txt "tests/perf/records/$(date +%F)-zzforward.json"
git commit -m "test: 新增端口转发性能门控测试并记录首次转发性能基线

规格 §八四项门控（Release 有效、不达标即失败、结果落盘 records/）：
- forward-setup-latency ≤200ms（createTunnel→listening，5 次平均）
- forward-echo-throughput ≥50MB/s（-L 经容器 socat 回显 64MB 双向）
- forward-concurrent-256：256 路全通且回显无错数据、第 257 路被拒、
  关闭后活动计数归零（无泄漏）
- shell 回归 ≤5%：本进程复测连接耗时与 shell 吞吐，对比 records 中
  变更前基线（git_commit 不同的最新 zzsshcore 记录）"
```

---

### 任务 10：全量回归与收尾

- [ ] **步骤 1：全量回归（三标签全绿）**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：unit 全 `Passed`；integration + perf 全 `Passed`。逐项核对规格覆盖：

- §三 组件：`ZzSshConnection`（createTunnel/createForwardChannel/createForwardListener）✔、`ZzSshConnectionWorker`（多 channel 泵）✔、`ZzSshChannel::openDirectTcpip` ✔、`ZzSshForwardListener` ✔、`ZzSshTunnel` ✔
- §四 数据流：-L/-R/-D 各有端到端集成测试 ✔；统一泵 ✔；1MB 水位背压 ✔；256 上限 ✔
- §六 错误矩阵：端口占用单规则失败 ✔（任务 7 单测）；服务端拒绝 ✔（任务 8）；断线销毁/重连重建 ✔（任务 7/8 invalidated + 集成测试）；单连接出错只关该连接 ✔（任务 6/7/8）；SOCKS5 错误回码 ✔（任务 5/7）
- §八 性能门控 ✔（任务 9）

- [ ] **步骤 2：提交当日全部性能记录（若有未提交的）**

```bash
git add tests/perf/records/
git status --short
```

若有暂存内容：

```bash
git commit -m "chore: 更新性能基线记录（端口转发全量回归自动生成）

端口转发功能全量回归（unit/integration/perf 全绿）的当日性能记录：
zzsshcore 既有基线两项 + zzforward 新增五项。"
```

- [ ] **步骤 3：收尾确认**

```bash
git log --oneline -12
```

预期：任务 1-9 的 commit 依次在列。随后向用户汇报：ZzSshCore 侧全部完成；**推送远端需用户确认**（规格 §十）；主仓库 ZzClawTerm 的 gitlink 更新与应用侧实现属于第二份计划（port-forwarding-02），不在本计划范围。

---

## 附：跨任务接口速查（实现时逐字以各任务代码块为准）

```cpp
// ZzSshChannel（任务 2/4/8）
using WaitFn = std::function<bool()>;
void setWaitFunction(WaitFn fn);
static std::unique_ptr<ZzSshChannel> adoptOpened(LIBSSH2_CHANNEL *handle);
bool openDirectTcpip(ZzSshSession &session, const QString &targetHost, quint16 targetPort,
                     const QString &originatorHost, quint16 originatorPort, QString *errorString);
qint64 writeSome(const QByteArray &data, QString *errorString);

// ZzSshSession（任务 1/3）
void setBlocking(bool blocking);
int blockDirections() const;

// ZzSshConnectionWorker 信号/槽（任务 3）
void doOpenDirectTcpip(quint32 channelId, const QString &targetHost, quint16 targetPort,
                       const QString &originatorHost, quint16 originatorPort);
void doSetChannelReadPaused(quint32 channelId, bool paused);
void doForwardListen(quint32 listenerId, const QString &listenHost, quint16 listenPort);
void doForwardCancel(quint32 listenerId);
void directTcpipOpened(quint32 channelId);
void channelWritePaused(quint32 channelId, bool paused);
void forwardListening(quint32 listenerId, quint16 boundPort);
void forwardListenFailed(quint32 listenerId, int code, const QString &message);
void forwardedTcpipAccepted(quint32 listenerId, quint32 channelId,
                            const QString &originatorHost, quint16 originatorPort);

// ZzSshForwardChannel（任务 4）
void write(const QByteArray &data);
void setReadPaused(bool paused);
void closeChannel();
// 信号：opened / dataReceived / writeCongestionChanged(bool) / errorOccurred / closed

// ZzSshConnection（任务 4/7/8）
ZzSshForwardChannel *createForwardChannel(const QString &targetHost, quint16 targetPort,
                                          const QString &originatorHost, quint16 originatorPort);
ZzSshTunnel *createTunnel(ZzSshTunnel::Type type, const QString &listenHost, quint16 listenPort,
                          const QString &targetHost = QString(), quint16 targetPort = 0);
ZzSshForwardListener *createForwardListener(const QString &listenHost, quint16 listenPort,
                                            const QString &targetHost, quint16 targetPort);
ZzSshForwardChannel *adoptForwardChannel(quint32 channelId); // private，friend ZzSshForwardListener

// ZzSocketChannelPump（任务 6）
ZzSocketChannelPump(QTcpSocket *socket, ZzSshForwardChannel *channel, QObject *parent = nullptr);
static constexpr qint64 HighWatermark = 1024 * 1024;
bool isChannelReadPaused() const;
// 信号：finished()

// ZzSshTunnel（任务 7）
enum class Type { Local, Dynamic };
static constexpr int MaxConnections = 256;
void start(); void stop();
Type type() const; QString listenHost() const; quint16 listenPort() const;
int activeConnectionCount() const;
// 信号：listening(quint16) / failed(int, QString) / connectionError(QString) / invalidated()

// ZzSshForwardListener（任务 8）
void start(); void stop();
quint32 listenerId() const; QString listenHost() const; quint16 listenPort() const;
int activeConnectionCount() const;
void onListening(quint16 boundPort);
void onListenFailed(int code, const QString &message);
void onForwarded(quint32 channelId);
// 信号：listening(quint16) / failed(int, QString) / connectionError(QString) /
//       connectionAccepted(ZzSshForwardChannel *) / invalidated()

// ZzSocks5Handshake（任务 5）
enum class Result { NeedMoreData, Ready, Error };
struct GreetingResult { Result result; quint8 method; int consumedBytes; };
struct RequestResult { Result result; quint8 replyCode; QString targetHost;
                       quint16 targetPort; int consumedBytes; };
static GreetingResult parseGreeting(const QByteArray &buffer);
static QByteArray buildMethodSelection(quint8 method);
static RequestResult parseRequest(const QByteArray &buffer);
static QByteArray buildReply(quint8 replyCode);
// 常量：ReplySucceeded=0x00 / ReplyGeneralFailure=0x01 / ReplyConnectionRefused=0x05 /
//       ReplyCommandNotSupported=0x07 / ReplyAddressTypeNotSupported=0x08

// ZzChannelWriteQueue（任务 3）
static constexpr qsizetype HighWatermark = 1024 * 1024;
static constexpr qsizetype ResumeWatermark = 512 * 1024;
void append(const QByteArray &data);
QByteArray head(qsizetype maxBytes) const;
void removeFirst(qsizetype n);
void clear();
qsizetype pendingBytes() const;
bool congested() const;
```
