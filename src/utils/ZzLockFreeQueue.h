/**
 * @file ZzLockFreeQueue.h
 * @brief 单生产者单消费者 (SPSC) 无锁环形队列。
 *
 * 用于 I/O 线程到主线程的零锁数据传递。容量为 2 的幂以便用位与
 * 取模。仅保证 SPSC 场景的正确性。
 */

#ifndef ZZ_LOCK_FREE_QUEUE_H
#define ZZ_LOCK_FREE_QUEUE_H

#include <atomic>
#include <cstddef>
#include <optional>
#include <vector>

namespace ZzUtils {

/**
 * @brief SPSC 无锁队列。
 * @tparam T 元素类型 (需可移动)。
 */
template <typename T>
class ZzLockFreeQueue
{
public:
    /**
     * @brief 构造队列。
     * @param capacityPow2 容量 (将向上取整为 2 的幂)。
     */
    explicit ZzLockFreeQueue(std::size_t capacityPow2 = 1024)
    {
        std::size_t cap = 1;
        while (cap < capacityPow2) {
            cap <<= 1;
        }
        m_capacity = cap;
        m_mask = cap - 1;
        m_buffer.resize(cap);
    }

    /**
     * @brief 生产者: 入队。
     * @return 队列已满返回 false。
     */
    bool push(T value)
    {
        const std::size_t head = m_head.load(std::memory_order_relaxed);
        const std::size_t next = (head + 1) & m_mask;
        if (next == m_tail.load(std::memory_order_acquire)) {
            return false;  // 已满
        }
        m_buffer[head] = std::move(value);
        m_head.store(next, std::memory_order_release);
        return true;
    }

    /**
     * @brief 消费者: 出队。
     * @return 队列为空返回 std::nullopt。
     */
    std::optional<T> pop()
    {
        const std::size_t tail = m_tail.load(std::memory_order_relaxed);
        if (tail == m_head.load(std::memory_order_acquire)) {
            return std::nullopt;  // 空
        }
        T value = std::move(m_buffer[tail]);
        m_tail.store((tail + 1) & m_mask, std::memory_order_release);
        return value;
    }

    /** @brief 容量。 */
    std::size_t capacity() const { return m_capacity; }

private:
    std::vector<T> m_buffer;
    std::size_t m_capacity = 0;
    std::size_t m_mask = 0;
    std::atomic<std::size_t> m_head{0};  ///< 生产者写位置
    std::atomic<std::size_t> m_tail{0};  ///< 消费者读位置
};

}  // namespace ZzUtils

#endif  // ZZ_LOCK_FREE_QUEUE_H
