#include <QtTest/QtTest>

#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>

#include <ZzPureTools/ZzWorkspaceShell.h>
#include <ZzPureTools/ZzWorkspaceTitleMode.h>

#include "ZzAppShell.h"
#include "ZzMockTransport.h"
#include "menu/ZzMenuBarService.h"
#include "panel/ZzSessionPanel.h"
#include "session/ZzSessionModel.h"
#include "session/ZzSessionProfile.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzLanguageManager.h"
#include "tab/ZzTabManager.h"
#include "transport/ZzTransportRegistry.h"
#include "x11/ZzX11Service.h"

/**
 * @brief 壳层装配冒烟：普通 QMainWindow 上验证 IDE 工作区、状态栏、双击到标签的完整链路。
 *
 * ZzAppShell::assemble 只依赖 QMainWindow&，因此无需拉起完整框架即可离屏测试；
 * 侧栏面板经延迟工厂首开才创建，用例里用 showPanel 触发实例化（等价点击活动栏入口）。
 */
class tst_ZzAppShell : public QObject
{
    Q_OBJECT
private:
    QString m_dir;
    bool m_originalX11Enabled = false; ///< initTestCase 保存的用户开关原值

    /** @brief 桩 X server 脚本（记录参数后 sleep；照 tst_ZzX11Service 的 makeSleepingStub 模式）。 */
    QString makeSleepingStub(const QString &argsFile)
    {
        const QString path = QDir(m_dir).filePath(QStringLiteral("stub-xserver.sh"));
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly))
            return {};
        f.write("#!/bin/sh\n"
                "printf '%s\\n' \"$@\" > '" + argsFile.toUtf8() + "'\n"
                "sleep 60\n");
        f.close();
        f.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                         QFileDevice::ExeOwner | QFileDevice::ReadUser |
                         QFileDevice::WriteUser | QFileDevice::ExeUser);
        return path;
    }

