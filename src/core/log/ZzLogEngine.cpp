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
    d_ptr->coldOpen = d_ptr->cold.open(storagePath);
    d_ptr->warmOpen = d_ptr->warm.open(storagePath + QStringLiteral(".warm"));
    if (d_ptr->coldOpen) {
        d_ptr->nextLine = d_ptr->cold.totalLines();
    }
    return d_ptr->coldOpen;
}

void ZzLogEngine::setHotCapacity(std::size_t lines)
{
    // 重设热层容量 (历史仍在温/冷层, 不丢失)。
    d_ptr->hot.setCapacity(lines);
}

void ZzLogEngine::appendLine(const QString& text)
{
    const std::uint64_t lineNo = d_ptr->nextLine;
    const std::uint64_t hotCap = d_ptr->hot.capacity();

    // 写入热层; 若挤出最老行, 该行 (行号 = lineNo - hotCap) 下沉到温层。
    const auto evicted = d_ptr->hot.append(text);
    if (evicted.has_value() && d_ptr->warmOpen && lineNo >= hotCap) {
        d_ptr->warm.appendLine(lineNo - hotCap, *evicted);
    }

    // 全量写入冷层 (持久化与搜索)。
    if (d_ptr->coldOpen) {
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
    // 依次尝试 热层 → 温层 → 冷层。
    const std::uint64_t hotCount = d_ptr->hot.size();
    const std::uint64_t hotStart = d_ptr->nextLine - hotCount;
    if (lineNumber >= hotStart) {
        return d_ptr->hot.at(static_cast<std::size_t>(lineNumber - hotStart));
    }
    if (d_ptr->warmOpen && d_ptr->warm.contains(lineNumber)) {
        return d_ptr->warm.getLine(lineNumber);
    }
    if (d_ptr->coldOpen) {
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
    if (d_ptr->coldOpen) {
        return d_ptr->cold.search(pattern, limit);
    }
    return {};
}

}  // namespace Log
}  // namespace ZzCore
