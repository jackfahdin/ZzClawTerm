#include "ZzColdStorage.h"

#include <sqlite3.h>
#include <zstd.h>

#include <QDateTime>
#include <QtEndian>
#include <cstring>

namespace {
constexpr int kCacheBlocks = 32; ///< 解压块 LRU 容量（规格 §五：32 块 ≈ 3.2 万行）

void putU32(char *p, quint32 v) { v = qToLittleEndian(v); std::memcpy(p, &v, 4); }
quint32 getU32(const char *p) { quint32 v; std::memcpy(&v, p, 4); return qFromLittleEndian(v); }

/// @brief 行编码（与温层一致）：textLen u32 | attrLen u32 | UTF-8 文本 | 属性负载。
QByteArray encodeLine(const ZzLogLine &line)
{
    const QByteArray text = line.text.toUtf8();
    QByteArray out;
    out.reserve(8 + text.size() + line.attributes.size());
    char hdr[8];
    putU32(hdr, quint32(text.size()));
    putU32(hdr + 4, quint32(line.attributes.size()));
    out.append(hdr, 8);
    out.append(text);
    out.append(line.attributes);
    return out;
}

/// @brief 解析一行，返回下一行偏移；数据截断返回 -1。
qint64 parseLine(const QByteArray &data, qint64 off, ZzLogLine *out)
{
    if (off < 0 || off + 8 > data.size())
        return -1;
    const char *p = data.constData() + off;
    const quint32 textLen = getU32(p);
    const quint32 attrLen = getU32(p + 4);
    const qint64 next = off + 8 + textLen + attrLen;
    if (next > data.size())
        return -1;
    out->text = QString::fromUtf8(p + 8, qsizetype(textLen));
    out->attributes = QByteArray(p + 8 + textLen, qsizetype(attrLen));
    return next;
}

/// @brief 块序列化：lineCount u32 | offsets u32×lineCount（行在块内的绝对偏移）| encodeLine × N。
QByteArray serializeBlock(const QVector<ZzLogLine> &lines)
{
    QVector<QByteArray> encoded;
    encoded.reserve(lines.size());
    qsizetype payloadBytes = 0;
    for (const ZzLogLine &line : lines) {
        encoded.append(encodeLine(line));
        payloadBytes += encoded.last().size();
    }
    const qsizetype headerBytes = 4 + 4 * lines.size();
    QByteArray out;
    out.reserve(headerBytes + payloadBytes);
    char u32buf[4];
    putU32(u32buf, quint32(lines.size()));
    out.append(u32buf, 4);
    qint64 off = headerBytes;
    for (const QByteArray &e : std::as_const(encoded)) {
        putU32(u32buf, quint32(off));
        out.append(u32buf, 4);
        off += e.size();
    }
    for (const QByteArray &e : std::as_const(encoded))
        out.append(e);
    return out;
}
} // namespace

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
    m_blockCache.setMaxCost(kCacheBlocks);
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

