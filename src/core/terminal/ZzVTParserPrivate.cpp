/**
 * @file ZzVTParserPrivate.cpp
 * @brief ZzVTParser 状态机实现。
 */

#include "core/terminal/ZzVTParserPrivate.h"

#include <QList>

#include <algorithm>

#include "core/terminal/ZzTextBuffer.h"

namespace ZzCore {
namespace Terminal {

namespace {

/** @brief 取参数, 不存在时返回默认值。 */
int paramOr(const std::vector<int>& params, std::size_t index, int defaultValue)
{
    if (index < params.size() && params[index] > 0) {
        return params[index];
    }
    if (index < params.size()) {
        return params[index];  // 显式 0
    }
    return defaultValue;
}

/** @brief 粗略判断是否为占两列的宽字符 (CJK / 全角 / 常见 Emoji)。 */
bool isWideChar(char32_t cp)
{
    return (cp >= 0x1100 && cp <= 0x115F) ||   // Hangul Jamo
           (cp >= 0x2E80 && cp <= 0xA4CF) ||   // CJK 部首 ~ 彝文
           (cp >= 0xAC00 && cp <= 0xD7A3) ||   // Hangul 音节
           (cp >= 0xF900 && cp <= 0xFAFF) ||   // CJK 兼容表意
           (cp >= 0xFF00 && cp <= 0xFF60) ||   // 全角 ASCII
           (cp >= 0xFFE0 && cp <= 0xFFE6) ||
           (cp >= 0x1F300 && cp <= 0x1FAFF) || // Emoji
           (cp >= 0x20000 && cp <= 0x3FFFD);   // CJK 扩展 B+
}

/** @brief 16 色调色板基准索引 (前 8 普通)。 */
ZzColor sgrColor(int base, int code)
{
    return ZzColor::indexed(static_cast<std::uint8_t>(base + code));
}

}  // namespace

void ZzVTParserPrivate::reset()
{
    state = State::Ground;
    utf8Acc = 0;
    utf8Remaining = 0;
    csiBuffer.clear();
    csiPrivate = false;
    params.clear();
    oscBuffer.clear();
}

void ZzVTParserPrivate::feed(std::uint8_t byte)
{
    switch (state) {
        case State::Ground:
            handleGround(byte);
            break;

        case State::Escape:
            if (byte == '[') {
                state = State::CsiEntry;
                csiBuffer.clear();
                csiPrivate = false;
            } else if (byte == ']') {
                state = State::OscString;
                oscBuffer.clear();
            } else if (byte == 'c') {
                buffer.eraseInDisplay(2);
                buffer.setCursor(0, 0);
                state = State::Ground;
            } else {
                // 其它 ESC 序列 (字符集等) 暂忽略。
                state = State::Ground;
            }
            break;

        case State::CsiEntry:
            if (byte == '?') {
                csiPrivate = true;
            } else if ((byte >= 0x40 && byte <= 0x7E)) {
                parseParams();
                dispatchCsi(byte);
                state = State::Ground;
            } else {
                csiBuffer.append(static_cast<char>(byte));
            }
            break;

        case State::OscString:
            if (byte == 0x07) {  // BEL 终止
                state = State::Ground;
            } else if (byte == 0x1B) {
                // 可能为 ST (ESC \) 起始, 简化处理: 回到 Escape。
                state = State::Escape;
            } else {
                oscBuffer.append(static_cast<char>(byte));
            }
            break;
    }
}

void ZzVTParserPrivate::handleGround(std::uint8_t byte)
{
    if (byte == 0x1B) {
        state = State::Escape;
        return;
    }
    if (byte < 0x20) {
        handleControl(byte);
        return;
    }

    // UTF-8 解码。
    if (utf8Remaining > 0) {
        if ((byte & 0xC0) == 0x80) {
            utf8Acc = (utf8Acc << 6) | (byte & 0x3F);
            if (--utf8Remaining == 0) {
                emitCodepoint(utf8Acc);
            }
        } else {
            utf8Remaining = 0;  // 非法续接, 丢弃。
        }
        return;
    }

    if (byte < 0x80) {
        emitCodepoint(byte);
    } else if ((byte & 0xE0) == 0xC0) {
        utf8Acc = byte & 0x1F;
        utf8Remaining = 1;
    } else if ((byte & 0xF0) == 0xE0) {
        utf8Acc = byte & 0x0F;
        utf8Remaining = 2;
    } else if ((byte & 0xF8) == 0xF0) {
        utf8Acc = byte & 0x07;
        utf8Remaining = 3;
    }
}

bool ZzVTParserPrivate::handleControl(std::uint8_t byte)
{
    switch (byte) {
        case 0x08: buffer.backspace(); return true;            // BS
        case 0x09: buffer.tab(); return true;                  // HT
        case 0x0A:                                             // LF
        case 0x0B:                                             // VT
        case 0x0C: buffer.lineFeed(); return true;             // FF
        case 0x0D: buffer.carriageReturn(); return true;       // CR
        case 0x07: return true;                                // BEL
        default: return false;
    }
}

void ZzVTParserPrivate::emitCodepoint(char32_t codepoint)
{
    const int width = isWideChar(codepoint) ? 2 : 1;
    buffer.writeChar(codepoint, width);
}

void ZzVTParserPrivate::parseParams()
{
    params.clear();
    int current = 0;
    bool hasDigit = false;
    for (char ch : csiBuffer) {
        if (ch >= '0' && ch <= '9') {
            current = current * 10 + (ch - '0');
            hasDigit = true;
        } else if (ch == ';') {
            params.push_back(hasDigit ? current : 0);
            current = 0;
            hasDigit = false;
        }
        // 其它 (中间字节) 忽略。
    }
    if (hasDigit) {
        params.push_back(current);
    }
}

void ZzVTParserPrivate::dispatchCsi(std::uint8_t finalByte)
{
    const int curRow = buffer.cursorRow();
    const int curCol = buffer.cursorCol();

    switch (finalByte) {
        case 'A': buffer.moveCursor(-paramOr(params, 0, 1), 0); break;
        case 'B': buffer.moveCursor(paramOr(params, 0, 1), 0); break;
        case 'C': buffer.moveCursor(0, paramOr(params, 0, 1)); break;
        case 'D': buffer.moveCursor(0, -paramOr(params, 0, 1)); break;
        case 'E': buffer.setCursor(curRow + paramOr(params, 0, 1), 0); break;
        case 'F': buffer.setCursor(curRow - paramOr(params, 0, 1), 0); break;
        case 'G': buffer.setCursor(curRow, paramOr(params, 0, 1) - 1); break;
        case 'd': buffer.setCursor(paramOr(params, 0, 1) - 1, curCol); break;
        case 'H':
        case 'f':
            buffer.setCursor(paramOr(params, 0, 1) - 1, paramOr(params, 1, 1) - 1);
            break;
        case 'J': buffer.eraseInDisplay(paramOr(params, 0, 0)); break;
        case 'K': buffer.eraseInLine(paramOr(params, 0, 0)); break;
        case 'S': buffer.scrollUp(paramOr(params, 0, 1)); break;
        case 'T': buffer.scrollDown(paramOr(params, 0, 1)); break;
        case 'r':
            buffer.setScrollRegion(paramOr(params, 0, 1) - 1, paramOr(params, 1, 1) - 1);
            break;
        case 'm': applySgr(); break;
        case 'h':
            if (csiPrivate && paramOr(params, 0, 0) == 25) {
                buffer.setCursorVisible(true);
            }
            break;
        case 'l':
            if (csiPrivate && paramOr(params, 0, 0) == 25) {
                buffer.setCursorVisible(false);
            }
            break;
        default:
            break;  // 未实现的序列忽略。
    }
}

void ZzVTParserPrivate::applySgr()
{
    if (params.empty()) {
        buffer.resetPen();
        return;
    }

    for (std::size_t i = 0; i < params.size(); ++i) {
        const int code = params[i];
        switch (code) {
            case 0: buffer.resetPen(); break;
            case 1: buffer.addAttribute(AttrBold); break;
            case 3: buffer.addAttribute(AttrItalic); break;
            case 4: buffer.addAttribute(AttrUnderline); break;
            case 5: buffer.addAttribute(AttrBlink); break;
            case 7: buffer.addAttribute(AttrInverse); break;
            case 8: buffer.addAttribute(AttrHidden); break;
            case 9: buffer.addAttribute(AttrStrikethrough); break;
            case 22: buffer.removeAttribute(AttrBold); break;
            case 23: buffer.removeAttribute(AttrItalic); break;
            case 24: buffer.removeAttribute(AttrUnderline); break;
            case 27: buffer.removeAttribute(AttrInverse); break;
            case 29: buffer.removeAttribute(AttrStrikethrough); break;
            case 39: buffer.setForeground(ZzColor{}); break;
            case 49: buffer.setBackground(ZzColor{}); break;
            case 38:
            case 48: {
                // 38;5;n (256 色) 或 38;2;r;g;b (真彩)
                const bool fg = (code == 38);
                if (i + 1 < params.size() && params[i + 1] == 5 && i + 2 < params.size()) {
                    const auto idx = static_cast<std::uint8_t>(params[i + 2]);
                    fg ? buffer.setForeground(ZzColor::indexed(idx))
                       : buffer.setBackground(ZzColor::indexed(idx));
                    i += 2;
                } else if (i + 1 < params.size() && params[i + 1] == 2 && i + 4 < params.size()) {
                    const ZzColor c = ZzColor::rgb(static_cast<std::uint8_t>(params[i + 2]),
                                                   static_cast<std::uint8_t>(params[i + 3]),
                                                   static_cast<std::uint8_t>(params[i + 4]));
                    fg ? buffer.setForeground(c) : buffer.setBackground(c);
                    i += 4;
                }
                break;
            }
            default:
                if (code >= 30 && code <= 37) {
                    buffer.setForeground(sgrColor(0, code - 30));
                } else if (code >= 40 && code <= 47) {
                    buffer.setBackground(sgrColor(0, code - 40));
                } else if (code >= 90 && code <= 97) {
                    buffer.setForeground(sgrColor(8, code - 90));
                } else if (code >= 100 && code <= 107) {
                    buffer.setBackground(sgrColor(8, code - 100));
                }
                break;
        }
    }
}

}  // namespace Terminal
}  // namespace ZzCore
