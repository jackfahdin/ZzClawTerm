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
class ZzSessionModel;
class ZzSessionPanel;
class ZzTabManager;

/**
 * @brief 组合根：持有后端服务（会话模型/凭据库），装配窗口 Dock、
 *        状态栏，并提供框架页面工厂（规格 §三/§七）。
 *
 * assemble() 只依赖 QMainWindow&——ZzApplicationWindow 是其子类，
 * 测试可用普通 QMainWindow 离屏验证全部装配链路。
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
     * @brief 装配窗口：停靠会话面板、安装状态栏三要素、接好双击到开标签的链路。
     *        作为 ZzApplicationBuilder 的窗口装配回调调用。
     */
    [[nodiscard]] ZzCore::ZzResult<void> assemble(QMainWindow &window);

    /** @brief 终端区页面工厂（ZzTabManager 所在页，Persistent）。 */
    [[nodiscard]] ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
        createTerminalPage(QWidget *pageParent);

    /** @brief 设置页页面工厂（导航 Footer）。 */
    [[nodiscard]] ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
        createSettingsPage(QWidget *pageParent);

    // ---- 测试观察口 ----
    [[nodiscard]] ZzSessionPanel *sessionPanel() const;
    [[nodiscard]] ZzTabManager *tabManager() const;
    [[nodiscard]] ZzSessionModel *sessionModel() const;
    [[nodiscard]] ZzCredentialStore *credentialStore() const;
    [[nodiscard]] QLabel *statusStateLabel() const;
    [[nodiscard]] QLabel *statusEncodingLabel() const;
    [[nodiscard]] QLabel *statusSizeLabel() const;
    [[nodiscard]] QLabel *statusTunnelLabel() const;

public slots:
    /** @brief 状态栏瞬时提示（5 秒自动消退，规格 §八错误不弹窗）。 */
    void showStatusMessage(const QString &message);

private:
    /** @brief 装配 ZzTabManager 的认证/主机密钥/状态栏接线（创建终端页时调用）。 */
    void wireTabManager(ZzTabManager *tabs);

    QString m_configDir;
    ZzSessionModel *m_sessionModel = nullptr;      ///< this 为父
    ZzCredentialStore *m_credentialStore = nullptr; ///< this 为父
    QPointer<ZzSessionPanel> m_sessionPanel;       ///< 窗口拥有
    QPointer<ZzTabManager> m_tabManager;           ///< pageParent 拥有
    QPointer<QLabel> m_stateLabel;
    QPointer<QLabel> m_encodingLabel;
    QPointer<QLabel> m_sizeLabel;
    QPointer<QLabel> m_tunnelLabel;
    QPointer<QStatusBar> m_statusBar;
};
