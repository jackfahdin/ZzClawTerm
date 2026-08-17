#include "ZzMmapBuffer.h"

#include <lz4.h>

#include <QtEndian>
#include <cstring>

namespace {
constexpr quint32 kFileMagic = 0x5A5A4C4D;   ///< 文件魔数 "ZZLM"
constexpr quint32 kFileVersion = 1;          ///< 存储格式版本
constexpr quint32 kBlockMagic = 0x5A5A424B;  ///< 块魔数 "ZZBK"
constexpr qint64 kHeaderSize = 4096;         ///< 文件头占一页
constexpr qint64 kGrowGranularity = 4 * 1024 * 1024; ///< 映射扩容粒度 4MB
constexpr qsizetype kBlockHeaderSize = 24;   ///< 块头字节数
constexpr int kCacheBlocks = 8;              ///< 解压缓存块数

void putU32(char *p, quint32 v) { v = qToLittleEndian(v); std::memcpy(p, &v, 4); }
void putU64(char *p, quint64 v) { v = qToLittleEndian(v); std::memcpy(p, &v, 8); }
quint32 getU32(const char *p) { quint32 v; std::memcpy(&v, p, 4); return qFromLittleEndian(v); }
quint64 getU64(const char *p) { quint64 v; std::memcpy(&v, p, 8); return qFromLittleEndian(v); }

/// @brief 行编码：textLen u32 | attrLen u32 | UTF-8 文本 | 属性负载。
QByteArray encodeLine(const ZzLogLine &line)
{
    const QByteArray text = line.text.toUtf8();
    QByteArray out;
    out.reserve(8 + text.size() + line.attributes.size());
    char hdr[8];
    putU32(hdr, quint32(text.size()));
    putU32(hdr + 4, quint32(line.attributes.size()));
    out.append(hdr, 8);
    out.append(text);
    out.append(line.attributes);
    return out;
}

/// @brief 跳过一行，返回下一行偏移；数据截断返回 -1。
qint64 skipLine(const QByteArray &data, qint64 off)
{
    if (off < 0 || off + 8 > data.size())
        return -1;
    const char *p = data.constData() + off;
    const quint32 textLen = getU32(p);
    const quint32 attrLen = getU32(p + 4);
    const qint64 next = off + 8 + textLen + attrLen;
    return next > data.size() ? -1 : next;
}

/// @brief 解析一行，返回下一行偏移；数据截断返回 -1。
qint64 parseLine(const QByteArray &data, qint64 off, ZzLogLine *out)
{
    const qint64 next = skipLine(data, off);
    if (next < 0)
        return -1;
    const char *p = data.constData() + off;
    const quint32 textLen = getU32(p);
    const quint32 attrLen = getU32(p + 4);
    out->text = QString::fromUtf8(p + 8, qsizetype(textLen));
    out->attributes = QByteArray(p + 8 + textLen, qsizetype(attrLen));
    return next;
}
} // namespace

ZzMmapBuffer::ZzMmapBuffer(const QString &filePath, quint64 maxLines)
    : m_file(filePath)
    , m_maxLines(qMax<quint64>(maxLines, 1))
{
}

ZzMmapBuffer::~ZzMmapBuffer()
{
    close();
}

bool ZzMmapBuffer::open()
{
    if (m_file.isOpen())
        return true;
    if (!m_file.open(QIODevice::ReadWrite))
        return false;

    const bool isNew = m_file.size() < kHeaderSize;
    if (isNew && !m_file.resize(kHeaderSize + kGrowGranularity)) {
        m_file.close();
        return false;
    }
    if (!remap()) {
        m_file.close();
        return false;
    }

    if (isNew) {
        m_skipBlocks = 0;
        writeHeader();
        m_appendOffset = kHeaderSize;
    } else {
        const char *base = reinterpret_cast<const char *>(m_map);
        if (getU32(base) != kFileMagic) { // 非本格式文件
            close();
            return false;
        }
        m_skipBlocks = getU32(base + 8);
        scanFile();
    }
    m_blockCache.setMaxCost(kCacheBlocks);
    return true;
}

void ZzMmapBuffer::close()
{
    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    m_mappedSize = 0;
    if (m_file.isOpen())
        m_file.close();
    m_blockCache.clear();
}

quint64 ZzMmapBuffer::firstLineId() const
{
    return m_blocks.isEmpty() ? m_nextLineId : m_blocks.first().lineStart;
}

