#pragma once

#include "ZzLogLine.h"
#include "ZzRingBuffer.h"

#include <QMutex>
#include <QObject>
#include <QReadWriteLock>
#include <QThread>

#include <atomic>
#include <memory>

class ZzColdStorage;
class ZzLogArchiveWorker;
class ZzMmapBuffer;

/**
 * @brief 日志引擎门面：热层环形缓冲 + 温层 mmap/LZ4 + 冷层 SQLite/ZSTD 的统一读写入口。
 *
 * 行号约定：绝对单调递增 ID，可读窗口为 [firstLineNo(), firstLineNo()+totalLines())；
 * 温层超限丢弃、纯内存模式驱逐或冷层清理时 firstLineNo() 前移。
 * 冷层启用时库内行号全局单调（跨会话共享 cold.db），引擎行号 = 库内行号 - m_coldOffset，
 * 每会话从 0 起；本会话只能看到 g >= m_coldOffset 的冷层行（历史会话行持久保留但不可见）。
 *
 * 线程模型：appendLine 可在任意线程调用（通常为终端 I/O 线程）；归档与预加载
 * 在内部独立 QThread 中执行，绝不阻塞调用方；getLine/getLines 可在任意线程调用
 * （热层互斥锁 + 温层读写锁 + 冷层内部互斥锁保护）。归档是异步的，flush() 返回后
 * 所有已排队批次保证完成归档（含温→冷推进，不足一块的尾批也落入冷层）。
 *
 * 降级：温层 I/O 失败 → degradedToMemoryOnly（纯内存）；冷层打开/写入失败 →
 * degradedToWarmOnly（无冷层，温层回到 v0.1 容量丢弃，温层文件保留）。
 * 降级不影响终端交互（规格 §八）。
 *
 * 干净退出：析构时 flush 后若冷层启用且未降级，删除温层文件（冷层为唯一持久真相）；
 * 异常退出温层文件残留，下次 open() 按文件头游标续传进冷层后删除。
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
        QString warmFilePath;              ///< 温层 mmap 文件路径；为空则纯内存模式（冷层配置同时被忽略）
        QString coldDbPath;                ///< 冷层全局单库路径；为空 = 冷层禁用（行为 = v0.1）
        QString sessionId;                 ///< 会话 profile id（冷层行归属标注）
        qint64 coldMaxBytes = 10LL * 1024 * 1024 * 1024; ///< 冷层清理水位：10GB
        int coldMaxAgeDays = 90;                         ///< 冷层清理水位：90 天
    };

    explicit ZzLogEngine(const Config &config, QObject *parent = nullptr);
    ~ZzLogEngine() override;

    /**
     * @brief 打开引擎（含温层文件与冷层库）；失败时按层降级并发射对应信号。
     * @return 恒返回 true（降级视为可用）；通过 isMemoryOnly()/isColdEnabled() 查询实际模式。
     * @note 建议在 open() 之前连接 degradedToMemoryOnly/degradedToWarmOnly 信号。
     * @note 冷层启用时 open() 会同步完成残留温层的崩溃恢复续传（仅异常退出后触发，
     *       通常秒级；见 recoverResidualWarm）。
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
     *        冷→温→热三路透明归并，调用方零改动。
     * @return 实际读到的行，按行 ID 递增排列。
     */
    QVector<ZzLogLine> getLines(quint64 startLine, quint64 count) const;

    quint64 totalLines() const;   ///< 当前可读总行数（冷层 + 温层 + 热层）
    quint64 firstLineNo() const;  ///< 当前最老可读行 ID
    bool isMemoryOnly() const { return m_memoryOnly.load(); }
    /// @brief 冷层启用且未降级。
    bool isColdEnabled() const { return m_cold && !m_coldDegraded.load(); }

    /// @brief 预加载 lineNo 附近块到解压缓存（异步，不阻塞调用方；按行号路由冷层或温层）。
    void preload(quint64 lineNo);

    /**
     * @brief 冷层全文搜索（FTS5）。
     * @param pattern FTS5 MATCH 表达式。
     * @param maxResults 最大返回行数。
     * @return 命中行的引擎空间行号（升序）；仅覆盖已归档进冷层的行
     *         （温层/热层行不入索引，属 v0.2 范围边界）。
     */
    QVector<quint64> searchLines(const QString &pattern, int maxResults = 1000) const;

    /**
     * @brief 阻塞至全部已排队归档批次完成并冲刷温层/冷层缓冲（尾批落入冷层）。
     * @note 不得从归档线程内部调用（BlockingQueuedConnection 会死锁）。
     */
    void flush();

signals:
    void archiveFinished();                            ///< 一批行完成温层归档
    void degradedToMemoryOnly(const QString &reason);  ///< 温层不可用，降级纯内存
    void degradedToWarmOnly(const QString &reason);    ///< 冷层不可用，降级 v0.1 温层模式

private:
    /// @brief 崩溃恢复：残留温层按文件头 coldCursor 游标续传进冷层后删除；
    ///        续传失败时降级温层模式（发射 degradedToWarmOnly）并删除残留全新开始。
    void recoverResidualWarm();

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
    std::atomic<bool> m_memoryOnly{false}; ///< 降级标志（archiveFailed 回调跨线程写）
    std::unique_ptr<ZzColdStorage> m_cold; ///< 冷层（coldDbPath 为空或打开/恢复失败时为空）
    quint64 m_coldOffset = 0;       ///< 本会话引擎行号 → 库内全局行号的平移量（open 时确定）
    std::atomic<quint64> m_coldBase{0};     ///< 冷层最老可读行（库内全局空间；读路径减 m_coldOffset 夹取）
    std::atomic<quint64> m_coldFrontier{0}; ///< 本会话已落冷层行数上界（引擎空间 == worker 游标）
    std::atomic<bool> m_coldDegraded{false}; ///< 冷层降级门闩（coldFailed 回调跨线程写）
};
