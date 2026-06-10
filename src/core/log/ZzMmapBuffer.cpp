/**
 * @file ZzMmapBuffer.cpp
 * @brief ZzMmapBuffer 公开接口实现。
 */

#include "core/log/ZzMmapBuffer.h"

#include <QIODevice>

#include "core/log/ZzMmapBufferPrivate.h"

namespace ZzCore {
namespace Log {

ZzMmapBuffer::ZzMmapBuffer() : d_ptr(std::make_unique<ZzMmapBufferPrivate>()) {}

ZzMmapBuffer::~ZzMmapBuffer()
{
    close();
}

bool ZzMmapBuffer::open(const QString& filePath)
{
    close();
    d_ptr->filePath = filePath;
    d_ptr->file.setFileName(filePath);
    if (!d_ptr->file.open(QIODevice::ReadWrite | QIODevice::Truncate)) {
        return false;
    }
    d_ptr->writeOffset = 0;
    d_ptr->deadBytes = 0;
    d_ptr->next = 0;
    d_ptr->pendingStart = 0;
    d_ptr->blocks.clear();
    d_ptr->pending.clear();
    d_ptr->blockCache.clear();
    return true;
}

void ZzMmapBuffer::close()
{
    if (d_ptr->file.isOpen()) {
        d_ptr->flushPending();
        d_ptr->file.close();
    }
}

void ZzMmapBuffer::setCapacityLines(std::uint64_t lines)
{
    d_ptr->capacityLines = lines == 0 ? 1 : lines;
}

void ZzMmapBuffer::appendLine(std::uint64_t lineNumber, const QString& text)
{
    if (d_ptr->pending.empty()) {
        d_ptr->pendingStart = lineNumber;
    }
    d_ptr->pending.push_back(text);
    d_ptr->next = lineNumber + 1;
    if (d_ptr->pending.size() >= ZzMmapBuffer::kBlockLines) {
        d_ptr->flushPending();
    }
}

void ZzMmapBuffer::flush()
{
    d_ptr->flushPending();
}

std::uint64_t ZzMmapBuffer::firstStoredLine() const
{
    if (!d_ptr->blocks.empty()) {
        return d_ptr->blocks.front().startLine;
    }
    return d_ptr->pendingStart;
}

std::uint64_t ZzMmapBuffer::nextLine() const
{
    return d_ptr->next;
}

bool ZzMmapBuffer::contains(std::uint64_t lineNumber) const
{
    return lineNumber >= firstStoredLine() && lineNumber < d_ptr->next;
}

QString ZzMmapBuffer::getLine(std::uint64_t lineNumber)
{
    if (!contains(lineNumber)) {
        return {};
    }

    // 命中尚未落盘的待写块。
    if (!d_ptr->pending.empty() && lineNumber >= d_ptr->pendingStart &&
        lineNumber < d_ptr->next) {
        return d_ptr->pending[static_cast<std::size_t>(lineNumber - d_ptr->pendingStart)];
    }

    // 在已落盘块中二分定位。
    const auto& blocks = d_ptr->blocks;
    if (blocks.empty()) {
        return {};
    }
    std::size_t lo = 0;
    std::size_t hi = blocks.size() - 1;
    std::size_t found = blocks.size();
    while (lo <= hi) {
        const std::size_t mid = lo + (hi - lo) / 2;
        const ZzWarmBlock& b = blocks[mid];
        if (lineNumber < b.startLine) {
            if (mid == 0) {
                break;
            }
            hi = mid - 1;
        } else if (lineNumber >= b.startLine + b.lineCount) {
            lo = mid + 1;
        } else {
            found = mid;
            break;
        }
    }
    if (found >= blocks.size()) {
        return {};
    }

    const ZzWarmBlock& block = blocks[found];
    QStringList lines;
    if (auto cached = d_ptr->blockCache.get(block.startLine)) {
        lines = *cached;
    } else {
        lines = d_ptr->loadBlock(block);
        d_ptr->blockCache.put(block.startLine, lines);
    }

    const std::size_t idx = static_cast<std::size_t>(lineNumber - block.startLine);
    if (idx < static_cast<std::size_t>(lines.size())) {
        return lines.at(static_cast<qsizetype>(idx));
    }
    return {};
}

}  // namespace Log
}  // namespace ZzCore
