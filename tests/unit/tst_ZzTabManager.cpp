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

    void splitOpensNewPaneWithSameProfile()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("分屏机")));
        QCOMPARE(tabs.count(), 1);
        QCOMPARE(tabs.paneCountAt(0), 1);
        auto *first = tabs.viewAt(0);
        QTRY_COMPARE(first->transportState(), ZzTransportInterface::State::Connected);

        tabs.splitCurrentTab(Qt::Horizontal);
        QCOMPARE(tabs.paneCountAt(0), 2);
        // 新窗格成为焦点窗格，viewAt 随之切换
        auto *second = tabs.viewAt(0);
        QVERIFY(second != nullptr);
        QVERIFY(second != first);
        // 同一 profile 开新会话：独立的全新传输实例
        QVERIFY(second->transport() != first->transport());
        QTRY_COMPARE(second->transportState(), ZzTransportInterface::State::Connected);
        QCOMPARE(tabs.tabText(0), QStringLiteral("分屏机"));
    }

    void closePaneKeepsTabUntilLastPane()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("关窗格")));
        auto *first = tabs.viewAt(0);
        tabs.splitCurrentTab(Qt::Vertical);
        QCOMPARE(tabs.paneCountAt(0), 2);

        tabs.closeCurrentPane(); // 关掉焦点窗格（新分出的）
        QCOMPARE(tabs.count(), 1);       // 标签保留
        QCOMPARE(tabs.paneCountAt(0), 1);
        QCOMPARE(tabs.viewAt(0), first); // 焦点回落到剩余窗格

        tabs.closeCurrentPane(); // 最后窗格关闭 → 整标签关闭
        QCOMPARE(tabs.count(), 0);
    }

    void splitShortcutsDriveSplitCloseAndFocus()
    {
        ZzTabManager tabs;
        tabs.resize(800, 400);
        tabs.openSession(makeProfile(QStringLiteral("快捷键")));
        auto *first = tabs.viewAt(0);
        tabs.show();
        QVERIFY(QTest::qWaitForWindowExposed(&tabs));
        first->setFocus();
        QTRY_VERIFY(QApplication::focusWidget() != nullptr);
        QTest::qWait(50);

        // Ctrl+Shift+E：左右分屏
        QTest::keyClick(QApplication::focusWidget(), Qt::Key_E,
                        Qt::ControlModifier | Qt::ShiftModifier);
        QTRY_COMPARE(tabs.paneCountAt(0), 2);

        // Ctrl+Shift+Left：焦点回到左窗格
        QTest::keyClick(QApplication::focusWidget(), Qt::Key_Left,
                        Qt::ControlModifier | Qt::ShiftModifier);
        QTRY_COMPARE(tabs.viewAt(0), first);

        // Ctrl+Shift+W：关闭焦点窗格，标签保留
        QTest::keyClick(QApplication::focusWidget(), Qt::Key_W,
                        Qt::ControlModifier | Qt::ShiftModifier);
        QTRY_COMPARE(tabs.paneCountAt(0), 1);
        QCOMPARE(tabs.count(), 1);
    }

    void nonFocusedPaneDisconnectGreysTab()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("非焦点断线")));
        tabs.splitCurrentTab(Qt::Horizontal);
        const auto panes = tabs.viewsAt(0);
        QCOMPARE(panes.size(), 2);
        // 焦点在新窗格 panes[1]；等两窗格都连通后断开非焦点窗格 panes[0]
        for (ZzTerminalView *pane : panes) {
            QTRY_COMPARE(pane->transportState(),
                         ZzTransportInterface::State::Connected);
        }
        auto *mock = static_cast<ZzMockTransport *>(panes.at(0)->transport());
        mock->simulateDisconnect(QStringLiteral("网络中断"));

        QCOMPARE(tabs.count(), 1); // 不自动关标签
        QCOMPARE(tabs.tabBar()->tabTextColor(0), QColor(Qt::gray));
        QVERIFY(tabs.isTabDisconnected(0)); // 非焦点窗格断线也判定为断线标签
    }

    void reconnectTabCoversAllDisconnectedPanes()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("多窗格重连")));
        tabs.splitCurrentTab(Qt::Horizontal);
        tabs.splitCurrentTab(Qt::Vertical);
        QCOMPARE(tabs.paneCountAt(0), 3);
        const auto panes = tabs.viewsAt(0);
        for (ZzTerminalView *pane : panes) {
            QTRY_COMPARE(pane->transportState(),
                         ZzTransportInterface::State::Connected);
        }

        // 断开两个窗格（含非焦点窗格），第三个保持连通
        auto *mockA = static_cast<ZzMockTransport *>(panes.at(0)->transport());
        auto *mockB = static_cast<ZzMockTransport *>(panes.at(1)->transport());
        auto *keepC = panes.at(2)->transport();
        mockA->simulateDisconnect(QStringLiteral("掉线A"));
        mockB->simulateDisconnect(QStringLiteral("掉线B"));
        QCOMPARE(tabs.tabBar()->tabTextColor(0), QColor(Qt::gray));
        QVERIFY(tabs.isTabDisconnected(0));

        tabs.reconnectTab(0);
        // 两个断线窗格都拿到全新传输实例并连通，连通窗格不动
        QVERIFY(panes.at(0)->transport() != mockA);
        QVERIFY(panes.at(1)->transport() != mockB);
        QCOMPARE(panes.at(2)->transport(), keepC);
        for (ZzTerminalView *pane : panes) {
            QTRY_COMPARE(pane->transportState(),
                         ZzTransportInterface::State::Connected);
        }
        // 全部窗格重连后恢复非灰色
        QVERIFY(tabs.tabBar()->tabTextColor(0) != QColor(Qt::gray));
        QVERIFY(!tabs.isTabDisconnected(0));
    }
};

QTEST_MAIN(tst_ZzTabManager)
#include "tst_ZzTabManager.moc"
