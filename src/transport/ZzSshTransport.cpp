#include "ZzSshTransport.h"

#include <utility>

#include <QFileInfo>
#include <QStandardPaths>

#include <ZzSshAuthConfig.h>
#include <ZzSshConnection.h>
#include <ZzSshShellChannel.h>
#include <ZzSshX11Bridge.h>

#include "ZzSshTunnelHandle.h"
#include "ZzTunnelManager.h"
#include "x11/ZzXServerDownloader.h"
#include "x11/ZzXServerManager.h"

ZzSshTransportAdapter::ZzKeyPassphraseResolver
    ZzSshTransportAdapter::s_keyPassphraseResolver;

ZzSshTransportAdapter::ZzSshTransportAdapter(QObject *parent)
    : ZzTransportInterface(parent)
{
}

ZzSshTransportAdapter::~ZzSshTransportAdapter()
{
    close();
}

void ZzSshTransportAdapter::setPasswordProvider(ZzPasswordProvider provider)
{
    m_passwordProvider = std::move(provider);
}

void ZzSshTransportAdapter::setHostKeyConfirmer(ZzHostKeyConfirmer confirmer)
{
    m_hostKeyConfirmer = std::move(confirmer);
}

void ZzSshTransportAdapter::setKeyPassphraseResolver(ZzKeyPassphraseResolver resolver)
{
    s_keyPassphraseResolver = std::move(resolver);
}

void ZzSshTransportAdapter::open(const ZzTransportEndpoint &endpoint)
{
    if (state() != State::Disconnected) {
        return;
    }
    // 重试/重连场景：废弃旧连接对象（规格 §4.2 注释：同一连接不可重复 connectToHost）
    if (m_conn) {
        m_suppressDisconnect = true;
        destroyTunnelManager(); // 隧道先于连接销毁（句柄持有观察连接的隧道）
        destroyX11();           // X11 桥观察旧连接，一并回收
        m_conn->disconnectFromHost();
        m_conn->deleteLater();
        m_conn = nullptr;
        m_channel = nullptr;
    }
    m_suppressDisconnect = false;
    m_endpoint = endpoint;
    setState(State::Connecting);

    m_conn = new ZzSshConnection(this);
    wireConnection();
    // keepalive 必须在 connectToHost 之前配置（ZzSshConnection 契约）
    m_conn->setKeepaliveInterval(endpoint.keepaliveIntervalSeconds);
    if (!endpoint.keyPath.isEmpty()) {
        ZzSshAuthConfig auth;
        auth.privateKeyPath = endpoint.keyPath;
        // 私钥口令：经进程级解析器从凭据后端取（口令存凭据库，不落明文 profile）；
        // 空解析器或返回空串均视为私钥无口令（ZzSshAuthConfig 契约：无口令留空）
        if (s_keyPassphraseResolver) {
            auth.passphrase = s_keyPassphraseResolver(endpoint);
        }
        m_conn->setAuthConfig(auth);
    }
    m_conn->connectToHost(endpoint.host, endpoint.port, endpoint.user);
}

