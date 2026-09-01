// 顶部 Windows 头文件块必须用 _WIN32（编译器预定义）：本块位于全文件
// 首部，早于任何 Qt 头文件，Q_OS_WIN（qglobal.h 定义）此刻尚不可用。
#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <winsock2.h>
#include <windows.h>
#endif

#include "ZzAppShell.h"

#include <memory>
#include <optional>
#include <utility>

#include <QtCore/QEvent>
#include <QtCore/QStandardPaths>
#include <QtCore/QDebug>
#include <QtCore/QTimer>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>

#include <ZzFluentUI/ZzIconDescriptor.h>
#include <ZzPureTools/ZzApplicationWindow.h>
#include <ZzPureTools/ZzWorkspaceShell.h>
#include <ZzPureTools/ZzWorkspaceTitleMode.h>

#include "dialog/ZzHostKeyDialog.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "panel/ZzPanelRegistry.h"
#include "panel/ZzSessionPanel.h"
#include "panel/ZzSftpPanel.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionModel.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzSettingsPage.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzSshTransport.h"
#include "x11/ZzX11Service.h"

namespace {

/** @brief 传输状态 → 状态栏文案。 */
QString zzStateText(ZzTransportInterface::State state)
{
    switch (state) {
    case ZzTransportInterface::State::Connected:
        return QStringLiteral("已连接");
    case ZzTransportInterface::State::Connecting:
        return QStringLiteral("连接中…");
    case ZzTransportInterface::State::Disconnected:
        return QStringLiteral("未连接");
    }
    return QStringLiteral("未连接");
}

} // namespace

ZzAppShell::ZzAppShell(const QString &configDir, QObject *parent)
    : QObject(parent)
    , m_configDir(configDir.isEmpty()
        ? QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        : configDir)
{
    m_sessionModel = new ZzSessionModel(
        m_configDir + QStringLiteral("/sessions.json"), this);
    m_sessionModel->load();
    // 凭据后端模式来自全局设置（auto：密钥环可用则用，否则 AES 文件）
    m_credentialStore = new ZzCredentialStore(
        ZzCredentialStore::backendModeFromString(
            ZzAppSettings::instance().credentialBackend()),
        m_configDir + QStringLiteral("/credentials.dat"), this);

    // 应用级共享 X server（M5 对齐 MobaXterm：启动即拉起、全局开关可关闭、
    // 关会话不杀；应用退出时随本对象析构，QProcess 析构终止 ZzXsrv）
    m_x11Service = new ZzX11Service(this);
    m_x11Service->setEnabled(ZzAppSettings::instance().x11ServerEnabled());
    connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged, this, [this] {
        m_x11Service->setEnabled(ZzAppSettings::instance().x11ServerEnabled());
    });
}

ZzAppShell::~ZzAppShell()
{
    // 卸载私钥口令解析器：lambda 捕获了本对象的 model/store，析构后必须断开
    ZzSshTransportAdapter::setKeyPassphraseResolver(nullptr);
    // 面板随窗口销毁，登记册中的裸指针随之失效，统一清空
    ZzPanelRegistry::instance().clear();
}

