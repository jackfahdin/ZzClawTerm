/**
 * @file ZzVTParser.h
 * @brief VT100 / xterm-256color / TrueColor 转义序列解析器。
 *
 * 接收原始字节流, 解析 ANSI/xterm 转义序列并驱动 ZzTextBuffer 更新。
 * 内部为查表式状态机, 支持跨数据块的 UTF-8 与序列续接。
 */

#ifndef ZZ_VT_PARSER_H
#define ZZ_VT_PARSER_H

#include <QByteArray>

#include <memory>

namespace ZzCore {
namespace Terminal {

class ZzTextBuffer;
class ZzVTParserPrivate;

/**
 * @brief 终端转义序列解析器。
 */
class ZzVTParser
{
public:
    /**
     * @brief 构造解析器。
     * @param buffer 目标文本缓冲区 (生命周期需长于解析器)。
     */
    explicit ZzVTParser(ZzTextBuffer& buffer);
    ~ZzVTParser();

    ZzVTParser(const ZzVTParser&) = delete;
    ZzVTParser& operator=(const ZzVTParser&) = delete;

    /**
     * @brief 解析一段输入数据。
     * @param data 原始字节流 (可能含不完整的多字节字符或序列)。
     */
    void parse(const QByteArray& data);

    /** @brief 重置状态机到初始 (Ground) 状态。 */
    void reset();

private:
    std::unique_ptr<ZzVTParserPrivate> d_ptr;
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_VT_PARSER_H