bool ZzColdStorage::appendBlock(const QVector<ZzLogLine> &lines, quint64 firstLine,
                                QString *errorString)
{
    QMutexLocker locker(&m_mutex);
    if (!m_db) {
        if (errorString)
            *errorString = QStringLiteral("冷层库未打开");
        return false;
    }
    if (lines.isEmpty() || quint64(lines.size()) > kMaxBlockLines) {
        if (errorString)
            *errorString = QStringLiteral("冷层块行数必须在 1..%1，实际 %2")
                               .arg(kMaxBlockLines)
                               .arg(lines.size());
        return false;
    }
    if (firstLine != m_frontier) {
        if (errorString)
            *errorString = QStringLiteral("冷层写入行号不连续：期望 %1，实际 %2")
                               .arg(m_frontier)
                               .arg(firstLine);
        return false;
    }

    const QByteArray raw = serializeBlock(lines);
    const size_t bound = ZSTD_compressBound(size_t(raw.size()));
    QByteArray packed;
    packed.resize(qsizetype(bound));
    const size_t packedSize = ZSTD_compress(packed.data(), bound, raw.constData(),
                                            size_t(raw.size()), kCompressionLevel);
    if (ZSTD_isError(packedSize)) {
        if (errorString)
            *errorString = QStringLiteral("ZSTD 压缩失败：%1")
                               .arg(QString::fromUtf8(ZSTD_getErrorName(packedSize)));
        return false;
    }
    packed.resize(qsizetype(packedSize));

    // 块时间戳 = 归档时刻（ZzLogLine 无时间戳字段；热层缓冲最多 1 万行，归档延迟通常秒级）
    const qint64 nowNs = QDateTime::currentMSecsSinceEpoch() * 1000000LL;

    if (!execSql("BEGIN IMMEDIATE", errorString))
        return false;
    bool ok = false;
    sqlite3_stmt *stmt = nullptr;
    // 1) blocks 行
    if (sqlite3_prepare_v2(m_db,
            "INSERT INTO blocks(first_line, line_count, session_id, start_ts_ns, end_ts_ns, payload)"
            " VALUES(?, ?, ?, ?, ?, ?)",
            -1, &stmt, nullptr)
        == SQLITE_OK) {
        const QByteArray sid = m_config.sessionId.toUtf8();
        sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine));
        sqlite3_bind_int(stmt, 2, int(lines.size()));
        sqlite3_bind_text(stmt, 3, sid.constData(), int(sid.size()), SQLITE_TRANSIENT);
        sqlite3_bind_int64(stmt, 4, nowNs);
        sqlite3_bind_int64(stmt, 5, nowNs);
        sqlite3_bind_blob(stmt, 6, packed.constData(), int(packed.size()), SQLITE_TRANSIENT);
        ok = sqlite3_step(stmt) == SQLITE_DONE;
        sqlite3_finalize(stmt);
        stmt = nullptr;
    }
    // 2) FTS5 索引（rowid = 绝对行号），与块写入同事务
    if (ok) {
        ok = sqlite3_prepare_v2(m_db, "INSERT INTO lines_fts(rowid, text) VALUES(?, ?)",
                                -1, &stmt, nullptr)
             == SQLITE_OK;
        for (qsizetype i = 0; i < lines.size() && ok; ++i) {
            const QByteArray text = lines[i].text.toUtf8();
            sqlite3_reset(stmt);
            sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine + quint64(i)));
            sqlite3_bind_text(stmt, 2, text.constData(), int(text.size()), SQLITE_TRANSIENT);
            ok = sqlite3_step(stmt) == SQLITE_DONE;
        }
        if (stmt) {
            sqlite3_finalize(stmt);
            stmt = nullptr;
        }
    }
    // 3) frontier 前移（同事务：崩溃不产生重复块/丢块）
    if (ok) {
        ok = sqlite3_prepare_v2(m_db, "UPDATE meta SET value = ? WHERE key = 'frontier'",
                                -1, &stmt, nullptr)
             == SQLITE_OK;
        if (ok) {
            sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine + quint64(lines.size())));
            ok = sqlite3_step(stmt) == SQLITE_DONE;
            sqlite3_finalize(stmt);
            stmt = nullptr;
        }
    }

    if (!ok) {
        const QString detail = QString::fromUtf8(sqlite3_errmsg(m_db));
        execSql("ROLLBACK", nullptr);
        if (errorString)
            *errorString = QStringLiteral("冷层块写入失败：%1").arg(detail);
        return false;
    }
    if (!execSql("COMMIT", errorString))
        return false;

    m_blocks.append({firstLine, quint32(lines.size())});
    m_frontier = firstLine + quint64(lines.size());
    m_blockCache.insert(firstLine, new QByteArray(raw), 1); // 刚序列化的块直接入缓存

    locker.unlock();
    enforceLimits(); // 规格 §七：每次写入后检查清理水位（内部自取锁，不能在写事务内调用）
    return true;
}

QVector<ZzLogLine> ZzColdStorage::readLines(quint64 startLine, quint64 count) const
{
    QVector<ZzLogLine> out;
    QMutexLocker locker(&m_mutex);
    if (!m_db || count == 0 || m_blocks.isEmpty())
        return out;
    quint64 id = qMax(startLine, m_baseLine);
    const quint64 end = qMin(startLine + count, m_frontier);
    if (end <= id)
        return out; // 区间完全落在已清理/未写入范围
    out.reserve(qsizetype(qMin<quint64>(end - id, 100000)));

    while (id < end) {
        const qsizetype bi = findBlockIndex(id);
        if (bi < 0)
            break;
        const BlockEntry &block = m_blocks[bi];
        const QByteArray raw = rawBlock(block.firstLine);
        if (raw.size() < 4)
            break; // 块损坏：按可得数据返回
        const quint32 n = getU32(raw.constData());
        if (n != block.lineCount || raw.size() < qint64(4 + 4 * n))
            break;
        // 块内偏移表直接定位（读 24 行最多解压 1 块，规格 §四）
        const quint64 blockEnd = block.firstLine + block.lineCount;
        while (id < end && id < blockEnd) {
            const qint64 off = getU32(raw.constData() + 4 + 4 * (id - block.firstLine));
            ZzLogLine line;
            if (parseLine(raw, off, &line) < 0)
                return out;
            out.append(line);
            ++id;
        }
    }
    return out;
}

