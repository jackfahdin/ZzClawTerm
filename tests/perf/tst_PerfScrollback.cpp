#include <QtTest/QtTest>

#include <QTemporaryDir>

#include "ZzPerfRecorder.h"
#include "log/ZzLogEngine.h"
#include "qtermwidget.h"
#include "terminal/ZzScrollbackBridge.h"

/**
 * @brief 性能门控：经桥向引擎追加 10 万行（热层+温层归档路径）。阈值 5000ms（Release）。
 * @note 滚动帧时间 ≤16ms 的渲染侧指标由 ZzTermWidget 自身性能测试覆盖；
 *       此处门控的是"输出行 → 引擎"这条装配链路不成为吞吐瓶颈。
 */
class tst_PerfScrollback : public QObject
{
    Q_OBJECT
private slots:
    void appendHundredThousandLines()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        QTermWidget term;
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config; // 默认：热 1 万 / 温 100 万 → 走归档路径
        config.warmFilePath = dir.filePath(QStringLiteral("warm.log"));
        ZzLogEngine engine(config);
        QVERIFY(engine.open());
        ZzScrollbackBridge bridge(&term, &engine);

        QElapsedTimer timer;
        timer.start();
        for (int i = 0; i < 100000; ++i) {
            const QByteArray line =
                QStringLiteral("perf-line-%1 abcdefghijklmnopqrstuvwxyz\n")
                    .arg(i).toUtf8();
            emit term.dupDisplayOutput(line.constData(), line.size());
        }
        QCoreApplication::processEvents();
        const double elapsed = static_cast<double>(timer.elapsed());
        engine.flush(); // 等待已排队批次归档完成后再核对总行数
        QCOMPARE(engine.totalLines(), 100000ULL);

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("scrollback-append"),
            QStringLiteral("滚动历史追加 10 万行"), 5000.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 5000ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfScrollback)
#include "tst_PerfScrollback.moc"
