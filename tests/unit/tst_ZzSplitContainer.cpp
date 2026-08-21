#include <QtTest/QtTest>
#include <QtWidgets/QSplitter>

#include "tab/ZzSplitContainer.h"
#include "terminal/ZzTerminalView.h"

/**
 * @brief 验证分屏容器：分屏结构、关闭收拢、焦点跟踪与方向导航（规格 §七分屏）。
 *
 * 窗格不绑传输（ZzTerminalView 无传输亦可构造显示）；
 * 焦点相关用例需容器可见，offscreen 平台同样支持窗内焦点。
 */
class tst_ZzSplitContainer : public QObject
{
    Q_OBJECT
private:
    /** @brief 构造一个无传输的窗格。 */
    static ZzTerminalView *makePane(QWidget *parent = nullptr)
    {
        return new ZzTerminalView(parent);
    }

private slots:
    void initialViewBecomesRoot()
    {
        ZzSplitContainer container;
        auto *a = makePane();
        container.addInitialView(a);
        QCOMPARE(container.paneCount(), 1);
        QCOMPARE(container.focusedView(), a);
        QVERIFY(container.containsView(a));
    }

    void splitHorizontalPlacesPanesSideBySide()
    {
        ZzSplitContainer container;
        auto *a = makePane();
        auto *b = makePane();
        container.addInitialView(a);
        container.splitFocused(Qt::Horizontal, b);
        QCOMPARE(container.paneCount(), 2);
        // 根分割器方向调整为 Horizontal，两窗格同层左右并排
        auto *root = qobject_cast<QSplitter *>(a->parentWidget());
        QVERIFY(root != nullptr);
        QCOMPARE(root->orientation(), Qt::Horizontal);
        QCOMPARE(b->parentWidget(), root);
        // 新窗格成为焦点窗格
        QCOMPARE(container.focusedView(), b);
    }

    void splitVerticalNestsInsideHorizontal()
    {
        ZzSplitContainer container;
        auto *a = makePane();
        auto *b = makePane();
        auto *c = makePane();
        container.addInitialView(a);
        container.splitFocused(Qt::Horizontal, b);
        container.splitFocused(Qt::Vertical, c); // 焦点在 b，b/c 上下嵌套
        QCOMPARE(container.paneCount(), 3);
        auto *nested = qobject_cast<QSplitter *>(b->parentWidget());
        QVERIFY(nested != nullptr);
        QCOMPARE(nested->orientation(), Qt::Vertical);
        QCOMPARE(c->parentWidget(), nested);
        // 根仍是 Horizontal：a 与嵌套分割器并排
        auto *root = qobject_cast<QSplitter *>(a->parentWidget());
        QVERIFY(root != nullptr);
        QCOMPARE(root->orientation(), Qt::Horizontal);
    }

    void closeViewCollapsesNestedSplitter()
    {
        ZzSplitContainer container;
        auto *a = makePane();
        auto *b = makePane();
        auto *c = makePane();
        container.addInitialView(a);
        container.splitFocused(Qt::Horizontal, b);
        container.splitFocused(Qt::Vertical, c);
        QPointer<ZzTerminalView> guardC(c);

        container.closeView(c);
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        QVERIFY(guardC.isNull());
        QCOMPARE(container.paneCount(), 2);
        // 单子嵌套分割器被收拢：b 提升回根
        auto *root = qobject_cast<QSplitter *>(a->parentWidget());
        QVERIFY(root != nullptr);
        QCOMPARE(b->parentWidget(), root);
        QCOMPARE(root->orientation(), Qt::Horizontal);
    }

    void closingLastPaneEmitsEmptied()
    {
        ZzSplitContainer container;
        QSignalSpy emptiedSpy(&container, &ZzSplitContainer::emptied);
        QSignalSpy closingSpy(&container, &ZzSplitContainer::paneClosing);
        auto *a = makePane();
        auto *b = makePane();
        container.addInitialView(a);
        container.splitFocused(Qt::Horizontal, b);

        container.closeView(b);
        QCOMPARE(emptiedSpy.count(), 0);
        QCOMPARE(container.paneCount(), 1);
        // 焦点回落到剩余窗格
        QCOMPARE(container.focusedView(), a);

        container.closeView(a);
        QCOMPARE(emptiedSpy.count(), 1);
        QCOMPARE(closingSpy.count(), 2);
        QCOMPARE(container.paneCount(), 0);
        QVERIFY(container.focusedView() == nullptr);
    }

    void focusTracksIntoPaneAndNavigatesDirectionally()
    {
        ZzSplitContainer container;
        container.resize(800, 400);
        auto *a = makePane();
        auto *b = makePane();
        container.addInitialView(a);
        container.splitFocused(Qt::Horizontal, b);
        container.show();
        QVERIFY(QTest::qWaitForWindowExposed(&container));
        // 等分割器完成布局（两窗格左右就位，中心点可用）
        QTest::qWait(50);

        QSignalSpy focusSpy(&container, &ZzSplitContainer::focusedViewChanged);
        a->setFocus(); // 经 QTermWidget 焦点代理落到内部显示件
        QTRY_VERIFY(QApplication::focusWidget() != nullptr);
        QTRY_COMPARE(container.focusedView(), a);
        QTRY_VERIFY(focusSpy.count() >= 1);

        container.focusPane(ZzSplitContainer::FocusDirection::Right);
        QTRY_COMPARE(container.focusedView(), b);

        container.focusPane(ZzSplitContainer::FocusDirection::Left);
        QTRY_COMPARE(container.focusedView(), a);
    }

    void focusPaneIgnoresDirectionWithoutNeighbor()
    {
        ZzSplitContainer container;
        container.resize(800, 400);
        auto *a = makePane();
        auto *b = makePane();
        container.addInitialView(a);
        container.splitFocused(Qt::Horizontal, b);
        container.show();
        QVERIFY(QTest::qWaitForWindowExposed(&container));
        QTest::qWait(50);

        b->setFocus();
        QTRY_COMPARE(container.focusedView(), b);
        // b 右侧无窗格：焦点不动
        container.focusPane(ZzSplitContainer::FocusDirection::Right);
        QTest::qWait(50);
        QCOMPARE(container.focusedView(), b);
    }
};

QTEST_MAIN(tst_ZzSplitContainer)
#include "tst_ZzSplitContainer.moc"
