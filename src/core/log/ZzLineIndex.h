/**
 * @file ZzLineIndex.h
 * @brief 温数据层: 行偏移索引 (借鉴 klogg)。
 *
 * 每 BLOCK_SIZE 行记录一个块级文件偏移, 以极小内存换取对任意行的
 * 快速定位。1000 万行仅需约 80KB 索引。支持实时追加。
 */

#ifndef ZZ_LINE_INDEX_H
#define ZZ_LINE_INDEX_H

#include <cstdint>
#include <memory>

namespace ZzCore {
namespace Log {

class ZzLineIndexPrivate;

/**
 * @brief 分块行偏移索引。
 */
class ZzLineIndex
{
public:
    /** @brief 每个索引块包含的行数。 */
    static constexpr std::uint64_t kBlockSize = 1024;

    ZzLineIndex();
    ~ZzLineIndex();

    ZzLineIndex(const ZzLineIndex&) = delete;
    ZzLineIndex& operator=(const ZzLineIndex&) = delete;

    /**
     * @brief 追加一行 (记录其在温层文件中的起始字节偏移)。
     * @param byteOffset 该行起始字节偏移。
     */
    void appendLine(std::uint64_t byteOffset);

    /** @brief 已索引的总行数。 */
    std::uint64_t totalLines() const;

    /**
     * @brief 返回包含指定行的索引块的起始字节偏移。
     * @param lineNumber 全局行号 (0 基)。
     */
    std::uint64_t blockOffsetForLine(std::uint64_t lineNumber) const;

    /**
     * @brief 返回包含指定行的索引块的起始行号。
     */
    std::uint64_t blockStartLine(std::uint64_t lineNumber) const;

    /** @brief 清空索引。 */
    void clear();

private:
    std::unique_ptr<ZzLineIndexPrivate> d_ptr;
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_LINE_INDEX_H
