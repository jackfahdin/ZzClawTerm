#include "ZzLogEngine.h"

#include "ZzColdStorage.h"
#include "ZzMmapBuffer.h"

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
    static ZzLogEngine::Config testColdConfig(const QString &warmPath, const QString &dbPath)
    {
        ZzLogEngine::Config c = testConfig(warmPath); // 热 100 / 批 16 / 温 10 万
        c.coldDbPath = dbPath;
        c.sessionId = QStringLiteral("test-profile");
        return c;
    }
    static QVector<ZzLogLine> makeLines(quint64 start, quint64 count)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(count));
        for (quint64 i = 0; i < count; ++i)
            out.append(line(start + i));
        return out;
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

    /// @brief 冷层禁用（coldDbPath 为空）：isColdEnabled 为 false，行为与 v0.1 完全一致。
    void coldDisabledKeepsV01Behavior()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        QVERIFY(!engine.isColdEnabled());
        QVERIFY(!engine.isMemoryOnly());
        for (quint64 i = 0; i < 200; ++i)
            engine.appendLine(line(i));
        engine.flush();
        QCOMPARE(engine.totalLines(), 200ULL);
        QVERIFY(engine.searchLines(QStringLiteral("engine-line")).isEmpty()); // 无冷层 → 空
    }

    /// @brief 三层归并：2500 行（2400 冷层 + 100 热层）滑动窗口与写入序列逐行一致，
    ///        跨冷/温/热边界无错位。
    void coldRoundtripThreeLayerMerge()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testColdConfig(dir.filePath(QStringLiteral("warm.log")),
                                          dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(engine.open());
        QVERIFY(engine.isColdEnabled());
        constexpr quint64 N = 2500; // 驱逐 150 批 × 16 = 2400 行入温层→flush 后全入冷层；100 行留热层
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush();
        QCOMPARE(engine.totalLines(), N);
        QCOMPARE(engine.firstLineNo(), 0ULL);

        for (quint64 start = 0; start + 60 <= N; start += 17) {
            const QVector<ZzLogLine> window = engine.getLines(start, 60);
            QCOMPARE(window.size(), 60);
            for (int j = 0; j < 60; ++j)
                QCOMPARE(window[j].text, line(start + quint64(j)).text);
        }
        // 冷层数据已由冷层承载（库内验证在 cleanExitDeletesWarmFile 用例做）
    }

    /// @brief 干净退出：flush 后析构删除温层文件，全部已归档行持久于冷层库。
    void cleanExitDeletesWarmFileAndPersistsCold()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString warmPath = dir.filePath(QStringLiteral("warm.log"));
        const QString dbPath = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzLogEngine engine(testColdConfig(warmPath, dbPath));
            QVERIFY(engine.open());
            for (quint64 i = 0; i < 2500; ++i)
                engine.appendLine(line(i));
            engine.flush();
        } // 析构：冷层健康 → 删除温层文件
        QVERIFY(!QFile::exists(warmPath));
        QVERIFY(QFile::exists(dbPath));

        // 直接打开冷层库验证（首个会话 offset 为 0，库内行号 == 引擎行号）
        ZzColdStorage::Config cc;
        cc.dbPath = dbPath;
        cc.sessionId = QStringLiteral("test-profile");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QCOMPARE(cold.frontier(), 2400ULL); // flush 把温层 2400 行全部推进冷层
        const QVector<ZzLogLine> head = cold.readLines(0, 3);
        QCOMPARE(head.size(), 3);
        QCOMPARE(head[0].text, line(0).text);
        QCOMPARE(head[2].text, line(2).text);
    }

    /// @brief 崩溃恢复：残留温层（游标 500，共 1500 行）+ 冷层已归档 500 行；
    ///        引擎 open 同步续传 [500,1500) 进冷层后删除残留、全新开始；
    ///        续传的行属上一会话，对本会话不可见但持久可查。
    void crashRecoveryResumesResidualWarm()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString warmPath = dir.filePath(QStringLiteral("warm.log"));
        const QString dbPath = dir.filePath(QStringLiteral("cold.db"));
        // 构造"崩溃现场"：温层 1500 行、游标 500；冷层已有 [0,500)
        {
            ZzMmapBuffer warm(warmPath, 100000);
            QVERIFY(warm.open());
            // 单次追加 ~43KB < 64KB：1500 行落在同一块内，floor 500 不丢块（跨整块才丢弃）
            QVERIFY(warm.appendLines(makeLines(0, 1500)));
            warm.setRetentionFloor(500); // 持久化续传游标
            QCOMPARE(warm.firstLineId(), 0ULL);
            warm.flush();
            warm.close();
            ZzColdStorage::Config cc;
            cc.dbPath = dbPath;
            cc.sessionId = QStringLiteral("test-profile");
            ZzColdStorage cold(cc);
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 500), 0));
        }
        // 引擎 open：同步续传 [500,1500) → 删除残留 → 全新温层
        ZzLogEngine engine(testColdConfig(warmPath, dbPath));
        QSignalSpy warmOnlySpy(&engine, &ZzLogEngine::degradedToWarmOnly);
        QVERIFY(engine.open());
        QCOMPARE(warmOnlySpy.count(), 0);
        QVERIFY(engine.isColdEnabled());
        QCOMPARE(engine.totalLines(), 0ULL); // 新会话从空开始（残留行已入冷层，不属于本会话）
        QCOMPARE(engine.firstLineNo(), 0ULL);

        // 冷层库内验证：1500 行全部在库且内容正确
        ZzColdStorage::Config cc;
        cc.dbPath = dbPath;
        cc.sessionId = QStringLiteral("test-profile");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QCOMPARE(cold.frontier(), 1500ULL);
        QCOMPARE(cold.readLines(499, 3).first().text, line(499).text);  // 续传边界
        QCOMPARE(cold.readLines(1499, 1).first().text, line(1499).text);

        // 新会话写入的行接续全局空间；searchLines 返回引擎空间行号
        for (quint64 i = 0; i < 200; ++i) {
            ZzLogLine l = line(i);
            if (i == 10)
                l.text = QStringLiteral("post-recovery FATALMARK line");
            engine.appendLine(l);
        }
        engine.flush(); // 驱逐 7 批 × 16 = 112 行入冷层（全局 [1500,1612)）
        QCOMPARE(engine.searchLines(QStringLiteral("FATALMARK")),
                 (QVector<quint64>{10ULL})); // 引擎空间：本会话第 10 行
    }

    /// @brief 冷层库打开失败：发射 degradedToWarmOnly 降级为 v0.1 温层模式；
    ///        残留温层无续传去向被删除全新开始；降级后析构不得删除温层文件。
    void coldOpenFailureDegradesToWarmOnly()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString warmPath = dir.filePath(QStringLiteral("warm.log"));
        // 预置残留温层（100 行）
        {
            ZzMmapBuffer warm(warmPath, 100000);
            QVERIFY(warm.open());
            QVERIFY(warm.appendLines(makeLines(0, 100)));
            warm.flush();
            warm.close();
        }
        // 用一个已存在的文件当“父目录”，其下 cold.db 必然打不开（跨平台）
        const QString blocker = dir.filePath(QStringLiteral("blocker"));
        QFile f(blocker);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();

        ZzLogEngine::Config c = testConfig(warmPath);
        c.coldDbPath = blocker + QStringLiteral("/cold.db");
        c.sessionId = QStringLiteral("test-profile");
        {
            ZzLogEngine engine(c);
            QSignalSpy warmOnlySpy(&engine, &ZzLogEngine::degradedToWarmOnly);
            QSignalSpy memoryOnlySpy(&engine, &ZzLogEngine::degradedToMemoryOnly);
            QVERIFY(engine.open());
            QCOMPARE(warmOnlySpy.count(), 1);
            QCOMPARE(memoryOnlySpy.count(), 0);
            QVERIFY(!engine.isColdEnabled());
            QVERIFY(!engine.isMemoryOnly());
            QCOMPARE(engine.totalLines(), 0ULL); // 残留温层已删除，全新开始
            for (quint64 i = 0; i < 200; ++i)
                engine.appendLine(line(i));
            engine.flush();
            QCOMPARE(engine.totalLines(), 200ULL); // v0.1 温层路径正常
        }
        QVERIFY(QFile::exists(warmPath)); // 冷层降级后不得删除温层文件（规格 §六）
    }

    /// @brief 审查修复轮 2（块映射表）：两个引擎实例共享同一 cold.db（不同温层文件、
    ///        不同 sessionId），交错写入到双方都发生冷层归档后各自 flush，验证：
    ///        各自 getLines 完整读回自己写入的行（不串行、不缺失、不空白）；
    ///        各自 searchLines 只命中自己的行；firstLineNo/totalLines 自洽。
    void multiSessionEnginesSharedColdDb()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString dbPath = dir.filePath(QStringLiteral("cold.db"));
        ZzLogEngine::Config ca =
            testColdConfig(dir.filePath(QStringLiteral("warm-a.log")), dbPath);
        ca.sessionId = QStringLiteral("session-a");
        ZzLogEngine::Config cb =
            testColdConfig(dir.filePath(QStringLiteral("warm-b.log")), dbPath);
        cb.sessionId = QStringLiteral("session-b");
        ZzLogEngine engineA(ca);
        ZzLogEngine engineB(cb);
        QVERIFY(engineA.open());
        QVERIFY(engineB.open());
        QVERIFY(engineA.isColdEnabled());
        QVERIFY(engineB.isColdEnabled());

        // 行文本带会话专属 token + 会话内序号：保证全局可区分、FTS 可按 token 过滤
        const auto lineA = [](quint64 i) {
            return ZzLogLine{QStringLiteral("row %1 tokenAlpha").arg(i), QByteArray("A")};
        };
        const auto lineB = [](quint64 i) {
            return ZzLogLine{QStringLiteral("row %1 tokenBravo").arg(i), QByteArray("B")};
        };

        // 交错写入：每轮各自 flush 强制落冷层，制造块级交错（热 100 / 批 16，
        // A 每轮 200 行共 6 轮 = 1200，B 每轮 150 行共 6 轮 = 900）
        constexpr quint64 NA = 1200;
        constexpr quint64 NB = 900;
        for (quint64 round = 0; round < 6; ++round) {
            for (quint64 i = round * 200; i < (round + 1) * 200; ++i)
                engineA.appendLine(lineA(i));
            engineA.flush();
            for (quint64 i = round * 150; i < (round + 1) * 150; ++i)
                engineB.appendLine(lineB(i));
            engineB.flush();
        }

        // firstLineNo/totalLines 自洽
        QCOMPARE(engineA.totalLines(), NA);
        QCOMPARE(engineA.firstLineNo(), 0ULL);
        QCOMPARE(engineB.totalLines(), NB);
        QCOMPARE(engineB.firstLineNo(), 0ULL);

        // 冷层段全量逐行校验（驱逐按 16 行整批：A 冷层 69×16=1104 行/热层 96 行，
        // B 冷层 50×16=800 行/热层 100 行）：不串行（读不到对方行）、不缺失、不空白
        const QVector<ZzLogLine> coldA = engineA.getLines(0, 1104);
        QCOMPARE(coldA.size(), 1104);
        for (qsizetype i = 0; i < coldA.size(); ++i) {
            QCOMPARE(coldA[i].text, lineA(quint64(i)).text);
            QCOMPARE(coldA[i].attributes, lineA(quint64(i)).attributes);
        }
        const QVector<ZzLogLine> coldB = engineB.getLines(0, 800);
        QCOMPARE(coldB.size(), 800);
        for (qsizetype i = 0; i < coldB.size(); ++i)
            QCOMPARE(coldB[i].text, lineB(quint64(i)).text);

        // 跨冷/热边界的滑动窗口
        for (quint64 start = NA - 130; start + 60 <= NA; start += 13) {
            const QVector<ZzLogLine> w = engineA.getLines(start, 60);
            QCOMPARE(w.size(), 60);
            for (int j = 0; j < 60; ++j)
                QCOMPARE(w[j].text, lineA(start + quint64(j)).text);
        }

        // searchLines：各自只命中自己的行（SQL 按 session_id 过滤 + 映射反查翻译）
        QCOMPARE(engineA.searchLines(QStringLiteral("tokenAlpha"), 2000).size(), 1104);
        QCOMPARE(engineB.searchLines(QStringLiteral("tokenBravo"), 2000).size(), 800);
        QVERIFY(engineA.searchLines(QStringLiteral("tokenBravo"), 10).isEmpty());
        QVERIFY(engineB.searchLines(QStringLiteral("tokenAlpha"), 10).isEmpty());
        // 共享 token "row"：双方各自命中己方冷层行且行号-内容自洽
        const QVector<quint64> sharedA = engineA.searchLines(QStringLiteral("row"), 2000);
        QCOMPARE(sharedA.size(), 1104);
        for (const quint64 h : sharedA)
            QCOMPARE(engineA.getLines(h, 1).first().text, lineA(h).text);
        const QVector<quint64> sharedB = engineB.searchLines(QStringLiteral("row"), 2000);
        QCOMPARE(sharedB.size(), 800);
        for (const quint64 h : sharedB)
            QCOMPARE(engineB.getLines(h, 1).first().text, lineB(h).text);
    }
};

QTEST_GUILESS_MAIN(ZzLogEngineTest)
#include "ZzLogEngineTest.moc"
