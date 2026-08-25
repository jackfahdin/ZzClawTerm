#include "ZzAppShell.h"

#include <optional>
#include <utility>

#include <QtCore/QStandardPaths>
#include <QtCore/QDebug>
#include <QtWidgets/QDockWidget>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>

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
    // 会话面板 Dock（规格 §七：可折叠、可停靠左右）
    auto *panel = new ZzSessionPanel(m_sessionModel, m_credentialStore, &window);
    ZzPanelRegistry::instance().registerPanel(panel);
    window.addDockWidget(Qt::LeftDockWidgetArea, panel);
    m_sessionPanel = panel;

    // SFTP 面板 Dock：跟随当前标签焦点窗格的 SSH 连接（本地会话面板内提示不可用）
    auto *sftpPanel = new ZzSftpPanel(&window);
    ZzPanelRegistry::instance().registerPanel(sftpPanel);
    window.addDockWidget(Qt::RightDockWidgetArea, sftpPanel);
    connect(sftpPanel, &ZzSftpPanel::statusMessage,
            this, &ZzAppShell::showStatusMessage);
    if (m_tabManager) {
        sftpPanel->setTabManager(m_tabManager); // 终端页先于 assemble 创建时补绑
    }
    m_sftpPanel = sftpPanel;

    // 状态栏三要素：连接状态 | 编码 | 行列
    m_statusBar = window.statusBar();
    m_stateLabel = new QLabel(QStringLiteral("未连接"), m_statusBar);
    m_encodingLabel = new QLabel(m_statusBar);
    m_sizeLabel = new QLabel(m_statusBar);
    m_statusBar->addPermanentWidget(m_stateLabel);
    m_statusBar->addPermanentWidget(m_encodingLabel);
    m_statusBar->addPermanentWidget(m_sizeLabel);
    m_tunnelLabel = new QLabel(QStringLiteral("隧道: 0"), m_statusBar);
    m_statusBar->addPermanentWidget(m_tunnelLabel);

    // 双击会话 → 开标签（终端页可能尚未创建，经 QPointer 惰性转发）
    connect(panel, &ZzSessionPanel::connectRequested, this,
            [this](const ZzSessionProfile &profile) {
                if (m_tabManager) {
                    m_tabManager->openSession(profile);
                }
            });
    return ZzCore::ZzResult<void>::success();
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
