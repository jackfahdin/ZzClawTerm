#include "ZzLogEngine.h"

#include <QFile>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QThread>
#include <QtTest>

#include <atomic>

/**
 * @brief ZzLogEngine 门面单元测试：归档往返、滚动读取等价性、预加载、降级。
 *
 * 测试配置：热层 100 行、归档批 16 行、温层上限 10 万行。
 * 追加 N 行时热层按批驱逐：第 100+16k 次追加（0 起计）驱逐 16 行。
 */
class ZzLogEngineTest : public QObject
{
    Q_OBJECT
    static ZzLogEngine::Config testConfig(const QString &warmPath)
    {
        ZzLogEngine::Config c;
        c.hotCapacity = 100;
        c.archiveBatchSize = 16;
        c.warmMaxLines = 100000;
        c.warmFilePath = warmPath;
        return c;
    }
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("engine-line-%1").arg(i),
                QByteArray("E") + QByteArray::number(qint64(i))};
    }

private slots:
    /// @brief 归档往返：溢出热层的行经温层完整读回（含属性），最新行仍走热层。
    void archiveRoundtrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        for (quint64 i = 0; i < 250; ++i)
            engine.appendLine(line(i));
        engine.flush(); // 等待全部已排队批次归档完成
        QCOMPARE(engine.totalLines(), 250ULL);
        QCOMPARE(engine.firstLineNo(), 0ULL);

        ZzLogLine got;
        QVERIFY(engine.getLine(0, &got)); // 最老行：温层
        QCOMPARE(got.text, line(0).text);
        QCOMPARE(got.attributes, line(0).attributes);
        QVERIFY(engine.getLine(249, &got)); // 最新行：热层
        QCOMPARE(got.text, line(249).text);
    }

    /// @brief 滚动读取等价性：跨热/温层的滑动窗口与写入序列逐行一致。
    void scrollReadEquivalence()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        constexpr quint64 N = 500; // 400 行入温层，100 行留热层
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush();
        for (quint64 start = 0; start + 60 <= N; start += 17) {
            QVector<ZzLogLine> window = engine.getLines(start, 60);
            QCOMPARE(window.size(), 60);
            for (int j = 0; j < 60; ++j)
                QCOMPARE(window[j].text, line(start + quint64(j)).text);
        }
    }

    /// @brief 预加载不改变读取语义。
    void preloadKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        for (quint64 i = 0; i < 300; ++i)
            engine.appendLine(line(i));
        engine.flush();
        engine.preload(10);
        QTest::qWait(50); // 让预加载在归档线程执行完
        QVector<ZzLogLine> got = engine.getLines(10, 20);
        QCOMPARE(got.size(), 20);
        QCOMPARE(got.first().text, line(10).text);
    }

    /// @brief 纯内存模式（无温层文件）：容量受限于热层，最老行被驱逐。
    void memoryOnlyModeCapsAtHotCapacity()
    {
        ZzLogEngine engine(testConfig(QString()));
        QVERIFY(engine.open());
        QVERIFY(engine.isMemoryOnly());
        for (quint64 i = 0; i < 250; ++i)
            engine.appendLine(line(i));
        // 10 批 × 16 行被驱逐：剩余 90 行，首行 ID 160
        QCOMPARE(engine.totalLines(), 90ULL);
        QCOMPARE(engine.firstLineNo(), 160ULL);
        ZzLogLine got;
        QVERIFY(engine.getLine(160, &got));
        QCOMPARE(got.text, line(160).text);
    }

    /// @brief 温层文件无法打开时降级纯内存并发射信号（规格 §八）。
    void warmOpenFailureDegrades()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        // 用一个已存在的文件当“父目录”，其下路径必然打不开（跨平台）
        const QString blocker = dir.filePath(QStringLiteral("blocker"));
        QFile f(blocker);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();

        ZzLogEngine::Config c = testConfig(blocker + QStringLiteral("/warm.log"));
        ZzLogEngine engine(c);
        QSignalSpy spy(&engine, &ZzLogEngine::degradedToMemoryOnly);
        QVERIFY(engine.open()); // 降级视为可用
        QVERIFY(engine.isMemoryOnly());
        QCOMPARE(spy.count(), 1);
        for (quint64 i = 0; i < 200; ++i)
            engine.appendLine(line(i));
        // 7 批 × 16 行被驱逐：剩余 88 行
        QCOMPARE(engine.totalLines(), 88ULL);
    }

    /// @brief 引擎析构后温层数据保留，重开引擎可继续读（热层随进程销毁属设计行为）。
    void engineReopenRestoresWarmLayer()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        {
            ZzLogEngine engine(testConfig(path));
            QVERIFY(engine.open());
            for (quint64 i = 0; i < 200; ++i)
                engine.appendLine(line(i));
            engine.flush(); // 温层 112 行
        }
        ZzLogEngine engine(testConfig(path));
        QVERIFY(engine.open());
        QVERIFY(engine.totalLines() >= 100ULL);
        ZzLogLine got;
        QVERIFY(engine.getLine(engine.firstLineNo(), &got));
        QCOMPARE(got.text, line(engine.firstLineNo()).text);
    }

    /// @brief 多线程并发读压力：两个线程并发 getLines 命中未缓存块，
    ///        同时主线程持续触发 preload，校验 QCache 并发保护下的数据正确性。
    void concurrentReadsWithPreloadStress()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config c = testConfig(dir.filePath(QStringLiteral("warm.log")));
        c.hotCapacity = 64;
        c.archiveBatchSize = 32;
        ZzLogEngine engine(c);
        QVERIFY(engine.open());

        // 每行约 1KB，单块（64KB）约 60 行；温层 900+ 行跨十余块，超出 8 块缓存容量，
        // 保证并发读取反复缓存未命中并写 QCache
        const QString payload = QString(1024, QLatin1Char('x'));
        const auto makeLine = [&payload](quint64 i) {
            return ZzLogLine{payload + QString::number(qint64(i)), QByteArray()};
        };
        constexpr quint64 N = 1000;
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(makeLine(i));
        engine.flush();
        QVERIFY(engine.totalLines() == N);

        std::atomic<bool> failed{false};
        const auto reader = [&engine, &failed, &makeLine]() {
            for (int round = 0; round < 50 && !failed.load(); ++round) {
                for (quint64 start = 0; start + 100 <= N; start += 37) {
                    const QVector<ZzLogLine> window = engine.getLines(start, 100);
                    if (window.size() != 100 ||
                        window.first().text != makeLine(start).text ||
                        window.last().text != makeLine(start + 99).text) {
                        failed.store(true);
                        return;
                    }
                }
            }
        };
        QThread *t1 = QThread::create(reader);
        QThread *t2 = QThread::create(reader);
        t1->start();
        t2->start();
        // 读线程运行期间持续触发归档线程的 preload（写解压缓存路径）
        while (!t1->isFinished() || !t2->isFinished()) {
            for (quint64 id = 0; id < N; id += 64)
                engine.preload(id);
            QThread::msleep(1);
        }
        t1->wait();
        t2->wait();
        delete t1;
        delete t2;
        QVERIFY(!failed.load());
    }
};

QTEST_GUILESS_MAIN(ZzLogEngineTest)
#include "ZzLogEngineTest.moc"
