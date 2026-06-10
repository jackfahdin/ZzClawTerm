/**
 * @file ZzColdStorage.h
 * @brief 冷数据层: 基于 SQLite 的无限日志存储。
 *
 * 将日志行持久化到 SQLite, 提供按行号随机访问与全文搜索 (优先
 * FTS5, 不可用时回退 LIKE)。写入采用批量事务以提升吞吐。
 */

#ifndef ZZ_COLD_STORAGE_H
#define ZZ_COLD_STORAGE_H

#include <QString>
#include <QStringList>
#include <QVector>

#include <cstdint>
#include <memory>

namespace ZzCore {
namespace Log {

class ZzColdStoragePrivate;

/**
 * @brief 冷存储 (SQLite)。
 */
class ZzColdStorage
{
public:
    ZzColdStorage();
    ~ZzColdStorage();

    ZzColdStorage(const ZzColdStorage&) = delete;
    ZzColdStorage& operator=(const ZzColdStorage&) = delete;

    /**
     * @brief 打开 (或创建) 数据库。
     * @param dbPath 数据库文件路径。
     * @return 成功返回 true。
     */
    bool open(const QString& dbPath);

    /** @brief 关闭数据库并提交未决批量写入。 */
    void close();

    /**
     * @brief 追加一行 (进入批量缓冲, 满批或 flush 时落盘)。
     * @param lineNumber 全局行号。
     * @param text 行文本。
     */
    void appendLine(std::uint64_t lineNumber, const QString& text);

    /** @brief 立即提交批量缓冲。 */
    void flush();

    /** @brief 总行数。 */
    std::uint64_t totalLines() const;

    /** @brief 取指定行, 不存在返回空。 */
    QString getLine(std::uint64_t lineNumber) const;

    /** @brief 取 [start, start+count) 行。 */
    QStringList getLines(std::uint64_t start, std::uint64_t count) const;

    /**
     * @brief 全文搜索, 返回匹配的行号 (升序)。
     * @param pattern 关键词。
     * @param limit 最大返回数。
     */
    QVector<std::uint64_t> search(const QString& pattern, int limit = 1000) const;

private:
    std::unique_ptr<ZzColdStoragePrivate> d_ptr;
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_COLD_STORAGE_H
