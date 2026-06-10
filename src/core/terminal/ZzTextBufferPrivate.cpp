/**
 * @file ZzTextBufferPrivate.cpp
 * @brief ZzTextBuffer 私有实现。
 */

#include "core/terminal/ZzTextBufferPrivate.h"

#include <algorithm>

namespace ZzCore {
namespace Terminal {

const ZzCell ZzTextBufferPrivate::kBlank{};

ZzCell& ZzTextBufferPrivate::at(int row, int col)
{
    return grid[static_cast<std::size_t>(row) * static_cast<std::size_t>(cols) +
                static_cast<std::size_t>(col)];
}

const ZzCell& ZzTextBufferPrivate::at(int row, int col) const
{
    return grid[static_cast<std::size_t>(row) * static_cast<std::size_t>(cols) +
                static_cast<std::size_t>(col)];
}

void ZzTextBufferPrivate::clearLine(int row)
{
    ZzCell blank;
    blank.bg = pen.bg;
    for (int c = 0; c < cols; ++c) {
        at(row, c) = blank;
    }
}

void ZzTextBufferPrivate::clampCursor()
{
    cursorRow = std::clamp(cursorRow, 0, rows - 1);
    cursorCol = std::clamp(cursorCol, 0, cols - 1);
}

void ZzTextBufferPrivate::scrollRegionUp(int lines, const ZzTextBuffer::ScrolledOffCallback& cb)
{
    lines = std::clamp(lines, 0, scrollBottom - scrollTop + 1);
    for (int i = 0; i < lines; ++i) {
        // 顶部行滚出: 上报后整体上移。
        if (cb && scrollTop == 0) {
            QString text;
            text.reserve(cols);
            for (int c = 0; c < cols; ++c) {
                text.append(QChar(static_cast<char16_t>(at(scrollTop, c).codepoint)));
            }
            while (text.endsWith(QLatin1Char(' '))) {
                text.chop(1);
            }
            cb(text);
        }
        for (int row = scrollTop; row < scrollBottom; ++row) {
            for (int c = 0; c < cols; ++c) {
                at(row, c) = at(row + 1, c);
            }
        }
        clearLine(scrollBottom);
    }
}

void ZzTextBufferPrivate::scrollRegionDown(int lines)
{
    lines = std::clamp(lines, 0, scrollBottom - scrollTop + 1);
    for (int i = 0; i < lines; ++i) {
        for (int row = scrollBottom; row > scrollTop; --row) {
            for (int c = 0; c < cols; ++c) {
                at(row, c) = at(row - 1, c);
            }
        }
        clearLine(scrollTop);
    }
}

}  // namespace Terminal
}  // namespace ZzCore
