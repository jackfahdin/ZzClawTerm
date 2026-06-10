/**
 * @file ZzLRUCache.h
 * @brief 通用 LRU (最近最少使用) 缓存。
 *
 * 用于字形缓存、温层解压块缓存等场景。访问与插入均摊 O(1)。
 */

#ifndef ZZ_LRU_CACHE_H
#define ZZ_LRU_CACHE_H

#include <cstddef>
#include <list>
#include <optional>
#include <unordered_map>
#include <utility>

namespace ZzUtils {

/**
 * @brief LRU 缓存。
 * @tparam Key 键类型。
 * @tparam Value 值类型。
 */
template <typename Key, typename Value>
class ZzLRUCache
{
public:
    /**
     * @brief 构造缓存。
     * @param capacity 最大条目数。
     */
    explicit ZzLRUCache(std::size_t capacity = 1024) : m_capacity(capacity ? capacity : 1) {}

    /**
     * @brief 查找并提升为最近使用。
     * @return 命中返回值, 否则 std::nullopt。
     */
    std::optional<Value> get(const Key& key)
    {
        auto it = m_map.find(key);
        if (it == m_map.end()) {
            return std::nullopt;
        }
        m_order.splice(m_order.begin(), m_order, it->second);
        return it->second->second;
    }

    /**
     * @brief 插入或更新, 必要时淘汰最久未用项。
     */
    void put(const Key& key, Value value)
    {
        auto it = m_map.find(key);
        if (it != m_map.end()) {
            it->second->second = std::move(value);
            m_order.splice(m_order.begin(), m_order, it->second);
            return;
        }
        if (m_map.size() >= m_capacity) {
            const Key& victim = m_order.back().first;
            m_map.erase(victim);
            m_order.pop_back();
        }
        m_order.emplace_front(key, std::move(value));
        m_map[key] = m_order.begin();
    }

    /** @brief 当前条目数。 */
    std::size_t size() const { return m_map.size(); }

    /** @brief 清空缓存。 */
    void clear()
    {
        m_map.clear();
        m_order.clear();
    }

private:
    using ListType = std::list<std::pair<Key, Value>>;
    std::size_t m_capacity;
    ListType m_order;  ///< 头部为最近使用
    std::unordered_map<Key, typename ListType::iterator> m_map;
};

}  // namespace ZzUtils

#endif  // ZZ_LRU_CACHE_H