void ZzColdStorage::preload(quint64 lineId)
{
    QMutexLocker locker(&m_mutex);
    if (!m_db)
        return;
    const qsizetype bi = findBlockIndex(lineId);
    if (bi < 0)
        return;
    rawBlock(m_blocks[bi].firstLine); // 取块即入缓存
    if (bi + 1 < m_blocks.size())
        rawBlock(m_blocks[bi + 1].firstLine);
}

QByteArray ZzColdStorage::rawBlock(quint64 firstLine) const
{
    // 调用方须已持有 m_mutex
    if (const QByteArray *cached = m_blockCache.object(firstLine))
        return *cached;
    QByteArray packed;
    sqlite3_stmt *stmt = nullptr;
    if (sqlite3_prepare_v2(m_db, "SELECT payload FROM blocks WHERE first_line = ?",
                           -1, &stmt, nullptr)
        == SQLITE_OK) {
        sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine));
        if (sqlite3_step(stmt) == SQLITE_ROW) {
            const void *blob = sqlite3_column_blob(stmt, 0);
            packed = QByteArray(reinterpret_cast<const char *>(blob),
                                sqlite3_column_bytes(stmt, 0));
        }
        sqlite3_finalize(stmt);
    }
    if (packed.isEmpty())
        return {};
    const unsigned long long rawSize =
        ZSTD_getFrameContentSize(packed.constData(), size_t(packed.size()));
    if (rawSize == ZSTD_CONTENTSIZE_ERROR || rawSize == ZSTD_CONTENTSIZE_UNKNOWN
        || rawSize > 256ULL * 1024 * 1024)
        return {}; // 数据损坏或尺寸不合理
    QByteArray raw;
    raw.resize(qsizetype(rawSize));
    const size_t n = ZSTD_decompress(raw.data(), size_t(raw.size()),
                                     packed.constData(), size_t(packed.size()));
    if (ZSTD_isError(n))
        return {};
    m_blockCache.insert(firstLine, new QByteArray(raw), 1);
    return raw;
}

qsizetype ZzColdStorage::findBlockIndex(quint64 lineId) const
{
    // 调用方须已持有 m_mutex；块表按 firstLine 递增，二分找最后 firstLine <= lineId 的块
    if (m_blocks.isEmpty() || lineId < m_blocks.first().firstLine)
        return -1;
    qsizetype lo = 0;
    qsizetype hi = m_blocks.size() - 1;
    qsizetype best = 0;
    while (lo <= hi) {
        const qsizetype mid = (lo + hi) / 2;
        if (m_blocks[mid].firstLine <= lineId) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    return best;
}

QVector<quint64> ZzColdStorage::search(const QString &pattern, int maxResults) const
{
    QVector<quint64> out;
    QMutexLocker locker(&m_mutex);
    if (!m_db || pattern.isEmpty() || maxResults <= 0)
        return out;
    sqlite3_stmt *stmt = nullptr;
    // 非法 MATCH 表达式时 prepare 失败，按无命中返回（调用方负责合法 FTS5 语法）
    if (sqlite3_prepare_v2(m_db,
                           "SELECT rowid FROM lines_fts WHERE lines_fts MATCH ? LIMIT ?",
                           -1, &stmt, nullptr)
        != SQLITE_OK)
        return out;
    const QByteArray utf8 = pattern.toUtf8();
    sqlite3_bind_text(stmt, 1, utf8.constData(), int(utf8.size()), SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 2, maxResults);
    while (sqlite3_step(stmt) == SQLITE_ROW)
        out.append(quint64(sqlite3_column_int64(stmt, 0)));
    sqlite3_finalize(stmt);
    return out;
}

void ZzColdStorage::enforceLimits()
{
    // 占位实现：清理逻辑由任务 5 补全（含 FTS5 同步删除与增量 VACUUM）。
    // appendBlock 提交后调用本函数检查水位，当前为空操作以保证链接可用。
}
