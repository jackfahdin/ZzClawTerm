#pragma once

#include <QVector>
#include <QtGlobal>

/**
 * @brief 分块行偏移索引：每 stride 行记录一条（块首行 ID + 块内未压缩偏移）。
 *
 * 定位时取不大于目标行的最近条目，随后由调用方在块内做小范围扫描；
 * 借鉴 klogg 行索引思路：stride=1024 时 1,000 万行索引仅约 80KB。
 */
class ZzLineIndex
{
public:
    /**
     * @brief 一条索引条目。
     */
    struct Entry {
        quint64 lineId = 0;           ///< 行 ID（绝对、单调递增）
        quint64 blockFirstLineId = 0; ///< 所在块的首行 ID
        quint64 offset = 0;           ///< 行首在块未压缩数据中的字节偏移
    };

    explicit ZzLineIndex(quint64 stride = 1024);

    quint64 stride() const { return m_stride; }
    qsizetype entryCount() const { return m_entries.size(); }

    /**
     * @brief 记录一行位置；仅当 lineId 为 stride 的整数倍时真正写入。
     * @param lineId 行 ID。
     * @param blockFirstLineId 该行所在块的首行 ID。
     * @param offset 行首在块未压缩数据中的字节偏移。
     */
    void recordLine(quint64 lineId, quint64 blockFirstLineId, quint64 offset);

    /**
     * @brief 定位不大于 lineId 的最近条目。
     * @param lineId 目标行 ID。
     * @param out 输出条目，不可为空。
     * @return 索引为空或 out 为空返回 false。
     */
    bool locate(quint64 lineId, Entry *out) const;

    void clear();

private:
    quint64 m_stride;
    QVector<Entry> m_entries;
};
