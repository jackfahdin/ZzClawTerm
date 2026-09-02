#include <memory>

#include <QtTest/QtTest>

#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QStandardPaths>
#include <QtCore/QTimer>
#include <QtWidgets/QApplication>
#include <QtWidgets/QMenu>
#include <QtWidgets/QTreeView>

#include <ZzPureTools/ZzApplicationBuilder.h>
#include <ZzPureTools/ZzApplicationWindow.h>
#include <ZzPureTools/ZzNavigationNode.h>
#include <ZzPureTools/ZzPageLifetimePolicy.h>
#include <ZzPureTools/ZzPageRegistration.h>
#include <ZzPureTools/ZzPureApplication.h>
#include <ZzPureTools/ZzRouteId.h>
#include <ZzPureTools/ZzWorkspaceShell.h>
#include <ZzWindowKit/ZzWindowKitBootstrap.h>

#include "ZzAppShell.h"
#include "ZzClawTermModule.h"
#include "panel/ZzSessionPanel.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionModel.h"
#include "session/ZzSessionProfile.h"
#include "settings/ZzAppSettings.h"

/**
 * @brief 会话树右键菜单真窗口冒烟：完整装配链（同 main.cpp）下验证菜单可弹出。
 *
 * QTest::mouseClick 直投控件、绕过 QWidgetWindow 的 ContextMenu 合成
 * （右键菜单在平台层合成 ContextMenu 事件），因此用编程方式发射
 * customContextMenuRequested 信号驱动 ZzSessionPanel::showContextMenu，
 * 断言菜单成为 activePopupWidget 且可见，并核对动作集。装配必须走
 * ZzApplicationBuilder + ZzApplicationWindow 真实分支（离屏 QMainWindow
 * 分支无导航迁移，行为不等价）。
 */
class tst_ZzShellContextMenu : public QObject
{
    Q_OBJECT

private:
    // main() 注入的装配上下文（窗口由 ZzPureApplication 独占，早于 shell 析构）
    static ZzAppShell *s_shell;
    static ZzPureTools::ZzWorkspaceShell *s_workspace;

    static ZzCore::ZzResult<void> assembleWindow(
        ZzPureTools::ZzApplicationWindow &window)
    {
        auto result = s_shell->assemble(window);
        if (!result) {
            return result;
        }
        s_workspace = s_shell->workspaceShell();
        return ZzCore::ZzResult<void>::success();
    }

