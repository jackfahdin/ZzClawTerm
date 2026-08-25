#include <QtTest/QtTest>

#include <QtWidgets/QDockWidget>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>

#include "ZzAppShell.h"
#include "ZzMockTransport.h"
#include "panel/ZzSessionPanel.h"
#include "session/ZzSessionModel.h"
#include "session/ZzSessionProfile.h"
#include "settings/ZzAppSettings.h"
#include "tab/ZzTabManager.h"
#include "transport/ZzTransportRegistry.h"
#include "x11/ZzX11Service.h"

/**
 * @brief 壳层装配冒烟：普通 QMainWindow 上验证 dock、状态栏、双击到标签的完整链路。
 *
 * ZzAppShell::assemble 只依赖 QMainWindow&，因此无需拉起完整框架即可离屏测试。
 */
class tst_ZzAppShell : public QObject
{
    Q_OBJECT
private:
    QString m_dir;

private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void init()
    {
        m_dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-shell-test"));
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
    }

    void assembleInstallsDockAndStatusBar()
    {
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QVERIFY(shell.sessionPanel() != nullptr);
        // 会话面板已停靠
        QCOMPARE(window.findChild<QDockWidget *>(
                     QStringLiteral("sessions")),
                 static_cast<QDockWidget *>(shell.sessionPanel()));
        // 状态栏三要素
        QVERIFY(shell.statusStateLabel() != nullptr);
        QVERIFY(shell.statusEncodingLabel() != nullptr);
        QVERIFY(shell.statusSizeLabel() != nullptr);
    }

    void doubleClickOpensTabAndUpdatesStatusBar()
    {
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
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
        QTRY_COMPARE(shell.statusStateLabel()->text(), QStringLiteral("已连接"));
        QVERIFY(!shell.statusEncodingLabel()->text().isEmpty());
    }
    void x11ServiceFollowsGlobalSetting()
    {
        // M5 规格 §三决策 1：全局开关驱动共享服务启停
        const bool original = ZzAppSettings::instance().x11ServerEnabled();
        ZzAppShell shell(m_dir); // 照本文件已有用例的构造参数
        QVERIFY(shell.x11Service());
        QCOMPARE(shell.x11Service()->isEnabled(), original);

        ZzAppSettings::instance().setX11ServerEnabled(false);
        QTRY_VERIFY(!shell.x11Service()->isEnabled());
        ZzAppSettings::instance().setX11ServerEnabled(true);
        QTRY_VERIFY(shell.x11Service()->isEnabled());

        ZzAppSettings::instance().setX11ServerEnabled(original); // 还原，免污染其他用例
    }
};

QTEST_MAIN(tst_ZzAppShell)
#include "tst_ZzAppShell.moc"
