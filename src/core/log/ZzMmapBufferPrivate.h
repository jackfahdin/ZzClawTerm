/**
 * @file ZzMmapBufferPrivate.h
 * @brief ZzMmapBuffer 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_MMAP_BUFFER_PRIVATE_H
#define ZZ_MMAP_BUFFER_PRIVATE_H

#include <QFile>
#include <QString>

#include <cstdint>
#include <deque>
#include <vector>

#include "utils/ZzLRUCache.h"

namespace ZzCore {
namespace Log {

/**
 * @brief 温层压缩块的元数据 (仅索引, 不含数据)。
 */
struct ZzWarmBlock
{
    std::uint64_t startLine = 0;   ///< 块首行全局行号
    std::uint32_t lineCount = 0;   ///< 块内行数
    std::uint64_t fileOffset = 0;  ///< 压缩数据在文件中的偏移
    std::uint32_t compSize = 0;    ///< 压缩后字节数
    std::uint32_t rawSize = 0;     ///< 压缩前字节数 (LZ4 解压需要)
};

/**
 * @brief 温层内部状态与底层文件操作。
 */
class ZzMmapBufferPrivate
{
public:
    ZzMmapBufferPrivate() : blockCache(64) {}

    /** @brief 将当前待写块压缩落盘。 */
    void flushPending();
    /** @brief 读取并解压指定块为行列表。 */
    QStringList loadBlock(const ZzWarmBlock& block);
    /** @brief 淘汰最老块并在死区过多时 compact。 */
    void enforceCapacity();
    /** @brief 重写文件, 仅保留存活块, 回收死区。 */
    void compact();

    QFile file;                       ///< 温层数据文件
    QString filePath;                 ///< 文件路径
    std::uint64_t writeOffset = 0;    ///< 下一次写入的文件偏移
    std::uint64_t deadBytes = 0;      ///< 已淘汰块占用的字节 (待回收)

    std::deque<ZzWarmBlock> blocks;   ///< 已落盘块索引 (按行号升序)

    std::vector<QString> pending;     ///< 尚未满块的待写行
    std::uint64_t pendingStart = 0;   ///< 待写块首行全局行号
    std::uint64_t next = 0;           ///< 下一个待追加行号

    std::uint64_t capacityLines = 1000000;  ///< 温层容量 (行)

    ZzUtils::ZzLRUCache<std::uint64_t, QStringList> blockCache;  ///< 块解压缓存 (键=startLine)
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_MMAP_BUFFER_PRIVATE_H
