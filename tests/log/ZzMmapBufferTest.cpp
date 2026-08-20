#include "ZzMmapBuffer.h"

#include <QTemporaryDir>
#include <QtEndian>
#include <QtTest>

#include <cstring>

/**
 * @brief ZzMmapBuffer 温层存储单元测试。
 */
class ZzMmapBufferTest : public QObject
{
    Q_OBJECT
    /// @brief 构造带中文与属性负载的测试行（平均每行约 40 字节）。
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("第 %1 行 the quick brown fox").arg(i),
                QByteArray("A") + QByteArray::number(qint64(i))};
    }

private slots:
    /// @brief 压缩解压一致性：5000 行（约 200KB，跨多个 64KB 块）随机抽查逐字节相等。
    void appendReadRoundtrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());

        QVector<ZzLogLine> written;
        for (quint64 i = 0; i < 5000; ++i)
            written.append(line(i));
        QVERIFY(buf.appendLines(written));
        QCOMPARE(buf.lineCount(), 5000ULL);
        QCOMPARE(buf.firstLineId(), 0ULL);

        for (quint64 i : {0ULL, 1ULL, 1024ULL, 2500ULL, 4096ULL, 4999ULL}) {
            QVector<ZzLogLine> got = buf.readLines(i, 1);
            QCOMPARE(got.size(), 1);
            QCOMPARE(got.first().text, line(i).text);
            QCOMPARE(got.first().attributes, line(i).attributes);
        }
    }

    /// @brief 分批追加后顺序全量读回与写入序列完全一致（滚动读取等价性的温层基础）。
    void sequentialReadMatchesWrite()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());
        for (quint64 batch = 0; batch < 10; ++batch) {
            QVector<ZzLogLine> lines;
            for (quint64 i = 0; i < 1000; ++i)
                lines.append(line(batch * 1000 + i));
            QVERIFY(buf.appendLines(lines));
        }
        QVector<ZzLogLine> all = buf.readLines(0, 10000);
        QCOMPARE(all.size(), 10000);
        for (int i = 0; i < 10000; ++i)
            QCOMPARE(all[i].text, line(quint64(i)).text);
    }

    /// @brief 关闭重开后数据完整，且可继续向后追加。
    void reopenPreservesData()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        {
            ZzMmapBuffer buf(path);
            QVERIFY(buf.open());
            QVERIFY(buf.appendLines({line(0), line(1), line(2)}));
            buf.flush();
            buf.close();
        }
        ZzMmapBuffer buf(path);
        QVERIFY(buf.open());
        QCOMPARE(buf.lineCount(), 3ULL);
        QCOMPARE(buf.readLines(0, 3).size(), 3);
        QVERIFY(buf.appendLines({line(3)}));
        QCOMPARE(buf.lineCount(), 4ULL);
        QCOMPARE(buf.readLines(3, 1).first().text, line(3).text);
    }

    /// @brief 越界读取按实际可得行数返回。
    void readOutOfRangeReturnsLess()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());
        QVERIFY(buf.appendLines({line(0), line(1)}));
        QCOMPARE(buf.readLines(0, 100).size(), 2);
        QVERIFY(buf.readLines(5, 1).isEmpty());
    }

    /// @brief 崩溃安全：尾随幽灵块（仅 magic 落盘、其余字段为零）重开时被忽略，行 ID 不回退。
    void reopenIgnoresGhostBlock()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        {
            ZzMmapBuffer buf(path);
            QVERIFY(buf.open());
            QVERIFY(buf.appendLines({line(0), line(1), line(2)}));
            buf.flush();
            buf.close();
        }
        // 模拟断电按 4KB 页落盘：在最后一个合法块之后写入仅含块魔数的幽灵块头
        // （magic 所在页已落盘，lineCount/uncompSize 等其余字段未落盘而为零）。
        {
            QFile f(path);
            QVERIFY(f.open(QIODevice::ReadWrite));
            QVERIFY(f.seek(4096)); // 文件头一页之后是首个块
            const QByteArray hdr = f.read(24);
            QCOMPARE(hdr.size(), 24);
            quint32 compSize = 0;
            std::memcpy(&compSize, hdr.constData() + 20, 4);
            compSize = qFromLittleEndian(compSize);
            QVERIFY(f.seek(4096 + 24 + compSize)); // 幽灵块紧跟最后一个合法块
            const quint32 magic = qToLittleEndian<quint32>(0x5A5A424B);
            QCOMPARE(f.write(reinterpret_cast<const char *>(&magic), 4), qint64(4));
            f.flush();
            f.close();
        }
        ZzMmapBuffer buf(path);
        QVERIFY(buf.open());
        QCOMPARE(buf.lineCount(), 3ULL);         // 幽灵块被忽略
        QCOMPARE(buf.readLines(0, 3).size(), 3); // 既有数据完好
        QVERIFY(buf.appendLines({line(3)}));     // 行 ID 从 3 继续分配，不回退到 0
        QCOMPARE(buf.lineCount(), 4ULL);
        QCOMPARE(buf.readLines(3, 1).first().text, line(3).text);
    }
    /// @brief 超出 maxLines 时按整块粒度丢弃最老数据，剩余数据完整连续。
    void capacityDropsOldestBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")), /*maxLines=*/2000);
        QVERIFY(buf.open());
        QVector<ZzLogLine> lines;
        for (quint64 i = 0; i < 5000; ++i)
            lines.append(line(i));
        QVERIFY(buf.appendLines(lines));

        QVERIFY(buf.lineCount() <= 2000ULL);
        QVERIFY(buf.lineCount() > 0ULL);
        QVERIFY(buf.firstLineId() > 0ULL);
        // 剩余数据完整且与写入序列逐字节一致
        QVector<ZzLogLine> rest = buf.readLines(buf.firstLineId(), buf.lineCount());
        QCOMPARE(quint64(rest.size()), buf.lineCount());
        for (int i = 0; i < rest.size(); ++i)
            QCOMPARE(rest[i].text, line(buf.firstLineId() + quint64(i)).text);
        // 已丢弃的行读不到（区间被裁剪后为空）
        QVERIFY(buf.readLines(0, 1).isEmpty());
    }

    /// @brief 裁剪状态持久化：重开后最老行不复活，且可继续追加。
    void dropSurvivesReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        quint64 firstAfterDrop = 0;
        {
            ZzMmapBuffer buf(path, /*maxLines=*/2000);
            QVERIFY(buf.open());
            QVector<ZzLogLine> lines;
            for (quint64 i = 0; i < 5000; ++i)
                lines.append(line(i));
            QVERIFY(buf.appendLines(lines));
            buf.flush();
            firstAfterDrop = buf.firstLineId();
            buf.close();
        }
        ZzMmapBuffer buf(path, 2000);
        QVERIFY(buf.open());
        QCOMPARE(buf.firstLineId(), firstAfterDrop);
        QVERIFY(buf.appendLines({line(5000)}));
        QCOMPARE(buf.readLines(5000, 1).first().text, line(5000).text);
    }

    /// @brief 保留下限：只丢弃整体行号 ≤ floor 的最老块，跨界块保留；游标可读回。
    void retentionFloorDropsOnlyCoveredBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")), 1000000); // 大 maxLines 排除 v0.1 丢弃
        QVERIFY(buf.open());
        // 每行约 1KB，单块（64KB）约 60 行；写 300 行形成多块
        const QString payload(1000, QLatin1Char('x'));
        QVector<ZzLogLine> batch;
        batch.reserve(300);
        for (int i = 0; i < 300; ++i)
            batch.append({payload + QString::number(i), QByteArray()});
        QVERIFY(buf.appendLines(batch));
        QCOMPARE(buf.firstLineId(), 0ULL);
        QCOMPARE(buf.lineCount(), 300ULL);
        QCOMPARE(buf.coldCursor(), 0ULL); // 未设置 floor 时游标为 0

        buf.setRetentionFloor(120); // 行 0..119 已被冷层覆盖
        const quint64 first = buf.firstLineId();
        QVERIFY(first > 0ULL);    // 至少丢了一块
        QVERIFY(first <= 120ULL); // 跨界块必须保留：首存活块覆盖 floor
        QCOMPARE(buf.lineCount(), 300ULL - first);
        QCOMPARE(buf.coldCursor(), 120ULL);
        // 幸存行内容正确
        QCOMPARE(buf.readLines(first, 1).first().text,
                 payload + QString::number(qint64(first)));
        QCOMPARE(buf.readLines(299, 1).first().text, payload + QStringLiteral("299"));
    }

    /// @brief floor 游标与丢弃进度跨重开持久（崩溃恢复依据）。
    void retentionFloorPersistsAcrossReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        const QString payload(1000, QLatin1Char('x'));
        quint64 firstBeforeClose = 0;
        {
            ZzMmapBuffer buf(path, 1000000);
            QVERIFY(buf.open());
            QVector<ZzLogLine> batch;
            for (int i = 0; i < 300; ++i)
                batch.append({payload + QString::number(i), QByteArray()});
            QVERIFY(buf.appendLines(batch));
            buf.setRetentionFloor(120);
            firstBeforeClose = buf.firstLineId();
            buf.flush();
            buf.close();
        }
        ZzMmapBuffer buf(path, 1000000);
        QVERIFY(buf.open());
        QCOMPARE(buf.coldCursor(), 120ULL);           // 游标持久
        QCOMPARE(buf.firstLineId(), firstBeforeClose); // 已丢块不复活（skipBlocks 持久）
        QVERIFY(buf.readLines(firstBeforeClose, 1).first().text
                == payload + QString::number(qint64(firstBeforeClose))); // 幸存首行内容正确
        QVERIFY(buf.readLines(0, 1).isEmpty()); // v0.1 语义：区间完全落在已丢弃范围时返回空
    }

    /// @brief 清除 floor 后恢复 v0.1 纯 maxLines 丢弃（冷层降级路径）。
    void clearRetentionFloorRestoresV01Dropping()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString payload(1000, QLatin1Char('x'));
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")), 150); // maxLines 150
        QVERIFY(buf.open());
        QVector<ZzLogLine> batch;
        for (int i = 0; i < 300; ++i)
            batch.append({payload + QString::number(i), QByteArray()});
        QVERIFY(buf.appendLines(batch)); // 未设 floor：v0.1 丢弃，lineCount ≤ 150
        QVERIFY(buf.lineCount() <= 150ULL);
        const quint64 firstAfterV01 = buf.firstLineId();

        buf.setRetentionFloor(firstAfterV01 + 30); // floor 模式：再丢一些整块
        const quint64 firstWithFloor = buf.firstLineId();
        QVERIFY(firstWithFloor >= firstAfterV01);
        buf.clearRetentionFloor(); // 降级：回到 v0.1

        QVector<ZzLogLine> more;
        for (int i = 300; i < 420; ++i)
            more.append({payload + QString::number(i), QByteArray()});
        QVERIFY(buf.appendLines(more)); // 触发 v0.1 maxLines 丢弃
        QVERIFY(buf.lineCount() <= 150ULL);
        QVERIFY(buf.firstLineId() > firstWithFloor); // 按 maxLines 继续前移
    }

    /// @brief 预读后读取结果不变（预读只影响缓存，不影响语义）。
    void preloadKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());
        QVector<ZzLogLine> lines;
        for (quint64 i = 0; i < 3000; ++i)
            lines.append(line(i));
        QVERIFY(buf.appendLines(lines));
        buf.preload(1500);
        buf.preload(0); // 边界：首块
        QVector<ZzLogLine> got = buf.readLines(1490, 30);
        QCOMPARE(got.size(), 30);
        for (int i = 0; i < 30; ++i)
            QCOMPARE(got[i].text, line(1490 + quint64(i)).text);
    }
};

QTEST_GUILESS_MAIN(ZzMmapBufferTest)
#include "ZzMmapBufferTest.moc"
