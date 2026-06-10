/**
 * @file ZzRingBuffer.h
 * @brief 热数据层: 固定容量环形缓冲区。
 *
 * 保存最近 N 行日志, 内存占用固定。新行追加为 O(1); 缓冲区满时
 * 最老的行被挤出并返回 (交由温/冷层归档)。
 */

#ifndef ZZ_RING_BUFFER_H
#define ZZ_RING_BUFFER_H

#include <QString>

#include <cstddef>
#include <memory>
#include <optional>

namespace ZzCore {
namespace Log {

class ZzRingBufferPrivate;

/**
 * @brief 行级环形缓冲区。
 */
class ZzRingBuffer
{
public:
    /**
     * @brief 构造缓冲区。
     * @param capacity 最大行数。
     */
    explicit ZzRingBuffer(std::size_t capacity = 10000);
    ~ZzRingBuffer();

    ZzRingBuffer(const ZzRingBuffer&) = delete;
    ZzRingBuffer& operator=(const ZzRingBuffer&) = delete;

    /**
     * @brief 追加一行。
     * @param line 行文本。
     * @return 若缓冲区已满, 返回被挤出的最老行; 否则为空。
     */
    std::optional<QString> append(const QString& line);

    /** @brief 当前行数。 */
    std::size_t size() const;

    /** @brief 容量 (最大行数)。 */
    std::size_t capacity() const;

    /**
     * @brief 重设容量 (清空现有内容)。
     * @param capacity 新的最大行数。
     */
    void setCapacity(std::size_t capacity);

    /**
     * @brief 按相对索引取行 (0 = 最老在缓冲区中的行)。
     * @return 越界返回空字符串。
     */
    QString at(std::size_t relativeIndex) const;

    /** @brief 清空缓冲区。 */
    void clear();

private:
    std::unique_ptr<ZzRingBufferPrivate> d_ptr;
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_RING_BUFFER_H
