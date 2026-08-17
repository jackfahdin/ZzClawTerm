#include <QtTest/QtTest>
#include <QtWidgets/QTabBar>

#include "ZzMockTransport.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 验证标签管理生命周期：开会话、关闭、断线变灰保留、右键重连（规格 §七/§九）。
 *
 * 注：简报样例按规划稿写成 profile.id=QString/profile.user；计划 03 实际交付的
 * ZzSessionProfile 契约（id 为 QUuid、用户名字段为 userName）为准，此处已适配，
 * 仅字段名/取值方式不同，断言与简报一致。
 */
class tst_ZzTabManager : public QObject
{
    Q_OBJECT
private:
    /** @brief 构造一个 mock 协议会话。 */
    static ZzSessionProfile makeProfile(const QString &name)
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = name;
        profile.groupPath = QStringLiteral("测试分组");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("example.com");
        profile.port = 22;
        profile.userName = QStringLiteral("root");
        return profile;
    }

private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        QVERIFY(ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void openAddsConnectedTab()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("生产A")));
        QCOMPARE(tabs.count(), 1);
        QCOMPARE(tabs.tabText(0), QStringLiteral("生产A"));

        auto *view = tabs.viewAt(0);
        QVERIFY(view != nullptr);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);
        QVERIFY(tabs.tabsClosable());
        QVERIFY(tabs.isMovable()); // 拖拽排序
    }

    void closeTabDestroysView()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("临时")));
        auto *view = tabs.viewAt(0);
        QPointer<ZzTerminalView> guard(view);
        tabs.closeTab(0);
        QCOMPARE(tabs.count(), 0);
        QCoreApplication::processEvents();
        // Qt 6.11 实测：processEvents 不冲刷 DeferredDelete 队列，需显式触发
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QVERIFY(guard.isNull());
    }

    void disconnectGreysTabButKeepsIt()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("断线机")));
        auto *view = tabs.viewAt(0);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        mock->simulateDisconnect(QStringLiteral("网络中断"));

        // 不自动关标签（规格 §七）
        QCOMPARE(tabs.count(), 1);
        // 标签变灰
        QCOMPARE(tabs.tabBar()->tabTextColor(0), QColor(Qt::gray));
        QVERIFY(tabs.isTabDisconnected(0));
    }

    void reconnectCreatesFreshTransport()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("重连机")));
        auto *view = tabs.viewAt(0);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);
        auto *firstMock = static_cast<ZzMockTransport *>(view->transport());
        firstMock->simulateDisconnect(QStringLiteral("掉线"));

        tabs.reconnectTab(0);
        auto *secondMock = static_cast<ZzMockTransport *>(view->transport());
        // ZzSshConnection 不可重复 connectToHost（规格 §十注释约定），重连必须换新实例
        QVERIFY(secondMock != firstMock);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);
        // 恢复非灰色
        QVERIFY(tabs.tabBar()->tabTextColor(0) != QColor(Qt::gray));
        QVERIFY(!tabs.isTabDisconnected(0));
    }

    void unknownProtocolShowsStatusMessage()
    {
        ZzTabManager tabs;
        QSignalSpy msgSpy(&tabs, &ZzTabManager::statusMessage);
        ZzSessionProfile bad = makeProfile(QStringLiteral("坏协议"));
        bad.protocol = QStringLiteral("telnet-1996");
        tabs.openSession(bad);
        QCOMPARE(tabs.count(), 0); // 未建标签
        QCOMPARE(msgSpy.count(), 1);
        QVERIFY(msgSpy.first().at(0).toString().contains(QStringLiteral("telnet-1996")));
    }

    void currentTabSignalsForStatusBar()
    {
        ZzTabManager tabs;
        QSignalSpy stateSpy(&tabs, &ZzTabManager::currentStateChanged);
        tabs.openSession(makeProfile(QStringLiteral("状态栏")));
        QTRY_VERIFY(stateSpy.count() >= 2); // Connecting + Connected
    }
};

QTEST_MAIN(tst_ZzTabManager)
#include "tst_ZzTabManager.moc"
