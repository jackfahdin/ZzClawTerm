#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "ZzPerfRecorder.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 性能门控：连续打开并关闭 50 个标签。阈值 3000ms（Release）。
 *
 * 注：简报样例按规划稿写成 profile.id=QString；计划 03 实际交付契约 id 为
 * QUuid，本测试不依赖 id 取值，已省略该字段（其余与简报一致）。
 */
class tst_PerfTabLifecycle : public QObject
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

    void openCloseFiftyTabs()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        ZzTabManager tabs;

        QElapsedTimer timer;
        timer.start();
        for (int i = 0; i < 50; ++i) {
            ZzSessionProfile profile;
            profile.name = QStringLiteral("性能标签%1").arg(i);
            profile.protocol = QStringLiteral("mock");
            tabs.openSession(profile);
        }
        for (int i = 49; i >= 0; --i) {
            tabs.closeTab(i);
        }
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("tab-lifecycle"),
            QStringLiteral("打开并关闭 50 个标签"), 3000.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 3000ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfTabLifecycle)
#include "tst_PerfTabLifecycle.moc"
