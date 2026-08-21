#pragma once

#include <functional>

#include <QtCore/QHash>
#include <QtCore/QList>
#include <QtWidgets/QTabWidget>

#include "session/ZzSessionProfile.h"
#include "tab/ZzSplitContainer.h"
#include "transport/ZzTransportInterface.h"

class ZzTerminalView;

/**
 * @brief 多标签管理：每标签一个 ZzSplitContainer（叶子为 ZzTerminalView，规格 §七）。
 *
 * 行为约定：关闭即销毁视图与传输；断线标签变灰保留、不自动关；
 * 重连对标签内全部断线窗格逐一换新传输实例
 * （ZzSshConnection 不可重复 connectToHost）；
 * 拖拽排序由 QTabWidget 自带 movable 提供。
 * 分屏：每标签一棵 ZzSplitContainer 分割树，新窗格复用锚点窗格的
 * profile 开独立会话；焦点导航/关闭最后窗格关闭整个标签由容器信号驱动。
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

    /** @brief 重连指定标签：对标签内全部断线窗格逐一换新传输实例并按记忆 profile 重新打开。 */
    void reconnectTab(int index);

    /** @brief 取标签内当前焦点窗格（测试与状态栏用），越界返回 nullptr。 */
    [[nodiscard]] ZzTerminalView *viewAt(int index) const;

    /** @brief 取标签的分屏容器，越界返回 nullptr。 */
    [[nodiscard]] ZzSplitContainer *containerAt(int index) const;

    /** @brief 标签内全部窗格（设置实时应用等批量操作用），越界返回空表。 */
    [[nodiscard]] QList<ZzTerminalView *> viewsAt(int index) const;

    /** @brief 标签内窗格数，越界返回 0。 */
    [[nodiscard]] int paneCountAt(int index) const;

    /** @brief 当前标签以焦点窗格为锚点分屏，新窗格用同一 profile 开新会话。 */
    void splitCurrentTab(Qt::Orientation orientation);

    /** @brief 关闭当前标签的焦点窗格（最后一个窗格关闭时整标签关闭）。 */
    void closeCurrentPane();

    /** @brief 当前标签内按方向移动窗格焦点。 */
    void focusPane(ZzSplitContainer::FocusDirection direction);

    /** @brief 标签内是否存在断线窗格（有断线窗格的标签变灰保留）。 */
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
    /** @brief 当前标签焦点窗格的活动隧道数变化（状态栏第四要素；按窗格统计）。 */
    void currentTunnelCountChanged(int count);
    /**
     * @brief 当前焦点窗格变化（切标签或标签内切窗格；末标签关闭后为 nullptr）。
     *        SFTP 面板据此跟随活动会话。
     */
    void currentViewChanged(ZzTerminalView *view);

private slots:
    void showTabContextMenu(const QPoint &pos);

private:
    /** @brief profile → 传输参数映射（终端类型等空值回落到全局设置）。 */
    ZzTransportEndpoint endpointFor(const ZzSessionProfile &profile) const;
    /** @brief 按 profile 建窗格（创建传输、装配 SSH 回调、应用设置），未知协议发状态消息并返回 nullptr。 */
    ZzTerminalView *createPane(const ZzSessionProfile &profile, QWidget *parent);
    /** @brief 视图所属标签索引（视图在分屏容器内，经容器反查），无则 -1。 */
    int tabIndexOfView(ZzTerminalView *view) const;
    /** @brief 按 profile 创建传输实例并装配 SSH 回调，未注册协议返回 nullptr（不发状态消息，由调用方决定文案）。 */
    ZzTransportInterface *createTransport(const ZzSessionProfile &profile,
                                          ZzTerminalView *view);
    /** @brief 指定标签以焦点窗格为锚点分屏（splitCurrentTab 与右键菜单共用）。 */
    void splitTab(int index, Qt::Orientation orientation);
    void wireView(ZzTerminalView *view);
    void wireContainer(ZzSplitContainer *container);
    void markTabDisconnected(int index, const QString &reason);

    QHash<ZzTerminalView *, ZzSessionProfile> m_tabProfiles;
    QHash<ZzTerminalView *, int> m_tabTunnelCounts; ///< 每窗格活动隧道数（按 ZzTerminalView 键控）
    ZzPasswordProvider m_passwordProvider;
    ZzHostKeyConfirmer m_hostKeyConfirmer;
};