void ZzSshTransportAdapter::wireConnection()
{
    connect(m_conn, &ZzSshConnection::connected,
            this, &ZzSshTransportAdapter::onConnected);
    connect(m_conn, &ZzSshConnection::errorOccurred, this,
            [this](int code, const QString &message) {
                setState(State::Disconnected);
                emit errorOccurred(code, message);
            });
    connect(m_conn, &ZzSshConnection::disconnected, this,
            [this](const QString &reason) {
                m_channel = nullptr;
                setState(State::Disconnected);
                // 主动 close 触发的底层断开（"用户主动断开"）按接口契约不上报
                if (!m_suppressDisconnect) {
                    emit disconnected(reason);
                }
            });
    // 认证链密码回调（规格 §4.2：上层不感知尝试顺序，只负责给密码）
    connect(m_conn, &ZzSshConnection::passwordRequested, this, [this]() {
        const QString password =
            m_passwordProvider ? m_passwordProvider() : QString();
        // 空串表示用户取消，走取消路径而非提交空密码
        password.isEmpty() ? m_conn->cancelPasswordRequest()
                           : m_conn->providePassword(password);
    });
    // 主机密钥确认（规格 §八安全底线，不可省略）
    connect(m_conn, &ZzSshConnection::hostKeyUnknown, this,
            [this](const QString &host, quint16 /*port*/,
                   const QString & /*keyType*/, const QString &fingerprint) {
                const bool accept = m_hostKeyConfirmer
                    ? m_hostKeyConfirmer(host, fingerprint, QString(), false)
                    : false;
                accept ? m_conn->trustHostKey() : m_conn->rejectHostKey();
            });
    connect(m_conn, &ZzSshConnection::hostKeyChanged, this,
            [this](const QString &host, quint16 /*port*/,
                   const QString & /*keyType*/,
                   const QString &oldFingerprint,
                   const QString &newFingerprint) {
                const bool accept = m_hostKeyConfirmer
                    ? m_hostKeyConfirmer(host, newFingerprint, oldFingerprint,
                                         true)
                    : false;
                accept ? m_conn->acceptHostKeyChange() : m_conn->rejectHostKey();
            });
}

void ZzSshTransportAdapter::onConnected()
{
    m_channel = m_conn->createShellChannel();
    if (!m_channel) {
        setState(State::Disconnected);
        emit errorOccurred(3001, QStringLiteral("创建 shell 通道失败"));
        return;
    }
    connect(m_channel, &ZzSshShellChannel::dataReceived, this,
            [this](const QByteArray &data) { emit dataReceived(data); });
    connect(m_channel, &ZzSshShellChannel::shellOpened, this,
            [this]() { setState(State::Connected); });
    connect(m_channel, &ZzSshShellChannel::errorOccurred, this,
            [this](int code, const QString &message) {
                setState(State::Disconnected);
                emit errorOccurred(code, message);
            });
    connect(m_channel, &ZzSshShellChannel::closed, this, [this]() {
        m_channel = nullptr;
        setState(State::Disconnected);
        if (!m_suppressDisconnect) {
            emit disconnected(QStringLiteral("远程 shell 已关闭"));
        }
    });

    // x11-req 必须在 shell 启动前发出（OpenSSH 仅 LARVAL 态受理）：X11 装配
    // 先行，requestX11Forwarding 先于 openShell 入队（库侧登记 pending 后在
    // doOpenShell 内 PTY/shell 之前发出）；装配任何失败只瞬时提示并照常开 shell
    if (m_endpoint.x11Forwarding) {
        startX11Forwarding(); // 内部保证以 openShellChannel() 收尾（同步或异步）
    } else {
        openShellChannel();
    }

    startTunnels(); // 连接已就绪：createTunnel/createForwardListener 均可用
}

void ZzSshTransportAdapter::openShellChannel()
{
    // openShell 为异步操作：保持 Connecting，待 shellOpened 后迁移 Connected
    if (m_channel) {
        m_channel->openShell(m_endpoint.terminalType,
                             m_endpoint.cols, m_endpoint.rows);
    }
}

void ZzSshTransportAdapter::write(const QByteArray &data)
{
    if (m_channel && state() == State::Connected) {
        m_channel->write(data);
    }
}

void ZzSshTransportAdapter::resize(int cols, int rows)
{
    if (m_channel && state() == State::Connected) {
        m_channel->resize(cols, rows);
    }
}

void ZzSshTransportAdapter::close()
{
    m_suppressDisconnect = true;
    if (m_channel) {
        m_channel->closeChannel();
        m_channel = nullptr;
    }
    destroyTunnelManager();
    destroyX11();
    if (m_conn) {
        m_conn->disconnectFromHost();
        m_conn->deleteLater();
        m_conn = nullptr;
    }
    setState(State::Disconnected);
}

