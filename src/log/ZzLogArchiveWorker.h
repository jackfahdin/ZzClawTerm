#pragma once

#include "ZzLogLine.h"

#include <QObject>
#include <QReadWriteLock>
#include <QVector>

#include <atomic>

class ZzColdStorage;
class ZzMmapBuffer;
class QMutex;

/**
 * @brief 冷层映射条目：本会话「引擎空间区间 → 库内全局区间」的一段对应关系。
 *
 * 全局单库被多会话实例共享，appendBlock 实际落点由事务内权威 frontier 决定，
 * 可能 != 引擎侧期望的固定偏移；每成功写一块追加一条映射，读回/搜索按表翻译
 * （审查修复轮 2：废弃固定 m_coldOffset 平移假设）。条目按 engineStart 递增追加。
 */
struct ZzColdMapEntry {
    quint64 engineStart = 0; ///< 引擎空间首行（== 写入时的归档游标）
    quint64 globalStart = 0; ///< 库内全局空间首行（appendBlock 回传的实际落点）
    quint64 count = 0;       ///< 行数（两空间一致）
};

/**
 * @brief 温层归档工作对象：运行在 ZzLogEngine 拥有的独立 QThread 中。
 *
 * 负责把热层驱逐出的批量行写入温层（绝不阻塞 I/O 线程与 UI 线程，规格 §5.3），
 * 并执行滚动方向上的块预加载；通过传入的原子计数器向读路径发布温层区间。
 *
 * v0.2 扩展（规格 §六）：温层批次写完后追加 coldAdvance——从温层读
 * [m_coldCursor, warmEnd) 凑满 1024 行一批写入冷层，提交后前移温层保留下限；
 * 冷层写入失败重试 3 次后发射 coldFailed 并清除温层保留下限（回到 v0.1 容量丢弃）。
 */
class ZzLogArchiveWorker : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 构造工作对象。
     * @param buffer 温层存储（由 ZzLogEngine 持有，生命周期长于本对象）。
     * @param lock 温层读写锁（归档持写锁、读路径持读锁）。
     * @param warmBase 温层首行 ID 发布位。
     * @param warmCount 温层行数发布位。
     * @param cold 冷层存储（可空；空 = v0.1 无冷层行为）。
     * @param coldBase 冷层最老可读行发布位（库内全局空间原始值；cold 为空时忽略）。
     * @param coldFrontier 本会话已落冷层行数上界发布位（引擎空间 == m_coldCursor；cold 为空时忽略）。
     * @param coldMap 冷层映射表（ZzLogEngine 持有，追加写；与 coldMapMutex 成对提供，可空）。
     * @param coldMapMutex 映射表互斥锁（worker 写、读路径读，参照 warmBase/warmCount 发布位模式）。
     */
    ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                       std::atomic<quint64> *warmBase, std::atomic<quint64> *warmCount,
                       ZzColdStorage *cold = nullptr,
                       std::atomic<quint64> *coldBase = nullptr,
                       std::atomic<quint64> *coldFrontier = nullptr,
                       QVector<ZzColdMapEntry> *coldMap = nullptr,
                       QMutex *coldMapMutex = nullptr,
                       QObject *parent = nullptr);

public slots:
    /// @brief 归档一批行到温层；失败发射 archiveFailed；成功后追加 coldAdvance。
    void archiveLines(const QVector<ZzLogLine> &lines);

    /// @brief 预加载 lineId 所在温层块及后一块到解压缓存。
    void preloadAround(quint64 lineId);

    /// @brief 预解压 globalLineId（库内全局空间）所在冷层块及后一块进 LRU。
    void preloadCold(quint64 globalLineId);

    /// @brief 冲刷温层文件缓冲，并把不足一块的尾批也推进冷层（测试与关闭前的确定性同步点）。
    void flush();

signals:
    void archiveCompleted();                    ///< 一批行完成归档
    void archiveFailed(const QString &message); ///< 温层 I/O 失败
    void coldFailed(const QString &message);    ///< 冷层写入重试 3 次仍失败（降级依据）

private:
    /**
     * @brief 温→冷推进：把 [m_coldCursor, warmEnd) 按块写入冷层并前移温层保留下限。
     * @param includePartial true 时不足一块的尾批也写入（flush/干净退出用）。
     * @note 仅在本对象所在线程由 archiveLines/flush 直接调用，不经事件队列。
     */
    void coldAdvance(bool includePartial);

    ZzMmapBuffer *m_buffer;
    QReadWriteLock *m_lock;
    std::atomic<quint64> *m_warmBase;
    std::atomic<quint64> *m_warmCount;
    ZzColdStorage *m_cold;            ///< 可空
    std::atomic<quint64> *m_coldBase; ///< 随 m_cold 一并提供
    std::atomic<quint64> *m_coldFrontier;
    QVector<ZzColdMapEntry> *m_coldMap = nullptr; ///< 冷层映射表（可空；非空时 m_coldMapMutex 必非空）
    QMutex *m_coldMapMutex = nullptr;             ///< 映射表互斥锁
    quint64 m_coldCursor = 0;         ///< 本会话已落入冷层的行数（温层局部行号空间）
    bool m_coldFailed = false;        ///< 冷层降级门闩（只发射一次 coldFailed）
};