ZzCore::ZzResult<void> ZzAppShell::assemble(QMainWindow &window)
{
    // 框架窗口（生产路径）取 Fluent 标题栏；普通 QMainWindow（离屏测试）允许为空
    auto *applicationWindow =
        qobject_cast<ZzPureTools::ZzApplicationWindow *>(&window);
    auto *titleBar =
        applicationWindow != nullptr ? applicationWindow->titleBar() : nullptr;

    auto created = ZzPureTools::ZzWorkspaceShell::create(&window, titleBar);
    if (!created) {
        return ZzCore::ZzResult<void>::failure(created.error());
    }
    m_workspaceShell = std::move(created).value();
    m_window = &window;

    // 会话面板 → 活动栏 LeftPrimary「会话」，延迟工厂首开才创建；
    // 面板内容无父对象交给 Shell，登记册与双击接线在工厂内完成
    auto sessions = m_workspaceShell->registerSidePanelFactory(
        ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sessions")),
        QStringLiteral("会话"),
        ZzFluentUI::ZzIconDescriptor::fromFontIcon(
            ZzFluentUI::ZzFontIcon::Terminal),
        ZzFluentUI::ZzActivityArea::LeftPrimary,
        [this]() -> ZzCore::ZzResult<std::unique_ptr<QWidget>> {
            auto panel = std::make_unique<ZzSessionPanel>(
                m_sessionModel, m_credentialStore);
            wireSessionPanel(panel.get());
            m_sessionPanel = panel.get();
            return ZzCore::ZzResult<std::unique_ptr<QWidget>>::success(
                std::move(panel));
        });
    if (!sessions) {
        return sessions;
    }

    // SFTP 面板 → 活动栏 RightPrimary「文件」，同样延迟创建
    auto files = m_workspaceShell->registerSidePanelFactory(
        ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sftp")),
        QStringLiteral("文件"),
        ZzFluentUI::ZzIconDescriptor::fromFontIcon(
            ZzFluentUI::ZzFontIcon::Folder),
        ZzFluentUI::ZzActivityArea::RightPrimary,
        [this]() -> ZzCore::ZzResult<std::unique_ptr<QWidget>> {
            auto panel = std::make_unique<ZzSftpPanel>();
            wireSftpPanel(panel.get());
            m_sftpPanel = panel.get();
            return ZzCore::ZzResult<std::unique_ptr<QWidget>>::success(
                std::move(panel));
        });
    if (!files) {
        return files;
    }

    // 页面路由事务式迁入 IDE 侧栏（官方捷径：导航面板入侧栏、页面宿主入中央
    // 固定标签，导航身份不变；失败回滚原状态）。仅框架窗口存在导航表面，
    // 普通 QMainWindow（离屏测试）无导航可迁，跳过
    if (applicationWindow != nullptr) {
        auto integrated = m_workspaceShell->integrateApplicationNavigation(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("navigation")),
            QStringLiteral("导航"),
            ZzFluentUI::ZzIconDescriptor::fromFontIcon(
                ZzFluentUI::ZzFontIcon::Sitemap),
            ZzFluentUI::ZzActivityArea::LeftPrimary,
            QStringLiteral("ZzClawTerm"));
        if (!integrated) {
            return integrated;
        }
    }
    window.setCentralWidget(m_workspaceShell->workspaceWidget());

    // 状态栏四件套：连接状态 | 编码 | 行列 | 隧道数
    m_statusBar = window.statusBar();
    m_stateLabel = new QLabel(QStringLiteral("未连接"), m_statusBar);
    m_encodingLabel = new QLabel(m_statusBar);
    m_sizeLabel = new QLabel(m_statusBar);
    m_statusBar->addPermanentWidget(m_stateLabel);
    m_statusBar->addPermanentWidget(m_encodingLabel);
    m_statusBar->addPermanentWidget(m_sizeLabel);
    m_tunnelLabel = new QLabel(QStringLiteral("隧道: 0"), m_statusBar);
    m_statusBar->addPermanentWidget(m_tunnelLabel);
    // 诊断埋点：状态栏几何/可见性（Windows 真机状态栏不可见定位，Show 后再采一次）
    qInfo().noquote() << QStringLiteral(
        "诊断：assemble 时状态栏 visible=%1 geo=%2,%3 %4x%5 windowSize=%6x%7")
        .arg(m_statusBar->isVisibleTo(&window))
        .arg(m_statusBar->geometry().x()).arg(m_statusBar->geometry().y())
        .arg(m_statusBar->geometry().width()).arg(m_statusBar->geometry().height())
        .arg(window.width()).arg(window.height());

    // 布局持久化：恢复上次工作区布局（版本化字节由 Shell 校验，不自解析）；
    // 恢复失败仅回落默认布局，不中止装配。窗口 Close 时经事件过滤保存
    const QByteArray layout = ZzAppSettings::instance().workspaceLayout();
    if (!layout.isEmpty()) {
        auto restored = m_workspaceShell->restoreLayout(layout);
        if (!restored) {
            qWarning().noquote() << QStringLiteral(
                "工作区布局恢复失败，使用默认布局：%1")
                .arg(restored.error().technicalMessage());
        }
    }

    // 窗口标题接管：库默认 Application 模式显示宿主窗口标题（框架
    // ZzApplicationWindowPrivate::refreshTranslations 固化为「ZzPureTools」），
    // 改为 当前标签标题 - 应用名；当前标签标题由页面 windowTitle 提供
    // （ZzWorkspaceShellPrivate::refreshTitle），终端页同步为会话名（见
    // ZzTabManager::refreshWindowTitle）。放在 restoreLayout 之后：布局字节
    // 含标题投影，旧版本存下的布局会覆盖此处设置（库事务语义，主仓不改库）。
    m_workspaceShell->setApplicationTitle(QStringLiteral("ZzClawTerm"));
    m_workspaceShell->setTitleMode(
        ZzPureTools::ZzWorkspaceTitleMode::CurrentTabAndApplication);
    window.installEventFilter(this);

    return ZzCore::ZzResult<void>::success();
}

