/**
 * @file ZzMmapBuffer.h
 * @brief 温数据层: 基于内存映射文件 + 分块压缩的中段日志缓存。
 *
 * 行按块 (kBlockLines 行) 累积, 满块后压缩并顺序写入磁盘文件;
 * 内存仅保留轻量块索引。读取时用 QFile::map 定位所在块并解压
 * (配合 LRU 块缓存)。容量上限触发淘汰最老块, 死区过多时 compact。
 *
 * 压缩后端默认 zlib (Qt 自带 qCompress); 定义 ZZ_HAVE_LZ4 时切换 LZ4。
 */

#ifndef ZZ_MMAP_BUFFER_H
#define ZZ_MMAP_BUFFER_H

#include <QString>

#include <cstdint>
#include <memory>

namespace ZzCore {
namespace Log {

class ZzMmapBufferPrivate;

/**
 * @brief 温层: 内存映射 + 分块压缩缓存。
 */
class ZzMmapBuffer
{
public:
    /** @brief 每个压缩块包含的行数。 */
    static constexpr std::uint32_t kBlockLines = 256;

    ZzMmapBuffer();
    ~ZzMmapBuffer();

    ZzMmapBuffer(const ZzMmapBuffer&) = delete;
    ZzMmapBuffer& operator=(const ZzMmapBuffer&) = delete;

    /**
     * @brief 打开 (新建) 温层数据文件。
     * @param filePath 数据文件路径 (已存在则清空重建)。
     * @return 成功返回 true。
     */
    bool open(const QString& filePath);

    /** @brief 关闭并释放资源。 */
    void close();

    /** @brief 设置温层容量 (最大保留行数)。 */
    void setCapacityLines(std::uint64_t lines);

    /**
     * @brief 追加一行 (全局行号必须单调递增)。
     */
    void appendLine(std::uint64_t lineNumber, const QString& text);

    /** @brief 将未满的待写块强制压缩落盘。 */
    void flush();

    /** @brief 该行是否仍在温层覆盖范围内 (未被淘汰)。 */
    bool contains(std::uint64_t lineNumber) const;

    /**
     * @brief 读取指定行 (必要时解压所在块); 不在范围返回空字符串。
     */
    QString getLine(std::uint64_t lineNumber);

    /** @brief 温层当前最老可访问行号。 */
    std::uint64_t firstStoredLine() const;

    /** @brief 已进入温层的下一个行号 (即累计追加行数)。 */
    std::uint64_t nextLine() const;

private:
    std::unique_ptr<ZzMmapBufferPrivate> d_ptr;
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_MMAP_BUFFER_H
