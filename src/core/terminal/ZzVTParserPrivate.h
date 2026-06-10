/**
 * @file ZzVTParserPrivate.h
 * @brief ZzVTParser 私有状态机声明 (Pimpl)。
 */

#ifndef ZZ_VT_PARSER_PRIVATE_H
#define ZZ_VT_PARSER_PRIVATE_H

#include <QByteArray>

#include <cstdint>
#include <vector>

namespace ZzCore {
namespace Terminal {

class ZzTextBuffer;

/**
 * @brief 解析器状态机实现。
 */
class ZzVTParserPrivate
{
public:
    explicit ZzVTParserPrivate(ZzTextBuffer& targetBuffer) : buffer(targetBuffer) {}

    /** @brief 状态机状态。 */
    enum class State
    {
        Ground,        ///< 普通文本
        Escape,        ///< 已读到 ESC
        CsiEntry,      ///< 已读到 ESC[
        OscString      ///< 已读到 ESC] (操作系统命令)
    };

    void feed(std::uint8_t byte);
    void reset();

    /** @brief 处理 Ground 状态的一个字节。 */
    void handleGround(std::uint8_t byte);
    /** @brief 处理一个 C0 控制字符, 返回是否已消费。 */
    bool handleControl(std::uint8_t byte);
    /** @brief 完成一个 CSI 序列 (final 为终止字符)。 */
    void dispatchCsi(std::uint8_t finalByte);
    /** @brief 处理 SGR (选择图形再现) 参数。 */
    void applySgr();
    /** @brief 将累积的 UTF-8 码点写入缓冲区。 */
    void emitCodepoint(char32_t codepoint);

    /** @brief 解析 CSI 参数串到 params。 */
    void parseParams();

    ZzTextBuffer& buffer;
    State state = State::Ground;

    // UTF-8 解码状态
    char32_t utf8Acc = 0;       ///< 累积码点
    int utf8Remaining = 0;      ///< 剩余续接字节数

    // CSI 收集
    QByteArray csiBuffer;       ///< CSI 参数与中间字节原文
    bool csiPrivate = false;    ///< 是否为 ? 私有序列
    std::vector<int> params;    ///< 解析后的数值参数

    QByteArray oscBuffer;       ///< OSC 字符串
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_VT_PARSER_PRIVATE_H
