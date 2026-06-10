/**
 * @file ZzCell.h
 * @brief 终端单元格数据结构。
 *
 * 表示终端屏幕上一个字符位置的完整渲染信息。该结构被解析器、
 * 文本缓冲区与渲染器共享，是终端核心层最基础的数据单元。
 */

#ifndef ZZ_CELL_H
#define ZZ_CELL_H

#include <cstdint>

namespace ZzCore {
namespace Terminal {

/**
 * @brief 颜色表示, 支持默认色 / 256 色索引 / 24 位真彩。
 */
struct ZzColor
{
    /** @brief 颜色类型。 */
    enum class Type : std::uint8_t
    {
        Default,  ///< 使用主题默认前景/背景色
        Indexed,  ///< 256 色调色板索引
        Rgb       ///< 24 位真彩色
    };

    Type type = Type::Default;  ///< 颜色类型
    std::uint8_t index = 0;     ///< 调色板索引 (type==Indexed 时有效)
    std::uint8_t r = 0;         ///< 红 (type==Rgb 时有效)
    std::uint8_t g = 0;         ///< 绿
    std::uint8_t b = 0;         ///< 蓝

    /**
     * @brief 构造一个 256 色索引颜色。
     * @param idx 调色板索引 (0-255)。
     */
    static constexpr ZzColor indexed(std::uint8_t idx)
    {
        ZzColor c;
        c.type = Type::Indexed;
        c.index = idx;
        return c;
    }

    /**
     * @brief 构造一个真彩色。
     */
    static constexpr ZzColor rgb(std::uint8_t red, std::uint8_t green, std::uint8_t blue)
    {
        ZzColor c;
        c.type = Type::Rgb;
        c.r = red;
        c.g = green;
        c.b = blue;
        return c;
    }

    constexpr bool operator==(const ZzColor&) const = default;
};

/**
 * @brief 文本样式属性位标志。
 */
enum ZzAttribute : std::uint16_t
{
    AttrNone = 0,
    AttrBold = 1u << 0,
    AttrItalic = 1u << 1,
    AttrUnderline = 1u << 2,
    AttrBlink = 1u << 3,
    AttrInverse = 1u << 4,
    AttrHidden = 1u << 5,
    AttrStrikethrough = 1u << 6
};

/**
 * @brief 终端单元格。
 *
 * 紧凑布局以降低大缓冲区的内存占用与提升缓存命中率。
 */
struct ZzCell
{
    char32_t codepoint = U' ';      ///< Unicode 码点
    ZzColor fg;                     ///< 前景色
    ZzColor bg;                     ///< 背景色
    std::uint16_t attributes = AttrNone;  ///< 样式属性 (ZzAttribute 组合)
    std::uint8_t width = 1;         ///< 字符宽度 (1=半角, 2=全角)

    constexpr bool operator==(const ZzCell&) const = default;
};

}  // namespace Terminal
}  // namespace ZzCore

#endif  // ZZ_CELL_H
