/**
 * @file ZzLocalSessionPrivate.h
 * @brief ZzLocalSession 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_LOCAL_SESSION_PRIVATE_H
#define ZZ_LOCAL_SESSION_PRIVATE_H

#include <memory>

#include "platform/ZzPtyInterface.h"

namespace ZzCore {
namespace Terminal {
class ZzTerminalCore;
}
namespace Log {
class ZzLogEngine;
}
}  // namespace ZzCore

namespace ZzSession {

/**
 * @brief 会话内部组件聚合。
 */
class ZzLocalSessionPrivate
{
public:
    std::unique_ptr<ZzPlatform::ZzPtyInterface> pty;  ///< 伪终端
    ZzCore::Terminal::ZzTerminalCore* terminal = nullptr;  ///< 终端核心 (QObject 子对象)
    ZzCore::Log::ZzLogEngine* log = nullptr;               ///< 日志引擎 (QObject 子对象)
};

}  // namespace ZzSession

#endif  // ZZ_LOCAL_SESSION_PRIVATE_H