void ZzSshTransportAdapter::destroyTunnelManager()
{
    if (!m_tunnelManager) {
        return;
    }
    m_tunnelManager->stopAll();
    m_tunnelManager->deleteLater();
    m_tunnelManager = nullptr;
    m_tunnelFactory.reset();
}

void ZzSshTransportAdapter::startTunnels()
{
    if (m_endpoint.portForwards.isEmpty()) {
        return;
    }
    m_tunnelFactory = std::make_unique<ZzSshTunnelFactory>(m_conn);
    m_tunnelManager = new ZzTunnelManager(m_tunnelFactory.get(),
                                          m_endpoint.portForwards, this);
    // 规则级失败 → 状态栏瞬时提示（规格 §六：单规则失败隔离，不动错误横幅）
    connect(m_tunnelManager, &ZzTunnelManager::ruleFailed, this,
            [this](const ZzForwardRule &rule, const QString &message) {
                emit statusNotice(QStringLiteral("转发规则 %1 启动失败：%2")
                                      .arg(rule.describe(), message));
            });
    connect(m_tunnelManager, &ZzTunnelManager::tunnelConnectionError, this,
            [this](const QString &message) { emit statusNotice(message); });
    connect(m_tunnelManager, &ZzTunnelManager::tunnelsChanged, this, [this]() {
        emit tunnelCountChanged(m_tunnelManager ? m_tunnelManager->activeTunnelCount() : 0);
    });
    m_tunnelManager->startAll();
}

void ZzSshTransportAdapter::startX11Forwarding()
{
    // 契约：无论装配成败，本函数保证以 openShellChannel() 收尾（Unix 同步、
    // Windows 经 downloader 异步续接），任何失败只瞬时提示、不阻断会话
    if (!m_channel) {
        return;
    }
#if defined(Q_OS_LINUX)
    // 无本地 X server（纯 Wayland/无头）时提前提示并跳过
    if (qgetenv("DISPLAY").isEmpty()) {
        emit statusNotice(QStringLiteral(
            "X11 转发已跳过：未检测到本地 X server（$DISPLAY 为空）"));
        openShellChannel();
        return;
    }
#elif defined(Q_OS_MAC)
    // macOS 依赖 XQuartz 的 Unix 域套接字目录
    if (!QFileInfo::exists(QStringLiteral("/tmp/.X11-unix"))) {
        emit statusNotice(QStringLiteral(
            "X11 转发已跳过：未检测到 XQuartz（/tmp/.X11-unix 不存在）"));
        openShellChannel();
        return;
    }
#endif
    m_x11Cookie = m_x11Authority.generateCookie();
#if defined(Q_OS_WIN)
    // 按需下载/校验/安装 ZzXsrv（回环绑定构建），就绪后经 onX11ServerReady 续接 openShell
    if (!m_x11Downloader) {
        m_x11Downloader = new ZzXServerDownloader(this);
        connect(m_x11Downloader, &ZzXServerDownloader::ready, this,
                &ZzSshTransportAdapter::onX11ServerReady);
        connect(m_x11Downloader, &ZzXServerDownloader::downloadFailed, this,
                [this](const QString &message) {
                    emit statusNotice(QStringLiteral("X11 转发不可用：%1").arg(message));
                    openShellChannel(); // X11 失败不阻断会话
                });
    }
    m_x11Downloader->ensureAvailable();
#else
    // Unix：复用系统 X server，start() 只解析 $DISPLAY 记录端点，不拉起进程
    if (!m_x11Manager) {
        m_x11Manager = new ZzXServerManager(this);
        connect(m_x11Manager, &ZzXServerManager::crashed, this,
                [this](const QString &message) {
                    emit statusNotice(QStringLiteral("X11 本地 server 异常：%1").arg(message));
                });
    }
    m_x11Manager->start(QString(), QString(), 0);
    const int display = m_x11Manager->display();
    if (display < 0) {
        emit statusNotice(QStringLiteral("X11 转发已跳过：无法解析 $DISPLAY"));
        openShellChannel();
        return;
    }
    // cookie 并入系统授权库，转发字节流经桥透传后由本地 X server 校验同一 cookie
    QString error;
    if (!m_x11Authority.addToSystemAuthority(display, m_x11Cookie, &error)) {
        emit statusNotice(QStringLiteral("X11 授权写入失败：%1").arg(error));
        openShellChannel();
        return;
    }
    // x11-req 先于 openShell 入队（OpenSSH 仅 LARVAL 态受理）
    requestX11Forwarding();
    openShellChannel();
#endif
}

