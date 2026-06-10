/**
 * @file ZzTextBufferPrivate.h
 * @brief ZzTextBuffer 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_TEXT_BUFFER_PRIVATE_H
#define ZZ_TEXT_BUFFER_PRIVATE_H

#include <QString>

#include <vector>

#include "core/terminal/ZzCell.h"
#include "core/terminal/ZzTextBuffer.h"

namespace ZzCore {
namespace Terminal {

/**
 * @brief 屏幕网格存储与光标/画笔状态。
 */
class ZzTextBufferPrivate
{
public:
    /** @brief 取可写单元引用 (调用方需保证坐标合法)。 */
    ZzCell& at(int row, int col);
    const ZzCell& at(int row, int col) const;

    /** @brief 用当前背景色清空一整行。 */
    void clearLine(int row);

    /** @brief 将光标钳制到合法范围。 */
    void clampCursor();

    /** @brief 滚动区域顶部一行滚出, 其余上移, 底部补空行。 */
    void scrollRegionUp(int lines, const ZzTextBuffer::ScrolledOffCallback& cb);
    void scrollRegionDown(int lines);

    int cols = 80;
    int rows = 24;
    std::vector<ZzCell> grid;  ///< 行优先扁平存储, 大小 = rows*cols

    int cursorRow = 0;
    int cursorCol = 0;
    bool cursorVisible = true;
    bool wrapPending = false;  ///< 光标处于行尾, 下一个字符触发换行

    ZzCell pen;  ///< 当前画笔 (颜色 + 属性), 写入时复制到单元

    int scrollTop = 0;             ///< 滚动区域顶 (含)
    int scrollBottom = 23;         ///< 滚动区域底 (含)

    ZzTextBuffer::ScrolledOffCallback scrolledOff;

    static const ZzCell kBlank;  ///< 越界访问返回的空白单元
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_TEXT_BUFFER_PRIVATE_H
