#include "ZzMenuBarService.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QDebug>
#include <QtCore/QHash>
#include <QtCore/QUrl>
#include <QtGui/QAction>
#include <QtGui/QActionGroup>
#include <QtGui/QDesktopServices>
#include <QtGui/QKeySequence>
#include <QtWidgets/QMenu>
#include <QtWidgets/QMenuBar>

#include <ZzFluentUI/ZzThemeController.h>
#include <ZzPureTools/ZzPureApplication.h>

#include <qtermwidget.h>

#include "ZzAppShell.h"
#include "dialog/ZzAboutDialog.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzLanguageManager.h"

namespace {
// 精选终端配色（不存在于 availableColorSchemes() 的条目跳过）
const char *kCuratedSchemes[] = {
    "Linux", "Solarized", "SolarizedLight", "Dracula", "nord",
    "tokyonight", "catppuccin-mocha", "catppuccin-latte",
    "catppuccin-frappe", "catppuccin-macchiato", "Tango", "Ubuntu",
};
} // namespace

ZzMenuBarService::ZzMenuBarService(QMenuBar *menuBar,
                                   ZzAppSettings *settings,
                                   ZzLanguageManager *languages,
                                   QObject *shellActions,
                                   QObject *parent)
    : QObject(parent)
    , m_menuBar(menuBar)
    , m_settings(settings)
    , m_langs(languages)
    , m_shell(shellActions)
{
    buildMenus();
    // 设置变更（如设置页改配色）时刷新终端主题勾选态
    connect(m_settings, &ZzAppSettings::settingsChanged,
            this, &ZzMenuBarService::syncCheckedStates);
    // 主题变更（含主题按钮切主题）时刷新主题勾选态
    auto *app = qobject_cast<ZzPureTools::ZzPureApplication *>(
        QCoreApplication::instance());
    if (app != nullptr && app->themeController() != nullptr) {
        connect(app->themeController(),
                &ZzFluentUI::ZzThemeController::snapshotChanged,
                this, &ZzMenuBarService::syncCheckedStates);
    }
    retranslate();
    syncCheckedStates();
}

QStringList ZzMenuBarService::curatedColorSchemes()
{
    const QStringList available = QTermWidget::availableColorSchemes();
    QStringList result;
    for (const char *name : kCuratedSchemes) {
        const QString qname = QString::fromLatin1(name);
        if (available.contains(qname)) {
            result.append(qname);
        } else {
            qWarning("ZzMenuBarService: 精选配色 %s 不在可用清单中，跳过", name);
        }
    }
    return result;
}

void ZzMenuBarService::buildMenus()
{
    // 编译期引用 ZzAppShell 类型（生产槽宿主）；空指针或类型不符时跳过接线
    auto *shell = qobject_cast<ZzAppShell *>(m_shell);

    // ---- 会话 ----
    m_sessionMenu = m_menuBar->addMenu(QString());
    QAction *newSession = m_sessionMenu->addAction(QString());
    newSession->setObjectName(QStringLiteral("menuSessionNew"));
    newSession->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+N")));
    if (shell != nullptr) {
        connect(newSession, &QAction::triggered,
                shell, &ZzAppShell::requestNewSession);
    }

    // ---- 视图 ----
    m_viewMenu = m_menuBar->addMenu(QString());

    m_themeSub = m_viewMenu->addMenu(QString());
    m_themeGroup = new QActionGroup(m_themeSub);
    m_themeGroup->setExclusive(true);
    const struct { QString key; ZzFluentUI::ZzThemeMode mode; } themeItems[] = {
        { QStringLiteral("system"), ZzFluentUI::ZzThemeMode::System },
        { QStringLiteral("light"),  ZzFluentUI::ZzThemeMode::Light },
        { QStringLiteral("dark"),   ZzFluentUI::ZzThemeMode::Dark },
        { QStringLiteral("hc"),     ZzFluentUI::ZzThemeMode::HighContrast },
    };
    for (const auto &item : themeItems) {
        QAction *action = m_themeSub->addAction(QString());
        action->setCheckable(true);
        action->setData(item.key);
        m_themeGroup->addAction(action);
        connect(action, &QAction::triggered, this, [item]() {
            auto *app = qobject_cast<ZzPureTools::ZzPureApplication *>(
                QCoreApplication::instance());
            if (app != nullptr && app->themeController() != nullptr) {
                app->themeController()->setMode(item.mode);
            }
        });
    }

    m_terminalThemeSub = m_viewMenu->addMenu(QString());
    m_terminalThemeGroup = new QActionGroup(m_terminalThemeSub);
    m_terminalThemeGroup->setExclusive(true);
    for (const QString &scheme : curatedColorSchemes()) {
        QAction *action = m_terminalThemeSub->addAction(scheme);
        action->setCheckable(true);
        action->setData(scheme);
        m_terminalThemeGroup->addAction(action);
        connect(action, &QAction::triggered, this, [this, scheme]() {
            m_settings->setColorScheme(scheme);
        });
    }
    m_terminalThemeSub->addSeparator();
    m_moreSchemesAction = m_terminalThemeSub->addAction(QString());
    if (shell != nullptr) {
        connect(m_moreSchemesAction, &QAction::triggered,
                shell, &ZzAppShell::openSettingsPage);
    }

    m_languageSub = m_viewMenu->addMenu(QString());
    m_languageGroup = new QActionGroup(m_languageSub);
    m_languageGroup->setExclusive(true);
    const QStringList langOptions = {
        ZzLanguageManager::kSystem, ZzLanguageManager::kZhCn, ZzLanguageManager::kEn };
    for (const QString &opt : langOptions) {
        QAction *action = m_languageSub->addAction(QString());
        action->setCheckable(true);
        action->setData(opt);
        m_languageGroup->addAction(action);
        connect(action, &QAction::triggered, this, [this, opt]() {
            m_langs->apply(opt);
        });
    }

    // ---- 帮助 ----
    m_helpMenu = m_menuBar->addMenu(QString());
    QAction *aboutAction = m_helpMenu->addAction(QString());
    aboutAction->setObjectName(QStringLiteral("menuHelpAbout"));
    connect(aboutAction, &QAction::triggered, this, [this]() {
        ZzAboutDialog dialog(m_menuBar);
        dialog.exec();
    });
    QAction *logDirAction = m_helpMenu->addAction(QString());
    logDirAction->setObjectName(QStringLiteral("menuHelpLogDir"));
    if (shell != nullptr) {
        connect(logDirAction, &QAction::triggered,
                shell, &ZzAppShell::openLogDirectory);
    }
    QAction *repoAction = m_helpMenu->addAction(QString());
    repoAction->setObjectName(QStringLiteral("menuHelpRepo"));
    connect(repoAction, &QAction::triggered, this, []() {
        QDesktopServices::openUrl(
            QUrl(QStringLiteral("https://github.com/jackfahdin/ZzClawTerm")));
    });
}

