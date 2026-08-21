#include <QtTest>
#include <QDir>
#include <QLabel>
#include <QMainWindow>

#include "ZzAppShell.h"
#include "ZzMockTransport.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 状态栏隧道指示链路测试：mock 注入 → 视图透传 → 标签管理器 → 状态栏。
 */
class tst_ZzTunnelIndicator : public QObject
{
    Q_OBJECT

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

    /** @brief mock 注入隧道计数 → 当前标签信号 → 状态栏第四要素文案。 */
    void tunnelCountReachesStatusBar()
    {
        // 组合根装配（临时配置目录，避免读写真实用户配置）
        const QString dir = QDir(QDir::tempPath()).filePath(
            QStringLiteral("zzclawterm-tunnel-ind-%1").arg(QCoreApplication::applicationPid()));
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        ZzAppShell shell(dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QWidget container;
        // 页面实例必须存活到用例结束（析构会连带销毁 View）
        auto page = shell.createTerminalPage(&container);
        QVERIFY(page.hasValue());
        auto *tabManager = shell.tabManager();
        QVERIFY(tabManager);
        QLabel *tunnelLabel = shell.statusTunnelLabel();
        QVERIFY(tunnelLabel);
        QCOMPARE(tunnelLabel->text(), QStringLiteral("隧道: 0"));

        // 开一个 mock 会话并注入隧道计数
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("隧道机");
        profile.protocol = QStringLiteral("mock");
        tabManager->openSession(profile);
        auto *view = tabManager->viewAt(0);
        QVERIFY(view);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        QSignalSpy countSpy(tabManager, &ZzTabManager::currentTunnelCountChanged);
        // 全链为同线程直接连接，注入后同步到位
        mock->simulateTunnelCount(2);
        QCOMPARE(countSpy.count(), 1);
        QCOMPARE(tunnelLabel->text(), QStringLiteral("隧道: 2"));

        mock->simulateTunnelCount(0);
        QCOMPARE(tunnelLabel->text(), QStringLiteral("隧道: 0"));
    }

    /** @brief statusNotice 链路到状态栏瞬时消息（不经错误横幅）。 */
    void statusNoticeBecomesTransientMessage()
    {
        ZzTabManager tabs;
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("提示机");
        profile.protocol = QStringLiteral("mock");
        tabs.openSession(profile);
        auto *view = tabs.viewAt(0);
        QVERIFY(view);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        QSignalSpy msgSpy(&tabs, &ZzTabManager::statusMessage);
        mock->simulateStatusNotice(QStringLiteral("转发规则 本地 127.0.0.1:13306 启动失败：监听端口被占用"));
        QTRY_VERIFY(msgSpy.count() >= 1);
        QCOMPARE(msgSpy.last().at(0).toString(),
                 QStringLiteral("转发规则 本地 127.0.0.1:13306 启动失败：监听端口被占用"));
        // 不触发错误横幅
        QVERIFY(!view->errorBanner()->isVisible());
    }
};

QTEST_MAIN(tst_ZzTunnelIndicator)
#include "tst_ZzTunnelIndicator.moc"
