#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "ZzPerfRecorder.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 性能门控：开会话到连接就绪的端到端耗时（mock 传输）。阈值 500ms（Release）。
 * @note 真实 SSH 建连耗时由计划 01 的性能测试覆盖；此处门控的是装配层自身开销。
 *
 * 注：简报样例按规划稿写成 profile.id=QString；计划 03 实际交付契约 id 为
 * QUuid，本测试不依赖 id 取值，已省略该字段（其余与简报一致）。
 */
class tst_PerfConnectFlow : public QObject
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

    void openSessionToConnected()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        ZzTabManager tabs;
        ZzSessionProfile profile;
        profile.name = QStringLiteral("性能流程");
        profile.protocol = QStringLiteral("mock");

        QElapsedTimer timer;
        timer.start();
        tabs.openSession(profile);
        auto *view = tabs.viewAt(0);
        QTRY_VERIFY_WITH_TIMEOUT(
            view->transportState() == ZzTransportInterface::State::Connected, 5000);
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("connect-flow"),
            QStringLiteral("开会话到连接就绪（mock）"), 500.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 500ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfConnectFlow)
#include "tst_PerfConnectFlow.moc"
