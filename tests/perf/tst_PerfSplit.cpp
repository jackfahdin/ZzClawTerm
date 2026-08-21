#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "ZzPerfRecorder.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzSplitContainer.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 性能门控：单标签分屏至 4 窗格，每窗格注入 100×1KB 远端输出后全部关闭。
 *        阈值 2000ms（Release），覆盖分屏结构变更与多窗格渲染路径。
 */
class tst_PerfSplit : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase()
    {
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void splitRenderAndClose()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        ZzTabManager tabs;
        tabs.resize(1024, 768);
        tabs.show();

        ZzSessionProfile profile;
        profile.name = QStringLiteral("分屏性能");
        profile.protocol = QStringLiteral("mock");

        QElapsedTimer timer;
        timer.start();
        tabs.openSession(profile);
        // 2×2：先左右分屏，再各自上下分屏
        tabs.splitCurrentTab(Qt::Horizontal);
        tabs.focusPane(ZzSplitContainer::FocusDirection::Left);
        tabs.splitCurrentTab(Qt::Vertical);
        tabs.focusPane(ZzSplitContainer::FocusDirection::Right);
        tabs.splitCurrentTab(Qt::Vertical);
        const QByteArray chunk(1024, 'x');
        for (ZzTerminalView *view : tabs.viewsAt(0)) {
            auto *mock = static_cast<ZzMockTransport *>(view->transport());
            for (int i = 0; i < 100; ++i) {
                mock->simulateData(chunk);
            }
        }
        tabs.closeTab(0);
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("split-rendering"),
            QStringLiteral("分屏 4 窗格渲染并关闭"), 2000.0, elapsed,
            QStringLiteral("ms"),
            QJsonObject{{QStringLiteral("panes"), 4},
                        {QStringLiteral("chunksPerPane"), 100},
                        {QStringLiteral("chunkBytes"), 1024}});
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 2000ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfSplit)
#include "tst_PerfSplit.moc"