void ZzAppShell::wireSessionPanel(ZzSessionPanel *panel)
{
    ZzPanelRegistry::instance().registerPanel(panel);

    // 双击会话 → 开标签（终端页可能尚未创建，经 QPointer 惰性转发）
    connect(panel, &ZzSessionPanel::connectRequested, this,
            [this](const ZzSessionProfile &profile) {
                if (m_tabManager) {
                    m_tabManager->openSession(profile);
                }
            });
}

void ZzAppShell::wireSftpPanel(ZzSftpPanel *panel)
{
    ZzPanelRegistry::instance().registerPanel(panel);
    connect(panel, &ZzSftpPanel::statusMessage,
            this, &ZzAppShell::showStatusMessage);
    if (m_tabManager) {
        panel->setTabManager(m_tabManager); // 终端页先于面板创建时补绑
    }
}

bool ZzAppShell::saveWorkspaceLayout()
{
    if (!m_workspaceShell) {
        return false;
    }
    auto saved = m_workspaceShell->saveLayout();
    if (!saved) {
        qWarning().noquote() << QStringLiteral("工作区布局保存失败：%1")
            .arg(saved.error().technicalMessage());
        return false;
    }
    ZzAppSettings::instance().setWorkspaceLayout(std::move(saved).value());
    return true;
}

bool ZzAppShell::eventFilter(QObject *watched, QEvent *event)
{
    // 窗口 Close 时保存工作区布局（此刻控件树仍存活；QCloseEvent 后窗口未必立即销毁）
    if (watched == m_window && event->type() == QEvent::Close) {
        saveWorkspaceLayout();
    }
    // 诊断埋点：窗口首次 Show 时状态栏几何/可见性（Windows 状态栏不可见定位）
    if (watched == m_window && event->type() == QEvent::Show && m_statusBar) {
        qInfo().noquote() << QStringLiteral(
            "诊断：窗口 Show 时状态栏 visible=%1 hidden=%2 geo=%3,%4 %5x%6 winGeo=%7,%8 %9x%10 central=%11")
            .arg(m_statusBar->isVisible()).arg(m_statusBar->isHidden())
            .arg(m_statusBar->geometry().x()).arg(m_statusBar->geometry().y())
            .arg(m_statusBar->geometry().width()).arg(m_statusBar->geometry().height())
            .arg(m_window->geometry().x()).arg(m_window->geometry().y())
            .arg(m_window->geometry().width()).arg(m_window->geometry().height())
            .arg(m_window->centralWidget() != nullptr);
        // 诊断临时：状态栏红色背景，肉眼确认绘制区域是否落在可见区（定位后移除）
        m_statusBar->setStyleSheet(QStringLiteral("QStatusBar { background: #ff0000; }"));
        // 延迟复查：Show 完成/DWM 归位后再采一次，并与原生窗口矩形对比。
        // Qt 几何与原生客户区不一致（QWindowKit 帧扩展）时，底部区域会被
        // 画在可见区之外——「Qt 报告可见但屏幕上看不到横条」的判别探针。
        QTimer::singleShot(1500, this, [this] {
            if (!m_window || !m_statusBar) {
                return;
            }
#ifdef Q_OS_WIN
            const HWND hwnd = reinterpret_cast<HWND>(m_window->winId());
            RECT winRect{};
            RECT clientRect{};
            GetWindowRect(hwnd, &winRect);
            GetClientRect(hwnd, &clientRect);
            POINT clientOrigin{0, 0};
            ClientToScreen(hwnd, &clientOrigin);
            qInfo().noquote() << QStringLiteral(
                "诊断：状态栏延迟复查 visible=%1 geo=%2,%3 %4x%5 "
                "qtWin=%6x%7 nativeWin=%8x%9 nativeClient=%10x%11 clientOrigin=%12,%13")
                .arg(m_statusBar->isVisible())
                .arg(m_statusBar->geometry().x()).arg(m_statusBar->geometry().y())
                .arg(m_statusBar->geometry().width()).arg(m_statusBar->geometry().height())
                .arg(m_window->width()).arg(m_window->height())
                .arg(winRect.right - winRect.left).arg(winRect.bottom - winRect.top)
                .arg(clientRect.right).arg(clientRect.bottom)
                .arg(clientOrigin.x).arg(clientOrigin.y);
#else
            qInfo().noquote() << QStringLiteral(
                "诊断：状态栏延迟复查 visible=%1 geo=%2,%3 %4x%5 qtWin=%6x%7")
                .arg(m_statusBar->isVisible())
                .arg(m_statusBar->geometry().x()).arg(m_statusBar->geometry().y())
                .arg(m_statusBar->geometry().width()).arg(m_statusBar->geometry().height())
                .arg(m_window->width()).arg(m_window->height());
#endif
        });
    }
    return QObject::eventFilter(watched, event);
}

ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
ZzAppShell::createTerminalPage(QWidget *pageParent)
{
    auto view = std::make_unique<ZzTabManager>(pageParent);
    wireTabManager(view.get());
    m_tabManager = view.get();

    auto viewModel = std::make_unique<QObject>();
    auto presenter = std::make_unique<QObject>();
    QWidget *viewObserver = view.release();
    return ZzPureTools::ZzPageInstance::create(
        pageParent, viewObserver, std::move(viewModel), std::move(presenter));
}

ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
ZzAppShell::createSettingsPage(QWidget *pageParent)
{
    auto view = std::make_unique<ZzSettingsPage>(
        &ZzAppSettings::instance(), pageParent);
    auto viewModel = std::make_unique<QObject>();
    auto presenter = std::make_unique<QObject>();
    QWidget *viewObserver = view.release();
    return ZzPureTools::ZzPageInstance::create(
        pageParent, viewObserver, std::move(viewModel), std::move(presenter));
}

