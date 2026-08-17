#include "ZzTabManager.h"

#include <utility>

#include <QtWidgets/QMenu>
#include <QtWidgets/QTabBar>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzSshTransport.h"
#include "transport/ZzTransportRegistry.h"

ZzTabManager::ZzTabManager(QWidget *parent)
    : QTabWidget(parent)
{
    setTabsClosable(true);
    setMovable(true);          // 拖拽排序
    setDocumentMode(true);
    tabBar()->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(tabBar(), &QTabBar::customContextMenuRequested,
            this, &ZzTabManager::showTabContextMenu);
    connect(this, &QTabWidget::tabCloseRequested,
            this, &ZzTabManager::closeTab);
    // 切换标签时刷新状态栏三要素
    connect(this, &QTabWidget::currentChanged, this, [this](int index) {
        ZzTerminalView *view = viewAt(index);
        if (!view) {
            return;
        }
        emit currentStateChanged(view->transportState());
        emit currentEncodingChanged(view->encoding());
        emit currentSizeChanged(view->termWidget()->screenColumnsCount(),
                                view->termWidget()->screenLinesCount());
    });
}

void ZzTabManager::openSession(const ZzSessionProfile &profile)
{
    auto *view = new ZzTerminalView(this);
    ZzTransportInterface *transport =
        ZzTransportRegistry::instance().create(profile.protocol, view);
    if (!transport) {
        emit statusMessage(QStringLiteral("未知协议「%1」：会话 %2 未打开")
                               .arg(profile.protocol, profile.name));
        view->deleteLater();
        return;
    }
    // SSH 传输装配认证与主机密钥回调（本地 PTY 不需要）
    if (auto *ssh = qobject_cast<ZzSshTransport *>(transport)) {
        const ZzPasswordProvider provider = m_passwordProvider;
        ssh->setPasswordProvider([provider, profile]() -> QString {
            return provider ? provider(profile) : QString();
        });
        ssh->setHostKeyConfirmer(
            [confirmer = m_hostKeyConfirmer](const QString &host,
                                             const QString &fingerprint,
                                             const QString &oldFingerprint,
                                             bool changed) {
                return confirmer
                    ? confirmer(host, fingerprint, oldFingerprint, changed)
                    : false;
            });
    }
    view->setTransport(transport);
    view->applySettings(ZzAppSettings::instance());

    m_tabProfiles.insert(view, profile);
    const int index = addTab(view, profile.name);
    setCurrentIndex(index);
    wireView(index, view);

    view->enableScrollback(profile.id.toString(QUuid::WithoutBraces));
    view->openEndpoint(endpointFor(profile));
}

void ZzTabManager::closeTab(int index)
{
    ZzTerminalView *view = viewAt(index);
    if (!view) {
        return;
    }
    if (view->transport()) {
        view->transport()->close();
    }
    m_tabProfiles.remove(view);
    removeTab(index);
    view->deleteLater();
}

void ZzTabManager::reconnectTab(int index)
{
    ZzTerminalView *view = viewAt(index);
    if (!view || !m_tabProfiles.contains(view)) {
        return;
    }
    const ZzSessionProfile profile = m_tabProfiles.value(view);
    if (view->transport()) {
        view->transport()->close();
        view->transport()->deleteLater(); // 旧实例废弃，重连必须新实例
    }
    ZzTransportInterface *transport =
        ZzTransportRegistry::instance().create(profile.protocol, view);
    if (!transport) {
        emit statusMessage(QStringLiteral("重连失败：协议「%1」未注册")
                               .arg(profile.protocol));
        return;
    }
    if (auto *ssh = qobject_cast<ZzSshTransport *>(transport)) {
        const ZzPasswordProvider provider = m_passwordProvider;
        ssh->setPasswordProvider([provider, profile]() -> QString {
            return provider ? provider(profile) : QString();
        });
        ssh->setHostKeyConfirmer(
            [confirmer = m_hostKeyConfirmer](const QString &host,
                                             const QString &fingerprint,
                                             const QString &oldFingerprint,
                                             bool changed) {
                return confirmer
                    ? confirmer(host, fingerprint, oldFingerprint, changed)
                    : false;
            });
    }
    view->setTransport(transport);
    tabBar()->setTabTextColor(index, palette().color(QPalette::WindowText));
    tabBar()->setTabToolTip(index, QString());
    // 注意：视图级信号接线（wireView）在 openSession 已建立，此处不得重复调用，
    // 否则 currentStateChanged 等信号会翻倍发射
    view->enableScrollback(profile.id.toString(QUuid::WithoutBraces));
    view->openEndpoint(endpointFor(profile));
}

