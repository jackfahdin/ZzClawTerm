/**
 * @file ZzLineIndex.cpp
 * @brief ZzLineIndex 公开接口实现。
 */

#include "core/log/ZzLineIndex.h"

#include "core/log/ZzLineIndexPrivate.h"

namespace ZzCore {
namespace Log {

ZzLineIndex::ZzLineIndex() : d_ptr(std::make_unique<ZzLineIndexPrivate>()) {}

ZzLineIndex::~ZzLineIndex() = default;

void ZzLineIndex::appendLine(std::uint64_t byteOffset)
{
    if (d_ptr->totalLines % kBlockSize == 0) {
        d_ptr->blocks.push_back(ZzBlockEntry{byteOffset, d_ptr->totalLines});
    }
    ++d_ptr->totalLines;
}

std::uint64_t ZzLineIndex::totalLines() const
{
    return d_ptr->totalLines;
}

std::uint64_t ZzLineIndex::blockOffsetForLine(std::uint64_t lineNumber) const
{
    if (d_ptr->blocks.empty()) {
        return 0;
    }
    const std::uint64_t blockIdx = lineNumber / kBlockSize;
    if (blockIdx >= d_ptr->blocks.size()) {
        return d_ptr->blocks.back().fileOffset;
    }
    return d_ptr->blocks[blockIdx].fileOffset;
}

std::uint64_t ZzLineIndex::blockStartLine(std::uint64_t lineNumber) const
{
    return (lineNumber / kBlockSize) * kBlockSize;
}

void ZzLineIndex::clear()
{
    d_ptr->blocks.clear();
    d_ptr->totalLines = 0;
}

}  // namespace Log
}  // namespace ZzCore
