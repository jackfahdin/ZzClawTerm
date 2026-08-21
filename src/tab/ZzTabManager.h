#pragma once

#include <functional>

#include <QtCore/QHash>
#include <QtWidgets/QTabWidget>

#include "session/ZzSessionProfile.h"
#include "transport/ZzTransportInterface.h"

class ZzTerminalView;

/**
 * @brief 多标签管理：每标签一个 ZzTerminalView（规格 §七）。
 *
 * 行为约定：关闭即销毁视图与传输；断线标签变灰保留、不自动关；
 * 重连创建全新传输实例（ZzSshConnection 不可重复 connectToHost）；
 * 拖拽排序由 QTabWidget 自带 movable 提供。
 */
class ZzTabManager : public QTabWidget
{
    Q_OBJECT
public:
    /** @brief SSH 密码索取回调：按 profile 返回明文密码（空=取消认证）。 */
    using ZzPasswordProvider =
        std::function<QString(const ZzSessionProfile &profile)>;
    /** @brief 主机密钥确认回调：host/fingerprint/oldFingerprint/changed → 是否接受。
     *         changed=false（首次连接）时 oldFingerprint 为空串。 */
    using ZzHostKeyConfirmer =
        std::function<bool(const QString &host, const QString &fingerprint,
                           const QString &oldFingerprint, bool changed)>;

    explicit ZzTabManager(QWidget *parent = nullptr);

    /** @brief 按会话 profile 新建标签并开始连接（协议未知则只发状态消息）。 */
    void openSession(const ZzSessionProfile &profile);

    /** @brief 关闭并销毁指定标签。 */
    void closeTab(int index);

    /** @brief 重连指定标签：换新传输实例后按记忆 profile 重新打开。 */
    void reconnectTab(int index);

    /** @brief 取标签内的终端视图（测试与状态栏用），越界返回 nullptr。 */
    [[nodiscard]] ZzTerminalView *viewAt(int index) const;

    /** @brief 标签是否处于断线保留状态。 */
    [[nodiscard]] bool isTabDisconnected(int index) const;

    /** @brief 装配 SSH 密码索取回调（ZzAppShell 注入）。 */
    void setPasswordProvider(ZzPasswordProvider provider);

    /** @brief 装配主机密钥确认回调（ZzAppShell 注入）。 */
    void setHostKeyConfirmer(ZzHostKeyConfirmer confirmer);

signals:
    /** @brief 当前标签传输状态变化（状态栏）。 */
    void currentStateChanged(ZzTransportInterface::State state);
    /** @brief 当前标签编码（状态栏）。 */
    void currentEncodingChanged(const QString &encoding);
    /** @brief 当前标签终端尺寸（状态栏）。 */
    void currentSizeChanged(int cols, int rows);
    /** @brief 需要状态栏展示的瞬时消息（错误处理走状态栏，规格 §八）。 */
    void statusMessage(const QString &message);
    /** @brief 当前标签活动隧道数变化（状态栏第四要素）。 */
    void currentTunnelCountChanged(int count);

private slots:
    void showTabContextMenu(const QPoint &pos);

private:
    /** @brief profile → 传输参数映射（终端类型等空值回落到全局设置）。 */
    ZzTransportEndpoint endpointFor(const ZzSessionProfile &profile) const;
    void wireView(int index, ZzTerminalView *view);
    void markTabDisconnected(int index, const QString &reason);

    QHash<ZzTerminalView *, ZzSessionProfile> m_tabProfiles;
    QHash<ZzTerminalView *, int> m_tabTunnelCounts; ///< 每标签活动隧道数
    ZzPasswordProvider m_passwordProvider;
    ZzHostKeyConfirmer m_hostKeyConfirmer;
};
