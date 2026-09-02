#pragma once

#include <memory>

#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>

#include <ZzCore/ZzResult.h>
#include <ZzPureTools/ZzPageInstance.h>

class QLabel;
class QMainWindow;
class QStatusBar;
class QWidget;
class ZzCredentialStore;
class ZzLanguageManager;
class ZzMenuBarService;
class ZzSessionModel;
class ZzSessionPanel;
class ZzSftpPanel;
class ZzTabManager;
class ZzX11Service;

namespace ZzPureTools {
class ZzWorkspaceShell;
}

/**
 * @brief 组合根：持有后端服务（会话模型/凭据库），装配 IDE 工作区外壳
 *        （活动栏侧栏 + 中央页面路由），安装状态栏，并提供框架页面工厂
 *        （规格 §三/§七）。
 *
 * assemble() 只依赖 QMainWindow&——ZzApplicationWindow 是其子类，
 * 测试可用普通 QMainWindow 离屏验证全部装配链路（此时无标题栏、
 * 页面路由不迁入，侧栏与工作区装配仍完整生效）。
 */
class ZzAppShell : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 构造组合根。
     * @param configDir 配置目录（sessions.json / credentials.dat 所在）；
     *        空串=按 QStandardPaths::AppConfigLocation 解析（生产路径）。
     */
    explicit ZzAppShell(const QString &configDir = QString(),
                        QObject *parent = nullptr);

    /** @brief 析构时清空面板登记册（面板随窗口销毁，避免悬挂指针）。 */
    ~ZzAppShell() override;

    /**
     * @brief 装配窗口：经 ZzWorkspaceShell 建 IDE 工作区（会话/SFTP 侧栏
     *        延迟工厂、页面路由事务迁入、布局恢复），安装状态栏四件套，
     *        接好双击到开标签的链路。作为 ZzApplicationBuilder 的窗口装配回调调用。
     */
    [[nodiscard]] ZzCore::ZzResult<void> assemble(QMainWindow &window);

    /** @brief 终端区页面工厂（ZzTabManager 所在页，Persistent）。 */
    [[nodiscard]] ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
        createTerminalPage(QWidget *pageParent);

    /** @brief 设置页页面工厂（导航 Footer）。 */
    [[nodiscard]] ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
        createSettingsPage(QWidget *pageParent);

    // ---- 测试观察口 ----
    [[nodiscard]] ZzPureTools::ZzWorkspaceShell *workspaceShell() const;
    [[nodiscard]] ZzSessionPanel *sessionPanel() const;
    [[nodiscard]] ZzSftpPanel *sftpPanel() const;
    [[nodiscard]] ZzTabManager *tabManager() const;
    [[nodiscard]] ZzSessionModel *sessionModel() const;
    [[nodiscard]] ZzCredentialStore *credentialStore() const;
    [[nodiscard]] QLabel *statusStateLabel() const;
    [[nodiscard]] QLabel *statusEncodingLabel() const;
    [[nodiscard]] QLabel *statusSizeLabel() const;
    [[nodiscard]] QLabel *statusTunnelLabel() const;
    [[nodiscard]] ZzX11Service *x11Service() const;
    [[nodiscard]] ZzMenuBarService *menuBarService() const;
    [[nodiscard]] ZzLanguageManager *languageManager() const;

public slots:
    /** @brief 状态栏瞬时提示（5 秒自动消退，规格 §八错误不弹窗）。 */
    void showStatusMessage(const QString &message);

    /** @brief 菜单「新建会话」：确保会话面板物化后转发 newSession()。 */
    void requestNewSession();
    /** @brief 菜单「更多方案…」：导航到设置页路由。 */
    void openSettingsPage();
    /** @brief 菜单「打开日志目录」：系统文件管理器打开 logs 目录。 */
    void openLogDirectory();

protected:
    /** @brief 窗口 Close 时保存工作区布局（此时控件树仍存活，saveLayout 可用）。 */
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    /** @brief 装配 ZzTabManager 的认证/主机密钥/状态栏接线（创建终端页时调用）。 */
    void wireTabManager(ZzTabManager *tabs);

    /** @brief 会话面板创建后的登记册/双击连接接线（延迟工厂首开时调用）。 */
    void wireSessionPanel(ZzSessionPanel *panel);
    /** @brief SFTP 面板创建后的登记册/状态栏/标签管理器接线（延迟工厂首开时调用）。 */
    void wireSftpPanel(ZzSftpPanel *panel);

    /** @brief 保存工作区布局字节到 ZzAppSettings；失败仅告警不影响关闭。 */
    bool saveWorkspaceLayout();

    /** @brief 按 Fluent 主题快照重套状态栏样式（背景/分隔线/标签文字色）。 */
    void applyStatusBarTheme();

    QString m_configDir;
    ZzSessionModel *m_sessionModel = nullptr;      ///< this 为父
    ZzCredentialStore *m_credentialStore = nullptr; ///< this 为父
    std::unique_ptr<ZzPureTools::ZzWorkspaceShell> m_workspaceShell; ///< assemble 创建，本对象持有
    QPointer<QMainWindow> m_window;                ///< 装配窗口（布局保存事件过滤）
    QPointer<ZzSessionPanel> m_sessionPanel;       ///< 侧栏拥有（延迟工厂首开创建）
    QPointer<ZzSftpPanel> m_sftpPanel;             ///< 侧栏拥有（延迟工厂首开创建）
    QPointer<ZzTabManager> m_tabManager;           ///< pageParent 拥有
    QPointer<QLabel> m_stateLabel;
    QPointer<QLabel> m_encodingLabel;
    QPointer<QLabel> m_sizeLabel;
    QPointer<QLabel> m_tunnelLabel;
    QPointer<QStatusBar> m_statusBar;
    ZzX11Service *m_x11Service = nullptr; ///< 本对象为父：应用级共享 X server（M5）
    ZzLanguageManager *m_languageManager = nullptr;   ///< this 为父，构造时创建并 apply
    ZzMenuBarService *m_menuBarService = nullptr;     ///< 窗口为父（menuBar 生命周期绑定）
};