void ZzSshTransportAdapter::onX11ServerReady(const QString &executablePath)
{
#if defined(Q_OS_WIN)
    // 下载/安装期间会话可能已关闭：channel 不在则无需再补 openShell
    if (!m_channel || !m_endpoint.x11Forwarding) {
        return;
    }
    if (!m_x11Manager) {
        m_x11Manager = new ZzXServerManager(this);
        connect(m_x11Manager, &ZzXServerManager::crashed, this,
                [this](const QString &message) {
                    emit statusNotice(QStringLiteral("X11 本地 server 异常：%1").arg(message));
                });
    }
    const int display = ZzXServerManager::allocateDisplay();
    if (display < 0) {
        emit statusNotice(QStringLiteral("X11 转发不可用：无空闲 display 号"));
        openShellChannel();
        return;
    }
    const QString xauthPath =
        QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
        + QStringLiteral("/xserver/xauth-%1").arg(display);
    if (!m_x11Authority.writeXauthorityFile(xauthPath, display, m_x11Cookie)) {
        emit statusNotice(QStringLiteral("X11 授权写入失败：%1").arg(xauthPath));
        openShellChannel();
        return;
    }
    // 桥按 channel 到达时才连本地端点，无需等待 server 完全就绪
    m_x11Manager->start(executablePath, xauthPath, display);
    requestX11Forwarding();
    openShellChannel();
#else
    Q_UNUSED(executablePath);
#endif
}

void ZzSshTransportAdapter::requestX11Forwarding()
{
    if (!m_channel || !m_x11Manager) {
        return;
    }
    // 应用侧 ZzXLocalEndpoint → 库侧 ZzSshX11Bridge::LocalEndpoint 字段映射
    const ZzXLocalEndpoint local = m_x11Manager->localEndpoint();
    ZzSshX11Bridge::LocalEndpoint endpoint;
    endpoint.host = local.host;
    endpoint.port = local.port;
    endpoint.localSocketPath = local.localSocketPath;
    m_x11Bridge = new ZzSshX11Bridge(m_conn, endpoint, this);
    connect(m_x11Bridge, &ZzSshX11Bridge::bridgeFailed, this,
            [this](quint32 /*channelId*/, int /*code*/, const QString &message) {
                emit statusNotice(QStringLiteral("X11 转发通道失败：%1").arg(message));
            });
    connect(m_channel, &ZzSshShellChannel::x11ForwardingReady, this, [this]() {
        emit statusNotice(QStringLiteral("X11 转发已启用"));
    });
    connect(m_channel, &ZzSshShellChannel::x11ForwardingFailed, this,
            [this](int /*code*/, const QString &message) {
                emit statusNotice(QStringLiteral("X11 转发被服务端拒绝：%1").arg(message));
            });
    m_channel->requestX11Forwarding(m_x11Cookie);
}

void ZzSshTransportAdapter::destroyX11()
{
    if (m_x11Bridge) {
        m_x11Bridge->deleteLater();
        m_x11Bridge = nullptr;
    }
    if (m_x11Manager) {
        // stop 为异步收尾；deleteLater 后 QObject 树兜底回收（Windows 子进程随析构终止）
        m_x11Manager->stop();
        m_x11Manager->deleteLater();
        m_x11Manager = nullptr;
    }
    m_x11Cookie.clear();
}
