/**
 * @file ZzLineIndexPrivate.h
 * @brief ZzLineIndex 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_LINE_INDEX_PRIVATE_H
#define ZZ_LINE_INDEX_PRIVATE_H

#include <cstdint>
#include <vector>

namespace ZzCore {
namespace Log {

/**
 * @brief 索引块条目。
 */
struct ZzBlockEntry
{
    std::uint64_t fileOffset;  ///< 块首行的字节偏移
    std::uint64_t lineNumber;  ///< 块首行的全局行号
};

/**
 * @brief 索引存储。
 */
class ZzLineIndexPrivate
{
public:
    std::vector<ZzBlockEntry> blocks;  ///< 块级索引
    std::uint64_t totalLines = 0;      ///< 总行数
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_LINE_INDEX_PRIVATE_H
