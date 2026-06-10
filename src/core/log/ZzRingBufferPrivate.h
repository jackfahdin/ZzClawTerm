/**
 * @file ZzRingBufferPrivate.h
 * @brief ZzRingBuffer 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_RING_BUFFER_PRIVATE_H
#define ZZ_RING_BUFFER_PRIVATE_H

#include <QString>

#include <cstddef>
#include <vector>

namespace ZzCore {
namespace Log {

/**
 * @brief 环形缓冲区底层存储。
 */
class ZzRingBufferPrivate
{
public:
    std::vector<QString> buffer;  ///< 固定大小存储
    std::size_t head = 0;         ///< 下一个写入位置
    std::size_t count = 0;        ///< 当前有效行数
    std::size_t capacity = 0;     ///< 容量
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_RING_BUFFER_PRIVATE_H
