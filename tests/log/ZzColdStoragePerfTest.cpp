#include "ZzColdStorage.h"
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

#if defined(Q_OS_MACOS)
#  include <sys/sysctl.h>
#elif defined(Q_OS_WIN)
#  include <windows.h>
#endif

/**
 * @brief ZzColdStorage 冷层性能门控测试（规格 §八）。
 *
 * 阈值失败即测试失败；结果持久化到 tests/perf/records/YYYY-MM-DD-ZzColdStorage-*.json，
 * 内容含阈值、实测值、环境信息与 git commit hash。仅 Release 构建数字有效，
 * Debug 构建整体跳过。
 */
class ZzColdStoragePerfTest : public QObject
{
    Q_OBJECT
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("冷层性能测试行 %1 0123456789abcdef").arg(i), QByteArray(8, 'x')};
    }
    static QVector<ZzLogLine> makeBlock(quint64 firstLine)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(ZzColdStorage::kMaxBlockLines));
        for (quint64 i = 0; i < ZzColdStorage::kMaxBlockLines; ++i)
            out.append(line(firstLine + i));
        return out;
    }
    static ZzColdStorage::Config coldConfig(const QString &dbPath)
    {
        ZzColdStorage::Config c;
        c.dbPath = dbPath;
        c.sessionId = QStringLiteral("perf-session");
        return c;
    }
    /// @brief 向冷层写入 totalLines 行（1024 行/块），返回耗时毫秒。
    static qint64 fillCold(ZzColdStorage &cold, quint64 totalLines)
    {
        QElapsedTimer timer;
        timer.start();
        quint64 frontier = 0;
        while (frontier < totalLines) {
            if (!cold.appendBlock(makeBlock(frontier), frontier))
                return -1;
            frontier += ZzColdStorage::kMaxBlockLines;
        }
        return timer.elapsed();
    }

    /// @brief 返回物理内存总量（MB），无法获取时返回 -1。
    static qint64 totalMemoryMB()
    {
#if defined(Q_OS_LINUX)
        QFile f(QStringLiteral("/proc/meminfo"));
        if (f.open(QIODevice::ReadOnly)) {
            const QByteArray content = f.readAll();
            const qsizetype pos = content.indexOf("MemTotal:");
            if (pos >= 0) {
                const QByteArray line = content.mid(pos, content.indexOf('\n', pos) - pos);
                return QString::fromLatin1(line).split(' ', Qt::SkipEmptyParts).value(1).toLongLong()
                       / 1024;
            }
        }
        return -1;
#elif defined(Q_OS_MACOS)
        int mib[2] = {CTL_HW, HW_MEMSIZE};
        int64_t mem = 0;
        size_t len = sizeof(mem);
        if (sysctl(mib, 2, &mem, &len, nullptr, 0) == 0)
            return mem / 1048576;
        return -1;
#elif defined(Q_OS_WIN)
        MEMORYSTATUSEX status{sizeof(status)};
        if (GlobalMemoryStatusEx(&status))
            return static_cast<qint64>(status.ullTotalPhys / 1048576);
        return -1;
#else
        return -1; // 其他平台无内存采集实现，记录为 -1
#endif
    }

    /// @brief 采集环境信息：CPU/OS/内存/Qt 版本/编译器/构建类型/git commit hash。
    static QJsonObject environmentInfo()
    {
        QJsonObject env;
        env[QStringLiteral("cpu")] = QSysInfo::currentCpuArchitecture();
        env[QStringLiteral("os")] = QSysInfo::prettyProductName();
        env[QStringLiteral("kernel")] = QSysInfo::kernelVersion();
        env[QStringLiteral("memory_mb")] = double(totalMemoryMB());
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
        QSKIP("性能门控仅在 Release 构建下有效（规格 §八）");
#endif
    }

    /// @brief 冷层写入吞吐门控：50 万行（1024 行/块，含 ZSTD + SQLite 事务 + FTS5 索引）
    ///        ≥ 500,000 行/秒。
    /// @note 块粒度写入：500,000 非 1024 整倍，实际落 489 块 = 500,736 行，
    ///       吞吐按实际写入行数计（500,736 行 / 耗时）。
    void coldWriteThroughput()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 500000;
        const qint64 ms = fillCold(cold, N);
        QVERIFY(ms > 0);
        const quint64 written = cold.frontier();
        QCOMPARE(written, quint64(489) * ZzColdStorage::kMaxBlockLines); // 500,736 行（向上取整块）

        const double linesPerSec = double(written) / (double(ms) / 1000.0);
        constexpr double threshold = 500000.0;
        const bool passed = linesPerSec >= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-write-throughput"), threshold,
                    QStringLiteral("lines/s"), linesPerSec, passed,
                    {{QStringLiteral("lineCount"), qint64(written)},
                     {QStringLiteral("blockLines"), qint64(ZzColdStorage::kMaxBlockLines)},
                     {QStringLiteral("elapsedMs"), ms}});
        QVERIFY2(passed, qPrintable(QStringLiteral("冷层写入吞吐 %1 行/秒，低于阈值 %2")
                                        .arg(linesPerSec)
                                        .arg(threshold)));
    }

    /// @brief 冷层随机读 24 行（缓存未命中）门控：3,379,200 行（3300 块），
    ///        100 个采样点间隔 33 块（严格递增，保证 LRU 未命中），最差值 ≤ 5ms。
    void coldRandomRead24Lines()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 3300 * ZzColdStorage::kMaxBlockLines; // 3,379,200 行
        QVERIFY(fillCold(cold, N) > 0);

        constexpr int samples = 100;
        qint64 worstNs = 0;
        for (int s = 0; s < samples; ++s) {
            const quint64 start = quint64(s) * 33 * ZzColdStorage::kMaxBlockLines + 7;
            QVERIFY(start + 24 <= N);
            QElapsedTimer t;
            t.start();
            const QVector<ZzLogLine> window = cold.readLines(start, 24);
            const qint64 ns = t.nsecsElapsed();
            QCOMPARE(window.size(), 24);
            QCOMPARE(window.first().text, line(start).text);
            worstNs = qMax(worstNs, ns);
        }

        const double worstMs = double(worstNs) / 1e6;
        constexpr double threshold = 5.0;
        const bool passed = worstMs <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-random-read-24"), threshold,
                    QStringLiteral("ms"), worstMs, passed,
                    {{QStringLiteral("samples"), samples},
                     {QStringLiteral("windowRows"), 24},
                     {QStringLiteral("totalLines"), qint64(N)},
                     {QStringLiteral("cacheBlocks"), 32},
                     {QStringLiteral("sampleStrideBlocks"), 33}});
        QVERIFY2(passed, qPrintable(QStringLiteral("冷层随机读 24 行最差 %1ms，超过阈值 %2ms")
                                        .arg(worstMs)
                                        .arg(threshold)));
    }

    /// @brief 冷层连续滚动（缓存命中）门控：1,024,000 行从头按 24 行/页连读 1000 页，
    ///        平均值 ≤ 1ms/页。
    void coldSequentialScroll()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 1000 * ZzColdStorage::kMaxBlockLines; // 1,024,000 行
        QVERIFY(fillCold(cold, N) > 0);

        constexpr int pages = 1000;
        qint64 totalNs = 0;
        qint64 worstNs = 0;
        for (int p = 0; p < pages; ++p) {
            const quint64 start = quint64(p) * 24;
            QElapsedTimer t;
            t.start();
            const QVector<ZzLogLine> window = cold.readLines(start, 24);
            const qint64 ns = t.nsecsElapsed();
            QCOMPARE(window.size(), 24);
            totalNs += ns;
            worstNs = qMax(worstNs, ns);
        }

        const double avgMs = double(totalNs) / pages / 1e6;
        constexpr double threshold = 1.0;
        const bool passed = avgMs <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-sequential-scroll"), threshold,
                    QStringLiteral("ms/page"), avgMs, passed,
                    {{QStringLiteral("pages"), pages},
                     {QStringLiteral("worstMs"), double(worstNs) / 1e6},
                     {QStringLiteral("pageRows"), 24},
                     {QStringLiteral("totalLines"), qint64(N)}});
        QVERIFY2(passed, qPrintable(QStringLiteral("冷层连续滚动平均 %1ms/页，超过阈值 %2ms")
                                        .arg(avgMs)
                                        .arg(threshold)));
    }

    /// @brief FTS5 全文搜索门控：1000 万行库内单关键词搜索 ≤ 500ms。
    /// @note 建库写入阶段约数十秒（≥50 万行/s 时 10M 行 ≈ 20s+），属预期耗时。
    void coldFtsSearch10M()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 10000 * ZzColdStorage::kMaxBlockLines; // 10,240,000 行
        quint64 frontier = 0;
        while (frontier < N) {
            QVector<ZzLogLine> block = makeBlock(frontier);
            // 每 100003 行埋一个关键词 NEEDLE（约 102 个命中）
            const quint64 nextNeedle = (frontier / 100003 + 1) * 100003;
            if (nextNeedle < frontier + ZzColdStorage::kMaxBlockLines)
                block[qsizetype(nextNeedle - frontier)].text =
                    QStringLiteral("埋点 NEEDLE %1").arg(nextNeedle);
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += ZzColdStorage::kMaxBlockLines;
        }

        QElapsedTimer t;
        t.start();
        const QVector<quint64> hits = cold.search(QStringLiteral("NEEDLE"));
        const qint64 ms = t.elapsed();
        QVERIFY(!hits.isEmpty());

        constexpr double threshold = 500.0;
        const bool passed = double(ms) <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-fts-search-10m"), threshold,
                    QStringLiteral("ms"), double(ms), passed,
                    {{QStringLiteral("totalLines"), qint64(N)},
                     {QStringLiteral("hits"), hits.size()}});
        QVERIFY2(passed, qPrintable(QStringLiteral("FTS5 搜索 1000 万行耗时 %1ms，超过阈值 %2ms")
                                        .arg(ms)
                                        .arg(threshold)));
    }

    /// @brief 三层归并滚动门控：30 万行（冷+温+热）随机窗口读 60 行，最差值 ≤ 16ms。
    ///        随机采样统计覆盖冷/温、温/热边界穿越。
    void mergedScrollRead()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config; // 默认：热 10,000 / 温 1,000,000 / 批 1024
        config.warmFilePath = dir.filePath(QStringLiteral("warm.log"));
        config.coldDbPath = dir.filePath(QStringLiteral("cold.db"));
        config.sessionId = QStringLiteral("perf-session");
        ZzLogEngine engine(config);
        QVERIFY(engine.open());
        QVERIFY(engine.isColdEnabled());

        constexpr quint64 N = 300000;
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush(); // 温层全部推进冷层，热层留存尾部
        QCOMPARE(engine.totalLines(), N);
        QVERIFY(engine.firstLineNo() == 0);

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
            QCOMPARE(window.first().text, line(start).text);
            worstNs = qMax(worstNs, ns);
            totalNs += ns;
        }

        const double worstMs = double(worstNs) / 1e6;
        const double avgMs = double(totalNs) / samples / 1e6;
        constexpr double threshold = 16.0; // 滚动帧时间上限（规格 §一红线）
        const bool passed = worstMs <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-merged-scroll-read"), threshold,
                    QStringLiteral("ms"), worstMs, passed,
                    {{QStringLiteral("samples"), samples},
                     {QStringLiteral("avgMs"), avgMs},
                     {QStringLiteral("windowRows"), 60},
                     {QStringLiteral("totalLines"), qint64(N)}});
        QVERIFY2(passed, qPrintable(QStringLiteral("三层归并滚动最差 %1ms，超过阈值 %2ms")
                                        .arg(worstMs)
                                        .arg(threshold)));
    }
};

QTEST_GUILESS_MAIN(ZzColdStoragePerfTest)
#include "ZzColdStoragePerfTest.moc"
