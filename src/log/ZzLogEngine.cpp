#include "ZzLogEngine.h"

#include "ZzColdStorage.h"
#include "ZzLogArchiveWorker.h"
#include "ZzMmapBuffer.h"

#include <QFile>
#include <QMetaObject>

ZzLogEngine::ZzLogEngine(const Config &config, QObject *parent)
    : QObject(parent)
    , m_config(config)
    , m_hot(qMax<qsizetype>(config.hotCapacity, 1))
{
}

ZzLogEngine::~ZzLogEngine()
{
    flush(); // 让已排队批次落盘（含温→冷推进与尾批）
    if (m_workerThread.isRunning()) {
        m_workerThread.quit();
        m_workerThread.wait();
    }
    delete m_worker; // 队列已排空、线程已停止，直接删除安全
    m_worker = nullptr;
    // 干净退出：冷层启用且未降级时温层数据已全量落冷层，删除温层文件
    // （冷层为唯一持久真相，规格 §六）；降级后保留温层文件
    const bool removeWarm = isColdEnabled();
    if (m_warm) {
        m_warm->close();
        m_warm.reset();
    }
    if (removeWarm)
        QFile::remove(m_config.warmFilePath);
    if (m_cold)
        m_cold->close();
}

bool ZzLogEngine::open()
{
    qRegisterMetaType<QVector<ZzLogLine>>("QVector<ZzLogLine>");
    if (m_config.warmFilePath.isEmpty()) {
        m_memoryOnly = true;
        return true; // 纯内存模式：无归档线程可推进，冷层配置忽略
    }
    if (!m_config.coldDbPath.isEmpty()) {
        ZzColdStorage::Config coldConfig;
        coldConfig.dbPath = m_config.coldDbPath;
        coldConfig.sessionId = m_config.sessionId;
        coldConfig.maxBytes = m_config.coldMaxBytes;
        coldConfig.maxAgeDays = m_config.coldMaxAgeDays;
        m_cold = std::make_unique<ZzColdStorage>(coldConfig);
        QString error;
        if (!m_cold->open(&error)) {
            m_cold.reset();
            m_coldDegraded = true;
            // 冷层不可用：残留温层无续传去向，按 v0.1 崩溃语义删除后全新开始
            QFile::remove(m_config.warmFilePath);
            emit degradedToWarmOnly(
                QStringLiteral("冷层库打开失败，降级为温层模式：%1").arg(error));
        } else {
            recoverResidualWarm(); // 崩溃恢复（失败时内部降级并发射信号）
            if (isColdEnabled()) {
                // 本会话行号从 0 起（引擎空间）；库内落点由映射表逐块记录，
                // 不做固定平移假设（续传进的残留行属上一会话，不进本会话映射，不可见）
                m_coldBase.store(m_cold->baseLine());
                m_coldFrontier.store(0); // 引擎空间：本会话尚未有任何行落冷层
            }
        }
    }
    m_warm = std::make_unique<ZzMmapBuffer>(m_config.warmFilePath, m_config.warmMaxLines);
    if (!m_warm->open()) {
        const QString path = m_config.warmFilePath;
        m_warm.reset();
        m_memoryOnly = true;
        m_cold.reset(); // 无温层即无归档线程，冷层对本会话无意义
        m_coldDegraded = false;
        emit degradedToMemoryOnly(
            QStringLiteral("温层文件打开失败，降级为纯内存模式：%1").arg(path));
        return true; // 降级不影响终端交互（规格 §八）
    }
    // 恢复既有温层数据（引擎重开场景；冷层模式下残留已在上方处理，此处读回为空）
    m_warmBase.store(m_warm->firstLineId());
    m_warmCount.store(m_warm->lineCount());
    {
        QMutexLocker locker(&m_hotMutex);
        m_hotBase = m_warmBase.load() + m_warmCount.load();
    }
    m_worker = new ZzLogArchiveWorker(m_warm.get(), &m_warmLock, &m_warmBase, &m_warmCount,
                                      m_cold.get(), &m_coldBase, &m_coldFrontier,
                                      &m_coldMap, &m_coldMapMutex);
    m_worker->moveToThread(&m_workerThread);
    connect(m_worker, &ZzLogArchiveWorker::archiveCompleted,
            this, &ZzLogEngine::archiveFinished);
    connect(m_worker, &ZzLogArchiveWorker::archiveFailed, this,
            [this](const QString &message) {
                // 门闩：exchange 返回旧值，已降级则直接返回，避免队列中后续
                // 失败批次逐批重复发射 degradedToMemoryOnly
                if (m_memoryOnly.exchange(true))
                    return;
                emit degradedToMemoryOnly(message);
            });
    connect(m_worker, &ZzLogArchiveWorker::coldFailed, this,
            [this](const QString &message) {
                // 门闩同上：冷层降级为 v0.1 温层模式（与温层降级对称），温层文件保留
                if (m_coldDegraded.exchange(true))
                    return;
                emit degradedToWarmOnly(message);
            });
    m_workerThread.start();
    return true;
}