    /**
     * @brief 发射树右键信号，在 menu.exec 模态循环内抓取弹出菜单的动作集并关闭。
     * @return 捕获到的菜单动作文本；菜单未弹出时返回空列表。
     */
    static QStringList triggerContextMenuAndCapture(QTreeView *tree,
                                                    const QPoint &pos,
                                                    bool &visibleAtCapture)
    {
        QStringList titles;
        visibleAtCapture = false;
        QTimer closer;
        closer.setInterval(50);
        QObject::connect(&closer, &QTimer::timeout, qApp, [&titles, &visibleAtCapture] {
            QWidget *popup = QApplication::activePopupWidget();
            auto *menu = qobject_cast<QMenu *>(popup);
            // 菜单是 showContextMenu 的栈对象，exec 返回即销毁，必须在循环内取完数据
            if (menu != nullptr && titles.isEmpty()) {
                visibleAtCapture = menu->isVisible();
                const auto actions = menu->actions();
                for (QAction *action : actions) {
                    titles << action->text();
                }
                menu->close();
            }
        });
        closer.start();
        if (!QMetaObject::invokeMethod(
                tree, "customContextMenuRequested", Q_ARG(QPoint, pos))) {
            QTest::qFail("invoke customContextMenuRequested failed",
                         __FILE__, __LINE__);
            return {};
        }
        QTest::qWait(800); // 等 showContextMenu → exec 的模态循环跑起并被定时器关闭
        closer.stop();
        return titles;
    }

private slots:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
        // 与 tst_ZzAppShell 同款隔离：不拉起真实 X server
        ZzAppSettings::instance().setX11ServerEnabled(false);
    }

    void cleanupTestCase()
    {
        delete s_shell;
        s_shell = nullptr;
        s_workspace = nullptr;
    }

    void sessionTreeContextMenuPopsUpInRealWindow()
    {
        // bootstrap 已在 main()（QApplication 创建前）完成，此处直接装配
        const QString dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-shell-cm-test"));
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        s_shell = new ZzAppShell(dir);

        // —— 装配链路同 main.cpp ——
        ZzPureTools::ZzApplicationBuilder builder;
        QVERIFY(builder.addModule(std::make_unique<ZzClawTermModule>()));
        const ZzPureTools::ZzRouteId terminalRoute(
            QStringLiteral("terminal"));
        ZzPureTools::ZzPageRegistration terminalPage;
        terminalPage.routeId = terminalRoute;
        terminalPage.lifetime = ZzPureTools::ZzPageLifetimePolicy::Persistent;
        terminalPage.factory = [](QWidget *pageParent) {
            return s_shell->createTerminalPage(pageParent);
        };
        QVERIFY(builder.addPage(std::move(terminalPage)));
        ZzPureTools::ZzNavigationNode terminalNode{
            terminalRoute, QStringLiteral("ZzClawTerm"),
            QStringLiteral("Terminal"), {}};
        QVERIFY(builder.addNavigationNode(std::move(terminalNode)));
        QVERIFY(builder.setInitialRoute(terminalRoute));
        QVERIFY(builder.setWindowSetupCallback(
            [](ZzPureTools::ZzApplicationWindow &window) {
                return assembleWindow(window);
            }));
        auto *application = qobject_cast<ZzPureTools::ZzPureApplication *>(
            QCoreApplication::instance());
        QVERIFY(application != nullptr);
        const auto buildResult = builder.build(*application);
        QVERIFY2(buildResult,
                 qPrintable(QStringLiteral("build 失败：%1 | %2")
                     .arg(buildResult ? QString() : buildResult.error().technicalMessage(),
                          buildResult ? QString() : buildResult.error().context())));

        // 真实框架窗口（非离屏 QMainWindow 分支）
        QWidget *window = s_workspace->workspaceWidget()->window();
        QVERIFY(window != nullptr);
        window->show();
        QVERIFY(QTest::qWaitForWindowExposed(window));

        // 延迟工厂首开（等价点击活动栏「会话」）
        QVERIFY(s_workspace->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sessions"))));
        QVERIFY(s_shell->sessionPanel() != nullptr);
        QTRY_VERIFY(s_shell->sessionPanel()->isVisible());

        auto *tree = s_shell->sessionPanel()->findChild<QTreeView *>();
        QVERIFY(tree != nullptr);
        QTRY_VERIFY(tree->viewport()->width() > 20);
        QCOMPARE(tree->contextMenuPolicy(), Qt::CustomContextMenu);

        // 空白区右键：「新建会话」
        bool visibleAtCapture = false;
        const QStringList blankTitles =
            triggerContextMenuAndCapture(tree, QPoint(5, 5), visibleAtCapture);
        QVERIFY2(!blankTitles.isEmpty(),
                 "context menu did not become the active popup");
        QVERIFY(visibleAtCapture);
        QCOMPARE(blankTitles, QStringList{ZzSessionPanel::tr("新建会话")});

        // 会话项右键：新建/编辑/删除/复制
        ZzSessionProfile profile;
        profile.name = QStringLiteral("冒烟机");
        profile.protocol = QStringLiteral("ssh");
        profile.host = QStringLiteral("example.test");
        profile.userName = QStringLiteral("root");
        QVERIFY(!s_shell->sessionModel()->addSession(profile).isNull());
        const QModelIndex index = tree->indexAt(QPoint(5, 5));
        QVERIFY(index.isValid());
        const QPoint itemPos = tree->visualRect(index).center();
        const QStringList itemTitles =
            triggerContextMenuAndCapture(tree, itemPos, visibleAtCapture);
        QVERIFY(!itemTitles.isEmpty());
        QVERIFY(visibleAtCapture);
        QCOMPARE(itemTitles,
                 (QStringList{ZzSessionPanel::tr("新建会话"),
                              ZzSessionPanel::tr("编辑"),
                              ZzSessionPanel::tr("删除"),
                              ZzSessionPanel::tr("复制")}));
    }
};

ZzAppShell *tst_ZzShellContextMenu::s_shell = nullptr;
ZzPureTools::ZzWorkspaceShell *tst_ZzShellContextMenu::s_workspace = nullptr;

int main(int argc, char *argv[])
{
    const auto bootstrap = ZzWindowKit::ZzWindowKitBootstrap::prepare();
    if (!bootstrap) {
        return EXIT_FAILURE;
    }
    ZzPureTools::ZzPureApplication application(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("ZzClawTerm"));
    QCoreApplication::setOrganizationName(QStringLiteral("ZzClaw"));
    tst_ZzShellContextMenu test;
    return QTest::qExec(&test, argc, argv);
}
#include "tst_ZzShellContextMenu.moc"
