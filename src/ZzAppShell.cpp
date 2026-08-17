#include "ZzAppShell.h"

#include <optional>
#include <utility>

#include <QtCore/QStandardPaths>
#include <QtWidgets/QDockWidget>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>

#include "dialog/ZzHostKeyDialog.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "panel/ZzPanelRegistry.h"
#include "panel/ZzSessionPanel.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionModel.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzSettingsPage.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"

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
    m_credentialStore = new ZzCredentialStore(
        m_configDir + QStringLiteral("/credentials.dat"), this);
}

ZzAppShell::~ZzAppShell()
{
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

    // 状态栏三要素：连接状态 | 编码 | 行列
    m_statusBar = window.statusBar();
    m_stateLabel = new QLabel(QStringLiteral("未连接"), m_statusBar);
    m_encodingLabel = new QLabel(m_statusBar);
    m_sizeLabel = new QLabel(m_statusBar);
    m_statusBar->addPermanentWidget(m_stateLabel);
    m_statusBar->addPermanentWidget(m_encodingLabel);
    m_statusBar->addPermanentWidget(m_sizeLabel);

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
    // 主机密钥确认（规格 §八安全底线）
    tabs->setHostKeyConfirmer(
        [tabs](const QString &host, const QString &fingerprint, bool changed) {
            return ZzHostKeyDialog::confirm(host, fingerprint, changed, tabs);
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
    tabs->connect(tabs, &ZzTabManager::statusMessage, this,
                  &ZzAppShell::showStatusMessage);

    // 设置变更实时应用到全部已打开标签（规格 §七）
    tabs->connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged,
                  tabs, [tabs]() {
                      const ZzAppSettings &settings = ZzAppSettings::instance();
                      for (int i = 0; i < tabs->count(); ++i) {
                          if (auto *view = tabs->viewAt(i)) {
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
ZzTabManager *ZzAppShell::tabManager() const { return m_tabManager; }
ZzSessionModel *ZzAppShell::sessionModel() const { return m_sessionModel; }
ZzCredentialStore *ZzAppShell::credentialStore() const { return m_credentialStore; }
QLabel *ZzAppShell::statusStateLabel() const { return m_stateLabel; }
QLabel *ZzAppShell::statusEncodingLabel() const { return m_encodingLabel; }
QLabel *ZzAppShell::statusSizeLabel() const { return m_sizeLabel; }
