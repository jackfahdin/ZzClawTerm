#include "ZzMmapBuffer.h"

#include <QTemporaryDir>
#include <QtTest>

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
};

QTEST_GUILESS_MAIN(ZzMmapBufferTest)
#include "ZzMmapBufferTest.moc"
