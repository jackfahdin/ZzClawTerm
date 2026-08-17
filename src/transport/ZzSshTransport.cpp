#include "ZzSshTransport.h"

#include <utility>

#include <ZzSshAuthConfig.h>
#include <ZzSshConnection.h>
#include <ZzSshShellChannel.h>

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

void ZzSshTransportAdapter::open(const ZzTransportEndpoint &endpoint)
{
    if (state() != State::Disconnected) {
        return;
    }
    // 重试/重连场景：废弃旧连接对象（规格 §4.2 注释：同一连接不可重复 connectToHost）
    if (m_conn) {
        m_suppressDisconnect = true;
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
    if (!endpoint.keyPath.isEmpty()) {
        ZzSshAuthConfig auth;
        auth.privateKeyPath = endpoint.keyPath;
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

    // openShell 为异步操作：保持 Connecting，待 shellOpened 后迁移 Connected
    m_channel->openShell(m_endpoint.terminalType,
                         m_endpoint.cols, m_endpoint.rows);
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
    if (m_conn) {
        m_conn->disconnectFromHost();
        m_conn->deleteLater();
        m_conn = nullptr;
    }
    setState(State::Disconnected);
}
