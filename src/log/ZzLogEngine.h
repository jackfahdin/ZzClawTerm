#pragma once

#include "ZzLogLine.h"
#include "ZzRingBuffer.h"

#include <QMutex>
#include <QObject>
#include <QReadWriteLock>
#include <QThread>

#include <atomic>
#include <memory>

class ZzLogArchiveWorker;
class ZzMmapBuffer;

/**
 * @brief 日志引擎门面：热层环形缓冲 + 温层 mmap/LZ4 的统一读写入口（规格 §五）。
 *
 * 行号约定：绝对单调递增 ID，可读窗口为 [firstLineNo(), firstLineNo()+totalLines())；
 * 温层超限丢弃或纯内存模式驱逐时 firstLineNo() 前移。
 *
 * 线程模型：appendLine 可在任意线程调用（通常为终端 I/O 线程）；归档与预加载
 * 在内部独立 QThread 中执行，绝不阻塞调用方；getLine/getLines 可在任意线程调用
 * （热层互斥锁 + 温层读写锁保护）。归档是异步的，flush() 返回后所有已排队批次
 * 保证完成归档（测试与关闭前的确定性同步点）。
 *
 * 温层 I/O 失败（磁盘满等）时降级为纯内存模式并发射 degradedToMemoryOnly，
 * 不影响终端交互（规格 §八）。
 *
 * @code
 * ZzLogEngine::Config config;
 * config.warmFilePath = QStringLiteral("/path/to/session-warm.log");
 * ZzLogEngine engine(config);
 * if (engine.open()) {
 *     engine.appendLine({QStringLiteral("hello"), QByteArray()});
 *     QVector<ZzLogLine> window = engine.getLines(0, 60);
 * }
 * @endcode
 */
class ZzLogEngine : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 引擎配置。
     */
    struct Config {
        qsizetype hotCapacity = 10000;     ///< 热层环形缓冲行数（规格默认 1 万）
        quint64 warmMaxLines = 1000000;    ///< 温层最大行数（规格默认 100 万）
        qsizetype archiveBatchSize = 1024; ///< 单次归档批量行数
        QString warmFilePath;              ///< 温层 mmap 文件路径；为空则纯内存模式
    };

    explicit ZzLogEngine(const Config &config, QObject *parent = nullptr);
    ~ZzLogEngine() override;

    /**
     * @brief 打开引擎（含温层文件）；温层打开失败时降级纯内存并发射信号。
     * @return 恒返回 true（降级视为可用）；通过 isMemoryOnly() 查询实际模式。
     * @note 建议在 open() 之前连接 degradedToMemoryOnly 信号。
     */
    bool open();

    /// @brief 追加一行（线程安全；热层满时最老批次异步归档到温层）。
    void appendLine(const ZzLogLine &line);

    /**
     * @brief 读取单行。
     * @param lineNo 绝对行 ID。
     * @param out 输出行，不可为空。
     * @return 行在当前可读窗口内返回 true。
     */
    bool getLine(quint64 lineNo, ZzLogLine *out) const;

    /**
     * @brief 读取 [startLine, startLine+count) 窗口（滚动读取主路径）。
     * @return 实际读到的行，按行 ID 递增排列。
     */
    QVector<ZzLogLine> getLines(quint64 startLine, quint64 count) const;

    quint64 totalLines() const;   ///< 当前可读总行数（热层 + 温层）
    quint64 firstLineNo() const;  ///< 当前最老可读行 ID
    bool isMemoryOnly() const { return m_memoryOnly; }

    /// @brief 预加载 lineNo 附近温层块到解压缓存（异步，不阻塞调用方）。
    void preload(quint64 lineNo);

    /**
     * @brief 阻塞至全部已排队归档批次完成并冲刷温层缓冲。
     * @note 不得从归档线程内部调用（BlockingQueuedConnection 会死锁）。
     */
    void flush();

signals:
    void archiveFinished();                            ///< 一批行完成温层归档
    void degradedToMemoryOnly(const QString &reason);  ///< 温层不可用，降级纯内存

private:
    Config m_config;
    ZzRingBuffer m_hot;             ///< 热层（m_hotMutex 保护）
    mutable QMutex m_hotMutex;
    std::unique_ptr<ZzMmapBuffer> m_warm; ///< 温层（纯内存模式为空）
    mutable QReadWriteLock m_warmLock; ///< 温层读写锁（读路径读锁、归档写锁）
    QThread m_workerThread;         ///< 归档线程
    ZzLogArchiveWorker *m_worker = nullptr; ///< 运行于归档线程
    std::atomic<quint64> m_warmBase{0};  ///< 温层首行 ID（归档线程发布）
    std::atomic<quint64> m_warmCount{0}; ///< 温层行数（归档线程发布）
    quint64 m_hotBase = 0;          ///< 热层首行 ID（m_hotMutex 保护）
    bool m_memoryOnly = false;
};
