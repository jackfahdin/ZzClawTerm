/**
 * @file ZzTerminalCorePrivate.h
 * @brief ZzTerminalCore 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_TERMINAL_CORE_PRIVATE_H
#define ZZ_TERMINAL_CORE_PRIVATE_H

#include <memory>

#include "core/terminal/ZzTextBuffer.h"
#include "core/terminal/ZzVTParser.h"

namespace ZzCore {
namespace Terminal {

/**
 * @brief 终端核心内部状态。
 */
class ZzTerminalCorePrivate
{
public:
    ZzTerminalCorePrivate() : parser(buffer) {}

    ZzTextBuffer buffer;  ///< 屏幕缓冲区
    ZzVTParser parser;    ///< 解析器 (绑定 buffer)
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_TERMINAL_CORE_PRIVATE_H
