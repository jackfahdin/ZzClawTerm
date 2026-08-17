#include "ZzLogEngine.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QRandomGenerator>
#include <QSysInfo>
#include <QTemporaryDir>
#include <QtTest>

/**
 * @brief ZzLogEngine 性能门控测试（规格 §9.1）。
 *
 * 阈值失败即测试失败；结果持久化到 tests/perf/records/YYYY-MM-DD-<功能名>.json，
 * 内容含阈值、实测值、环境信息与 git commit hash。仅 Release 构建数字有效，
 * Debug 构建整体跳过。
 */
class ZzLogEnginePerfTest : public QObject
{
    Q_OBJECT
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("性能测试行 %1 0123456789abcdef").arg(i), QByteArray(8, 'x')};
    }

    /// @brief 采集环境信息：CPU/OS/Qt 版本/编译器/构建类型/git commit hash。
    static QJsonObject environmentInfo()
    {
        QJsonObject env;
        env[QStringLiteral("cpu")] = QSysInfo::currentCpuArchitecture();
        env[QStringLiteral("os")] = QSysInfo::prettyProductName();
        env[QStringLiteral("kernel")] = QSysInfo::kernelVersion();
        env[QStringLiteral("qtVersion")] = QStringLiteral(QT_VERSION_STR);
        env[QStringLiteral("buildType")] = QStringLiteral("Release");
#if defined(Q_CC_MSVC)
        env[QStringLiteral("compiler")] = QStringLiteral("MSVC %1").arg(_MSC_VER);
#elif defined(Q_CC_CLANG)
        env[QStringLiteral("compiler")] = QStringLiteral("Clang %1").arg(__clang_major__);
#elif defined(Q_CC_GNU)
        env[QStringLiteral("compiler")] = QStringLiteral("GCC %1.%2.%3")
                                              .arg(__GNUC__)
                                              .arg(__GNUC_MINOR__)
                                              .arg(__GNUC_PATCHLEVEL__);
#else
        env[QStringLiteral("compiler")] = QStringLiteral("unknown");
#endif
        QProcess git;
        git.start(QStringLiteral("git"), {QStringLiteral("rev-parse"), QStringLiteral("HEAD")});
        if (git.waitForFinished(5000) && git.exitCode() == 0)
            env[QStringLiteral("gitCommit")] =
                QString::fromUtf8(git.readAllStandardOutput()).trimmed();
        return env;
    }

    /// @brief 写性能记录到 tests/perf/records/YYYY-MM-DD-<name>.json。
    static void writeRecord(const QString &name, double threshold, const QString &unit,
                            double measured, bool passed, const QJsonObject &details)
    {
        QJsonObject root;
        root[QStringLiteral("testName")] = name;
        root[QStringLiteral("threshold")] = threshold;
        root[QStringLiteral("unit")] = unit;
        root[QStringLiteral("measured")] = measured;
        root[QStringLiteral("passed")] = passed;
        root[QStringLiteral("environment")] = environmentInfo();
        root[QStringLiteral("details")] = details;
        root[QStringLiteral("timestamp")] = QDateTime::currentDateTimeUtc().toString(Qt::ISODate);

        const QString dirPath = QStringLiteral(PERF_RECORDS_DIR);
        QDir().mkpath(dirPath);
        const QString filePath = QDir(dirPath).filePath(
            QDateTime::currentDateTimeUtc().toString(QStringLiteral("yyyy-MM-dd"))
            + QStringLiteral("-") + name + QStringLiteral(".json"));
        QFile f(filePath);
        QVERIFY2(f.open(QIODevice::WriteOnly), qPrintable(filePath));
        f.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    }

private slots:
    void initTestCase()
    {
#ifdef QT_NO_DEBUG
        // Release：执行门控
#else
        QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
#endif
    }

    /// @brief 写入吞吐门控：20 万行（含热层驱逐 + 温层归档落盘）≥ 50,000 行/秒。
    void writeThroughput()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config; // 默认：热 10,000 / 温 1,000,000 / 批 1024
        config.warmFilePath = dir.filePath(QStringLiteral("perf-warm.log"));
        ZzLogEngine engine(config);
        QVERIFY(engine.open());

        constexpr quint64 N = 200000;
        QElapsedTimer timer;
        timer.start();
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush(); // 含全部归档与冲刷
        const qint64 ms = timer.elapsed();

        const double linesPerSec = double(N) / (double(ms) / 1000.0);
        constexpr double threshold = 50000.0;
        const bool passed = linesPerSec >= threshold;
        writeRecord(QStringLiteral("ZzLogEngine-write-throughput"), threshold,
                    QStringLiteral("lines/s"), linesPerSec, passed,
                    {{QStringLiteral("lineCount"), qint64(N)},
                     {QStringLiteral("elapsedMs"), ms}});
        QVERIFY2(passed, qPrintable(QStringLiteral("写入吞吐 %1 行/秒，低于阈值 %2")
                                        .arg(linesPerSec)
                                        .arg(threshold)));
    }

    /// @brief 滚动读取延迟门控：20 万行中随机窗口读取 60 行，最差值 ≤ 16ms。
    void scrollReadLatency()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config;
        config.warmFilePath = dir.filePath(QStringLiteral("perf-warm.log"));
        ZzLogEngine engine(config);
        QVERIFY(engine.open());

        constexpr quint64 N = 200000;
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush();
        QCOMPARE(engine.totalLines(), N);

        constexpr int samples = 200;
        qint64 worstNs = 0;
        qint64 totalNs = 0;
        for (int s = 0; s < samples; ++s) {
            const quint64 start = quint64(QRandomGenerator::global()->bounded(qint64(N - 60)));
            QElapsedTimer t;
            t.start();
            const QVector<ZzLogLine> window = engine.getLines(start, 60);
            const qint64 ns = t.nsecsElapsed();
            QCOMPARE(window.size(), 60);
            worstNs = qMax(worstNs, ns);
            totalNs += ns;
        }

        const double worstMs = double(worstNs) / 1e6;
        const double avgMs = double(totalNs) / samples / 1e6;
        constexpr double threshold = 16.0; // 滚动帧时间上限（规格 §5.1 / §9.1）
        const bool passed = worstMs <= threshold;
        writeRecord(QStringLiteral("ZzLogEngine-scroll-read-latency"), threshold,
                    QStringLiteral("ms"), worstMs, passed,
                    {{QStringLiteral("samples"), samples},
                     {QStringLiteral("avgMs"), avgMs},
                     {QStringLiteral("windowRows"), 60},
                     {QStringLiteral("totalLines"), qint64(N)}});
        QVERIFY2(passed, qPrintable(QStringLiteral("滚动读取最差 %1ms，超过阈值 %2ms")
                                        .arg(worstMs)
                                        .arg(threshold)));
    }
};

QTEST_GUILESS_MAIN(ZzLogEnginePerfTest)
#include "ZzLogEnginePerfTest.moc"