void ZzLogEngine::recoverResidualWarm()
{
    // 异常退出残留的温层文件：按文件头持久化的 coldCursor 续传进冷层后删除（规格 §六）。
    // 残留温层不能复用为活温层——新会话显示层行号从 0 起，复用会让读回命中
    // 上一会话的行（错行，v0.1 enableScrollback 删文件逻辑同源）。
    if (!QFile::exists(m_config.warmFilePath))
        return;
    ZzMmapBuffer residual(m_config.warmFilePath, m_config.warmMaxLines);
    if (!residual.open()) {
        // 打不开：无法续传，按 v0.1 崩溃语义删除残留（冷层仍保有已归档部分）
        QFile::remove(m_config.warmFilePath);
        return;
    }
    // 游标之前的行已被冷层覆盖（floor 整块丢弃不变式保证）；首存活块可能跨界
    quint64 cursor = qMax(residual.coldCursor(), residual.firstLineId());
    const quint64 warmEnd = residual.firstLineId() + residual.lineCount();
    bool ok = true;
    while (cursor < warmEnd && ok) {
        const quint64 batch = qMin(warmEnd - cursor, ZzColdStorage::kMaxBlockLines);
        const QVector<ZzLogLine> lines = residual.readLines(cursor, batch);
        if (quint64(lines.size()) != batch)
            break; // 块损坏：终止续传
        QString error;
        ok = m_cold->appendBlock(lines, m_cold->frontier(), &error);
        if (ok) {
            cursor += batch;
            residual.setRetentionFloor(cursor); // 续传进度随批持久化（崩溃窗口最小化）
        }
    }
    residual.close();
    if (cursor < warmEnd) {
        // 续传失败：降级温层模式；残留按 v0.1 崩溃语义删除全新开始（避免错行）
        m_coldDegraded = true;
        m_cold.reset();
        QFile::remove(m_config.warmFilePath);
        emit degradedToWarmOnly(QStringLiteral("残留温层续传失败，降级为温层模式"));
        return;
    }
    QFile::remove(m_config.warmFilePath); // 续传完成，删除残留
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

    // 0) 冷层区间：引擎空间 [?, m_coldFrontier)，按映射表翻译成库内全局区间段逐段读
    if (isColdEnabled() && id < m_coldFrontier.load() && remaining > 0) {
        trimColdMap(); // 惰性裁剪：其他会话实例可能已触发全局清理删除本会话的块
        QVector<ZzColdMapEntry> segs; // 快照后释放锁，解压/读库不占用映射表互斥锁
        {
            QMutexLocker locker(&m_coldMapMutex);
            const quint64 coldEnd = m_coldFrontier.load();
            // 二分：最后一个 engineStart <= id 的条目；id 早于首个幸存条目（已清理）
            // 时从首条目起（前移到冷层最老可读行）
            qsizetype lo = 0;
            qsizetype hi = m_coldMap.size() - 1;
            qsizetype best = -1;
            while (lo <= hi) {
                const qsizetype mid = (lo + hi) / 2;
                if (m_coldMap[mid].engineStart <= id) {
                    best = mid;
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            }
            for (qsizetype i = qMax<qsizetype>(best, 0); i < m_coldMap.size(); ++i) {
                if (m_coldMap[i].engineStart >= coldEnd)
                    break;
                segs.append(m_coldMap[i]);
            }
        }
        for (const ZzColdMapEntry &e : std::as_const(segs)) {
            if (remaining == 0)
                break;
            if (id < e.engineStart)
                id = e.engineStart; // 前段已被清理：跳到幸存区间起点（不占 remaining）
            if (id >= e.engineStart + e.count)
                continue;
            const quint64 off = id - e.engineStart;
            const quint64 want = qMin(remaining, e.count - off);
            const QVector<ZzLogLine> got = m_cold->readLines(e.globalStart + off, want);
            out += got;
            id += quint64(got.size());
            remaining -= quint64(got.size());
            if (quint64(got.size()) < want)
                id = e.engineStart + e.count; // 块损坏短读：跳到下一映射条目，避免原地踏步
        }
    }
    // 1) 温层区间
    if (m_warm && remaining > 0) {
        const quint64 warmEnd = m_warmBase.load() + m_warmCount.load();
        if (id < warmEnd) {
            QReadLocker locker(&m_warmLock);
            const qsizetype before = out.size(); // out 可能已含冷层行，按增量计 got
            out += m_warm->readLines(id, remaining);
            const quint64 got = quint64(out.size() - before);
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
    quint64 hotBase;
    quint64 hotEnd;
    {
        QMutexLocker locker(&m_hotMutex);
        hotBase = m_hotBase;
        hotEnd = m_hotBase + quint64(m_hot.count());
    }
    // 行空间连续 [firstLineNo(), hotEnd)：冷层启用时窗口起点为冷层基线（引擎空间），
    // 否则为温层首行/热层首行（与 v0.1 的 m_warmCount + hot.count() 数值等价）
    quint64 first;
    if (isColdEnabled()) {
        trimColdMap();
        first = coldFirstLineNo();
    } else if (m_warm) {
        first = m_warmBase.load();
    } else {
        first = hotBase;
    }
    return hotEnd - first;
}

quint64 ZzLogEngine::firstLineNo() const
{
    if (isColdEnabled()) {
        trimColdMap();
        return coldFirstLineNo();
    }
    if (m_warm)
        return m_warmBase.load();
    QMutexLocker locker(&m_hotMutex);
    return m_hotBase;
}

void ZzLogEngine::trimColdMap() const
{
    if (!m_cold)
        return;
    const quint64 base = m_cold->baseLine(); // 先取库内基线（冷层内部锁），再持映射表锁
    QMutexLocker locker(&m_coldMapMutex);
    // 整块低于 base 的条目丢弃
    while (!m_coldMap.isEmpty()
           && m_coldMap.first().globalStart + m_coldMap.first().count <= base)
        m_coldMap.removeFirst();
    // 首条目部分重叠：截断（全局/引擎两空间同步前移，行数差一致）
    if (!m_coldMap.isEmpty() && m_coldMap.first().globalStart < base) {
        ZzColdMapEntry &e = m_coldMap.first();
        const quint64 trim = base - e.globalStart;
        e.globalStart = base;
        e.engineStart += trim;
        e.count -= trim;
    }
}

quint64 ZzLogEngine::coldFirstLineNo() const
{
    QMutexLocker locker(&m_coldMapMutex);
    if (!m_coldMap.isEmpty())
        return m_coldMap.first().engineStart;
    // 映射表为空：全部冷层行已清理（或尚无行落冷层，此时 coldFrontier == 0）；
    // 可读窗口从温层首行起（floor 不变式保证 warmBase >= coldFrontier）
    return m_coldFrontier.load();
}

QVector<quint64> ZzLogEngine::searchLines(const QString &pattern, int maxResults) const
{
    if (!isColdEnabled())
        return {};
    trimColdMap();
    // FTS 命中为库内全局行号（SQL 层已按 session_id 过滤）；经映射表反查翻译成
    // 引擎行号。同 sessionId 但不在本会话映射内的命中（如崩溃续传的历史会话行）丢弃
    const QVector<quint64> hits = m_cold->search(pattern, maxResults);
    QVector<quint64> out;
    out.reserve(hits.size());
    QMutexLocker locker(&m_coldMapMutex);
    for (const quint64 g : hits) {
        // 二分：最后一个 globalStart <= g 的条目（映射按 engineStart 有序，globalStart 同序）
        qsizetype lo = 0;
        qsizetype hi = m_coldMap.size() - 1;
        qsizetype best = -1;
        while (lo <= hi) {
            const qsizetype mid = (lo + hi) / 2;
            if (m_coldMap[mid].globalStart <= g) {
                best = mid;
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
        if (best < 0)
            continue;
        const ZzColdMapEntry &e = m_coldMap[best];
        if (g >= e.globalStart + e.count)
            continue;
        out.append(e.engineStart + (g - e.globalStart));
    }
    return out;
}

void ZzLogEngine::preload(quint64 lineNo)
{
    if (!m_worker)
        return;
    // 按行号路由：落入本会话冷层区间的经映射表翻译成库内全局行号走冷层预解压，
    // 其余走温层（现有路径）；已清理区间无块可预载，直接返回
    if (isColdEnabled() && lineNo < m_coldFrontier.load()) {
        trimColdMap();
        quint64 globalId = 0;
        bool found = false;
        {
            QMutexLocker locker(&m_coldMapMutex);
            qsizetype lo = 0;
            qsizetype hi = m_coldMap.size() - 1;
            qsizetype best = -1;
            while (lo <= hi) {
                const qsizetype mid = (lo + hi) / 2;
                if (m_coldMap[mid].engineStart <= lineNo) {
                    best = mid;
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            }
            if (best >= 0) {
                const ZzColdMapEntry &e = m_coldMap[best];
                if (lineNo < e.engineStart + e.count) {
                    globalId = e.globalStart + (lineNo - e.engineStart);
                    found = true;
                }
            }
        }
        if (!found)
            return;
        QMetaObject::invokeMethod(m_worker, "preloadCold", Qt::QueuedConnection,
                                  Q_ARG(quint64, globalId));
        return;
    }
    QMetaObject::invokeMethod(m_worker, "preloadAround", Qt::QueuedConnection,
                              Q_ARG(quint64, lineNo));
}

void ZzLogEngine::flush()
{
    if (!m_worker || !m_workerThread.isRunning())
        return;
    // 队列先进先出：先前排队的 archiveLines 先于 flush 执行，返回即归档完成
    // （worker 的 flush 槽会把不足一块的尾批也推进冷层）
    QMetaObject::invokeMethod(m_worker, "flush", Qt::BlockingQueuedConnection);
}
