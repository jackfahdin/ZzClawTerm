/**
 * @file ZzTerminalWidget.cpp
 * @brief ZzTerminalWidget 公开接口与 QPainter 渲染实现。
 */

#include "ui/ZzTerminalWidget.h"

#include <QKeyEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QResizeEvent>

#include <algorithm>
#include <cmath>
#include <utility>

#include "core/terminal/ZzCell.h"
#include "core/terminal/ZzTerminalCore.h"
#include "core/terminal/ZzTextBuffer.h"
#include "ui/ZzTerminalWidgetPrivate.h"

namespace ZzUi {

using ZzCore::Terminal::ZzAttribute;
using ZzCore::Terminal::ZzCell;
using ZzCore::Terminal::ZzTextBuffer;

ZzTerminalWidget::ZzTerminalWidget(QWidget* parent)
    : QWidget(parent), d_ptr(std::make_unique<ZzTerminalWidgetPrivate>())
{
    setFocusPolicy(Qt::StrongFocus);
    // paintEvent 会铺满整个区域, 关闭系统背景擦除以消除缩放时的黑白闪烁。
    setAttribute(Qt::WA_OpaquePaintEvent);
    setAttribute(Qt::WA_NoSystemBackground);
    d_ptr->scheme = ZzBusiness::zzDraculaScheme();
    d_ptr->font = QFont(QStringLiteral("monospace"), 12);
    d_ptr->font.setStyleHint(QFont::Monospace);
    d_ptr->recomputeCellMetrics();
}

ZzTerminalWidget::~ZzTerminalWidget() = default;

void ZzTerminalWidget::setTerminalCore(ZzCore::Terminal::ZzTerminalCore* core)
{
    d_ptr->core = core;
    if (core) {
        connect(core, &ZzCore::Terminal::ZzTerminalCore::contentChanged, this,
                qOverload<>(&QWidget::update));
    }
    update();
}

void ZzTerminalWidget::setColorScheme(const ZzBusiness::ZzColorScheme& scheme)
{
    d_ptr->scheme = scheme;
    update();
}

void ZzTerminalWidget::setTerminalFont(const QFont& font)
{
    d_ptr->font = font;
    d_ptr->recomputeCellMetrics();
    update();
}

QSize ZzTerminalWidget::viewportCells() const
{
    const int cols = std::max(1, static_cast<int>(width() / d_ptr->cellWidth));
    const int rows = std::max(1, static_cast<int>(height() / d_ptr->cellHeight));
    return {cols, rows};
}

void ZzTerminalWidget::paintEvent(QPaintEvent* /*event*/)
{
    QPainter painter(this);
    painter.fillRect(rect(), d_ptr->scheme.background);

    if (!d_ptr->core) {
        return;
    }

    painter.setFont(d_ptr->font);
    const ZzTextBuffer& buf = d_ptr->core->buffer();
    const qreal cw = d_ptr->cellWidth;
    const qreal ch = d_ptr->cellHeight;

    for (int row = 0; row < buf.rows(); ++row) {
        for (int col = 0; col < buf.cols(); ++col) {
            const ZzCell& cell = buf.cell(row, col);
            const bool inverse = (cell.attributes & ZzAttribute::AttrInverse) != 0;

            QColor fg = d_ptr->scheme.resolve(cell.fg, true);
            QColor bg = d_ptr->scheme.resolve(cell.bg, false);
            if (inverse) {
                std::swap(fg, bg);
            }

            const QRectF cellRect(col * cw, row * ch, cw * cell.width, ch);

            if (bg != d_ptr->scheme.background || inverse) {
                painter.fillRect(cellRect, bg);
            }

            if (cell.codepoint != U' ' && cell.codepoint != 0) {
                if (cell.attributes & ZzAttribute::AttrBold) {
                    QFont f = d_ptr->font;
                    f.setBold(true);
                    painter.setFont(f);
                } else {
                    painter.setFont(d_ptr->font);
                }
                painter.setPen(fg);
                const QString glyph = QString::fromUcs4(&cell.codepoint, 1);
                painter.drawText(QPointF(col * cw, row * ch + d_ptr->baseline), glyph);
            }
        }
    }

    // 绘制光标 (块状)。
    if (buf.cursorVisible()) {
        const QRectF cursorRect(buf.cursorCol() * cw, buf.cursorRow() * ch, cw, ch);
        painter.fillRect(cursorRect, d_ptr->scheme.cursor);
        const ZzCell& under = buf.cell(buf.cursorRow(), buf.cursorCol());
        if (under.codepoint != U' ' && under.codepoint != 0) {
            painter.setPen(d_ptr->scheme.background);
            painter.drawText(QPointF(buf.cursorCol() * cw, buf.cursorRow() * ch + d_ptr->baseline),
                             QString::fromUcs4(&under.codepoint, 1));
        }
    }
}

void ZzTerminalWidget::keyPressEvent(QKeyEvent* event)
{
    const QByteArray bytes = ZzTerminalWidgetPrivate::translateKey(event);
    if (!bytes.isEmpty()) {
        emit keyInput(bytes);
        event->accept();
        return;
    }
    QWidget::keyPressEvent(event);
}

void ZzTerminalWidget::resizeEvent(QResizeEvent* /*event*/)
{
    const QSize cells = viewportCells();
    // 仅当字符网格 (列/行数) 真正变化时才重设终端, 避免像素级缩放反复重排。
    if (cells.width() != d_ptr->lastCols || cells.height() != d_ptr->lastRows) {
        d_ptr->lastCols = cells.width();
        d_ptr->lastRows = cells.height();
        emit viewportResized(cells.width(), cells.height());
    }
    update();
}

}  // namespace ZzUi
