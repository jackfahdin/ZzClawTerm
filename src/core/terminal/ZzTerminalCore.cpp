/**
 * @file ZzTerminalCore.cpp
 * @brief ZzTerminalCore 公开接口实现。
 */

#include "core/terminal/ZzTerminalCore.h"

#include "core/terminal/ZzTerminalCorePrivate.h"

namespace ZzCore {
namespace Terminal {

ZzTerminalCore::ZzTerminalCore(int cols, int rows, QObject* parent)
    : QObject(parent), d_ptr(std::make_unique<ZzTerminalCorePrivate>())
{
    d_ptr->buffer.resize(cols, rows);
    d_ptr->buffer.setScrolledOffCallback(
        [this](const QString& text) { emit lineArchived(text); });
}

ZzTerminalCore::~ZzTerminalCore() = default;

void ZzTerminalCore::processInput(const QByteArray& data)
{
    d_ptr->parser.parse(data);
    emit contentChanged();
    emit cursorMoved(d_ptr->buffer.cursorRow(), d_ptr->buffer.cursorCol());
}

void ZzTerminalCore::resize(int cols, int rows)
{
    d_ptr->buffer.resize(cols, rows);
    emit contentChanged();
}

QSize ZzTerminalCore::size() const
{
    return {d_ptr->buffer.cols(), d_ptr->buffer.rows()};
}

const ZzTextBuffer& ZzTerminalCore::buffer() const
{
    return d_ptr->buffer;
}

}  // namespace Terminal
}  // namespace ZzCore