private slots:
    void initTestCase()
    {
        // 设置写盘隔离到测试目录（必须早于 ZzAppSettings::instance() 首次调用，
        // 否则单例按真实 AppConfigLocation 落盘 settings.ini）
        QStandardPaths::setTestModeEnabled(true);
        qRegisterMetaType<ZzTransportInterface::State>();
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
        // 基线隔离：全局 X server 开关置 false，保证各用例构造 ZzAppShell 时
        // 共享服务不启动——否则 Unix 下 started 处理器会把生成的 cookie 经
        // xauth 写入用户真实 ~/.Xauthority（测试沙盒外副作用）
        m_originalX11Enabled = ZzAppSettings::instance().x11ServerEnabled();
        ZzAppSettings::instance().setX11ServerEnabled(false);
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
        ZzAppSettings::instance().setX11ServerEnabled(m_originalX11Enabled); // 还原用户原值
    }

    void init()
    {
        m_dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-shell-test"));
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
    }

    void assembleCreatesWorkspaceAndStatusBar()
    {
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QVERIFY(shell.workspaceShell() != nullptr);
        // 工作区根控件已挂为中央控件
        QCOMPARE(window.centralWidget(),
                 shell.workspaceShell()->workspaceWidget());
        // 窗口标题接管：应用名 + 当前标签模式（此前显示库默认「ZzPureTools」）
        QCOMPARE(shell.workspaceShell()->applicationTitle(),
                 QStringLiteral("ZzClawTerm"));
        QCOMPARE(shell.workspaceShell()->titleMode(),
                 ZzPureTools::ZzWorkspaceTitleMode::CurrentTabAndApplication);
        // 状态栏四件套
        QVERIFY(shell.statusStateLabel() != nullptr);
        QVERIFY(shell.statusEncodingLabel() != nullptr);
        QVERIFY(shell.statusSizeLabel() != nullptr);
        QVERIFY(shell.statusTunnelLabel() != nullptr);

        // 会话面板为延迟工厂：首开（等价点击活动栏「会话」）才创建
        QVERIFY(shell.sessionPanel() == nullptr);
        auto shown = shell.workspaceShell()->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sessions")));
        QVERIFY(shown);
        QVERIFY(shell.sessionPanel() != nullptr);
        QCOMPARE(shell.sessionPanel()->panelTitle(),
                 ZzSessionPanel::tr("会话"));
        // SFTP 面板同理首开
        QVERIFY(shell.sftpPanel() == nullptr);
        auto filesShown = shell.workspaceShell()->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sftp")));
        QVERIFY(filesShown);
        QVERIFY(shell.sftpPanel() != nullptr);
    }

    void doubleClickOpensTabAndUpdatesStatusBar()
    {
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        auto shown = shell.workspaceShell()->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sessions")));
        QVERIFY(shown);
        QWidget container;
        // 页面实例必须存活到用例结束（析构会连带销毁 View）
        auto page = shell.createTerminalPage(&container);
        QVERIFY(page.hasValue());
        QVERIFY(shell.tabManager() != nullptr);

        // 放一条会话进模型，等价于用户双击
        ZzSessionProfile profile;
        profile.name = QStringLiteral("装配机");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("example.test");
        const QUuid id = shell.sessionModel()->addSession(profile);
        QVERIFY(!id.isNull());
        shell.sessionPanel()->triggerConnect(
            id.toString(QUuid::WithoutBraces));

        QCOMPARE(shell.tabManager()->count(), 1);
        QTRY_COMPARE(shell.statusStateLabel()->text(),
                     QCoreApplication::translate("ZzAppShell", "已连接"));
        QVERIFY(!shell.statusEncodingLabel()->text().isEmpty());
    }
    void x11ServiceFollowsGlobalSetting()
    {
        // M5 规格 §三决策 1：全局开关驱动共享服务启停
        const bool original = ZzAppSettings::instance().x11ServerEnabled();
        ZzAppShell shell(m_dir); // 照本文件已有用例的构造参数
        QVERIFY(shell.x11Service());
        QCOMPARE(shell.x11Service()->isEnabled(), original);

#ifdef Q_OS_UNIX
        // 拨 true 之前注入桩 server：override 路径拉起桩进程并跳过
        // addToSystemAuthority，true 分支不写用户真实 ~/.Xauthority
        const QString stub = makeSleepingStub(
            QDir(m_dir).filePath(QStringLiteral("stub-args.txt")));
        QVERIFY(!stub.isEmpty());
        shell.x11Service()->setServerProgramForTesting(stub);
#endif

        ZzAppSettings::instance().setX11ServerEnabled(false);
        QTRY_VERIFY(!shell.x11Service()->isEnabled());
        ZzAppSettings::instance().setX11ServerEnabled(true);
        QTRY_VERIFY(shell.x11Service()->isEnabled());

        ZzAppSettings::instance().setX11ServerEnabled(original); // 还原，免污染其他用例
    }
    void menuFallbacksOffscreen()
    {
        // 未装配（无工作区/窗口/状态栏）：槽位走兜底分支不崩。
        // 注意不能在已装配 shell 上直接调 requestNewSession：面板会经延迟工厂
        // 物化并弹出模态 ZzSessionConfigWindow::exec()，离屏测试将永久阻塞
        ZzAppShell bare(m_dir);
        QVERIFY(bare.languageManager() != nullptr);
        QVERIFY(bare.menuBarService() == nullptr);
        bare.requestNewSession();   // 工作区为空 → 状态栏兜底提示（无状态栏则静默）
        bare.openSettingsPage();    // 无窗口 → 静默返回

        // 离屏普通 QMainWindow 无 Fluent 标题栏：菜单服务不装配
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QVERIFY(shell.menuBarService() == nullptr);
        QVERIFY(shell.languageManager() != nullptr);
        shell.openSettingsPage();   // 普通窗口无导航控制器 → 静默返回
    }
};

QTEST_MAIN(tst_ZzAppShell)
#include "tst_ZzAppShell.moc"
