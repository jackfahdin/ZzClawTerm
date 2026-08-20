#pragma once

#include "ZzLogLine.h"

#include <QCache>
#include <QMutex>
#include <QString>
#include <QVector>

struct sqlite3;

/**
 * @brief 冷层存储：全局单库 SQLite + ZSTD 分块压缩 + FTS5 全文索引（规格 §四）。
 *
 * - 每块最多 kMaxBlockLines 行，ZSTD level 3 压缩为一个 blob 存 blocks 表；
 *   块内行编码与温层一致（textLen u32|attrLen u32|UTF-8|属性），块头带行内偏移表；
 * - 行号：库内全局单调（跨会话共享单库），frontier() == 下一个待写入绝对行号；
 * - 块时间戳 = 归档时刻（ZzLogLine 无时间戳字段；热层缓冲最多 1 万行，归档延迟通常秒级，
 *   该近似对按时间清理/查询足够）；
 * - frontier 与块写入同一 SQLite 事务提交：崩溃不产生重复块/丢块；
 * - FTS5 contentless 索引行文本（rowid = 绝对行号），与块写入同事务；
 * - 线程安全：内部 QMutex 保护 sqlite3 连接与全部内部状态（写事务短，读只解压 1 块），
 *   全部公开方法可在任意线程调用。
 */
class ZzColdStorage
{
public:
    /**
     * @brief 冷层配置。
     */
    struct Config {
        QString dbPath;          ///< 全局单库路径
        QString sessionId;       ///< 写入行的会话归属
        qint64 maxBytes = 10LL * 1024 * 1024 * 1024; ///< 清理水位：10GB
        int maxAgeDays = 90;     ///< 清理水位：90 天
    };

    static constexpr quint64 kMaxBlockLines = 1024; ///< 单块最大行数（规格 §四）
    static constexpr int kCompressionLevel = 3;     ///< ZSTD 压缩级别（规格 §二）

    explicit ZzColdStorage(const Config &config);
    ~ZzColdStorage();

    ZzColdStorage(const ZzColdStorage &) = delete;
    ZzColdStorage &operator=(const ZzColdStorage &) = delete;

    /**
     * @brief 打开（或创建）库文件，建 schema、置 WAL、读回 meta 与块表。
     * @param errorString 失败时输出原因，可为空。
     * @return 成功返回 true；meta 与块表不一致（库损坏）时返回 false。
     */
    bool open(QString *errorString = nullptr);

    /// @brief 关闭连接（WAL 已提交数据不丢）。
    void close();

    bool isOpen() const;

    quint64 baseLine() const;    ///< 最老可读行（清理后前移）
    quint64 frontier() const;    ///< 已覆盖行数上界 == 下一个待写入绝对行号

    /**
     * @brief 追加一块（归档线程调用）。
     * @param lines 块内行，1..kMaxBlockLines 行。
     * @param firstLine 块首行绝对行号，必须 == frontier()（全局连续，崩溃无重复块）。
     * @param errorString 失败时输出原因，可为空。
     * @return 成功返回 true 且 frontier() 前移 lines.size()；失败返回 false 且状态不变。
     * @note 提交后内部自动执行一次 enforceLimits()（规格 §七：每次写入后检查水位）。
     */
    bool appendBlock(const QVector<ZzLogLine> &lines, quint64 firstLine,
                     QString *errorString = nullptr);

    /**
     * @brief 读取 [startLine, startLine + count) 区间内的行（任意线程）。
     * @return 实际读到的行；起点早于 baseLine() 或超出 frontier() 时按实际可得数量返回。
     */
    QVector<ZzLogLine> readLines(quint64 startLine, quint64 count) const;

    /// @brief 预解压 lineId 所在块及后一块进 LRU（供滚动方向预取）。
    void preload(quint64 lineId);

    /**
     * @brief FTS5 全文搜索。
     * @param pattern FTS5 MATCH 表达式（非法表达式返回空，不报错）。
     * @param maxResults 最大返回行数。
     * @return 命中行的绝对行号（升序）；仅覆盖已归档进冷层的行。
     */
    QVector<quint64> search(const QString &pattern, int maxResults = 1000) const;

    /// @brief 按 maxBytes/maxAgeDays 清理最老块（含 FTS5 同步删除与增量 VACUUM）。
    void enforceLimits();

private:
    struct BlockEntry {
        quint64 firstLine = 0;  ///< 块首行绝对行号
        quint32 lineCount = 0;  ///< 块内行数
    };

    bool execSql(const char *sql, QString *errorString) const;
    bool loadState(QString *errorString); ///< open 时读 meta + 块表并做一致性校验
    void closeLocked();                   ///< 调用方须已持有 m_mutex
    QByteArray rawBlock(quint64 firstLine) const; ///< 解压块（LRU 命中或 SELECT+ZSTD）；须持锁
    qsizetype findBlockIndex(quint64 lineId) const; ///< 二分找最后 firstLine <= lineId 的块
    bool deleteOldestBlocks(qsizetype count, QString *errorString); ///< 删最老 count 块；须持锁

    Config m_config;
    sqlite3 *m_db = nullptr;
    mutable QMutex m_mutex;       ///< 保护 sqlite3 连接与全部内部状态
    QVector<BlockEntry> m_blocks; ///< 存活块表（按 firstLine 递增）
    quint64 m_frontier = 0;
    quint64 m_baseLine = 0;
    mutable QCache<quint64, QByteArray> m_blockCache; ///< 解压块 LRU（键：块首行号）
};
