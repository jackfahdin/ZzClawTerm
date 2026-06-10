/**
 * @file ZzLogEngine.cpp
 * @brief ZzLogEngine 公开接口实现。
 */

#include "core/log/ZzLogEngine.h"

#include <algorithm>

#include "core/log/ZzLogEnginePrivate.h"

namespace ZzCore {
namespace Log {

ZzLogEngine::ZzLogEngine(QObject* parent)
    : QObject(parent), d_ptr(std::make_unique<ZzLogEnginePrivate>())
{
}

ZzLogEngine::~ZzLogEngine() = default;

bool ZzLogEngine::open(const QString& storagePath)
{
    d_ptr->opened = d_ptr->cold.open(storagePath);
    if (d_ptr->opened) {
        d_ptr->nextLine = d_ptr->cold.totalLines();
    }
    return d_ptr->opened;
}

void ZzLogEngine::setHotCapacity(std::size_t lines)
{
    // 重设热层容量 (历史仍在冷层, 不丢失)。
    d_ptr->hot.setCapacity(lines);
}

void ZzLogEngine::appendLine(const QString& text)
{
    const std::uint64_t lineNo = d_ptr->nextLine;

    d_ptr->hot.append(text);
    d_ptr->index.appendLine(d_ptr->byteOffset);
    d_ptr->byteOffset += static_cast<std::uint64_t>(text.toUtf8().size()) + 1;

    if (d_ptr->opened) {
        d_ptr->cold.appendLine(lineNo, text);
    }

    ++d_ptr->nextLine;
    emit lineAppended(d_ptr->nextLine);
}

std::uint64_t ZzLogEngine::totalLines() const
{
    return d_ptr->nextLine;
}

QString ZzLogEngine::getLine(std::uint64_t lineNumber) const
{
    if (lineNumber >= d_ptr->nextLine) {
        return {};
    }
    // 热层覆盖最近 hot.size() 行。
    const std::uint64_t hotCount = d_ptr->hot.size();
    const std::uint64_t hotStart = d_ptr->nextLine - hotCount;
    if (lineNumber >= hotStart) {
        return d_ptr->hot.at(static_cast<std::size_t>(lineNumber - hotStart));
    }
    if (d_ptr->opened) {
        return d_ptr->cold.getLine(lineNumber);
    }
    return {};
}

QStringList ZzLogEngine::getLines(std::uint64_t start, std::uint64_t count) const
{
    QStringList result;
    const std::uint64_t end = std::min(start + count, d_ptr->nextLine);
    for (std::uint64_t i = start; i < end; ++i) {
        result.append(getLine(i));
    }
    return result;
}

QVector<std::uint64_t> ZzLogEngine::search(const QString& pattern, int limit) const
{
    if (d_ptr->opened) {
        return d_ptr->cold.search(pattern, limit);
    }
    return {};
}

}  // namespace Log
}  // namespace ZzCore
