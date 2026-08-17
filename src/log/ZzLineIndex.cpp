#include "ZzLineIndex.h"

ZzLineIndex::ZzLineIndex(quint64 stride)
    : m_stride(stride)
{
}

void ZzLineIndex::recordLine(quint64 lineId, quint64 blockFirstLineId, quint64 offset)
{
    if (m_stride == 0 || lineId % m_stride != 0)
        return;
    m_entries.append({lineId, blockFirstLineId, offset});
}

bool ZzLineIndex::locate(quint64 lineId, Entry *out) const
{
    if (!out || m_entries.isEmpty())
        return false;
    // 条目按 lineId 严格递增，二分查找最后一个 lineId <= 目标 的条目
    qsizetype lo = 0;
    qsizetype hi = m_entries.size() - 1;
    qsizetype best = 0;
    while (lo <= hi) {
        const qsizetype mid = (lo + hi) / 2;
        if (m_entries[mid].lineId <= lineId) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    *out = m_entries[best];
    return true;
}

void ZzLineIndex::clear()
{
    m_entries.clear();
}
