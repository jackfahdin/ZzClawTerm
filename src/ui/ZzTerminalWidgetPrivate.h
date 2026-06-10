/**
 * @file ZzTerminalWidgetPrivate.h
 * @brief ZzTerminalWidget 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_TERMINAL_WIDGET_PRIVATE_H
#define ZZ_TERMINAL_WIDGET_PRIVATE_H

#include <QByteArray>
#include <QFont>
#include <QFontMetricsF>

#include "business/ZzColorScheme.h"

class QKeyEvent;

namespace ZzCore {
namespace Terminal {
class ZzTerminalCore;
}
}  // namespace ZzCore

namespace ZzUi {

/**
 * @brief 终端控件内部状态与渲染参数。
 */
class ZzTerminalWidgetPrivate
{
public:
    ZzTerminalWidgetPrivate() : metrics(font) {}

    /** @brief 根据当前字体重算单元宽高与基线。 */
    void recomputeCellMetrics();

    /** @brief 将按键事件转换为终端字节序列。 */
    static QByteArray translateKey(const QKeyEvent* event);

    ZzCore::Terminal::ZzTerminalCore* core = nullptr;  ///< 终端核心 (弱引用)
    ZzBusiness::ZzColorScheme scheme;                  ///< 配色方案

    QFont font{QStringLiteral("monospace"), 12};
    QFontMetricsF metrics;
    qreal cellWidth = 8.0;
    qreal cellHeight = 16.0;
    qreal baseline = 12.0;
};

}  // namespace ZzUi

#endif  // ZZ_TERMINAL_WIDGET_PRIVATE_H
