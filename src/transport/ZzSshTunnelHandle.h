#pragma once

#include "ZzTunnelHandle.h"
#include "ZzTunnelManager.h" // ZzTunnelFactory 基类定义
#include "session/ZzForwardRule.h"

class ZzSshConnection;
class ZzSshForwardListener;
class ZzSshTunnel;

/**
 * @brief 生产隧道句柄：包装 ZzSshTunnel（-L/-D）或 ZzSshForwardListener（-R）。
 *
 * 构造即取得被包装对象所有权（setParent 到本句柄），信号逐一转发。
 * 两种实体信号集一致（listening/failed/connectionError/invalidated），
 * 仅创建方式不同，故共用一个句柄类。
 */
class ZzSshTunnelHandle : public ZzTunnelHandle
{
    Q_OBJECT
public:
    /** @brief 包装本地/动态隧道（-L/-D）。tunnel 不可为空。 */
    explicit ZzSshTunnelHandle(ZzSshTunnel *tunnel, QObject *parent = nullptr);

    /** @brief 包装远程转发监听器（-R）。listener 不可为空。 */
    explicit ZzSshTunnelHandle(ZzSshForwardListener *listener, QObject *parent = nullptr);

    void start() override;
    void stop() override;
    int activeConnectionCount() const override;

private:
    /** @brief 统一接线：源对象四信号转发为本句柄信号。 */
    void wireSignals(QObject *source);

    ZzSshTunnel *m_tunnel = nullptr;            ///< -L/-D 实体（本句柄为父）
    ZzSshForwardListener *m_listener = nullptr; ///< -R 实体（本句柄为父）
};

/**
 * @brief 生产隧道工厂：经 ZzSshConnection 按规则类型创建句柄（规格 §三）。
 *
 * Local/Dynamic → createTunnel（任意连接状态可创建）；
 * Remote → createForwardListener（仅 Connected，否则返回 nullptr）。
 */
class ZzSshTunnelFactory : public ZzTunnelFactory
{
public:
    /** @brief 构造工厂。connection 非拥有，须比工厂活得久。 */
    explicit ZzSshTunnelFactory(ZzSshConnection *connection);

    ZzTunnelHandle *createHandle(const ZzForwardRule &rule, QObject *parent) override;

private:
    ZzSshConnection *m_connection; ///< 非拥有
};
