#pragma once

#include "ZzLogLine.h"

#include <QObject>
#include <QReadWriteLock>
#include <QVector>

#include <atomic>

class ZzMmapBuffer;

/**
 * @brief 温层归档工作对象：运行在 ZzLogEngine 拥有的独立 QThread 中。
 *
 * 负责把热层驱逐出的批量行写入温层（绝不阻塞 I/O 线程与 UI 线程，规格 §5.3），
 * 并执行滚动方向上的块预加载；通过传入的原子计数器向读路径发布温层区间。
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
     */
    ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                       std::atomic<quint64> *warmBase, std::atomic<quint64> *warmCount,
                       QObject *parent = nullptr);

public slots:
    /// @brief 归档一批行到温层；失败发射 archiveFailed。
    void archiveLines(const QVector<ZzLogLine> &lines);

    /// @brief 预加载 lineId 所在块及后一块到解压缓存。
    void preloadAround(quint64 lineId);

    /// @brief 冲刷温层文件缓冲（测试与关闭前的确定性同步点）。
    void flush();

signals:
    void archiveCompleted();                    ///< 一批行完成归档
    void archiveFailed(const QString &message); ///< 温层 I/O 失败

private:
    ZzMmapBuffer *m_buffer;
    QReadWriteLock *m_lock;
    std::atomic<quint64> *m_warmBase;
    std::atomic<quint64> *m_warmCount;
};
