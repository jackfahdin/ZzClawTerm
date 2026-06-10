/**
 * @file ZzLogEnginePrivate.h
 * @brief ZzLogEngine 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_LOG_ENGINE_PRIVATE_H
#define ZZ_LOG_ENGINE_PRIVATE_H

#include <cstdint>
#include <memory>

#include "core/log/ZzColdStorage.h"
#include "core/log/ZzLineIndex.h"
#include "core/log/ZzRingBuffer.h"

namespace ZzCore {
namespace Log {

/**
 * @brief 日志引擎内部状态。
 */
class ZzLogEnginePrivate
{
public:
    ZzLogEnginePrivate() : hot(10000) {}

    ZzRingBuffer hot;           ///< 热层环形缓冲
    ZzColdStorage cold;         ///< 冷层 SQLite
    ZzLineIndex index;          ///< 温层行偏移索引
    std::uint64_t nextLine = 0; ///< 下一个全局行号
    std::uint64_t byteOffset = 0;  ///< 温层累计字节偏移 (索引用)
    bool opened = false;        ///< 冷层是否已打开
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_LOG_ENGINE_PRIVATE_H
