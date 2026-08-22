#include "ZzTabManager.h"

#include <utility>

#include <QtGui/QKeySequence>
#include <QtGui/QShortcut>
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
        emit currentViewChanged(view); // SFTP 面板跟随（末标签关闭后为 nullptr）
        if (!view) {
            return;
        }
        emit currentStateChanged(view->transportState());
        emit currentEncodingChanged(view->encoding());
        emit currentSizeChanged(view->termWidget()->screenColumnsCount(),
                                view->termWidget()->screenLinesCount());
        emit currentTunnelCountChanged(m_tabTunnelCounts.value(view, 0));
    });

    // 分屏快捷键（WidgetWithChildrenShortcut：终端内焦点也生效；
    // Ctrl+Shift 双修饰键不会被 QTermWidget 的 ShortcutOverride 吞掉）
    auto addShortcut = [this](const QKeySequence &key,
                              std::function<void()> slot) {
        auto *shortcut = new QShortcut(key, this);
        shortcut->setContext(Qt::WidgetWithChildrenShortcut);
        connect(shortcut, &QShortcut::activated, this, std::move(slot));
    };
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+E")),
                [this]() { splitCurrentTab(Qt::Horizontal); }); // 左右分屏
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+O")),
                [this]() { splitCurrentTab(Qt::Vertical); });   // 上下分屏
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+W")),
                [this]() { closeCurrentPane(); });
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+Left")),
                [this]() { focusPane(ZzSplitContainer::FocusDirection::Left); });
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+Right")),
                [this]() { focusPane(ZzSplitContainer::FocusDirection::Right); });
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+Up")),
                [this]() { focusPane(ZzSplitContainer::FocusDirection::Up); });
    addShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+Down")),
                [this]() { focusPane(ZzSplitContainer::FocusDirection::Down); });
}

void ZzTabManager::openSession(const ZzSessionProfile &profile)
{
    auto *container = new ZzSplitContainer(this);
    ZzTerminalView *view = createPane(profile, container);
    if (!view) {
        container->deleteLater();
        return;
    }
    container->addInitialView(view);
    m_tabProfiles.insert(view, profile);
    const int index = addTab(container, profile.name);
    setCurrentIndex(index);
    wireView(view);
    wireContainer(container);

    view->enableScrollback(profile.id.toString(QUuid::WithoutBraces));
    view->openEndpoint(endpointFor(profile));
}

ZzTerminalView *ZzTabManager::createPane(const ZzSessionProfile &profile,
                                         QWidget *parent)
{
    auto *view = new ZzTerminalView(parent);
    ZzTransportInterface *transport = createTransport(profile, view);
    if (!transport) {
        emit statusMessage(QStringLiteral("未知协议「%1」：会话 %2 未打开")
                               .arg(profile.protocol, profile.name));
        view->deleteLater();
        return nullptr;
    }
    view->setTransport(transport);
    view->applySettings(ZzAppSettings::instance());
    return view;
}

