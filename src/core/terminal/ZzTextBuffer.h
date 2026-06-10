/**
 * @file ZzTextBuffer.h
 * @brief 终端当前屏文本缓冲区。
 *
 * 维护可见屏幕的 ZzCell 网格、光标位置与当前画笔属性。滚出顶部
 * 的行通过回调上报 (交由日志引擎归档)。滚动历史不在本类中保存。
 */

#ifndef ZZ_TEXT_BUFFER_H
#define ZZ_TEXT_BUFFER_H

#include <QString>

#include <functional>
#include <memory>

#include "core/terminal/ZzCell.h"

namespace ZzCore {
namespace Terminal {

class ZzTextBufferPrivate;

/**
 * @brief 终端屏幕缓冲区。
 *
 * 非 QObject 的纯数据类, 由 ZzTerminalCore 持有。所有坐标均为
 * 0 基, row 自顶向下, col 自左向右。
 */
class ZzTextBuffer
{
public:
    /** @brief 滚出行回调签名 (参数为该行纯文本)。 */
    using ScrolledOffCallback = std::function<void(const QString&)>;

    ZzTextBuffer();
    ~ZzTextBuffer();

    ZzTextBuffer(const ZzTextBuffer&) = delete;
    ZzTextBuffer& operator=(const ZzTextBuffer&) = delete;

    /** @brief 重设屏幕尺寸 (列数, 行数)。 */
    void resize(int cols, int rows);

    int cols() const;
    int rows() const;

    /** @brief 只读访问单元格 (越界返回空白单元)。 */
    const ZzCell& cell(int row, int col) const;

    int cursorRow() const;
    int cursorCol() const;
    bool cursorVisible() const;
    void setCursorVisible(bool visible);

    /** @brief 取某行纯文本 (去除尾部空白)。 */
    QString lineText(int row) const;

    // ── 以下方法由 VT 解析器驱动 ──

    /** @brief 在光标处写入一个字符并前移光标。 */
    void writeChar(char32_t codepoint, int width);
    void carriageReturn();
    void lineFeed();
    void backspace();
    void tab();

    void setCursor(int row, int col);
    void moveCursor(int deltaRow, int deltaCol);

    /** @brief 行内擦除: 0=至行尾, 1=至行首, 2=整行。 */
    void eraseInLine(int mode);
    /** @brief 屏幕擦除: 0=至屏尾, 1=至屏首, 2=整屏。 */
    void eraseInDisplay(int mode);

    void setForeground(const ZzColor& color);
    void setBackground(const ZzColor& color);
    void addAttribute(ZzAttribute attr);
    void removeAttribute(ZzAttribute attr);
    void resetPen();

    void setScrollRegion(int top, int bottom);
    void scrollUp(int lines);
    void scrollDown(int lines);

    /** @brief 设置滚出行回调。 */
    void setScrolledOffCallback(ScrolledOffCallback callback);

private:
    std::unique_ptr<ZzTextBufferPrivate> d_ptr;
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_TEXT_BUFFER_H
