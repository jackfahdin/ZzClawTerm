#pragma once

#include <QtCore/QObject>

#include "ZzTransportEndpoint.h"

/**
 * @brief 传输协议抽象：SSH 与本地 PTY 的统一接口（规格 §2.3）。
 *
 * 实现必须保证：open() 之后状态经 Connecting 到 Connected，或经 errorOccurred 失败；
 * 断开（主动或被动）后进入 Disconnected 并发射 disconnected()（主动 close 除外，
 * 主动 close 只改状态不报断线）。所有方法只在 GUI 线程调用。
 */
class ZzTransportInterface : public QObject
{
    Q_OBJECT
public:
    /** @brief 传输生命周期三态。 */
    enum class State {
        Disconnected,   ///< 未连接或已断开
        Connecting,     ///< 连接/认证进行中
        Connected       ///< 字节流通道就绪
    };
    Q_ENUM(State)

    using QObject::QObject;

    /**
     * @brief 按 endpoint 参数异步打开传输。
     * @param endpoint 连接参数。
     * @note 同一对象可被重复 open（重连场景由调用方先 close 或另建新实例，
     *       见 ZzTabManager::reconnectTab 的新实例策略）。
     */
    virtual void open(const ZzTransportEndpoint &endpoint) = 0;

    /** @brief 向远端/子进程写入字节（终端键盘输入方向）。 */
    virtual void write(const QByteArray &data) = 0;

    /** @brief 通知终端尺寸变化。 */
    virtual void resize(int cols, int rows) = 0;

    /** @brief 主动关闭传输并释放底层资源。 */
    virtual void close() = 0;

    /** @brief 当前状态。 */
    [[nodiscard]] State state() const noexcept { return m_state; }

signals:
    /** @brief 状态迁移通知。 */
    void stateChanged(ZzTransportInterface::State state);
    /** @brief 收到远端/子进程输出（终端显示方向）。 */
    void dataReceived(const QByteArray &data);
    /** @brief 打开或运行期失败。code 为封装层自定义码或透传底层错误码。 */
    void errorOccurred(int code, const QString &message);
    /** @brief 被动断开（远端关闭、网络中断、子进程退出）。 */
    void disconnected(const QString &reason);

protected:
    /** @brief 实现类迁移状态并发射 stateChanged。 */
    void setState(State state)
    {
        if (m_state == state) {
            return;
        }
        m_state = state;
        emit stateChanged(state);
    }

private:
    State m_state = State::Disconnected;
};
