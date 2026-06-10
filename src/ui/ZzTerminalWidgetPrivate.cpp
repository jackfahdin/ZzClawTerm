/**
 * @file ZzTerminalWidgetPrivate.cpp
 * @brief ZzTerminalWidget 私有实现 (单元度量与按键转换)。
 */

#include "ui/ZzTerminalWidgetPrivate.h"

#include <QKeyEvent>

namespace ZzUi {

void ZzTerminalWidgetPrivate::recomputeCellMetrics()
{
    metrics = QFontMetricsF(font);
    cellWidth = metrics.horizontalAdvance(QLatin1Char('M'));
    if (cellWidth < 1.0) {
        cellWidth = metrics.averageCharWidth();
    }
    cellHeight = metrics.height();
    baseline = metrics.ascent();
}

QByteArray ZzTerminalWidgetPrivate::translateKey(const QKeyEvent* event)
{
    switch (event->key()) {
        case Qt::Key_Return:
        case Qt::Key_Enter:
            return QByteArrayLiteral("\r");
        case Qt::Key_Backspace:
            return QByteArray(1, '\x7f');
        case Qt::Key_Tab:
            return QByteArrayLiteral("\t");
        case Qt::Key_Escape:
            return QByteArray(1, '\x1b');
        case Qt::Key_Up:
            return QByteArrayLiteral("\x1b[A");
        case Qt::Key_Down:
            return QByteArrayLiteral("\x1b[B");
        case Qt::Key_Right:
            return QByteArrayLiteral("\x1b[C");
        case Qt::Key_Left:
            return QByteArrayLiteral("\x1b[D");
        case Qt::Key_Home:
            return QByteArrayLiteral("\x1b[H");
        case Qt::Key_End:
            return QByteArrayLiteral("\x1b[F");
        case Qt::Key_PageUp:
            return QByteArrayLiteral("\x1b[5~");
        case Qt::Key_PageDown:
            return QByteArrayLiteral("\x1b[6~");
        case Qt::Key_Delete:
            return QByteArrayLiteral("\x1b[3~");
        default:
            break;
    }

    // Ctrl + 字母 → 控制字符。
    if (event->modifiers().testFlag(Qt::ControlModifier)) {
        const QString text = event->text();
        if (!text.isEmpty()) {
            const QChar ch = text.at(0).toUpper();
            if (ch >= QLatin1Char('A') && ch <= QLatin1Char('_')) {
                return QByteArray(1, static_cast<char>(ch.toLatin1() - 'A' + 1));
            }
        }
    }

    const QString text = event->text();
    if (!text.isEmpty()) {
        return text.toUtf8();
    }
    return {};
}

}  // namespace ZzUi
