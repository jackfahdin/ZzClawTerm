#pragma once

#include <QtWidgets/QWidget>

#include "transport/ZzTransportEndpoint.h"
#include "transport/ZzTransportInterface.h"

class QTermWidget;
class ZzAppSettings;

/**
 * @brief 单标签终端视图：组合 QTermWidget 与一个传输实例（规格 §七）。
 *
 * 职责只有胶水：远端输出 → recvData，键盘输入 → transport->write，
 * 尺寸变化 → transport->resize；外加设置应用与错误/断开信号透传
 * （错误横幅 UI 属任务 13，由 errorOccurred/disconnected 信号接入）。
 * 不拥有传输的所有权以外的语义——传输以本视图为 QObject 父对象随视图销毁。
 */
class ZzTerminalView : public QWidget
{
    Q_OBJECT
public:
    explicit ZzTerminalView(QWidget *parent = nullptr);

    /**
     * @brief 绑定传输并接线（可重复调用，用于断线重连换新实例）。
     * @param transport 必须已将本视图设为 QObject 父对象。
     */
    void setTransport(ZzTransportInterface *transport);

    /** @brief 当前绑定的传输（可空）。 */
    [[nodiscard]] ZzTransportInterface *transport() const;

    /** @brief 内部 QTermWidget（测试与滚动历史桥使用）。 */
    [[nodiscard]] QTermWidget *termWidget() const;

    /** @brief 以给定参数打开传输并记忆，供重连复用。 */
    void openEndpoint(const ZzTransportEndpoint &endpoint);

    /** @brief 按当前编码名展示（状态栏用）。 */
    [[nodiscard]] QString encoding() const;

    /** @brief 当前传输状态（未绑定视为 Disconnected）。 */
    [[nodiscard]] ZzTransportInterface::State transportState() const;

    /** @brief 应用全局设置：字号、编码、配色、内存历史行数。 */
    void applySettings(const ZzAppSettings &settings);

signals:
    /** @brief 传输状态透传（ZzTabManager 据此刷新标签外观与状态栏）。 */
    void stateChanged(ZzTransportInterface::State state);
    /** @brief 终端尺寸变化（列、行），状态栏用。 */
    void sizeChanged(int cols, int rows);
    /** @brief 传输错误透传（横幅展示由任务 13 接入本信号链路）。 */
    void errorOccurred(const QString &message);
    /** @brief 被动断开透传。 */
    void disconnected(const QString &reason);

private:
    QTermWidget *m_term = nullptr;
    ZzTransportInterface *m_transport = nullptr;
    ZzTransportEndpoint m_lastEndpoint;  ///< 最近一次 open 参数（重连用）
    QString m_encoding;                  ///< 状态栏展示的编码名
};
