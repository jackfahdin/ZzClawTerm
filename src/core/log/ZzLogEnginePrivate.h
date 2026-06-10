/**
 * @file ZzLogEnginePrivate.h
 * @brief ZzLogEngine 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_LOG_ENGINE_PRIVATE_H
#define ZZ_LOG_ENGINE_PRIVATE_H

#include <cstdint>
#include <memory>

#include "core/log/ZzColdStorage.h"
#include "core/log/ZzMmapBuffer.h"
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

    ZzRingBuffer hot;            ///< 热层环形缓冲 (最近若干行, 内存)
    ZzMmapBuffer warm;           ///< 温层 mmap + 压缩 (中段行, 加速)
    ZzColdStorage cold;          ///< 冷层 SQLite (全量持久 + 搜索)
    std::uint64_t nextLine = 0;  ///< 下一个全局行号
    bool coldOpen = false;       ///< 冷层是否已打开
    bool warmOpen = false;       ///< 温层是否已打开
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_LOG_ENGINE_PRIVATE_H
