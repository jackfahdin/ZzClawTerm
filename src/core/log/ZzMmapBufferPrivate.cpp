/**
 * @file ZzMmapBufferPrivate.cpp
 * @brief ZzMmapBuffer 私有实现 (压缩、分块落盘、内存映射读取、容量回收)。
 */

#include "core/log/ZzMmapBufferPrivate.h"

#include <QDataStream>
#include <QIODevice>

#if defined(ZZ_HAVE_LZ4)
#include <lz4.h>
#endif

namespace ZzCore {
namespace Log {

namespace {

/** @brief 压缩一个块 (默认 zlib, 定义 ZZ_HAVE_LZ4 时用 LZ4)。 */
QByteArray warmCompress(const QByteArray& raw)
{
#if defined(ZZ_HAVE_LZ4)
    const int srcSize = static_cast<int>(raw.size());
    const int bound = LZ4_compressBound(srcSize);
    QByteArray out(static_cast<qsizetype>(bound), Qt::Uninitialized);
    const int n = LZ4_compress_default(raw.constData(), out.data(), srcSize, bound);
    out.resize(static_cast<qsizetype>(n));
    return out;
#else
    return qCompress(raw, 1);
#endif
}

/** @brief 解压一个块。 */
QByteArray warmDecompress(const QByteArray& comp, std::uint32_t rawSize)
{
#if defined(ZZ_HAVE_LZ4)
    QByteArray out(static_cast<qsizetype>(rawSize), Qt::Uninitialized);
    const int n = LZ4_decompress_safe(comp.constData(), out.data(),
                                      static_cast<int>(comp.size()), static_cast<int>(rawSize));
    if (n < 0) {
        return {};
    }
    out.resize(static_cast<qsizetype>(n));
    return out;
#else
    Q_UNUSED(rawSize);
    return qUncompress(comp);
#endif
}

}  // namespace

void ZzMmapBufferPrivate::flushPending()
{
    if (pending.empty()) {
        return;
    }

    QByteArray raw;
    {
        QDataStream ds(&raw, QIODevice::WriteOnly);
        ds.setVersion(QDataStream::Qt_6_0);
        for (const QString& line : pending) {
            ds << line;
        }
    }

    const QByteArray comp = warmCompress(raw);

    ZzWarmBlock block;
    block.startLine = pendingStart;
    block.lineCount = static_cast<std::uint32_t>(pending.size());
    block.fileOffset = writeOffset;
    block.compSize = static_cast<std::uint32_t>(comp.size());
    block.rawSize = static_cast<std::uint32_t>(raw.size());

    file.seek(static_cast<qint64>(writeOffset));
    file.write(comp);
    file.flush();
    writeOffset += static_cast<std::uint64_t>(comp.size());

    blocks.push_back(block);
    pending.clear();
    pendingStart = next;

    enforceCapacity();
}

QStringList ZzMmapBufferPrivate::loadBlock(const ZzWarmBlock& block)
{
    QByteArray raw;

    uchar* mapped = file.map(static_cast<qint64>(block.fileOffset),
                             static_cast<qint64>(block.compSize));
    if (mapped) {
        const QByteArray comp = QByteArray::fromRawData(reinterpret_cast<const char*>(mapped),
                                                        static_cast<qsizetype>(block.compSize));
        raw = warmDecompress(comp, block.rawSize);
        file.unmap(mapped);
    } else {
        // 映射失败时回退到 seek+read。
        file.seek(static_cast<qint64>(block.fileOffset));
        const QByteArray comp = file.read(static_cast<qint64>(block.compSize));
        raw = warmDecompress(comp, block.rawSize);
    }

    QStringList out;
    QDataStream ds(raw);
    ds.setVersion(QDataStream::Qt_6_0);
    for (std::uint32_t i = 0; i < block.lineCount; ++i) {
        QString s;
        ds >> s;
        out.append(s);
    }
    return out;
}

void ZzMmapBufferPrivate::enforceCapacity()
{
    while (blocks.size() > 1 && (next - blocks.front().startLine) > capacityLines) {
        deadBytes += blocks.front().compSize;
        blocks.pop_front();
    }
    if (!blocks.empty() && deadBytes > writeOffset / 2) {
        compact();
    }
}

void ZzMmapBufferPrivate::compact()
{
    if (blocks.empty()) {
        return;
    }

    const QString tmpPath = filePath + QStringLiteral(".compact");
    QFile tmp(tmpPath);
    if (!tmp.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        return;  // compact 失败不影响正确性, 仅占用磁盘。
    }

    std::uint64_t newOffset = 0;
    for (ZzWarmBlock& block : blocks) {
        file.seek(static_cast<qint64>(block.fileOffset));
        const QByteArray comp = file.read(static_cast<qint64>(block.compSize));
        tmp.write(comp);
        block.fileOffset = newOffset;
        newOffset += static_cast<std::uint64_t>(block.compSize);
    }
    tmp.close();

    file.close();
    QFile::remove(filePath);
    QFile::rename(tmpPath, filePath);
    file.setFileName(filePath);
    if (!file.open(QIODevice::ReadWrite)) {
        return;  // 重开失败: 温层失效, 数据仍在冷层, 不影响正确性。
    }
    file.seek(static_cast<qint64>(newOffset));

    writeOffset = newOffset;
    deadBytes = 0;
    blockCache.clear();
}

}  // namespace Log
}  // namespace ZzCore
