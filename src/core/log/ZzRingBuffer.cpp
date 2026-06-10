/**
 * @file ZzRingBuffer.cpp
 * @brief ZzRingBuffer 公开接口实现。
 */

#include "core/log/ZzRingBuffer.h"

#include <algorithm>

#include "core/log/ZzRingBufferPrivate.h"

namespace ZzCore {
namespace Log {

ZzRingBuffer::ZzRingBuffer(std::size_t capacity)
    : d_ptr(std::make_unique<ZzRingBufferPrivate>())
{
    d_ptr->capacity = std::max<std::size_t>(1, capacity);
    d_ptr->buffer.resize(d_ptr->capacity);
}

ZzRingBuffer::~ZzRingBuffer() = default;

std::optional<QString> ZzRingBuffer::append(const QString& line)
{
    std::optional<QString> evicted;
    if (d_ptr->count == d_ptr->capacity) {
        // 满: head 指向最老元素, 即将被覆盖。
        evicted = d_ptr->buffer[d_ptr->head];
    }
    d_ptr->buffer[d_ptr->head] = line;
    d_ptr->head = (d_ptr->head + 1) % d_ptr->capacity;
    if (d_ptr->count < d_ptr->capacity) {
        ++d_ptr->count;
    }
    return evicted;
}

std::size_t ZzRingBuffer::size() const
{
    return d_ptr->count;
}

std::size_t ZzRingBuffer::capacity() const
{
    return d_ptr->capacity;
}

void ZzRingBuffer::setCapacity(std::size_t capacity)
{
    d_ptr->capacity = std::max<std::size_t>(1, capacity);
    d_ptr->buffer.assign(d_ptr->capacity, QString());
    d_ptr->head = 0;
    d_ptr->count = 0;
}

QString ZzRingBuffer::at(std::size_t relativeIndex) const
{
    if (relativeIndex >= d_ptr->count) {
        return {};
    }
    // 最老元素的物理位置。
    const std::size_t oldest =
        (d_ptr->head + d_ptr->capacity - d_ptr->count) % d_ptr->capacity;
    const std::size_t physical = (oldest + relativeIndex) % d_ptr->capacity;
    return d_ptr->buffer[physical];
}

void ZzRingBuffer::clear()
{
    d_ptr->head = 0;
    d_ptr->count = 0;
}

}  // namespace Log
}  // namespace ZzCore