void ZzMenuBarService::retranslate()
{
    m_sessionMenu->setTitle(tr("会话(&S)"));
    m_sessionMenu->findChild<QAction *>(
        QStringLiteral("menuSessionNew"))->setText(tr("新建会话(&N)"));

    m_viewMenu->setTitle(tr("视图(&V)"));
    m_themeSub->setTitle(tr("主题"));
    const QHash<QString, QString> themeTexts = {
        { QStringLiteral("system"), tr("跟随系统") },
        { QStringLiteral("light"),  tr("浅色") },
        { QStringLiteral("dark"),   tr("深色") },
        { QStringLiteral("hc"),     tr("高对比") },
    };
    for (QAction *action : m_themeGroup->actions()) {
        action->setText(themeTexts.value(action->data().toString()));
    }
    m_terminalThemeSub->setTitle(tr("终端主题"));
    m_moreSchemesAction->setText(tr("更多方案…"));
    m_languageSub->setTitle(tr("语言"));
    const QHash<QString, QString> langTexts = {
        { ZzLanguageManager::kSystem, tr("跟随系统") },
        { ZzLanguageManager::kZhCn,   tr("简体中文") },
        { ZzLanguageManager::kEn,     tr("English") },
    };
    for (QAction *action : m_languageGroup->actions()) {
        action->setText(langTexts.value(action->data().toString()));
    }

    m_helpMenu->setTitle(tr("帮助(&H)"));
    m_helpMenu->findChild<QAction *>(
        QStringLiteral("menuHelpAbout"))->setText(tr("关于 ZzClawTerm(&A)"));
    m_helpMenu->findChild<QAction *>(
        QStringLiteral("menuHelpLogDir"))->setText(tr("打开日志目录"));
    m_helpMenu->findChild<QAction *>(
        QStringLiteral("menuHelpRepo"))->setText(tr("GitHub 仓库"));
}

void ZzMenuBarService::syncCheckedStates()
{
    // 主题勾选 ← themeController
    auto *app = qobject_cast<ZzPureTools::ZzPureApplication *>(
        QCoreApplication::instance());
    if (app != nullptr && app->themeController() != nullptr) {
        const ZzFluentUI::ZzThemeMode mode = app->themeController()->mode();
        const QHash<ZzFluentUI::ZzThemeMode, QString> keys = {
            { ZzFluentUI::ZzThemeMode::System, QStringLiteral("system") },
            { ZzFluentUI::ZzThemeMode::Light, QStringLiteral("light") },
            { ZzFluentUI::ZzThemeMode::Dark, QStringLiteral("dark") },
            { ZzFluentUI::ZzThemeMode::HighContrast, QStringLiteral("hc") },
        };
        const QString key = keys.value(mode);
        for (QAction *action : m_themeGroup->actions()) {
            action->setChecked(action->data().toString() == key);
        }
    }
    // 终端主题勾选 ← 设置
    const QString scheme = m_settings->colorScheme();
    for (QAction *action : m_terminalThemeGroup->actions()) {
        action->setChecked(action->data().toString() == scheme);
    }
    // 语言勾选 ← 语言管理器
    const QString lang = m_langs->option();
    for (QAction *action : m_languageGroup->actions()) {
        action->setChecked(action->data().toString() == lang);
    }
}

bool ZzMenuBarService::event(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslate();
        syncCheckedStates();
    }
    return QObject::event(event);
}

// ---- 测试观察口 ----

QMenu *ZzMenuBarService::sessionMenu() const { return m_sessionMenu; }
QMenu *ZzMenuBarService::viewMenu() const { return m_viewMenu; }
QMenu *ZzMenuBarService::helpMenu() const { return m_helpMenu; }
QMenu *ZzMenuBarService::themeSubMenu() const { return m_themeSub; }
QMenu *ZzMenuBarService::terminalThemeSubMenu() const { return m_terminalThemeSub; }
QMenu *ZzMenuBarService::languageSubMenu() const { return m_languageSub; }
QActionGroup *ZzMenuBarService::themeGroup() const { return m_themeGroup; }
QActionGroup *ZzMenuBarService::terminalThemeGroup() const { return m_terminalThemeGroup; }
QActionGroup *ZzMenuBarService::languageGroup() const { return m_languageGroup; }
