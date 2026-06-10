/**
 * @file ZzColdStoragePrivate.cpp
 * @brief ZzColdStorage 私有实现 (schema 与批量提交)。
 */

#include "core/log/ZzColdStoragePrivate.h"

#include <QSqlError>
#include <QSqlQuery>
#include <QVariant>

namespace ZzCore {
namespace Log {

bool ZzColdStoragePrivate::createSchema()
{
    QSqlQuery query(db);

    // 主表: 行号为主键, 内容为文本。
    if (!query.exec(QStringLiteral(
            "CREATE TABLE IF NOT EXISTS logs ("
            "  line INTEGER PRIMARY KEY,"
            "  content TEXT NOT NULL)"))) {
        return false;
    }

    // 尝试创建 FTS5 全文索引 (独立表, rowid 对应行号; 更稳健, 不依赖触发器)。
    ftsAvailable = query.exec(QStringLiteral(
        "CREATE VIRTUAL TABLE IF NOT EXISTS logs_fts USING fts5("
        "  content, tokenize='unicode61')"));

    // 加速性能: 关闭同步、使用 WAL。
    query.exec(QStringLiteral("PRAGMA journal_mode=WAL"));
    query.exec(QStringLiteral("PRAGMA synchronous=NORMAL"));

    // 恢复已有行数。
    if (query.exec(QStringLiteral("SELECT COUNT(*) FROM logs")) && query.next()) {
        totalLines = query.value(0).toULongLong();
    }
    return true;
}

void ZzColdStoragePrivate::commitBatch()
{
    if (batch.empty()) {
        return;
    }

    db.transaction();
    QSqlQuery query(db);
    query.prepare(QStringLiteral("INSERT OR REPLACE INTO logs (line, content) VALUES (?, ?)"));
    for (const auto& [line, text] : batch) {
        query.addBindValue(static_cast<qulonglong>(line));
        query.addBindValue(text);
        query.exec();
    }

    if (ftsAvailable) {
        QSqlQuery ftsQuery(db);
        ftsQuery.prepare(
            QStringLiteral("INSERT OR REPLACE INTO logs_fts (rowid, content) VALUES (?, ?)"));
        for (const auto& [line, text] : batch) {
            ftsQuery.addBindValue(static_cast<qulonglong>(line));
            ftsQuery.addBindValue(text);
            ftsQuery.exec();
        }
    }

    db.commit();
    batch.clear();
}

}  // namespace Log
}  // namespace ZzCore
