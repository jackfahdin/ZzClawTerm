/**
 * @file ZzColdStoragePrivate.h
 * @brief ZzColdStorage 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_COLD_STORAGE_PRIVATE_H
#define ZZ_COLD_STORAGE_PRIVATE_H

#include <QSqlDatabase>
#include <QString>

#include <cstdint>
#include <utility>
#include <vector>

namespace ZzCore {
namespace Log {

/**
 * @brief 冷存储内部状态与 SQL 操作。
 */
class ZzColdStoragePrivate
{
public:
    /** @brief 创建表结构与 (可选) FTS5 全文索引。 */
    bool createSchema();

    /** @brief 提交当前批量缓冲。 */
    void commitBatch();

    QSqlDatabase db;                   ///< 数据库连接
    QString connectionName;            ///< 唯一连接名
    bool ftsAvailable = false;         ///< FTS5 是否可用
    std::uint64_t totalLines = 0;      ///< 已写入总行数

    std::vector<std::pair<std::uint64_t, QString>> batch;  ///< 批量写缓冲
    static constexpr std::size_t kBatchSize = 1000;        ///< 批量阈值
};

}  // namespace Log
}  // namespace ZzCore

#endif  // ZZ_COLD_STORAGE_PRIVATE_H
