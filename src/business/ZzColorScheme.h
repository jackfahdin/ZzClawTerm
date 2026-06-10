/**
 * @file ZzColorScheme.h
 * @brief 终端配色方案 (主题) 数据结构与解析工具。
 */

#ifndef ZZ_COLOR_SCHEME_H
#define ZZ_COLOR_SCHEME_H

#include <QColor>
#include <QString>

#include <array>

#include "core/terminal/ZzCell.h"

namespace ZzBusiness {

/**
 * @brief 配色方案。
 *
 * 包含 16 色基础调色板、默认前景/背景与光标色。256 色与真彩
 * 由 resolve 动态计算。
 */
struct ZzColorScheme
{
    QString name = QStringLiteral("Default");
    QColor background = QColor(0x28, 0x2a, 0x36);
    QColor foreground = QColor(0xf8, 0xf8, 0xf2);
    QColor cursor = QColor(0xf8, 0xf8, 0xf2);
    std::array<QColor, 16> palette{};

    /**
     * @brief 将终端逻辑颜色解析为实际 QColor。
     * @param color 终端颜色 (默认/索引/真彩)。
     * @param isForeground 默认色时是否取前景。
     */
    QColor resolve(const ZzCore::Terminal::ZzColor& color, bool isForeground) const;
};

/** @brief 返回内置 Dracula 配色。 */
ZzColorScheme zzDraculaScheme();

/** @brief 返回内置 Nord 配色。 */
ZzColorScheme zzNordScheme();

}  // namespace ZzBusiness

#endif  // ZZ_COLOR_SCHEME_H
