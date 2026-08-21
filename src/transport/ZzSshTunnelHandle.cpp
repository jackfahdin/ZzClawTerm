#include "ZzSshTunnelHandle.h"

#include <ZzSshConnection.h>
#include <ZzSshForwardListener.h>
#include <ZzSshTunnel.h>

ZzSshTunnelHandle::ZzSshTunnelHandle(ZzSshTunnel *tunnel, QObject *parent)
    : ZzTunnelHandle(parent)
    , m_tunnel(tunnel)
{
    Q_ASSERT(m_tunnel);
    m_tunnel->setParent(this); // 句柄取得所有权（原 parent 为连接）
    wireSignals(m_tunnel);
}

ZzSshTunnelHandle::ZzSshTunnelHandle(ZzSshForwardListener *listener, QObject *parent)
    : ZzTunnelHandle(parent)
    , m_listener(listener)
{
    Q_ASSERT(m_listener);
    m_listener->setParent(this);
    wireSignals(m_listener);
}

void ZzSshTunnelHandle::wireSignals(QObject *source)
{
    // 两侧实体信号集一致，按名连接逐一转发
    connect(source, SIGNAL(listening(quint16)), this, SIGNAL(listening(quint16)));
    connect(source, SIGNAL(failed(int,QString)), this, SIGNAL(failed(int,QString)));
    connect(source, SIGNAL(connectionError(QString)), this, SIGNAL(connectionError(QString)));
    connect(source, SIGNAL(invalidated()), this, SIGNAL(invalidated()));
}

void ZzSshTunnelHandle::start()
{
    if (m_tunnel) {
        m_tunnel->start();
    } else if (m_listener) {
        m_listener->start();
    }
}

void ZzSshTunnelHandle::stop()
{
    if (m_tunnel) {
        m_tunnel->stop();
    } else if (m_listener) {
        m_listener->stop();
    }
}

int ZzSshTunnelHandle::activeConnectionCount() const
{
    if (m_tunnel) {
        return m_tunnel->activeConnectionCount();
    }
    if (m_listener) {
        return m_listener->activeConnectionCount();
    }
    return 0;
}

ZzSshTunnelFactory::ZzSshTunnelFactory(ZzSshConnection *connection)
    : m_connection(connection)
{
}

ZzTunnelHandle *ZzSshTunnelFactory::createHandle(const ZzForwardRule &rule, QObject *parent)
{
    switch (rule.type) {
    case ZzForwardRule::Type::Local:
        if (auto *tunnel = m_connection->createTunnel(
                ZzSshTunnel::Type::Local, rule.listenHost, rule.listenPort,
                rule.targetHost, rule.targetPort)) {
            return new ZzSshTunnelHandle(tunnel, parent);
        }
        return nullptr;
    case ZzForwardRule::Type::Dynamic:
        if (auto *tunnel = m_connection->createTunnel(
                ZzSshTunnel::Type::Dynamic, rule.listenHost, rule.listenPort)) {
            return new ZzSshTunnelHandle(tunnel, parent);
        }
        return nullptr;
    case ZzForwardRule::Type::Remote:
        if (auto *listener = m_connection->createForwardListener(
                rule.listenHost, rule.listenPort, rule.targetHost, rule.targetPort)) {
            return new ZzSshTunnelHandle(listener, parent);
        }
        return nullptr;
    }
    return nullptr;
}
