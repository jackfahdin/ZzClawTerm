/**
 * @file ZzTextBuffer.cpp
 * @brief ZzTextBuffer 公开接口实现。
 */

#include "core/terminal/ZzTextBuffer.h"

#include <algorithm>

#include "core/terminal/ZzTextBufferPrivate.h"

namespace ZzCore {
namespace Terminal {

ZzTextBuffer::ZzTextBuffer() : d_ptr(std::make_unique<ZzTextBufferPrivate>())
{
    // 按 Private 默认尺寸 (80x24) 分配初始网格。
    d_ptr->grid.assign(
        static_cast<std::size_t>(d_ptr->cols) * static_cast<std::size_t>(d_ptr->rows), ZzCell{});
    d_ptr->scrollBottom = d_ptr->rows - 1;
}

ZzTextBuffer::~ZzTextBuffer() = default;

void ZzTextBuffer::resize(int cols, int rows)
{
    cols = std::max(1, cols);
    rows = std::max(1, rows);

    // 尺寸未变则不动, 避免无谓清空导致缩放时闪烁。
    if (cols == d_ptr->cols && rows == d_ptr->rows && !d_ptr->grid.empty()) {
        return;
    }

    // 重排时保留已有内容 (左上对齐), 而非清空整屏。
    std::vector<ZzCell> newGrid(static_cast<std::size_t>(cols) * static_cast<std::size_t>(rows),
                                ZzCell{});
    const int copyRows = std::min(d_ptr->rows, rows);
    const int copyCols = std::min(d_ptr->cols, cols);
    for (int r = 0; r < copyRows && !d_ptr->grid.empty(); ++r) {
        for (int c = 0; c < copyCols; ++c) {
            newGrid[static_cast<std::size_t>(r) * static_cast<std::size_t>(cols) +
                    static_cast<std::size_t>(c)] = d_ptr->at(r, c);
        }
    }

    d_ptr->grid = std::move(newGrid);
    d_ptr->cols = cols;
    d_ptr->rows = rows;
    d_ptr->scrollTop = 0;
    d_ptr->scrollBottom = rows - 1;
    d_ptr->clampCursor();
    d_ptr->wrapPending = false;
}

int ZzTextBuffer::cols() const
{
    return d_ptr->cols;
}

int ZzTextBuffer::rows() const
{
    return d_ptr->rows;
}

const ZzCell& ZzTextBuffer::cell(int row, int col) const
{
    if (row < 0 || row >= d_ptr->rows || col < 0 || col >= d_ptr->cols) {
        return ZzTextBufferPrivate::kBlank;
    }
    return d_ptr->at(row, col);
}

int ZzTextBuffer::cursorRow() const
{
    return d_ptr->cursorRow;
}

int ZzTextBuffer::cursorCol() const
{
    return d_ptr->cursorCol;
}

bool ZzTextBuffer::cursorVisible() const
{
    return d_ptr->cursorVisible;
}

void ZzTextBuffer::setCursorVisible(bool visible)
{
    d_ptr->cursorVisible = visible;
}

QString ZzTextBuffer::lineText(int row) const
{
    if (row < 0 || row >= d_ptr->rows) {
        return {};
    }
    QString text;
    text.reserve(d_ptr->cols);
    for (int c = 0; c < d_ptr->cols; ++c) {
        text.append(QChar(static_cast<char16_t>(d_ptr->at(row, c).codepoint)));
    }
    while (text.endsWith(QLatin1Char(' '))) {
        text.chop(1);
    }
    return text;
}

void ZzTextBuffer::writeChar(char32_t codepoint, int width)
{
    if (d_ptr->wrapPending) {
        carriageReturn();
        lineFeed();
        d_ptr->wrapPending = false;
    }

    ZzCell& target = d_ptr->at(d_ptr->cursorRow, d_ptr->cursorCol);
    target = d_ptr->pen;
    target.codepoint = codepoint;
    target.width = static_cast<std::uint8_t>(std::max(1, width));

    d_ptr->cursorCol += target.width;
    if (d_ptr->cursorCol >= d_ptr->cols) {
        d_ptr->cursorCol = d_ptr->cols - 1;
        d_ptr->wrapPending = true;
    }
}

void ZzTextBuffer::carriageReturn()
{
    d_ptr->cursorCol = 0;
    d_ptr->wrapPending = false;
}

void ZzTextBuffer::lineFeed()
{
    if (d_ptr->cursorRow == d_ptr->scrollBottom) {
        d_ptr->scrollRegionUp(1, d_ptr->scrolledOff);
    } else if (d_ptr->cursorRow < d_ptr->rows - 1) {
        ++d_ptr->cursorRow;
    }
    d_ptr->wrapPending = false;
}

void ZzTextBuffer::backspace()
{
    if (d_ptr->cursorCol > 0) {
        --d_ptr->cursorCol;
    }
    d_ptr->wrapPending = false;
}

void ZzTextBuffer::tab()
{
    const int next = ((d_ptr->cursorCol / 8) + 1) * 8;
    d_ptr->cursorCol = std::min(next, d_ptr->cols - 1);
    d_ptr->wrapPending = false;
}

void ZzTextBuffer::setCursor(int row, int col)
{
    d_ptr->cursorRow = row;
    d_ptr->cursorCol = col;
    d_ptr->clampCursor();
    d_ptr->wrapPending = false;
}

void ZzTextBuffer::moveCursor(int deltaRow, int deltaCol)
{
    d_ptr->cursorRow += deltaRow;
    d_ptr->cursorCol += deltaCol;
    d_ptr->clampCursor();
    d_ptr->wrapPending = false;
}

void ZzTextBuffer::eraseInLine(int mode)
{
    const int row = d_ptr->cursorRow;
    ZzCell blank;
    blank.bg = d_ptr->pen.bg;
    if (mode == 0) {
        for (int c = d_ptr->cursorCol; c < d_ptr->cols; ++c) d_ptr->at(row, c) = blank;
    } else if (mode == 1) {
        for (int c = 0; c <= d_ptr->cursorCol && c < d_ptr->cols; ++c) d_ptr->at(row, c) = blank;
    } else if (mode == 2) {
        d_ptr->clearLine(row);
    }
}

void ZzTextBuffer::eraseInDisplay(int mode)
{
    if (mode == 0) {
        eraseInLine(0);
        for (int r = d_ptr->cursorRow + 1; r < d_ptr->rows; ++r) d_ptr->clearLine(r);
    } else if (mode == 1) {
        eraseInLine(1);
        for (int r = 0; r < d_ptr->cursorRow; ++r) d_ptr->clearLine(r);
    } else if (mode == 2 || mode == 3) {
        for (int r = 0; r < d_ptr->rows; ++r) d_ptr->clearLine(r);
    }
}

void ZzTextBuffer::setForeground(const ZzColor& color)
{
    d_ptr->pen.fg = color;
}

void ZzTextBuffer::setBackground(const ZzColor& color)
{
    d_ptr->pen.bg = color;
}

void ZzTextBuffer::addAttribute(ZzAttribute attr)
{
    d_ptr->pen.attributes |= static_cast<std::uint16_t>(attr);
}

void ZzTextBuffer::removeAttribute(ZzAttribute attr)
{
    d_ptr->pen.attributes &= static_cast<std::uint16_t>(~static_cast<std::uint16_t>(attr));
}

void ZzTextBuffer::resetPen()
{
    d_ptr->pen = ZzCell{};
}

void ZzTextBuffer::setScrollRegion(int top, int bottom)
{
    if (top < 0) top = 0;
    if (bottom >= d_ptr->rows) bottom = d_ptr->rows - 1;
    if (top < bottom) {
        d_ptr->scrollTop = top;
        d_ptr->scrollBottom = bottom;
        setCursor(0, 0);
    }
}

void ZzTextBuffer::scrollUp(int lines)
{
    d_ptr->scrollRegionUp(lines, d_ptr->scrolledOff);
}

void ZzTextBuffer::scrollDown(int lines)
{
    d_ptr->scrollRegionDown(lines);
}

void ZzTextBuffer::setScrolledOffCallback(ScrolledOffCallback callback)
{
    d_ptr->scrolledOff = std::move(callback);
}

}  // namespace Terminal
}  // namespace ZzCore
