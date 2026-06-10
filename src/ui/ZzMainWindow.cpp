/**
 * @file ZzMainWindow.cpp
 * @brief ZzMainWindow 公开接口与界面装配实现。
 */

#include "ui/ZzMainWindow.h"

#include <QAction>
#include <QKeySequence>
#include <QLabel>
#include <QMenuBar>
#include <QSplitter>
#include <QStatusBar>
#include <QTabWidget>
#include <QToolBar>
#include <QVBoxLayout>

#include <cstdint>

#include "business/ZzThemeManager.h"
#include "core/log/ZzLogEngine.h"
#include "session/ZzLocalSession.h"
#include "ui/ZzMainWindowPrivate.h"
#include "ui/ZzSearchBar.h"
#include "ui/ZzTerminalWidget.h"

namespace ZzUi {

using ZzSession::ZzLocalSession;

namespace {
/** @brief 从标签页内容控件中取出当前终端 (兼容分屏)。 */
ZzTerminalWidget* terminalIn(QWidget* container)
{
    if (auto* term = qobject_cast<ZzTerminalWidget*>(container)) {
        return term;
    }
    if (container) {
        return container->findChild<ZzTerminalWidget*>();
    }
    return nullptr;
}
}  // namespace

ZzMainWindow::ZzMainWindow(QWidget* parent)
    : QMainWindow(parent), d_ptr(std::make_unique<ZzMainWindowPrivate>())
{
    d_ptr->config = std::make_unique<ZzBusiness::ZzConfigManager>();
    d_ptr->themes = new ZzBusiness::ZzThemeManager(this);
    d_ptr->themes->setScheme(d_ptr->config->themeName());

    // ── 中央区域: 标签页 + 搜索栏 ──
    auto* central = new QWidget(this);
    auto* layout = new QVBoxLayout(central);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    d_ptr->tabs = new QTabWidget(central);
    d_ptr->tabs->setTabsClosable(true);
    d_ptr->tabs->setDocumentMode(true);
    d_ptr->searchBar = new ZzSearchBar(central);

    layout->addWidget(d_ptr->tabs, 1);
    layout->addWidget(d_ptr->searchBar);
    setCentralWidget(central);

    // ── 菜单与工具栏 ──
    auto* fileMenu = menuBar()->addMenu(tr("文件(&F)"));
    auto* newTabAction = fileMenu->addAction(tr("新建标签页"));
    newTabAction->setShortcut(QKeySequence(QStringLiteral("Ctrl+T")));
    connect(newTabAction, &QAction::triggered, this, &ZzMainWindow::onNewTab);

    auto* viewMenu = menuBar()->addMenu(tr("视图(&V)"));
    auto* splitAction = viewMenu->addAction(tr("水平分屏"));
    splitAction->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+D")));
    connect(splitAction, &QAction::triggered, this, &ZzMainWindow::onSplitHorizontally);

    auto* searchAction = viewMenu->addAction(tr("搜索"));
    searchAction->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+F")));
    connect(searchAction, &QAction::triggered, this, &ZzMainWindow::onToggleSearch);

    auto* themeMenu = menuBar()->addMenu(tr("主题(&T)"));
    for (const QString& name : d_ptr->themes->availableSchemes()) {
        auto* act = themeMenu->addAction(name);
        connect(act, &QAction::triggered, this, [this, name]() {
            d_ptr->themes->setScheme(name);
            d_ptr->config->setThemeName(name);
        });
    }

    // ── 状态栏 ──
    d_ptr->statusLabel = new QLabel(tr("就绪"), this);
    statusBar()->addPermanentWidget(d_ptr->statusLabel);

    // ── 信号 ──
    connect(d_ptr->tabs, &QTabWidget::tabCloseRequested, this, &ZzMainWindow::onCloseTab);
    connect(d_ptr->searchBar, &ZzSearchBar::searchRequested, this, &ZzMainWindow::onSearch);
    connect(d_ptr->themes, &ZzBusiness::ZzThemeManager::schemeChanged, this,
            [this](const ZzBusiness::ZzColorScheme& scheme) {
                const auto terms = findChildren<ZzTerminalWidget*>();
                for (ZzTerminalWidget* t : terms) {
                    t->setColorScheme(scheme);
                }
            });

    setWindowTitle(QStringLiteral("ZzClawTerm"));
    resize(1000, 640);

    onNewTab();
}

ZzMainWindow::~ZzMainWindow()
{
    d_ptr->config->sync();
}

void ZzMainWindow::onNewTab()
{
    auto* widget = new ZzTerminalWidget;
    widget->setColorScheme(d_ptr->themes->currentScheme());
    widget->setTerminalFont(d_ptr->config->terminalFont());

    auto* session = new ZzLocalSession(widget);
    widget->setTerminalCore(session->terminal());

    connect(widget, &ZzTerminalWidget::keyInput, session, &ZzLocalSession::sendInput);
    connect(widget, &ZzTerminalWidget::viewportResized, session, &ZzLocalSession::resize);
    connect(session->log(), &ZzCore::Log::ZzLogEngine::lineAppended, this,
            [this](std::uint64_t total) {
                d_ptr->statusLabel->setText(tr("历史日志: %1 行").arg(total));
            });
    connect(session, &ZzLocalSession::exited, this, [this, widget](int) {
        const int idx = d_ptr->tabs->indexOf(widget);
        if (idx >= 0) {
            onCloseTab(idx);
        }
    });

    const int index = d_ptr->tabs->addTab(widget, tr("终端 %1").arg(d_ptr->tabs->count() + 1));
    d_ptr->tabs->setCurrentIndex(index);
    widget->setFocus();

    // 以默认尺寸启动; 控件显示后的 resizeEvent 会触发对齐到真实视口。
    session->start(80, 24);
}

void ZzMainWindow::onCloseTab(int index)
{
    QWidget* w = d_ptr->tabs->widget(index);
    d_ptr->tabs->removeTab(index);
    w->deleteLater();
    if (d_ptr->tabs->count() == 0) {
        onNewTab();
    }
}

void ZzMainWindow::onSplitHorizontally()
{
    QWidget* current = d_ptr->tabs->currentWidget();
    if (!current) {
        return;
    }
    const int index = d_ptr->tabs->currentIndex();
    const QString label = d_ptr->tabs->tabText(index);

    auto* splitter = new QSplitter(Qt::Horizontal);

    // 先从标签页摘除原终端, 再移入分屏左侧 (避免父子关系冲突)。
    d_ptr->tabs->removeTab(index);
    splitter->addWidget(current);

    // 新建右侧终端。
    auto* widget = new ZzTerminalWidget;
    widget->setColorScheme(d_ptr->themes->currentScheme());
    widget->setTerminalFont(d_ptr->config->terminalFont());
    auto* session = new ZzLocalSession(widget);
    widget->setTerminalCore(session->terminal());
    connect(widget, &ZzTerminalWidget::keyInput, session, &ZzLocalSession::sendInput);
    connect(widget, &ZzTerminalWidget::viewportResized, session, &ZzLocalSession::resize);
    splitter->addWidget(widget);

    d_ptr->tabs->insertTab(index, splitter, label);
    d_ptr->tabs->setCurrentIndex(index);

    session->start(80, 24);
}

void ZzMainWindow::onToggleSearch()
{
    if (d_ptr->searchBar->isVisible()) {
        d_ptr->searchBar->hide();
    } else {
        d_ptr->searchBar->activate();
    }
}

void ZzMainWindow::onSearch(const QString& pattern)
{
    ZzTerminalWidget* term = terminalIn(d_ptr->tabs->currentWidget());
    if (!term) {
        return;
    }
    auto* session = term->findChild<ZzLocalSession*>();
    if (!session) {
        return;
    }
    const auto results = session->log()->search(pattern);
    d_ptr->searchBar->setResultCount(static_cast<int>(results.size()));
}

}  // namespace ZzUi