bool ZzMmapBuffer::appendLines(const QVector<ZzLogLine> &lines, QString *errorString)
{
    if (!m_map) {
        if (errorString)
            *errorString = QStringLiteral("温层文件未打开");
        return false;
    }
    QByteArray chunk;
    chunk.reserve(qsizetype(kChunkSize) + 256);
    quint64 chunkFirstId = m_nextLineId;
    quint32 chunkLines = 0;

    for (const ZzLogLine &line : lines) {
        const QByteArray encoded = encodeLine(line);
        // 单块不超 64KB（单行超过 64KB 时独占一块）
        if (chunkLines > 0 && chunk.size() + encoded.size() > qint64(kChunkSize)) {
            if (!writeBlock(chunk, chunkFirstId, chunkLines, errorString))
                return false;
            chunk.clear();
            chunkFirstId = m_nextLineId;
            chunkLines = 0;
        }
        m_lineIndex.recordLine(m_nextLineId, chunkFirstId, quint64(chunk.size()));
        chunk.append(encoded);
        ++chunkLines;
        ++m_nextLineId;
        ++m_lineCount;
    }
    if (chunkLines > 0 && !writeBlock(chunk, chunkFirstId, chunkLines, errorString))
        return false;
    return true;
}

QVector<ZzLogLine> ZzMmapBuffer::readLines(quint64 startId, quint64 count) const
{
    QVector<ZzLogLine> out;
    if (!m_map || count == 0 || m_blocks.isEmpty())
        return out;

    quint64 id = qMax(startId, firstLineId());
    const quint64 end = qMin(startId + count, m_nextLineId);
    if (end <= id)
        return out; // 区间完全落在已丢弃/未写入范围
    out.reserve(qsizetype(qMin<quint64>(end - id, 100000)));

    while (id < end) {
        const qsizetype bi = findBlockIndex(id);
        if (bi < 0)
            break;
        const BlockInfo &block = m_blocks[bi];
        const QByteArray data = decompressBlock(block);
        if (data.isEmpty())
            break; // 块损坏：按可得数据返回

        // 块内定位：优先行索引（每 1024 行一条），否则从块首小范围扫描
        quint64 scanId = block.lineStart;
        qint64 off = 0;
        ZzLineIndex::Entry e;
        if (m_lineIndex.locate(id, &e) && e.blockFirstLineId == block.lineStart) {
            scanId = e.lineId;
            off = qint64(e.offset);
        }
        while (scanId < id) {
            const qint64 next = skipLine(data, off);
            if (next < 0)
                return out;
            off = next;
            ++scanId;
        }

        const quint64 blockEnd = block.lineStart + block.lineCount;
        while (id < end && id < blockEnd) {
            ZzLogLine line;
            const qint64 next = parseLine(data, off, &line);
            if (next < 0)
                return out;
            out.append(line);
            off = next;
            ++id;
        }
    }
    return out;
}

void ZzMmapBuffer::preload(quint64 lineId) const
{
    if (!m_map)
        return;
    const qsizetype bi = findBlockIndex(lineId);
    if (bi < 0)
        return;
    decompressBlock(m_blocks[bi]); // 取块即入缓存
    if (bi + 1 < m_blocks.size())
        decompressBlock(m_blocks[bi + 1]);
}

void ZzMmapBuffer::flush()
{
    if (m_file.isOpen())
        m_file.flush();
}

bool ZzMmapBuffer::remap()
{
    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    m_mappedSize = m_file.size();
    if (m_mappedSize <= 0)
        return false;
    m_map = m_file.map(0, m_mappedSize);
    return m_map != nullptr;
}

