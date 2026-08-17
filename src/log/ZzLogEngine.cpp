#include "ZzLogEngine.h"

#include "ZzLogArchiveWorker.h"
#include "ZzMmapBuffer.h"

#include <QMetaObject>

ZzLogEngine::ZzLogEngine(const Config &config, QObject *parent)
    : QObject(parent)
    , m_config(config)
    , m_hot(qMax<qsizetype>(config.hotCapacity, 1))
{
}

ZzLogEngine::~ZzLogEngine()
{
    flush(); // 让已排队批次落盘
    if (m_workerThread.isRunning()) {
        m_workerThread.quit();
        m_workerThread.wait();
    }
    delete m_worker; // 队列已排空、线程已停止，直接删除安全
    m_worker = nullptr;
}

bool ZzLogEngine::open()
{
    qRegisterMetaType<QVector<ZzLogLine>>("QVector<ZzLogLine>");
    if (m_config.warmFilePath.isEmpty()) {
        m_memoryOnly = true;
        return true;
    }
    m_warm = std::make_unique<ZzMmapBuffer>(m_config.warmFilePath, m_config.warmMaxLines);
    if (!m_warm->open()) {
        const QString path = m_config.warmFilePath;
        m_warm.reset();
        m_memoryOnly = true;
        emit degradedToMemoryOnly(
            QStringLiteral("温层文件打开失败，降级为纯内存模式：%1").arg(path));
        return true; // 降级不影响终端交互（规格 §八）
    }
    // 恢复既有温层数据（引擎重开场景）
    m_warmBase.store(m_warm->firstLineId());
    m_warmCount.store(m_warm->lineCount());
    {
        QMutexLocker locker(&m_hotMutex);
        m_hotBase = m_warmBase.load() + m_warmCount.load();
    }
    m_worker = new ZzLogArchiveWorker(m_warm.get(), &m_warmLock, &m_warmBase, &m_warmCount);
    m_worker->moveToThread(&m_workerThread);
    connect(m_worker, &ZzLogArchiveWorker::archiveCompleted,
            this, &ZzLogEngine::archiveFinished);
    connect(m_worker, &ZzLogArchiveWorker::archiveFailed, this,
            [this](const QString &message) {
                m_memoryOnly = true; // 后续批次直接丢弃，不再尝试写盘
                emit degradedToMemoryOnly(message);
            });
    m_workerThread.start();
    return true;
}

void ZzLogEngine::appendLine(const ZzLogLine &line)
{
    QVector<ZzLogLine> batch;
    {
        QMutexLocker locker(&m_hotMutex);
        if (m_hot.isFull()) {
            batch = m_hot.takeOldest(qMin(m_config.archiveBatchSize, m_hot.count()));
            m_hotBase += quint64(batch.size());
        }
        m_hot.append(line);
    }
    if (batch.isEmpty() || m_memoryOnly || !m_worker)
        return; // 纯内存模式：最老批次直接丢弃
    QMetaObject::invokeMethod(m_worker, "archiveLines", Qt::QueuedConnection,
                              Q_ARG(QVector<ZzLogLine>, batch));
}

bool ZzLogEngine::getLine(quint64 lineNo, ZzLogLine *out) const
{
    if (!out)
        return false;
    const QVector<ZzLogLine> lines = getLines(lineNo, 1);
    if (lines.isEmpty())
        return false;
    *out = lines.first();
    return true;
}

QVector<ZzLogLine> ZzLogEngine::getLines(quint64 startLine, quint64 count) const
{
    QVector<ZzLogLine> out;
    if (count == 0)
        return out;
    out.reserve(qsizetype(qMin<quint64>(count, 10000)));

    quint64 id = startLine;
    quint64 remaining = count;

    // 1) 温层区间
    if (m_warm) {
        const quint64 warmEnd = m_warmBase.load() + m_warmCount.load();
        if (id < warmEnd) {
            QReadLocker locker(&m_warmLock);
            out = m_warm->readLines(id, remaining);
            const quint64 got = quint64(out.size());
            id += got;
            remaining -= got;
        }
    }
    // 2) 热层区间（in-flight 归档批次形成的小空洞直接跳过，属瞬时状态）
    if (remaining > 0) {
        QMutexLocker locker(&m_hotMutex);
        const quint64 hotBase = m_hotBase;
        const quint64 hotEnd = hotBase + quint64(m_hot.count());
        if (id < hotBase)
            id = hotBase;
        while (remaining > 0 && id < hotEnd) {
            out.append(m_hot.at(qsizetype(id - hotBase)));
            ++id;
            --remaining;
        }
    }
    return out;
}

quint64 ZzLogEngine::totalLines() const
{
    QMutexLocker locker(&m_hotMutex);
    return m_warmCount.load() + quint64(m_hot.count());
}

quint64 ZzLogEngine::firstLineNo() const
{
    if (m_warm)
        return m_warmBase.load();
    QMutexLocker locker(&m_hotMutex);
    return m_hotBase;
}

void ZzLogEngine::preload(quint64 lineNo)
{
    if (!m_worker)
        return;
    QMetaObject::invokeMethod(m_worker, "preloadAround", Qt::QueuedConnection,
                              Q_ARG(quint64, lineNo));
}

void ZzLogEngine::flush()
{
    if (!m_worker || !m_workerThread.isRunning())
        return;
    // 队列先进先出：先前排队的 archiveLines 先于 flush 执行，返回即归档完成
    QMetaObject::invokeMethod(m_worker, "flush", Qt::BlockingQueuedConnection);
}
