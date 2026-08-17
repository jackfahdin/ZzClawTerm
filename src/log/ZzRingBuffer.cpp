#include "ZzRingBuffer.h"

ZzRingBuffer::ZzRingBuffer(qsizetype capacity)
    : m_lines(capacity > 0 ? capacity : 1)
    , m_capacity(capacity > 0 ? capacity : 1)
{
}

bool ZzRingBuffer::append(const ZzLogLine &line, ZzLogLine *evicted)
{
    if (isFull()) {
        if (evicted)
            *evicted = m_lines[m_head];
        m_lines[m_head] = line;
        m_head = (m_head + 1) % m_capacity;
        return true;
    }
    m_lines[physical(m_count)] = line;
    ++m_count;
    return false;
}

QVector<ZzLogLine> ZzRingBuffer::takeOldest(qsizetype n)
{
    n = qMin(n, m_count);
    QVector<ZzLogLine> out;
    out.reserve(n);
    for (qsizetype i = 0; i < n; ++i)
        out.append(m_lines[physical(i)]);
    m_head = (m_head + n) % m_capacity;
    m_count -= n;
    return out;
}

const ZzLogLine &ZzRingBuffer::at(qsizetype index) const
{
    Q_ASSERT(index >= 0 && index < m_count);
    return m_lines[physical(index)];
}

void ZzRingBuffer::clear()
{
    m_head = 0;
    m_count = 0;
}