ZzTerminalView *ZzTabManager::viewAt(int index) const
{
    return qobject_cast<ZzTerminalView *>(widget(index));
}

bool ZzTabManager::isTabDisconnected(int index) const
{
    const ZzTerminalView *view = viewAt(index);
    return view && m_tabProfiles.contains(const_cast<ZzTerminalView *>(view))
        && view->transportState() == ZzTransportInterface::State::Disconnected
        && tabBar()->tabTextColor(index) == QColor(Qt::gray);
}

void ZzTabManager::setPasswordProvider(ZzPasswordProvider provider)
{
    m_passwordProvider = std::move(provider);
}

void ZzTabManager::setHostKeyConfirmer(ZzHostKeyConfirmer confirmer)
{
    m_hostKeyConfirmer = std::move(confirmer);
}

void ZzTabManager::showTabContextMenu(const QPoint &pos)
{
    const int index = tabBar()->tabAt(pos);
    if (index < 0) {
        return;
    }
    QMenu menu(this);
    QAction *reconnectAction =
        menu.addAction(QStringLiteral("重新连接"));
    reconnectAction->setEnabled(isTabDisconnected(index));
    QAction *closeAction = menu.addAction(QStringLiteral("关闭标签"));
    QAction *chosen = menu.exec(tabBar()->mapToGlobal(pos));
    if (chosen == reconnectAction) {
        reconnectTab(index);
    } else if (chosen == closeAction) {
        closeTab(index);
    }
}

ZzTransportEndpoint ZzTabManager::endpointFor(const ZzSessionProfile &profile) const
{
    const ZzAppSettings &settings = ZzAppSettings::instance();
    ZzTransportEndpoint endpoint;
    endpoint.host = profile.host;
    endpoint.port = profile.port;
    endpoint.user = profile.userName;
    endpoint.terminalType = profile.terminalType.isEmpty()
        ? settings.terminalType() : profile.terminalType;
    endpoint.keyPath = profile.privateKeyPath;
    endpoint.keepaliveIntervalSeconds = profile.keepAliveIntervalSeconds;
    endpoint.localShell = (profile.protocol == QStringLiteral("local"));
    if (endpoint.localShell) {
        // 契约约定：local 会话的 shell 程序路径存于 host 字段（可空=系统默认）
        endpoint.shellProgram = profile.host;
    }
    // 初始行列以视图当前尺寸为准，open 后由 termSizeChange 信号持续同步
    auto *view = qobject_cast<ZzTerminalView *>(currentWidget());
    endpoint.cols = view ? view->termWidget()->screenColumnsCount() : 80;
    endpoint.rows = view ? view->termWidget()->screenLinesCount() : 24;
    return endpoint;
}

void ZzTabManager::wireView(int index, ZzTerminalView *view)
{
    connect(view, &ZzTerminalView::disconnected, this,
            [this, view](const QString &reason) {
                const int i = indexOf(view);
                if (i >= 0) {
                    markTabDisconnected(i, reason);
                }
            });
    connect(view, &ZzTerminalView::stateChanged, this,
            [this, view](ZzTransportInterface::State state) {
                const int i = indexOf(view);
                if (i < 0) {
                    return;
                }
                // 重新连通：恢复正常颜色
                if (state == ZzTransportInterface::State::Connected) {
                    tabBar()->setTabTextColor(
                        i, palette().color(QPalette::WindowText));
                    tabBar()->setTabToolTip(i, QString());
                }
                if (i == currentIndex()) {
                    emit currentStateChanged(state);
                }
            });
    connect(view, &ZzTerminalView::sizeChanged, this,
            [this, view](int cols, int rows) {
                if (indexOf(view) == currentIndex()) {
                    emit currentSizeChanged(cols, rows);
                }
            });
    connect(view, &ZzTerminalView::errorOccurred, this,
            [this](const QString &message) { emit statusMessage(message); });
}

void ZzTabManager::markTabDisconnected(int index, const QString &reason)
{
    tabBar()->setTabTextColor(index, QColor(Qt::gray)); // 断线变灰保留
    tabBar()->setTabToolTip(index, reason);
    emit statusMessage(QStringLiteral("%1 已断开：%2")
                           .arg(tabText(index), reason));
}
