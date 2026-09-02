#include <QtWidgets/QMenuBar>
#include <QtWidgets/QMenu>
#include <QtGui/QActionGroup>
#include <QtTest/QtTest>

#include "ZzAppShell.h"
#include "menu/ZzMenuBarService.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzLanguageManager.h"

/**
 * @brief 菜单装配：三菜单结构、终端主题/语言动作意图转发；shell 为空容错。
 */
class tst_ZzMenuBarService : public QObject
{
    Q_OBJECT
private:
    bool m_originalX11Enabled = false; ///< initTestCase 保存的全局 X server 开关原值

private slots:
    void initTestCase()
    {
        // 设置写盘隔离到测试目录（须早于 ZzAppSettings::instance() 首次调用）
        QStandardPaths::setTestModeEnabled(true);
        // 构造真实 ZzAppShell 时不拉起共享 X server（避免测试沙盒外副作用）
        m_originalX11Enabled = ZzAppSettings::instance().x11ServerEnabled();
        ZzAppSettings::instance().setX11ServerEnabled(false);
    }

    void cleanupTestCase()
    {
        ZzAppSettings::instance().setX11ServerEnabled(m_originalX11Enabled);
    }

    void menusExistWithExpectedStructure()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        QMenuBar bar;
        ZzMenuBarService service(&bar, &settings, &langs, nullptr);

        QVERIFY(service.sessionMenu() != nullptr);
        QVERIFY(service.viewMenu() != nullptr);
        QVERIFY(service.helpMenu() != nullptr);
        QCOMPARE(bar.actions().size(), 3);
        // 会话：1 项（新建会话）
        QCOMPARE(service.sessionMenu()->actions().size(), 1);
        // 视图：3 个子菜单
        QCOMPARE(service.viewMenu()->actions().size(), 3);
        // 主题 4 项勾选组；语言 3 项勾选组
        QCOMPARE(service.themeGroup()->actions().size(), 4);
        QCOMPARE(service.languageGroup()->actions().size(), 3);
        QVERIFY(service.terminalThemeGroup()->actions().size() >= 10);
        // 帮助：3 项（关于/日志目录/仓库）
        QCOMPARE(service.helpMenu()->actions().size(), 3);
    }

    void terminalThemeActionWritesSettings()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        QMenuBar bar;
        ZzMenuBarService service(&bar, &settings, &langs, nullptr);
        QAction *action = service.terminalThemeGroup()->actions().first();
        const QString scheme = action->data().toString();
        action->trigger();
        QCOMPARE(settings.colorScheme(), scheme);
        QVERIFY(action->isChecked());
    }

    void languageActionAppliesOption()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        QMenuBar bar;
        ZzMenuBarService service(&bar, &settings, &langs, nullptr);
        QAction *en = nullptr;
        for (QAction *a : service.languageGroup()->actions()) {
            if (a->data().toString() == ZzLanguageManager::kEn) en = a;
        }
        QVERIFY(en != nullptr);
        en->trigger();
        QCOMPARE(settings.language(), ZzLanguageManager::kEn);
        QVERIFY(en->isChecked());
    }

    void retranslateFollowsLanguage()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        QMenuBar bar;
        ZzMenuBarService service(&bar, &settings, &langs, nullptr);
        QCOMPARE(service.viewMenu()->title(), QStringLiteral("视图(&V)"));
        langs.apply(ZzLanguageManager::kEn);
        QEvent ev(QEvent::LanguageChange);
        QCoreApplication::sendEvent(&service, &ev);   // 投递 LanguageChange 触发 retranslate
        QCOMPARE(service.viewMenu()->title(), QStringLiteral("View(&V)"));
        langs.apply(ZzLanguageManager::kZhCn);
    }

    void realShellWiresShellActions()
    {
        // 编译期 connect 到真实 ZzAppShell 槽：接线成立、动作结构完整。
        // 「新建会话」无窗口时走 showStatusMessage 兜底（状态栏为空，不崩），
        // 可安全触发；「打开日志目录」会真实弹文件管理器，只断言存在不触发。
        QTemporaryDir dir;
        ZzAppShell shell(dir.path());
        QMenuBar bar;
        ZzMenuBarService service(&bar, &ZzAppSettings::instance(),
                                 shell.languageManager(), &shell);

        QAction *newSession = service.sessionMenu()->findChild<QAction *>(
            QStringLiteral("menuSessionNew"));
        QVERIFY(newSession != nullptr);
        newSession->trigger();   // 无窗口兜底分支：面板未装配 → 状态栏空操作
        QCoreApplication::processEvents();

        QAction *logDir = service.helpMenu()->findChild<QAction *>(
            QStringLiteral("menuHelpLogDir"));
        QVERIFY(logDir != nullptr);   // 有副作用（弹文件管理器），不触发
    }
};

QTEST_MAIN(tst_ZzMenuBarService)
#include "tst_ZzMenuBarService.moc"