ZzTransportInterface *ZzTabManager::createTransport(
    const ZzSessionProfile &profile, ZzTerminalView *view)
{
    ZzTransportInterface *transport =
        ZzTransportRegistry::instance().create(profile.protocol, view);
    if (!transport) {
        return nullptr;
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
    return transport;
}

void ZzTabManager::splitCurrentTab(Qt::Orientation orientation)
{
    splitTab(currentIndex(), orientation);
}

void ZzTabManager::splitTab(int index, Qt::Orientation orientation)
{
    ZzSplitContainer *container = containerAt(index);
    ZzTerminalView *anchor = container ? container->focusedView() : nullptr;
    if (!anchor || !m_tabProfiles.contains(anchor)) {
        return;
    }
    // 分屏出的新窗格复用锚点窗格的 profile 开独立会话
    const ZzSessionProfile profile = m_tabProfiles.value(anchor);
    ZzTerminalView *view = createPane(profile, container);
    if (!view) {
        return;
    }
    container->splitFocused(orientation, view);
    m_tabProfiles.insert(view, profile);
    wireView(view);

    view->enableScrollback(profile.id.toString(QUuid::WithoutBraces));
    view->openEndpoint(endpointFor(profile));
}

void ZzTabManager::closeCurrentPane()
{
    ZzSplitContainer *container = containerAt(currentIndex());
    if (!container) {
        return;
    }
    if (ZzTerminalView *view = container->focusedView()) {
        container->closeView(view); // 最后窗格关闭时 emptied 信号收掉整标签
    }
}

void ZzTabManager::focusPane(ZzSplitContainer::FocusDirection direction)
{
    if (ZzSplitContainer *container = containerAt(currentIndex())) {
        container->focusPane(direction);
    }
}

void ZzTabManager::closeTab(int index)
{
    ZzSplitContainer *container = containerAt(index);
    if (!container) {
        return;
    }
    // 关闭即销毁：全部窗格的传输与视图随容器一起释放
    for (ZzTerminalView *view : container->views()) {
        if (view->transport()) {
            view->transport()->close();
        }
        m_tabProfiles.remove(view);
        m_tabTunnelCounts.remove(view);
    }
    removeTab(index);
    container->deleteLater();
}

void ZzTabManager::reconnectTab(int index)
{
    ZzSplitContainer *container = containerAt(index);
    if (!container) {
        return;
    }
    // 对标签内全部断线窗格逐一重连：旧实例废弃，重连必须换新实例
    // （ZzSshConnection 不可重复 connectToHost，规格 §十注释约定）
    for (ZzTerminalView *view : container->views()) {
        if (!m_tabProfiles.contains(view)
            || view->transportState()
                != ZzTransportInterface::State::Disconnected) {
            continue;
        }
        const ZzSessionProfile profile = m_tabProfiles.value(view);
        if (view->transport()) {
            view->transport()->close();
            view->transport()->deleteLater();
        }
        ZzTransportInterface *transport = createTransport(profile, view);
        if (!transport) {
            emit statusMessage(QStringLiteral("重连失败：协议「%1」未注册")
                                   .arg(profile.protocol));
            continue;
        }
        view->setTransport(transport);
        // 注意：视图级信号接线（wireView）在 openSession/splitTab 已建立，
        // 此处不得重复调用，否则 currentStateChanged 等信号会翻倍发射
        view->enableScrollback(profile.id.toString(QUuid::WithoutBraces));
        view->openEndpoint(endpointFor(profile));
    }
    // 无遗留断线窗格：同步恢复正常颜色（不等 Connected 信号，
    // 与单窗格时代的立即恢复行为一致）
    if (!isTabDisconnected(index)) {
        tabBar()->setTabTextColor(index, palette().color(QPalette::WindowText));
        tabBar()->setTabToolTip(index, QString());
    }
}

ZzTerminalView *ZzTabManager::viewAt(int index) const
{
    ZzSplitContainer *container = containerAt(index);
    return container ? container->focusedView() : nullptr;
}

ZzSplitContainer *ZzTabManager::containerAt(int index) const
{
    return qobject_cast<ZzSplitContainer *>(widget(index));
}

QList<ZzTerminalView *> ZzTabManager::viewsAt(int index) const
{
    ZzSplitContainer *container = containerAt(index);
    return container ? container->views() : QList<ZzTerminalView *>();
}

int ZzTabManager::paneCountAt(int index) const
{
    ZzSplitContainer *container = containerAt(index);
    return container ? container->paneCount() : 0;
}

int ZzTabManager::tabIndexOfView(ZzTerminalView *view) const
{
    for (int i = 0; i < count(); ++i) {
        ZzSplitContainer *container = containerAt(i);
        if (container && container->containsView(view)) {
            return i;
        }
    }
    return -1;
}

void ZzTabManager::wireContainer(ZzSplitContainer *container)
{
    // 窗格关闭：清理按视图键控的表并关闭其传输（视图本体由容器销毁）
    connect(container, &ZzSplitContainer::paneClosing, this,
            [this](ZzTerminalView *view) {
                if (view->transport()) {
                    view->transport()->close();
                }
                m_tabProfiles.remove(view);
                m_tabTunnelCounts.remove(view);
            });
    // 最后窗格已关：收掉整个标签
    connect(container, &ZzSplitContainer::emptied, this,
            [this, container]() {
                const int index = indexOf(container);
                if (index >= 0) {
                    removeTab(index);
                    container->deleteLater();
                }
            });
    // 焦点窗格切换：状态栏跟随刷新（仅当前标签）
    connect(container, &ZzSplitContainer::focusedViewChanged, this,
            [this, container](ZzTerminalView *view) {
                if (!view || indexOf(container) != currentIndex()) {
                    return;
                }
                emit currentViewChanged(view); // SFTP 面板跟随
                emit currentStateChanged(view->transportState());
                emit currentEncodingChanged(view->encoding());
                emit currentSizeChanged(
                    view->termWidget()->screenColumnsCount(),
                    view->termWidget()->screenLinesCount());
                emit currentTunnelCountChanged(m_tabTunnelCounts.value(view, 0));
            });
}

bool ZzTabManager::isTabDisconnected(int index) const
{
    const ZzSplitContainer *container = containerAt(index);
    if (!container) {
        return false;
    }
    // 任一窗格断线即视为标签断线（多窗格下非焦点断线窗格也应可达重连）
    for (ZzTerminalView *view : container->views()) {
        if (m_tabProfiles.contains(view)
            && view->transportState()
                == ZzTransportInterface::State::Disconnected) {
            return true;
        }
    }
    return false;
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
    QAction *splitHorizontalAction =
        menu.addAction(QStringLiteral("左右分屏\tCtrl+Shift+E"));
    QAction *splitVerticalAction =
        menu.addAction(QStringLiteral("上下分屏\tCtrl+Shift+O"));
    QAction *closePaneAction =
        menu.addAction(QStringLiteral("关闭窗格\tCtrl+Shift+W"));
    menu.addSeparator();
    QAction *reconnectAction =
        menu.addAction(QStringLiteral("重新连接"));
    reconnectAction->setEnabled(isTabDisconnected(index));
    QAction *closeAction = menu.addAction(QStringLiteral("关闭标签"));
    QAction *chosen = menu.exec(tabBar()->mapToGlobal(pos));
    if (chosen == splitHorizontalAction) {
        splitTab(index, Qt::Horizontal);
    } else if (chosen == splitVerticalAction) {
        splitTab(index, Qt::Vertical);
    } else if (chosen == closePaneAction) {
        // 右键标签即成为操作目标：先切到该标签再关焦点窗格
        setCurrentIndex(index);
        closeCurrentPane();
    } else if (chosen == reconnectAction) {
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
    // 端口转发与 X11 转发仅 SSH 会话有效；local 会话保持空列表/关闭
    // （契约：localShell 时 portForwards 为空、x11Forwarding 忽略）
    if (!endpoint.localShell) {
        endpoint.portForwards = profile.portForwards;
        endpoint.x11Forwarding = profile.x11Forwarding;
    }
    // 初始行列以视图当前尺寸为准，open 后由 termSizeChange 信号持续同步
    auto *view = qobject_cast<ZzSplitContainer *>(currentWidget());
    ZzTerminalView *focused = view ? view->focusedView() : nullptr;
    endpoint.cols = focused ? focused->termWidget()->screenColumnsCount() : 80;
    endpoint.rows = focused ? focused->termWidget()->screenLinesCount() : 24;
    return endpoint;
}

void ZzTabManager::wireView(ZzTerminalView *view)
{
    connect(view, &ZzTerminalView::disconnected, this,
            [this, view](const QString &reason) {
                const int i = tabIndexOfView(view);
                if (i >= 0) {
                    markTabDisconnected(i, reason);
                }
            });
    connect(view, &ZzTerminalView::stateChanged, this,
            [this, view](ZzTransportInterface::State state) {
                const int i = tabIndexOfView(view);
                if (i < 0) {
                    return;
                }
                // 重新连通且无其他断线窗格：恢复正常颜色
                if (state == ZzTransportInterface::State::Connected
                    && !isTabDisconnected(i)) {
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
                if (tabIndexOfView(view) == currentIndex()) {
                    emit currentSizeChanged(cols, rows);
                }
            });
    connect(view, &ZzTerminalView::errorOccurred, this,
            [this](const QString &message) { emit statusMessage(message); });
    connect(view, &ZzTerminalView::tunnelCountChanged, this,
            [this, view](int count) {
                m_tabTunnelCounts.insert(view, count);
                if (tabIndexOfView(view) == currentIndex()) {
                    emit currentTunnelCountChanged(count);
                }
            });
    connect(view, &ZzTerminalView::statusNotice, this,
            [this](const QString &message) { emit statusMessage(message); });
}

void ZzTabManager::markTabDisconnected(int index, const QString &reason)
{
    tabBar()->setTabTextColor(index, QColor(Qt::gray)); // 断线变灰保留
    tabBar()->setTabToolTip(index, reason);
    emit statusMessage(QStringLiteral("%1 已断开：%2")
                           .arg(tabText(index), reason));
}
