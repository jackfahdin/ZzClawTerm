#pragma once

#include <QObject>

/**
 * @brief 隧道句柄抽象：一条已创建转发规则的运行时实体（GUI 线程）。
 *
 * 生产实现（ZzSshTunnelHandle）包装 ZzSshCore 的 ZzSshTunnel（-L/-D）与
 * ZzSshForwardListener（-R）；测试用 fake 注入生命周期事件。
 * 信号语义与 ZzSshCore 两侧逐一对齐（规格 §三/§六）。
 */
class ZzTunnelHandle : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;

    /** @brief 启动监听/转发（幂等）。结局为 listening() 或 failed()。 */
    virtual void start() = 0;

    /** @brief 停止并关闭全部转发连接（幂等；不发射信号）。 */
    virtual void stop() = 0;

    /** @brief 当前活动转发连接数。 */
    virtual int activeConnectionCount() const = 0;

signals:
    /** @brief 监听就绪（boundPort 为实际绑定端口）。 */
    void listening(quint16 boundPort);

    /** @brief 规则级失败：仅该规则受影响，会话保留（规格 §六）。 */
    void failed(int code, const QString &message);

    /** @brief 单连接级错误提示（目标不可达等）：该连接已关闭，隧道继续服务。 */
    void connectionError(const QString &message);

    /** @brief SSH 连接断开，隧道已自动停止；重连后由新 manager 重建。 */
    void invalidated();
};
