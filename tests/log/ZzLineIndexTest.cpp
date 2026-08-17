#include "ZzLineIndex.h"

#include <QtTest>

/**
 * @brief ZzLineIndex 分块行偏移索引单元测试。
 */
class ZzLineIndexTest : public QObject
{
    Q_OBJECT
private slots:
    /// @brief 默认步长为 1024（规格 §5.2）。
    void defaultStrideIs1024()
    {
        ZzLineIndex index;
        QCOMPARE(index.stride(), 1024ULL);
    }

    /// @brief 仅记录步长整数倍的行。
    void recordsOnlyStrideMultiples()
    {
        ZzLineIndex index(4);
        for (quint64 i = 0; i < 10; ++i)
            index.recordLine(i, 1000 + i, i * 10);
        QCOMPARE(index.entryCount(), 3); // 行 0、4、8
    }

    /// @brief 定位返回不大于目标行的最近条目。
    void locateReturnsNearestLowerEntry()
    {
        ZzLineIndex index(4);
        for (quint64 i = 0; i < 10; ++i)
            index.recordLine(i, 1000 + i, i * 10);

        ZzLineIndex::Entry e;
        QVERIFY(index.locate(6, &e));
        QCOMPARE(e.lineId, 4ULL);
        QCOMPARE(e.blockFirstLineId, 1004ULL);
        QCOMPARE(e.offset, 40ULL);

        QVERIFY(index.locate(0, &e));
        QCOMPARE(e.lineId, 0ULL);
    }

    /// @brief 空索引定位失败。
    void locateOnEmptyIndexFails()
    {
        ZzLineIndex index;
        ZzLineIndex::Entry e;
        QVERIFY(!index.locate(0, &e));
    }
};

QTEST_GUILESS_MAIN(ZzLineIndexTest)
#include "ZzLineIndexTest.moc"