bool ZzMmapBuffer::scanFile()
{
    m_blocks.clear();
    m_lineIndex.clear();
    m_lineCount = 0;
    m_nextLineId = 0;
    m_droppedBytes = 0;

    const char *base = reinterpret_cast<const char *>(m_map);
    qint64 offset = kHeaderSize;
    QVector<BlockInfo> physical;
    while (offset + kBlockHeaderSize <= m_mappedSize) {
        if (getU32(base + offset) != kBlockMagic)
            break; // 到达未写区域（预分配零页）或尾部半写块
        BlockInfo b;
        b.fileOffset = offset;
        b.lineStart = getU64(base + offset + 4);
        b.lineCount = getU32(base + offset + 12);
        b.uncompSize = getU32(base + offset + 16);
        b.compSize = getU32(base + offset + 20);
        const qint64 next = offset + kBlockHeaderSize + b.compSize;
        if (next > m_mappedSize)
            break; // 半写块
        physical.append(b);
        offset = next;
    }
    m_appendOffset = offset;

    // 跳过文件头记录的已逻辑丢弃块
    const qsizetype skip = qMin<qsizetype>(m_skipBlocks, physical.size());
    for (qsizetype i = 0; i < skip; ++i)
        m_droppedBytes += kBlockHeaderSize + physical[i].compSize;
    m_blocks = physical.mid(skip);
    for (const BlockInfo &b : std::as_const(m_blocks))
        m_lineCount += b.lineCount;
    if (!physical.isEmpty())
        m_nextLineId = physical.last().lineStart + physical.last().lineCount;
    return true;
}

void ZzMmapBuffer::writeHeader()
{
    char *base = reinterpret_cast<char *>(m_map);
    putU32(base, kFileMagic);
    putU32(base + 4, kFileVersion);
    putU32(base + 8, m_skipBlocks);
}

bool ZzMmapBuffer::ensureCapacity(qint64 extraBytes)
{
    if (m_appendOffset + extraBytes <= m_mappedSize)
        return true;
    const qint64 need = m_appendOffset + extraBytes - m_mappedSize;
    const qint64 grow =
        ((qMax(need, kGrowGranularity) + kGrowGranularity - 1) / kGrowGranularity) * kGrowGranularity;
    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    if (!m_file.resize(m_mappedSize + grow))
        return false; // 磁盘满等：由上层降级为纯内存模式
    return remap();
}

bool ZzMmapBuffer::writeBlock(const QByteArray &chunk, quint64 lineStart, quint32 lineCount,
                              QString *errorString)
{
    const int bound = LZ4_compressBound(int(chunk.size()));
    QByteArray compressed;
    compressed.resize(bound);
    const int compSize = LZ4_compress_default(chunk.constData(), compressed.data(),
                                              int(chunk.size()), bound);
    if (compSize <= 0) {
        if (errorString)
            *errorString = QStringLiteral("LZ4 压缩失败");
        return false;
    }
    compressed.resize(compSize);

    if (!ensureCapacity(kBlockHeaderSize + compSize)) {
        if (errorString)
            *errorString = QStringLiteral("温层映射文件扩容失败（磁盘空间不足？）");
        return false;
    }

    char *p = reinterpret_cast<char *>(m_map) + m_appendOffset;
    putU32(p, kBlockMagic);
    putU64(p + 4, lineStart);
    putU32(p + 12, lineCount);
    putU32(p + 16, quint32(chunk.size()));
    putU32(p + 20, quint32(compSize));
    std::memcpy(p + kBlockHeaderSize, compressed.constData(), size_t(compSize));

    m_blocks.append({lineStart, lineCount, m_appendOffset, quint32(chunk.size()), quint32(compSize)});
    m_appendOffset += kBlockHeaderSize + compSize;
    return true;
}

QByteArray ZzMmapBuffer::decompressBlock(const BlockInfo &block) const
{
    if (const QByteArray *cached = m_blockCache.object(block.lineStart))
        return *cached;
    QByteArray out;
    out.resize(qsizetype(block.uncompSize));
    const int n = LZ4_decompress_safe(
        reinterpret_cast<const char *>(m_map) + block.fileOffset + kBlockHeaderSize,
        out.data(), int(block.compSize), int(block.uncompSize));
    if (n != int(block.uncompSize))
        return {}; // 数据损坏
    m_blockCache.insert(block.lineStart, new QByteArray(out), 1);
    return out;
}

qsizetype ZzMmapBuffer::findBlockIndex(quint64 lineId) const
{
    if (m_blocks.isEmpty() || lineId < m_blocks.first().lineStart)
        return -1;
    // 块表按 lineStart 递增，二分找最后一个 lineStart <= lineId 的块
    qsizetype lo = 0;
    qsizetype hi = m_blocks.size() - 1;
    qsizetype best = 0;
    while (lo <= hi) {
        const qsizetype mid = (lo + hi) / 2;
        if (m_blocks[mid].lineStart <= lineId) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    return best;
}
