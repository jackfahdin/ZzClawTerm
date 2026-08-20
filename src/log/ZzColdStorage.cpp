#include "ZzColdStorage.h"

#include <sqlite3.h>

/**
 * @file ZzColdStorage.cpp
 * @brief 冷层存储实现。行编码函数（encodeLine/skipLine/parseLine）与
 *        ZzMmapBuffer.cpp 同款（textLen u32|attrLen u32|UTF-8|属性负载），
 *        保持冷/温两层块内格式一致；任务 3 追加块读写时引入该匿名命名空间。
 */

ZzColdStorage::ZzColdStorage(const Config &config)
    : m_config(config)
{
}

ZzColdStorage::~ZzColdStorage()
{
    close();
}

bool ZzColdStorage::execSql(const char *sql, QString *errorString) const
{
    char *errmsg = nullptr;
    if (sqlite3_exec(m_db, sql, nullptr, nullptr, &errmsg) == SQLITE_OK)
        return true;
    if (errorString) {
        *errorString = QStringLiteral("SQLite 执行失败：%1（SQL：%2）")
                           .arg(QString::fromUtf8(errmsg ? errmsg : "unknown"),
                                QString::fromUtf8(sql));
    }
    sqlite3_free(errmsg);
    return false;
}

bool ZzColdStorage::open(QString *errorString)
{
    QMutexLocker locker(&m_mutex);
    if (m_db)
        return true;
    const QByteArray path = m_config.dbPath.toUtf8();
    if (sqlite3_open_v2(path.constData(), &m_db,
                        SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX,
                        nullptr)
        != SQLITE_OK) {
        if (errorString)
            *errorString = QStringLiteral("sqlite3_open_v2 失败：%1")
                               .arg(m_db ? QString::fromUtf8(sqlite3_errmsg(m_db))
                                         : QStringLiteral("内存不足"));
        if (m_db) {
            sqlite3_close(m_db);
            m_db = nullptr;
        }
        return false;
    }

    // schema 与规格 §四 原文一致（IF NOT EXISTS 保证重开幂等）
    static const char *const kSetup[] = {
        "PRAGMA journal_mode=WAL",     // WAL：崩溃不丢已提交数据（规格 §四）
        "PRAGMA synchronous=NORMAL",   // WAL 下 NORMAL 足够（崩溃不丢已提交事务）
        "CREATE TABLE IF NOT EXISTS blocks ("
        " block_id    INTEGER PRIMARY KEY,"
        " first_line  INTEGER NOT NULL,"
        " line_count  INTEGER NOT NULL,"
        " session_id  TEXT NOT NULL,"
        " start_ts_ns INTEGER NOT NULL,"
        " end_ts_ns   INTEGER NOT NULL,"
        " payload     BLOB NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_blocks_range ON blocks(first_line)",
        "CREATE INDEX IF NOT EXISTS idx_blocks_session_ts ON blocks(session_id, start_ts_ns)",
        "CREATE VIRTUAL TABLE IF NOT EXISTS lines_fts USING fts5(text, content='')",
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value INTEGER)",
        "INSERT OR IGNORE INTO meta(key, value) VALUES('schema_version', 1)",
        "INSERT OR IGNORE INTO meta(key, value) VALUES('frontier', 0)",
        "INSERT OR IGNORE INTO meta(key, value) VALUES('base', 0)",
    };
    for (const char *sql : kSetup) {
        if (!execSql(sql, errorString)) {
            closeLocked();
            return false;
        }
    }
    if (!loadState(errorString)) {
        closeLocked();
        return false;
    }
    return true;
}

bool ZzColdStorage::loadState(QString *errorString)
{
    // 调用方须已持有 m_mutex
    const auto readMeta = [this](const char *key, quint64 *out) -> bool {
        sqlite3_stmt *stmt = nullptr;
        if (sqlite3_prepare_v2(m_db, "SELECT value FROM meta WHERE key = ?", -1, &stmt, nullptr)
            != SQLITE_OK)
            return false;
        sqlite3_bind_text(stmt, 1, key, -1, SQLITE_TRANSIENT);
        const bool found = sqlite3_step(stmt) == SQLITE_ROW;
        if (found)
            *out = quint64(sqlite3_column_int64(stmt, 0));
        sqlite3_finalize(stmt);
        return found;
    };
    if (!readMeta("frontier", &m_frontier) || !readMeta("base", &m_baseLine)) {
        if (errorString)
            *errorString = QStringLiteral("meta 表缺少 frontier/base 记录（库损坏？）");
        return false;
    }

    m_blocks.clear();
    sqlite3_stmt *stmt = nullptr;
    if (sqlite3_prepare_v2(m_db, "SELECT first_line, line_count FROM blocks ORDER BY first_line",
                           -1, &stmt, nullptr)
        != SQLITE_OK) {
        if (errorString)
            *errorString = QStringLiteral("块表读取失败：%1")
                               .arg(QString::fromUtf8(sqlite3_errmsg(m_db)));
        return false;
    }
    while (sqlite3_step(stmt) == SQLITE_ROW)
        m_blocks.append({quint64(sqlite3_column_int64(stmt, 0)),
                         quint32(sqlite3_column_int(stmt, 1))});
    sqlite3_finalize(stmt);

    // 一致性校验：frontier 与块写入同事务，meta 与块表必须吻合
    if (!m_blocks.isEmpty()) {
        const BlockEntry &last = m_blocks.last();
        if (m_blocks.first().firstLine != m_baseLine
            || last.firstLine + last.lineCount != m_frontier) {
            if (errorString)
                *errorString = QStringLiteral("meta 与块表不一致（库损坏？）："
                                              "base=%1 first=%2 frontier=%3 lastEnd=%4")
                                   .arg(m_baseLine)
                                   .arg(m_blocks.first().firstLine)
                                   .arg(m_frontier)
                                   .arg(last.firstLine + last.lineCount);
            return false;
        }
    } else if (m_baseLine > m_frontier) {
        if (errorString)
            *errorString = QStringLiteral("meta base 超过 frontier（库损坏？）");
        return false;
    }
    return true;
}

void ZzColdStorage::closeLocked()
{
    if (m_db) {
        sqlite3_close(m_db);
        m_db = nullptr;
    }
}

void ZzColdStorage::close()
{
    QMutexLocker locker(&m_mutex);
    closeLocked();
    m_blocks.clear();
    m_frontier = 0;
    m_baseLine = 0;
}

bool ZzColdStorage::isOpen() const
{
    QMutexLocker locker(&m_mutex);
    return m_db != nullptr;
}

quint64 ZzColdStorage::baseLine() const
{
    QMutexLocker locker(&m_mutex);
    return m_baseLine;
}

quint64 ZzColdStorage::frontier() const
{
    QMutexLocker locker(&m_mutex);
    return m_frontier;
}