void ZzAppShell::wireTabManager(ZzTabManager *tabs)
{
    // SFTP 面板跟随焦点窗格（assemble 先于终端页创建时在此补绑）
    if (m_sftpPanel) {
        m_sftpPanel->setTabManager(tabs);
    }

    // SSH 认证：密码经主密码解锁后从凭据库取（规格 §七连接流程）
    ZzCredentialStore *store = m_credentialStore;
    tabs->setPasswordProvider(
        [store, tabs](const ZzSessionProfile &profile) -> QString {
            if (profile.credentialId.isNull()) {
                return {};
            }
            if (!ZzMasterPasswordDialog::ensureUnlocked(store, tabs)) {
                return {}; // 用户取消解锁 → 取消密码认证
            }
            return store->credential(profile.credentialId)
                .value_or(QString());
        });

    // SSH 私钥口令：profile 只留 keyPassphraseCredentialId 引用，口令明文在连接时
    // 按 endpoint（host/port/user/keyPath）反查会话模型后从凭据库取（不经过 endpoint 下传）
    // 捕获用 QPointer：组合根析构虽会卸载解析器，双保险防悬挂（契约：仅 GUI 线程调用）
    QPointer<ZzSessionModel> model = m_sessionModel;
    QPointer<ZzCredentialStore> storePtr = m_credentialStore;
    ZzSshTransportAdapter::setKeyPassphraseResolver(
        [model, storePtr, tabs](const ZzTransportEndpoint &endpoint) -> QString {
            if (endpoint.localShell || endpoint.keyPath.isEmpty()) {
                return {};
            }
            if (!model || !storePtr) {
                return {}; // 组合根已析构
            }
            for (const ZzSessionProfile &candidate : model->allSessions()) {
                if (candidate.authMethod != ZzAuthMethod::PrivateKey
                    || candidate.keyPassphraseCredentialId.isNull()
                    || candidate.host != endpoint.host
                    || candidate.port != endpoint.port
                    || candidate.userName != endpoint.user
                    || candidate.privateKeyPath != endpoint.keyPath) {
                    continue;
                }
                if (!ZzMasterPasswordDialog::ensureUnlocked(storePtr, tabs)) {
                    return {}; // 用户取消解锁 → 视为无口令，交给服务端拒绝
                }
                return storePtr->credential(candidate.keyPassphraseCredentialId)
                    .value_or(QString());
            }
            // 无匹配档案：认证失败时便于区分「口令未取到」与「口令错误」（不含敏感信息）
            qWarning().noquote() << QStringLiteral(
                "私钥口令解析：未找到匹配会话档案（host=%1 port=%2 user=%3），按无口令继续")
                .arg(endpoint.host).arg(endpoint.port).arg(endpoint.user);
            return {};
        });
    // 主机密钥确认（规格 §八安全底线）
    tabs->setHostKeyConfirmer(
        [tabs](const QString &host, const QString &fingerprint,
               const QString &oldFingerprint, bool changed) {
            return ZzHostKeyDialog::confirm(host, fingerprint, oldFingerprint,
                                            changed, tabs);
        });

    // 共享 X server 门面注入各 SSH 传输；服务级异常上状态栏（无会话时也能感知）
    tabs->setX11Service(m_x11Service);
    // 防重复装配：wireTabManager 被多次调用时先撤后接（lambda 无法用 UniqueConnection）
    disconnect(m_x11Service, nullptr, this, nullptr);
    connect(m_x11Service, &ZzX11Service::startFailed, this,
            &ZzAppShell::showStatusMessage);
    connect(m_x11Service, &ZzX11Service::serverCrashed, this,
            [this](const QString &message) {
                showStatusMessage(QStringLiteral("X server 异常退出：%1").arg(message));
            });

    // 状态栏：状态 / 编码 / 行列 / 瞬时消息
    tabs->connect(tabs, &ZzTabManager::currentStateChanged, this,
                  [this](ZzTransportInterface::State state) {
                      if (m_stateLabel) {
                          m_stateLabel->setText(zzStateText(state));
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::currentEncodingChanged, this,
                  [this](const QString &encoding) {
                      if (m_encodingLabel) {
                          m_encodingLabel->setText(encoding);
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::currentSizeChanged, this,
                  [this](int cols, int rows) {
                      if (m_sizeLabel) {
                          m_sizeLabel->setText(
                              QStringLiteral("%1×%2").arg(cols).arg(rows));
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::currentTunnelCountChanged, this,
                  [this](int count) {
                      if (m_tunnelLabel) {
                          m_tunnelLabel->setText(QStringLiteral("隧道: %1").arg(count));
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::statusMessage, this,
                  &ZzAppShell::showStatusMessage);

    // 设置变更实时应用到全部已打开标签的全部窗格（规格 §七）
    tabs->connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged,
                  tabs, [tabs]() {
                      const ZzAppSettings &settings = ZzAppSettings::instance();
                      for (int i = 0; i < tabs->count(); ++i) {
                          for (ZzTerminalView *view : tabs->viewsAt(i)) {
                              view->applySettings(settings);
                          }
                      }
                  });
}

void ZzAppShell::showStatusMessage(const QString &message)
{
    if (m_statusBar) {
        m_statusBar->showMessage(message, 5000);
    }
}

ZzPureTools::ZzWorkspaceShell *ZzAppShell::workspaceShell() const
{
    return m_workspaceShell.get();
}

ZzSessionPanel *ZzAppShell::sessionPanel() const { return m_sessionPanel; }
ZzSftpPanel *ZzAppShell::sftpPanel() const { return m_sftpPanel; }
ZzTabManager *ZzAppShell::tabManager() const { return m_tabManager; }
ZzSessionModel *ZzAppShell::sessionModel() const { return m_sessionModel; }
ZzCredentialStore *ZzAppShell::credentialStore() const { return m_credentialStore; }
QLabel *ZzAppShell::statusStateLabel() const { return m_stateLabel; }
QLabel *ZzAppShell::statusEncodingLabel() const { return m_encodingLabel; }
QLabel *ZzAppShell::statusSizeLabel() const { return m_sizeLabel; }
QLabel *ZzAppShell::statusTunnelLabel() const { return m_tunnelLabel; }
ZzX11Service *ZzAppShell::x11Service() const { return m_x11Service; }
