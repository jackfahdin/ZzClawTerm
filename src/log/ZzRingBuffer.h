#pragma once

#include "ZzLogLine.h"

#include <QVector>

/**
 * @brief 热层内存环形缓冲区（规格 §5.2 热层）。
 *
 * 固定容量、固定内存占用，append / at / takeOldest 均为 O(1) 或 O(n) 批量，
 * 完整保留字符属性；写满后继续追加将驱逐最老行（由上层归档到温层）。
 */
class ZzRingBuffer
{
public:
    explicit ZzRingBuffer(qsizetype capacity = 10000);

    qsizetype capacity() const { return m_capacity; }
    qsizetype count() const { return m_count; }
    bool isFull() const { return m_count == m_capacity; }

    /**
     * @brief 追加一行；已满时驱逐最老行。
     * @param line 待追加行。
     * @param evicted 若非空且发生驱逐，写入被驱逐的最老行。
     * @return 发生驱逐返回 true。
     */
    bool append(const ZzLogLine &line, ZzLogLine *evicted = nullptr);

    /**
     * @brief 按序取走最老的至多 n 行（批量归档用）。
     * @param n 期望取走的行数。
     * @return 实际取走的行（可能少于 n）。
     */
    QVector<ZzLogLine> takeOldest(qsizetype n);

    /**
     * @brief 读取逻辑下标 index 处的行（0 为当前最老行）。
     * @note 调用方须保证 0 <= index < count()，否则触发断言。
     */
    const ZzLogLine &at(qsizetype index) const;

    void clear();

private:
    qsizetype physical(qsizetype index) const { return (m_head + index) % m_capacity; }

    QVector<ZzLogLine> m_lines;
    qsizetype m_capacity;
    qsizetype m_head = 0; ///< 最老行的物理下标
    qsizetype m_count = 0;
};
