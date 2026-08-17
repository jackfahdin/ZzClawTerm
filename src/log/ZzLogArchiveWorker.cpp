#include "ZzLogArchiveWorker.h"

#include "ZzMmapBuffer.h"

ZzLogArchiveWorker::ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                                       std::atomic<quint64> *warmBase,
                                       std::atomic<quint64> *warmCount, QObject *parent)
    : QObject(parent)
    , m_buffer(buffer)
    , m_lock(lock)
    , m_warmBase(warmBase)
    , m_warmCount(warmCount)
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
}

void ZzLogArchiveWorker::preloadAround(quint64 lineId)
{
    QReadLocker locker(m_lock);
    m_buffer->preload(lineId);
}

void ZzLogArchiveWorker::flush()
{
    QWriteLocker locker(m_lock);
    m_buffer->flush();
}
