#include "ZzRingBuffer.h"

#include <QtTest>

/**
 * @brief ZzRingBuffer 热层环形缓冲单元测试。
 */
class ZzRingBufferTest : public QObject
{
    Q_OBJECT
    /// @brief 构造带属性负载的测试行。
    static ZzLogLine makeLine(const QString &text)
    {
        return {text, QByteArray("attr:") + text.toUtf8()};
    }

private slots:
    /// @brief 顺序追加后可按逻辑下标原序读回，属性完整保留。
    void appendAndReadInOrder()
    {
        ZzRingBuffer ring(8);
        for (int i = 0; i < 5; ++i)
            QVERIFY(!ring.append(makeLine(QStringLiteral("line%1").arg(i))));
        QCOMPARE(ring.count(), 5);
        for (int i = 0; i < 5; ++i)
            QCOMPARE(ring.at(i).text, QStringLiteral("line%1").arg(i));
        QCOMPARE(ring.at(3).attributes, QByteArray("attr:line3"));
    }

    /// @brief 写满后追加驱逐最老行，并通过出参返回被驱逐行。
    void overflowEvictsOldest()
    {
        ZzRingBuffer ring(4);
        for (int i = 0; i < 4; ++i)
            QVERIFY(!ring.append(makeLine(QStringLiteral("line%1").arg(i))));
        QVERIFY(ring.isFull());

        ZzLogLine evicted;
        QVERIFY(ring.append(makeLine(QStringLiteral("line4")), &evicted));
        QCOMPARE(evicted.text, QStringLiteral("line0"));
        QCOMPARE(ring.count(), 4);
        QCOMPARE(ring.at(0).text, QStringLiteral("line1"));
        QCOMPARE(ring.at(3).text, QStringLiteral("line4"));
    }

    /// @brief takeOldest 按序批量取走最老行（归档路径）。
    void takeOldestReturnsBatchInOrder()
    {
        ZzRingBuffer ring(8);
        for (int i = 0; i < 5; ++i)
            ring.append(makeLine(QStringLiteral("line%1").arg(i)));
        QVector<ZzLogLine> batch = ring.takeOldest(3);
        QCOMPARE(batch.size(), 3);
        QCOMPARE(batch.first().text, QStringLiteral("line0"));
        QCOMPARE(batch.last().text, QStringLiteral("line2"));
        QCOMPARE(ring.count(), 2);
        QCOMPARE(ring.at(0).text, QStringLiteral("line3"));
    }

    /// @brief takeOldest 超过现存行数时按现存行数截取。
    void takeOldestClampsToCount()
    {
        ZzRingBuffer ring(8);
        ring.append(makeLine(QStringLiteral("only")));
        QCOMPARE(ring.takeOldest(10).size(), 1);
        QCOMPARE(ring.count(), 0);
    }

    /// @brief 溢出驱逐不破坏多字节文本与二进制属性。
    void attributesPreservedOnEviction()
    {
        ZzRingBuffer ring(2);
        ring.append({QStringLiteral("中文行"), QByteArray("\x01\x02", 2)});
        ring.append(makeLine("b"));
        ZzLogLine evicted;
        ring.append(makeLine("c"), &evicted);
        QCOMPARE(evicted.text, QStringLiteral("中文行"));
        QCOMPARE(evicted.attributes, QByteArray("\x01\x02", 2));
    }
};

QTEST_GUILESS_MAIN(ZzRingBufferTest)
#include "ZzRingBufferTest.moc"
