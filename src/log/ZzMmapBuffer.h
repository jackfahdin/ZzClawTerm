#pragma once

#include "ZzLineIndex.h"
#include "ZzLogLine.h"

#include <QCache>
#include <QFile>
#include <QMutex>
#include <QVector>

/**
 * @brief 温层存储：mmap 内存映射文件 + LZ4 分块压缩（规格 §5.2 温层）。
 *
 * - 每 64KB 未压缩数据压缩为一块，按需解压（单块微秒级），附带 8 块解压缓存；
 * - 通过 ZzLineIndex（每 1024 行一条）定位块内偏移，块内小范围扫描；
 * - 行 ID 绝对单调递增且永不复用，裁剪/压缩文件后保持可读；
 * - 文件预分配并按 4MB 粒度增长；尾部半写块与字段不合理的幽灵块在重开扫描时被安全忽略（崩溃安全）；
 * - 超出 maxLines 时按整块粒度丢弃最老数据（v0.2 改为归档冷层）。
 *
 * @note 线程安全由上层（ZzLogEngine 的 QReadWriteLock）保证，本类自身不加锁；
 *       唯一的例外是解压缓存 m_blockCache：上层读锁允许多线程并发进入 const 读路径，
 *       故缓存访问由专用的 m_cacheMutex 保护（QCache 本身非线程安全）。
 */
class ZzMmapBuffer
{
public:
    explicit ZzMmapBuffer(const QString &filePath, quint64 maxLines = 1000000);
    ~ZzMmapBuffer();

    ZzMmapBuffer(const ZzMmapBuffer &) = delete;
    ZzMmapBuffer &operator=(const ZzMmapBuffer &) = delete;

    /**
     * @brief 打开（或创建）映射文件并重建块表。
     * @return 文件无法打开、扩容或映射失败、魔数不匹配时返回 false。
     */
    bool open();

    /// @brief 解除映射并关闭文件（unmap 时脏页由 OS 落盘）。
    void close();

    bool isOpen() const { return m_file.isOpen(); }

    /// @brief 当前最老存活行 ID（无数据时等于下一个待分配 ID）。
    quint64 firstLineId() const;

    quint64 lineCount() const { return m_lineCount; }
    quint64 maxLines() const { return m_maxLines; }

    /**
     * @brief 追加一批行（序列化 → 64KB 分块 → LZ4 压缩 → 写映射区）。
     * @param lines 待追加行。
     * @param errorString 失败时输出原因，可为空。
     * @return 压缩或扩容失败返回 false；失败批次不产生幻影行，
     *         行计数与下一个行 ID 保持未变，已落盘的前序块保持一致可读。
     */
    bool appendLines(const QVector<ZzLogLine> &lines, QString *errorString = nullptr);

    /**
     * @brief 读取 [startId, startId + count) 区间内的行。
     * @return 实际读到的行；起点早于 firstLineId() 或超出末尾时按实际可得数量返回。
     */
    QVector<ZzLogLine> readLines(quint64 startId, quint64 count) const;

    /// @brief 预加载：将 lineId 所在块及其后一块解压进缓存（供滚动方向预取）。
    void preload(quint64 lineId) const;

    /// @brief 刷新文件流缓冲；mmap 脏页由 OS 回写，close/unmap 时保证落盘。
    void flush();

    /**
     * @brief 设置保留下限（冷层模式）：dropOldestBlocks 只丢弃整体行号区间
     *        （lineStart + lineCount <= firstDisposableLineId）的最老块，跨界块保留。
     * @param firstDisposableLineId 第一个可被丢弃的行 ID == 冷层已覆盖行数上界（本文件局部行号空间）。
     * @note 立即生效（调用 dropOldestBlocks），并把该游标持久化到文件头偏移 12 处
     *       （coldCursor，异常退出后冷层续传依据）；未设置 floor 时保持 v0.1 纯 maxLines 丢弃。
     */
    void setRetentionFloor(quint64 firstDisposableLineId);

    /// @brief 清除保留下限，恢复 v0.1 纯 maxLines 丢弃（冷层降级时由归档线程调用）。
    void clearRetentionFloor();

    /// @brief 冷层续传游标（文件头持久化；未启用冷层的文件读出 0）。
    quint64 coldCursor() const { return m_coldCursor; }

    static constexpr quint64 kChunkSize = 64 * 1024; ///< 单块未压缩数据上限（字节）

private:
    struct BlockInfo {
        quint64 lineStart = 0;  ///< 块首行 ID
        quint32 lineCount = 0;  ///< 块内行数
        qint64 fileOffset = 0;  ///< 块头在文件中的偏移
        quint32 uncompSize = 0; ///< 未压缩字节数
        quint32 compSize = 0;   ///< 压缩后字节数
    };

    bool remap();
    bool scanFile();
    void writeHeader();
    bool ensureCapacity(qint64 extraBytes);
    bool writeBlock(const QByteArray &chunk, quint64 lineStart, quint32 lineCount,
                    QString *errorString);
    QByteArray decompressBlock(const BlockInfo &block) const;
    qsizetype findBlockIndex(quint64 lineId) const;
    void dropOldestBlocks();
    void compact();

    QFile m_file;
    uchar *m_map = nullptr;     ///< 全文件映射基址
    qint64 m_mappedSize = 0;    ///< 映射长度（== 文件长度）
    qint64 m_appendOffset = 0;  ///< 下一块写入偏移
    quint64 m_maxLines;
    quint64 m_lineCount = 0;    ///< 存活行数
    quint64 m_nextLineId = 0;   ///< 下一个待分配行 ID
    quint32 m_skipBlocks = 0;   ///< 文件头部已逻辑丢弃的块数（持久化于文件头）
    qint64 m_droppedBytes = 0;  ///< 已逻辑丢弃的字节数（触发物理压缩用）
    quint64 m_retentionFloor = 0;      ///< 保留下限（m_hasRetentionFloor 为 true 时有效）
    bool m_hasRetentionFloor = false;  ///< 是否处于冷层保留下限模式
    quint64 m_coldCursor = 0;          ///< 文件头持久化的冷层续传游标（== 最近一次 floor）
    QVector<BlockInfo> m_blocks;              ///< 存活块表（按 lineStart 递增）
    ZzLineIndex m_lineIndex;                  ///< 块内行偏移索引
    mutable QCache<quint64, QByteArray> m_blockCache; ///< 解压缓存，键为块首行 ID
    mutable QMutex m_cacheMutex; ///< m_blockCache 专用锁（并发读路径下的 QCache 保护）
};
