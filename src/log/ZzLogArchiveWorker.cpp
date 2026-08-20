#include "ZzLogArchiveWorker.h"

#include "ZzColdStorage.h"
#include "ZzMmapBuffer.h"

ZzLogArchiveWorker::ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                                       std::atomic<quint64> *warmBase,
                                       std::atomic<quint64> *warmCount,
                                       ZzColdStorage *cold,
                                       std::atomic<quint64> *coldBase,
                                       std::atomic<quint64> *coldFrontier, QObject *parent)
    : QObject(parent)
    , m_buffer(buffer)
    , m_lock(lock)
    , m_warmBase(warmBase)
    , m_warmCount(warmCount)
    , m_cold(cold)
    , m_coldBase(coldBase)
    , m_coldFrontier(coldFrontier)
{
}

void ZzLogArchiveWorker::archiveLines(const QVector<ZzLogLine> &lines)
{
    QString error;
    {
        QWriteLocker locker(m_lock);
        if (!m_buffer->appendLines(lines, &error)) {
            emit archiveFailed(error);
            return;
        }
        m_warmBase->store(m_buffer->firstLineId());
        m_warmCount->store(m_buffer->lineCount());
    }
    emit archiveCompleted();
    coldAdvance(false); // 温层批次写完后推进冷层（规格 §六；本线程直接调用）
}

void ZzLogArchiveWorker::coldAdvance(bool includePartial)
{
    if (!m_cold || m_coldFailed)
        return; // 不做 isOpen 预判：appendBlock 自带未打开失败路径，须走重试→降级让 coldFailed 可观察
    for (;;) {
        const quint64 warmEnd = m_warmBase->load() + m_warmCount->load();
        const quint64 available = warmEnd - m_coldCursor;
        if (available < ZzColdStorage::kMaxBlockLines
            && !(includePartial && available > 0))
            break;
        const quint64 batch = qMin(available, ZzColdStorage::kMaxBlockLines);
        QVector<ZzLogLine> lines;
        {
            QReadLocker locker(m_lock);
            lines = m_buffer->readLines(m_coldCursor, batch);
        }
        if (quint64(lines.size()) != batch)
            break; // 温层读回不完整（块损坏）：留待下一批，避免写入错位
        QString error;
        bool ok = false;
        for (int attempt = 0; attempt < 3 && !ok; ++attempt)
            ok = m_cold->appendBlock(lines, m_cold->frontier(), &error); // 库内全局空间连续追加
        if (!ok) {
            m_coldFailed = true;
            {
                QWriteLocker locker(m_lock);
                m_buffer->clearRetentionFloor(); // 回到 v0.1 容量丢弃（规格 §六失败隔离）
            }
            emit coldFailed(QStringLiteral("冷层写入失败（已重试 3 次）：%1").arg(error));
            return;
        }
        m_coldCursor += batch; // 游标只增不减：setRetentionFloor 入参天然单调，防 floor 回退造成续传空洞
        m_coldFrontier->store(m_coldCursor); // 引擎空间：本会话覆盖上界（读路径按此行号路由）
        m_coldBase->store(m_cold->baseLine()); // 库内全局空间原始值（读路径减 m_coldOffset 夹取）
        {
            QWriteLocker locker(m_lock);
            m_buffer->setRetentionFloor(m_coldCursor); // 温层截头 + 游标持久化
        }
    }
}

void ZzLogArchiveWorker::preloadAround(quint64 lineId)
{
    // 持写锁而非读锁：preload 会写解压缓存（QCache），须与调用线程的读路径互斥
    QWriteLocker locker(m_lock);
    m_buffer->preload(lineId);
}

void ZzLogArchiveWorker::preloadCold(quint64 globalLineId)
{
    if (m_cold && !m_coldFailed)
        m_cold->preload(globalLineId); // 冷层自带互斥锁，不占用温层读写锁
}

void ZzLogArchiveWorker::flush()
{
    {
        QWriteLocker locker(m_lock);
        m_buffer->flush();
    }
    coldAdvance(true); // 干净退出同步点：不足一块的尾批也写入冷层
}
