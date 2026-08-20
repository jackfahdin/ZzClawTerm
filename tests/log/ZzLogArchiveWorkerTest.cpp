#include "ZzLogArchiveWorker.h"

#include "ZzColdStorage.h"
#include "ZzMmapBuffer.h"

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

#include <atomic>

/**
 * @brief ZzLogArchiveWorker 温→冷推进单元测试：整块推进、尾批冲刷、冷层失败降级。
 *
 * 行号说明：worker 的 m_coldCursor 与温层一致使用温层局部行号空间；
 * 写入冷层时 firstLine 取 cold->frontier()（库内全局空间）。本测试单写入者，
 * 两个空间数值相同（偏移 0）。
 */
class ZzLogArchiveWorkerTest : public QObject
{
    Q_OBJECT
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("worker-line-%1").arg(i), QByteArray()};
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
    /// @brief 温层批次写完后 coldAdvance 只推进整块（1024 行）：3348 行 → 冷层 3072 行，
    ///        温层保留下限生效且跨界块保留。
    void archiveAdvancesColdInFullBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 1000000);
        QVERIFY(warm.open());
        ZzColdStorage::Config cc;
        cc.dbPath = dir.filePath(QStringLiteral("cold.db"));
        cc.sessionId = QStringLiteral("test-session");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0}, coldBase{0}, coldFrontier{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount,
                                  &cold, &coldBase, &coldFrontier);

        worker.archiveLines(makeLines(0, 2048));
        worker.archiveLines(makeLines(2048, 1300)); // 温层共 3348 行

        QCOMPARE(cold.frontier(), 3072ULL);          // 只推进了 3 个整块
        QCOMPARE(coldFrontier.load(), 3072ULL);      // 发布位同步
        QCOMPARE(coldBase.load(), 0ULL);
        // floor=3072 截头：整体行号 ≤3072 的温层整块被丢弃，跨界块保留
        QVERIFY(warm.firstLineId() <= 3072ULL);
        QVERIFY(warm.firstLineId() + warm.lineCount() > 3072ULL);
        // 冷层读回与写入一致
        const QVector<ZzLogLine> got = cold.readLines(1020, 8); // 跨块 0/1 边界
        QCOMPARE(got.size(), 8);
        QCOMPARE(got.first().text, line(1020).text);
        QCOMPARE(got.last().text, line(1027).text);
    }

    /// @brief flush() 冲刷不足一块的尾批进冷层（干净退出同步点）。
    void flushArchivesPartialTail()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 1000000);
        QVERIFY(warm.open());
        ZzColdStorage::Config cc;
        cc.dbPath = dir.filePath(QStringLiteral("cold.db"));
        cc.sessionId = QStringLiteral("test-session");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0}, coldBase{0}, coldFrontier{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount,
                                  &cold, &coldBase, &coldFrontier);

        worker.archiveLines(makeLines(0, 3348));
        QCOMPARE(cold.frontier(), 3072ULL); // 尾批 276 行未进冷层
        worker.flush();
        QCOMPARE(cold.frontier(), 3348ULL); // flush 后尾批已进冷层
        QCOMPARE(coldFrontier.load(), 3348ULL);
        QCOMPARE(cold.readLines(3347, 1).first().text, line(3347).text);
        QCOMPARE(warm.coldCursor(), 3348ULL); // 游标持久化到温层头
    }

    /// @brief 冷层写入失败：重试 3 次后发射 coldFailed（门闩只发一次），
    ///        温层清除 floor 回到 v0.1 maxLines 丢弃。
    void coldFailureEmitsColdFailedAndRevertsWarmDropping()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        // 短行单块约 1700 行；maxLines 2000 → v0.1 恢复后超限整块丢弃可观察
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 2000);
        QVERIFY(warm.open());
        ZzColdStorage::Config cc;
        cc.dbPath = dir.filePath(QStringLiteral("cold.db"));
        cc.sessionId = QStringLiteral("test-session");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0}, coldBase{0}, coldFrontier{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount,
                                  &cold, &coldBase, &coldFrontier);
        QSignalSpy coldFailedSpy(&worker, &ZzLogArchiveWorker::coldFailed);
        QSignalSpy archiveOkSpy(&worker, &ZzLogArchiveWorker::archiveCompleted);

        cold.close(); // 制造冷层写失败（appendBlock 立即失败）
        worker.archiveLines(makeLines(0, 1500));
        QCOMPARE(archiveOkSpy.count(), 1);    // 温层写入成功
        QCOMPARE(coldFailedSpy.count(), 1);   // 重试 3 次后降级
        QVERIFY(!coldFailedSpy.first().first().toString().isEmpty());

        worker.archiveLines(makeLines(1500, 1500)); // 门闩：不再重复发射
        QCOMPARE(coldFailedSpy.count(), 1);

        // floor 已清除：继续写超 maxLines 后 v0.1 容量丢弃生效
        worker.archiveLines(makeLines(3000, 1500));
        QVERIFY(warm.lineCount() <= 2000ULL + 1700ULL); // 至多一个块的余量
        QVERIFY(warm.firstLineId() > 0ULL);
    }

    /// @brief 无冷层（cold == nullptr）：纯 v0.1 行为，archiveLines 不触碰冷层路径。
    void nullColdKeepsV01Behavior()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 1000000);
        QVERIFY(warm.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount); // 不传冷层
        QSignalSpy coldFailedSpy(&worker, &ZzLogArchiveWorker::coldFailed);
        worker.archiveLines(makeLines(0, 3000));
        worker.flush();
        QCOMPARE(warmCount.load(), 3000ULL);
        QCOMPARE(coldFailedSpy.count(), 0);
        QCOMPARE(warm.coldCursor(), 0ULL); // floor 从未设置
    }
};

QTEST_GUILESS_MAIN(ZzLogArchiveWorkerTest)
#include "ZzLogArchiveWorkerTest.moc"
