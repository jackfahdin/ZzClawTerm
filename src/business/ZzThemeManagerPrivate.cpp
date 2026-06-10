/**
 * @file ZzThemeManagerPrivate.cpp
 * @brief ZzColorScheme 解析与内置配色方案实现。
 */

#include "business/ZzColorScheme.h"
#include "business/ZzThemeManagerPrivate.h"

namespace ZzBusiness {

QColor ZzColorScheme::resolve(const ZzCore::Terminal::ZzColor& color, bool isForeground) const
{
    using Type = ZzCore::Terminal::ZzColor::Type;
    switch (color.type) {
        case Type::Default:
            return isForeground ? foreground : background;

        case Type::Rgb:
            return QColor(color.r, color.g, color.b);

        case Type::Indexed: {
            const int idx = color.index;
            if (idx < 16) {
                return palette[static_cast<std::size_t>(idx)];
            }
            if (idx >= 16 && idx <= 231) {
                const int n = idx - 16;
                const int r = n / 36;
                const int g = (n / 6) % 6;
                const int b = n % 6;
                const auto comp = [](int v) { return v == 0 ? 0 : 55 + 40 * v; };
                return QColor(comp(r), comp(g), comp(b));
            }
            // 232-255 灰阶。
            const int level = 8 + (idx - 232) * 10;
            return QColor(level, level, level);
        }
    }
    return isForeground ? foreground : background;
}

ZzColorScheme zzDraculaScheme()
{
    ZzColorScheme s;
    s.name = QStringLiteral("Dracula");
    s.background = QColor(0x28, 0x2a, 0x36);
    s.foreground = QColor(0xf8, 0xf8, 0xf2);
    s.cursor = QColor(0xf8, 0xf8, 0xf2);
    s.palette = {
        QColor(0x21, 0x22, 0x2c), QColor(0xff, 0x55, 0x55), QColor(0x50, 0xfa, 0x7b),
        QColor(0xf1, 0xfa, 0x8c), QColor(0xbd, 0x93, 0xf9), QColor(0xff, 0x79, 0xc6),
        QColor(0x8b, 0xe9, 0xfd), QColor(0xf8, 0xf8, 0xf2), QColor(0x62, 0x72, 0xa4),
        QColor(0xff, 0x6e, 0x6e), QColor(0x69, 0xff, 0x94), QColor(0xff, 0xff, 0xa5),
        QColor(0xd6, 0xac, 0xff), QColor(0xff, 0x92, 0xdf), QColor(0xa4, 0xff, 0xff),
        QColor(0xff, 0xff, 0xff)};
    return s;
}

ZzColorScheme zzNordScheme()
{
    ZzColorScheme s;
    s.name = QStringLiteral("Nord");
    s.background = QColor(0x2e, 0x34, 0x40);
    s.foreground = QColor(0xd8, 0xde, 0xe9);
    s.cursor = QColor(0xd8, 0xde, 0xe9);
    s.palette = {
        QColor(0x3b, 0x42, 0x52), QColor(0xbf, 0x61, 0x6a), QColor(0xa3, 0xbe, 0x8c),
        QColor(0xeb, 0xcb, 0x8b), QColor(0x81, 0xa1, 0xc1), QColor(0xb4, 0x8e, 0xad),
        QColor(0x88, 0xc0, 0xd0), QColor(0xe5, 0xe9, 0xf0), QColor(0x4c, 0x56, 0x6a),
        QColor(0xbf, 0x61, 0x6a), QColor(0xa3, 0xbe, 0x8c), QColor(0xeb, 0xcb, 0x8b),
        QColor(0x81, 0xa1, 0xc1), QColor(0xb4, 0x8e, 0xad), QColor(0x8f, 0xbc, 0xbb),
        QColor(0xec, 0xef, 0xf4)};
    return s;
}

}  // namespace ZzBusiness
