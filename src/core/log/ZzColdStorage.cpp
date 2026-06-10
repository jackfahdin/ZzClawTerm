/**
 * @file ZzColdStorage.cpp
 * @brief ZzColdStorage 公开接口实现。
 */

#include "core/log/ZzColdStorage.h"

#include <QSqlError>
#include <QSqlQuery>
#include <QVariant>

#include <atomic>

#include "core/log/ZzColdStoragePrivate.h"

namespace ZzCore {
namespace Log {

namespace {
/** @brief 生成进程内唯一的数据库连接名。 */
QString makeConnectionName()
{
    static std::atomic<quint64> counter{0};
    return QStringLiteral("zz_cold_%1").arg(counter.fetch_add(1));
}
}  // namespace

ZzColdStorage::ZzColdStorage() : d_ptr(std::make_unique<ZzColdStoragePrivate>()) {}

ZzColdStorage::~ZzColdStorage()
{
    close();
}

bool ZzColdStorage::open(const QString& dbPath)
{
    d_ptr->connectionName = makeConnectionName();
    d_ptr->db = QSqlDatabase::addDatabase(QStringLiteral("QSQLITE"), d_ptr->connectionName);
    d_ptr->db.setDatabaseName(dbPath);
    if (!d_ptr->db.open()) {
        return false;
    }
    return d_ptr->createSchema();
}

void ZzColdStorage::close()
{
    if (d_ptr->db.isOpen()) {
        d_ptr->commitBatch();
        d_ptr->db.close();
    }
    if (!d_ptr->connectionName.isEmpty()) {
        const QString name = d_ptr->connectionName;
        d_ptr->db = QSqlDatabase();  // 释放本地句柄, 避免 removeDatabase 警告
        d_ptr->connectionName.clear();
        QSqlDatabase::removeDatabase(name);
    }
}

void ZzColdStorage::appendLine(std::uint64_t lineNumber, const QString& text)
{
    d_ptr->batch.emplace_back(lineNumber, text);
    ++d_ptr->totalLines;
    if (d_ptr->batch.size() >= ZzColdStoragePrivate::kBatchSize) {
        d_ptr->commitBatch();
    }
}

void ZzColdStorage::flush()
{
    d_ptr->commitBatch();
}

std::uint64_t ZzColdStorage::totalLines() const
{
    return d_ptr->totalLines;
}

QString ZzColdStorage::getLine(std::uint64_t lineNumber) const
{
    const_cast<ZzColdStorage*>(this)->flush();
    QSqlQuery query(d_ptr->db);
    query.prepare(QStringLiteral("SELECT content FROM logs WHERE line = ?"));
    query.addBindValue(static_cast<qulonglong>(lineNumber));
    if (query.exec() && query.next()) {
        return query.value(0).toString();
    }
    return {};
}

QStringList ZzColdStorage::getLines(std::uint64_t start, std::uint64_t count) const
{
    const_cast<ZzColdStorage*>(this)->flush();
    QStringList result;
    QSqlQuery query(d_ptr->db);
    query.prepare(QStringLiteral(
        "SELECT content FROM logs WHERE line >= ? AND line < ? ORDER BY line ASC"));
    query.addBindValue(static_cast<qulonglong>(start));
    query.addBindValue(static_cast<qulonglong>(start + count));
    if (query.exec()) {
        while (query.next()) {
            result.append(query.value(0).toString());
        }
    }
    return result;
}

QVector<std::uint64_t> ZzColdStorage::search(const QString& pattern, int limit) const
{
    const_cast<ZzColdStorage*>(this)->flush();
    QVector<std::uint64_t> result;
    QSqlQuery query(d_ptr->db);

    if (d_ptr->ftsAvailable) {
        query.prepare(QStringLiteral(
            "SELECT rowid FROM logs_fts WHERE logs_fts MATCH ? ORDER BY rowid ASC LIMIT ?"));
        query.addBindValue(pattern);
        query.addBindValue(limit);
    } else {
        query.prepare(QStringLiteral(
            "SELECT line FROM logs WHERE content LIKE ? ORDER BY line ASC LIMIT ?"));
        query.addBindValue(QStringLiteral("%") + pattern + QStringLiteral("%"));
        query.addBindValue(limit);
    }

    if (query.exec()) {
        while (query.next()) {
            result.append(query.value(0).toULongLong());
        }
    }
    return result;
}

}  // namespace Log
}  // namespace ZzCore
