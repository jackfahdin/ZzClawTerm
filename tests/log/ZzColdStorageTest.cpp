#include "ZzColdStorage.h"

#include <QFile>
#include <QRandomGenerator>
#include <QTemporaryDir>
#include <QtTest>

#include <sqlite3.h>

/**
 * @brief ZzColdStorage 冷层单元测试：骨架（schema/WAL/meta）、块读写、FTS5、清理。
 */
class ZzColdStorageTest : public QObject
{
    Q_OBJECT
    static ZzColdStorage::Config testConfig(const QString &dbPath)
    {
        ZzColdStorage::Config c;
        c.dbPath = dbPath;
        c.sessionId = QStringLiteral("test-session");
        return c;
    }
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("cold-line-%1").arg(i),
                QByteArray("C") + QByteArray::number(qint64(i))};
    }
    /// @brief 生成 [start, start+count) 的连续行序列。
    static QVector<ZzLogLine> makeLines(quint64 start, quint64 count)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(count));
        for (quint64 i = 0; i < count; ++i)
            out.append(line(start + i));
        return out;
    }

private slots:
    /// @brief 新库打开：frontier/base 均为 0，库文件落盘。
    void freshOpenCreatesDatabase()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage cold(testConfig(path));
        QString error;
        QVERIFY2(cold.open(&error), qPrintable(error));
        QVERIFY(cold.isOpen());
        QCOMPARE(cold.frontier(), 0ULL);
        QCOMPARE(cold.baseLine(), 0ULL);
        QVERIFY(QFile::exists(path));
    }

    /// @brief schema 与 WAL：用 sqlite3 C API 独立验证（测试目标直接链接 sqlite3_static）。
    void schemaAndWalCreated()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
        } // 关闭后 WAL 模式持久于库头

        sqlite3 *db = nullptr;
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);

        // WAL 模式
        sqlite3_stmt *stmt = nullptr;
        QVERIFY(sqlite3_prepare_v2(db, "PRAGMA journal_mode", -1, &stmt, nullptr) == SQLITE_OK);
        QCOMPARE(sqlite3_step(stmt), SQLITE_ROW);
        QCOMPARE(QString::fromUtf8(reinterpret_cast<const char *>(sqlite3_column_text(stmt, 0))),
                 QStringLiteral("wal"));
        sqlite3_finalize(stmt);

        // meta 初始值
        const auto metaValue = [&db](const char *key) -> qint64 {
            sqlite3_stmt *s = nullptr;
            if (sqlite3_prepare_v2(db, "SELECT value FROM meta WHERE key = ?", -1, &s, nullptr)
                != SQLITE_OK)
                return -1;
            sqlite3_bind_text(s, 1, key, -1, SQLITE_TRANSIENT);
            const qint64 v = sqlite3_step(s) == SQLITE_ROW ? sqlite3_column_int64(s, 0) : -1;
            sqlite3_finalize(s);
            return v;
        };
        QCOMPARE(metaValue("schema_version"), qint64(1));
        QCOMPARE(metaValue("frontier"), qint64(0));
        QCOMPARE(metaValue("base"), qint64(0));

        // blocks / lines_fts 已建
        const auto tableExists = [&db](const char *name) -> bool {
            sqlite3_stmt *s = nullptr;
            if (sqlite3_prepare_v2(db,
                    "SELECT count(*) FROM sqlite_master WHERE name = ?", -1, &s, nullptr)
                != SQLITE_OK)
                return false;
            sqlite3_bind_text(s, 1, name, -1, SQLITE_TRANSIENT);
            const bool ok = sqlite3_step(s) == SQLITE_ROW && sqlite3_column_int(s, 0) > 0;
            sqlite3_finalize(s);
            return ok;
        };
        QVERIFY(tableExists("blocks"));
        QVERIFY(tableExists("lines_fts"));
        sqlite3_close(db);
    }

    /// @brief 路径不可用时打开失败并给出原因（父路径是已存在文件，跨平台必失败）。
    void openFailureReturnsFalseWithError()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString blocker = dir.filePath(QStringLiteral("blocker"));
        QFile f(blocker);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();

        ZzColdStorage cold(testConfig(blocker + QStringLiteral("/cold.db")));
        QString error;
        QVERIFY(!cold.open(&error));
        QVERIFY(!cold.isOpen());
        QVERIFY(!error.isEmpty());
        QCOMPARE(cold.frontier(), 0ULL);
    }

    /// @brief 块读写往返：3 整块（1024×3）+ 1 尾块（37），多种窗口读回与写入逐行一致（含属性）。
    void appendBlockAndReadBack()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());

        quint64 frontier = 0;
        for (int b = 0; b < 3; ++b) {
            QVERIFY(cold.appendBlock(makeLines(frontier, 1024), frontier));
            frontier += 1024;
        }
        QVERIFY(cold.appendBlock(makeLines(frontier, 37), frontier));
        frontier += 37;
        QCOMPARE(cold.frontier(), 3109ULL);
        QCOMPARE(cold.baseLine(), 0ULL);

        // 全量顺序读
        const QVector<ZzLogLine> all = cold.readLines(0, 3109);
        QCOMPARE(all.size(), 3109);
        for (qsizetype i = 0; i < all.size(); ++i) {
            QCOMPARE(all[i].text, line(quint64(i)).text);
            QCOMPARE(all[i].attributes, line(quint64(i)).attributes);
        }
        // 跨块窗口读
        const QVector<ZzLogLine> window = cold.readLines(1000, 100); // 跨块 0/1 边界
        QCOMPARE(window.size(), 100);
        QCOMPARE(window.first().text, line(1000).text);
        QCOMPARE(window.last().text, line(1099).text);
    }

    /// @brief frontier/base 与块数据跨重开持久。
    void statePersistsAcrossReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
            QVERIFY(cold.appendBlock(makeLines(1024, 500), 1024));
        }
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QCOMPARE(cold.frontier(), 1524ULL);
        QCOMPARE(cold.baseLine(), 0ULL);
        const QVector<ZzLogLine> tail = cold.readLines(1520, 4);
        QCOMPARE(tail.size(), 4);
        QCOMPARE(tail.first().text, line(1520).text);
        QCOMPARE(tail.last().text, line(1523).text);
    }

    /// @brief 非连续写入（firstLine != frontier）拒绝且状态不变。
    void nonContiguousAppendFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 100), 0));

        QString error;
        QVERIFY(!cold.appendBlock(makeLines(200, 10), 200, &error)); // 期望 firstLine 100
        QVERIFY(!error.isEmpty());
        QCOMPARE(cold.frontier(), 100ULL); // 状态不变
        QVERIFY(cold.readLines(0, 200).size() == 100);

        // 非法块尺寸
        QVERIFY(!cold.appendBlock({}, 100, &error));                    // 空块
        QVERIFY(!cold.appendBlock(makeLines(100, 1025), 100, &error));  // 超 1024 行
        QCOMPARE(cold.frontier(), 100ULL);
    }

    /// @brief 越界读取按实际可得数量返回。
    void readOutOfRangeClamps()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 100), 0));
        QCOMPARE(cold.readLines(0, 1000).size(), 100);  // 超出 frontier 截断
        QVERIFY(cold.readLines(100, 1).isEmpty());      // 起点 == frontier
        QVERIFY(cold.readLines(5000, 10).isEmpty());    // 完全越界
        ZzColdStorage empty(testConfig(dir.filePath(QStringLiteral("empty.db"))));
        QVERIFY(empty.open());
        QVERIFY(empty.readLines(0, 10).isEmpty());      // 空库
    }

    /// @brief LRU 淘汰后读回仍正确：40 块 > 默认 32 块缓存，交替读最老/最新块强制重解压。
    void lruEvictionKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        quint64 frontier = 0;
        for (int b = 0; b < 40; ++b) {
            QVERIFY(cold.appendBlock(makeLines(frontier, 1024), frontier));
            frontier += 1024;
        }
        // 顺序扫一遍（缓存被最新 32 块占满，块 0..7 被淘汰）
        QCOMPARE(cold.readLines(0, frontier).size(), qsizetype(frontier));
        // 重读最老两块（缓存未命中 → SQLite + ZSTD 重解压）与最新块（缓存命中）
        for (const quint64 start : {0ULL, 1024ULL, 38ULL * 1024, 39ULL * 1024}) {
            const QVector<ZzLogLine> got = cold.readLines(start, 24);
            QCOMPARE(got.size(), 24);
            for (qsizetype j = 0; j < 24; ++j)
                QCOMPARE(got[j].text, line(start + quint64(j)).text);
        }
    }

    /// @brief 预加载不改变读取语义。
    void preloadKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
        QVERIFY(cold.appendBlock(makeLines(1024, 1024), 1024));
        cold.preload(10);        // 预解压块 0 与块 1
        cold.preload(999999999); // 越界：静默忽略
        const QVector<ZzLogLine> got = cold.readLines(2040, 8);
        QCOMPARE(got.size(), 8);
        QCOMPARE(got.first().text, line(2040).text);
    }

    /// @brief FTS5 搜索：关键字命中返回精确绝对行号（升序）。
    void searchFindsLinesByKeyword()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        // 3 块；关键字 ERROR 出现在第 7 / 2048 / 3071 行（块内/跨块边界均覆盖）
        quint64 frontier = 0;
        for (int b = 0; b < 3; ++b) {
            QVector<ZzLogLine> block = makeLines(frontier, 1024);
            if (b == 0)
                block[7].text = QStringLiteral("line 7 ERROR happened");
            if (b == 2) {
                block[0].text = QStringLiteral("line 2048 ERROR happened");
                block[1023].text = QStringLiteral("line 3071 ERROR happened");
            }
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += 1024;
        }
        const QVector<quint64> hits = cold.search(QStringLiteral("ERROR"));
        QCOMPARE(hits, (QVector<quint64>{7ULL, 2048ULL, 3071ULL}));
        // 命中行原文可读回
        QCOMPARE(cold.readLines(hits[1], 1).first().text,
                 QStringLiteral("line 2048 ERROR happened"));
    }

    /// @brief maxResults 限制生效；非法 MATCH 表达式返回空不报错。
    void searchRespectsMaxResultsAndBadPattern()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVector<ZzLogLine> block = makeLines(0, 1024);
        for (qsizetype i = 0; i < 100; ++i)
            block[i].text = QStringLiteral("row %1 TOKENXYZ tail").arg(i);
        QVERIFY(cold.appendBlock(block, 0));

        QCOMPARE(cold.search(QStringLiteral("TOKENXYZ"), 10).size(), 10);
        QCOMPARE(cold.search(QStringLiteral("TOKENXYZ"), 1000).size(), 100);
        QVERIFY(cold.search(QStringLiteral("\"unclosed")).isEmpty()); // 非法表达式 → 空
        QVERIFY(cold.search(QString(), 10).isEmpty());                // 空 pattern → 空
        QVERIFY(cold.search(QStringLiteral("TOKENXYZ"), 0).isEmpty()); // maxResults<=0 → 空
    }

    /// @brief 无命中返回空；FTS 索引跨重开持久。
    void searchNoMatchAndReopenPersistence()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVector<ZzLogLine> block = makeLines(0, 512);
            block[42].text = QStringLiteral("persistent NEEDLE42 here");
            QVERIFY(cold.appendBlock(block, 0));
            QVERIFY(cold.search(QStringLiteral("NOSUCHTOKEN")).isEmpty());
        }
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QCOMPARE(cold.search(QStringLiteral("NEEDLE42")),
                 (QVector<quint64>{42ULL})); // 重开后索引仍可查
    }

    /// @brief 超龄清理：把前两块时间戳改为 100 天前，enforceLimits 后 baseLine 前移、
    ///        老行不可读、FTS 不再命中、剩余行保持可读。
    void maxAgeDaysDropsOldBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        quint64 frontier = 0;
        for (int b = 0; b < 5; ++b) {
            QVector<ZzLogLine> block = makeLines(frontier, 1024);
            if (b == 0)
                block[5].text = QStringLiteral("old OLDTOKEN5 line");
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += 1024;
        }
        QCOMPARE(cold.search(QStringLiteral("OLDTOKEN5")), (QVector<quint64>{5ULL}));

        // 测试直接改库：前两块（行 0..2047）时间戳回拨 100 天
        sqlite3 *db = nullptr;
        // 先关闭 ZzColdStorage 持有的连接，避免 WAL 下双连接写冲突
        cold.close();
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        char *errmsg = nullptr;
        QCOMPARE(sqlite3_exec(db,
                              "UPDATE blocks SET start_ts_ns = 1000000000, end_ts_ns = 1000000000"
                              " WHERE first_line < 2048",
                              nullptr, nullptr, &errmsg),
                 SQLITE_OK);
        sqlite3_free(errmsg);
        sqlite3_close(db);
        QVERIFY(cold.open());

        cold.enforceLimits();
        QCOMPARE(cold.baseLine(), 2048ULL);   // 前两块被清理
        QCOMPARE(cold.frontier(), 5120ULL);   // frontier 不回退
        QVERIFY(cold.readLines(0, 10).isEmpty());            // 已清理区间
        QCOMPARE(cold.readLines(2048, 3).size(), 3);         // 幸存区间从 baseLine 起可读
        QCOMPARE(cold.readLines(2048, 3).first().text, line(2048).text);
        QVERIFY(cold.search(QStringLiteral("OLDTOKEN5")).isEmpty()); // FTS 同步删除
    }

    /// @brief 超容量清理：maxBytes 压小，写入超限后 enforceLimits 按最老块批删，
    ///        baseLine 前移且剩余行完整可读。
    void maxBytesDropsOldestBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage::Config c =
            testConfig(dir.filePath(QStringLiteral("cold.db")));
        c.maxBytes = 2 * 1024 * 1024; // 2MB
        c.maxAgeDays = 36500;         // 关闭超龄干扰
        ZzColdStorage cold(c);
        QVERIFY(cold.open());

        // 每行约 200 字节伪随机文本（按行号确定性生成，ZSTD 基本压不动），
        // 单块约 200KB（含 FTS 索引），30 块库体积约 6MB > 2MB
        const auto randLine = [](quint64 i) {
            QRandomGenerator rng(quint32(i * 2654435761ULL)); // 按行号播种：可重复验证
            QString text;
            text.reserve(200);
            for (int k = 0; k < 25; ++k)
                text.append(QString::number(rng.generate(), 16));
            return ZzLogLine{text + QString::number(qint64(i)), QByteArray()};
        };
        quint64 frontier = 0;
        for (int b = 0; b < 30; ++b) {
            QVector<ZzLogLine> block;
            block.reserve(1024);
            for (quint64 i = 0; i < 1024; ++i)
                block.append(randLine(frontier + i));
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += 1024;
        }
        cold.enforceLimits();
        const quint64 base = cold.baseLine();
        QVERIFY(base > 0ULL);                    // 最老块已被清理
        QCOMPARE(base % 1024ULL, 0ULL);          // 整块粒度删除
        QCOMPARE(cold.frontier(), 30720ULL);
        QVERIFY(base + 1024 <= 30720ULL);        // 至少幸存一块（自适应批量不得删空）
        // 幸存区间完整可读且内容正确
        const QVector<ZzLogLine> head = cold.readLines(base, 24);
        QCOMPARE(head.size(), 24);
        for (qsizetype j = 0; j < 24; ++j)
            QCOMPARE(head[j].text, randLine(base + quint64(j)).text);
    }

    /// @brief 写入后自动触发清理（appendBlock 内部调用 enforceLimits），无需手动调用。
    void enforceLimitsRunsAutomaticallyOnAppend()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage::Config c =
            testConfig(dir.filePath(QStringLiteral("cold.db")));
        c.maxBytes = 512 * 1024;
        c.maxAgeDays = 36500;
        ZzColdStorage cold(c);
        QVERIFY(cold.open());
        QRandomGenerator rng(7);
        quint64 frontier = 0;
        for (int b = 0; b < 20; ++b) {
            QVector<ZzLogLine> block;
            block.reserve(1024);
            for (quint64 i = 0; i < 1024; ++i) {
                QString text;
                for (int k = 0; k < 25; ++k)
                    text.append(QString::number(rng.generate(), 16));
                block.append({text, QByteArray()});
            }
            QVERIFY(cold.appendBlock(block, frontier)); // 仅 append，不调 enforceLimits
            frontier += 1024;
        }
        QVERIFY(cold.baseLine() > 0ULL); // 自动清理已生效
    }

    /// @brief baseLine 跨重开持久：清理后重开，老行不复活。
    void baseLinePersistsAcrossReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
            QVERIFY(cold.appendBlock(makeLines(1024, 1024), 1024));
            cold.close();
            // 回拨第一块时间戳触发超龄清理
            sqlite3 *db = nullptr;
            QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
            QCOMPARE(sqlite3_exec(db,
                                  "UPDATE blocks SET start_ts_ns = 1, end_ts_ns = 1"
                                  " WHERE first_line = 0",
                                  nullptr, nullptr, nullptr),
                     SQLITE_OK);
            sqlite3_close(db);
            QVERIFY(cold.open());
            cold.enforceLimits();
            QCOMPARE(cold.baseLine(), 1024ULL);
        }
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QCOMPARE(cold.baseLine(), 1024ULL);
        QCOMPARE(cold.frontier(), 2048ULL);
        QVERIFY(cold.readLines(0, 1).isEmpty());
        QCOMPARE(cold.readLines(1024, 1).first().text, line(1024).text);
    }

    /// @brief 审查加固：块 payload 损坏（ZSTD 解压失败/尺寸不符）时按块损坏返回空。
    void corruptedPayloadReadsAsEmpty()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
        cold.close();

        // 截断 payload：ZSTD 帧头完好但数据不完整，解压必失败
        sqlite3 *db = nullptr;
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        QCOMPARE(sqlite3_exec(db,
                              "UPDATE blocks SET payload = substr(payload, 1, length(payload) / 2)"
                              " WHERE first_line = 0",
                              nullptr, nullptr, nullptr),
                 SQLITE_OK);
        sqlite3_close(db);

        QVERIFY(cold.open());
        QVERIFY(cold.readLines(0, 10).isEmpty()); // 损坏块按空返回，不崩溃不误读
    }

    /// @brief 审查加固：写事务中途失败回滚后连接保持可用（后续写入成功、失败前状态不变）。
    void appendFailureRollsBackAndConnectionStaysUsable()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        cold.close();

        // 外部连接注入触发器：UPDATE meta 时强制失败（meta 是普通表，可挂触发器）
        sqlite3 *db = nullptr;
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        QCOMPARE(sqlite3_exec(db,
                              "CREATE TRIGGER fail_meta BEFORE UPDATE ON meta"
                              " BEGIN SELECT RAISE(FAIL, 'injected'); END",
                              nullptr, nullptr, nullptr),
                 SQLITE_OK);
        sqlite3_close(db);

        QVERIFY(cold.open());
        QString error;
        QVERIFY(!cold.appendBlock(makeLines(0, 100), 0, &error)); // 事务第 3 步失败 → ROLLBACK
        QVERIFY(!error.isEmpty());
        QCOMPARE(cold.frontier(), 0ULL);                 // 状态不变
        QVERIFY(cold.readLines(0, 10).isEmpty());        // 块写入已随事务回滚
        QVERIFY(cold.search(QStringLiteral("cold-line-0")).isEmpty()); // FTS 同步回滚
        cold.close();

        // 拆除触发器后同一库可继续写入（连接未挂在事务里）
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        QCOMPARE(sqlite3_exec(db, "DROP TRIGGER fail_meta", nullptr, nullptr, nullptr),
                 SQLITE_OK);
        sqlite3_close(db);
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 100), 0));
        QCOMPARE(cold.frontier(), 100ULL);
        QCOMPARE(cold.readLines(0, 100).size(), 100);
    }

    /// @brief 审查加固：meta 声称有行（base < frontier）但块表为空 → 视为库损坏拒绝打开。
    void emptyBlockTableWithNonzeroMetaRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
        }
        // 删光块表但保留 meta：base=0 < frontier=1024 的"声称有行但无块"情形
        sqlite3 *db = nullptr;
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        QCOMPARE(sqlite3_exec(db, "DELETE FROM blocks", nullptr, nullptr, nullptr), SQLITE_OK);
        sqlite3_close(db);

        ZzColdStorage cold(testConfig(path));
        QString error;
        QVERIFY(!cold.open(&error));
        QVERIFY(!error.isEmpty());
    }

    /// @brief 审查修复（多会话并发写单库）：两个 ZzColdStorage 实例打开同一 cold.db，
    ///        交错 appendBlock——双方写入都成功（事务内重读 meta.frontier 权威值接管
    ///        过期缓存），库内数据连续无丢失无重复，frontier == 两边行数之和，
    ///        各自 readLines 能读回自己写的行内容。
    void concurrentInstancesInterleavedAppend()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage::Config ca = testConfig(path);
        ca.sessionId = QStringLiteral("session-a");
        ZzColdStorage::Config cb = testConfig(path);
        cb.sessionId = QStringLiteral("session-b");
        ZzColdStorage a(ca);
        ZzColdStorage b(cb);
        QVERIFY(a.open());
        QVERIFY(b.open());

        QString error;
        quint64 actual = 0; // appendBlock 事务内权威落点回传
        // 交错写入：行内容按全局行号生成（cold-line-<N>），便于连续性校验
        QVERIFY2(a.appendBlock(makeLines(0, 10), a.frontier(), &error, &actual),
                 qPrintable(error));
        QCOMPARE(actual, 0ULL);
        // b 的缓存 frontier=0 已过期（库内实为 10）：事务内重读接管，写入落 10..14
        QVERIFY2(b.appendBlock(makeLines(10, 5), b.frontier(), &error, &actual),
                 qPrintable(error));
        QCOMPARE(actual, 10ULL); // 落点漂移如实回传
        QCOMPARE(b.frontier(), 15ULL);
        // a 的缓存 frontier=10 已过期（库内实为 15）：写入落 15..21
        QVERIFY2(a.appendBlock(makeLines(15, 7), a.frontier(), &error, &actual),
                 qPrintable(error));
        QCOMPARE(actual, 15ULL);
        QCOMPARE(a.frontier(), 22ULL);
        // b 的缓存 frontier=15 已过期（库内实为 22）：写入落 22..24
        QVERIFY2(b.appendBlock(makeLines(22, 3), b.frontier(), &error, &actual),
                 qPrintable(error));
        QCOMPARE(actual, 22ULL);
        QCOMPARE(b.frontier(), 25ULL);

        // 第三方实例重开验证：frontier == 两边行数之和，全量连续读回无丢失无重复
        ZzColdStorage verifier(testConfig(path));
        QVERIFY(verifier.open());
        QCOMPARE(verifier.frontier(), 25ULL);
        QCOMPARE(verifier.baseLine(), 0ULL);
        const QVector<ZzLogLine> all = verifier.readLines(0, 25);
        QCOMPARE(all.size(), 25);
        for (qsizetype i = 0; i < all.size(); ++i) {
            QCOMPARE(all[i].text, line(quint64(i)).text);
            QCOMPARE(all[i].attributes, line(quint64(i)).attributes);
        }

        // 各自实例读回自己写的行（内存块索引仅含本实例写入的块）
        const QVector<ZzLogLine> aHead = a.readLines(0, 10);
        QCOMPARE(aHead.size(), 10);
        QCOMPARE(aHead.first().text, line(0).text);
        QCOMPARE(aHead.last().text, line(9).text);
        const QVector<ZzLogLine> aTail = a.readLines(15, 7);
        QCOMPARE(aTail.size(), 7);
        QCOMPARE(aTail.first().text, line(15).text);
        QCOMPARE(b.readLines(10, 5).first().text, line(10).text);
        QCOMPARE(b.readLines(22, 3).first().text, line(22).text);

        // 跨空洞读窗（a 的索引不含 b 的块）：按可得数据返回，不死循环
        const QVector<ZzLogLine> span = a.readLines(8, 10); // 8..17 含 b 的 10..14
        QCOMPARE(span.size(), 2); // 仅 8、9 可得
        QCOMPARE(span.last().text, line(9).text);

        // search 按 session_id 过滤：各自只见己方行的命中（短语查询：FTS5 查询词
        // 不能带连字符，"cold-line" 会解析失败；文本 tokenize 为 cold/line/<N>）
        QCOMPARE(a.search(QStringLiteral("\"cold line\""), 100).size(), 17); // 0..9 + 15..21
        QCOMPARE(b.search(QStringLiteral("\"cold line\""), 100).size(), 8);  // 10..14 + 22..24
    }

    /// @brief 审查修复轮 2（Important #2）：两实例交错写后由其中一个实例触发全局清理，
    ///        newBase 必须等于库内真实最老幸存块首行（而非本实例部分索引计算值），
    ///        重开库 open 成功（loadState 一致性校验不被误触发）且幸存数据完好。
    void enforceLimitsWithInterleavedLayoutKeepsConsistentBase()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage::Config ca = testConfig(path);
        ca.sessionId = QStringLiteral("session-a");
        ZzColdStorage::Config cb = testConfig(path);
        cb.sessionId = QStringLiteral("session-b");
        ZzColdStorage a(ca);
        ZzColdStorage b(cb);
        QVERIFY(a.open());
        QVERIFY(b.open());

        // 交错布局：A[0,100) B[100,200) A[200,300)；块 0 内嵌独立 token 标记行
        QVector<ZzLogLine> first = makeLines(0, 100);
        first[5].text = QStringLiteral("doomed GONETOKEN5 line");
        QVERIFY(a.appendBlock(first, a.frontier()));
        QVERIFY(b.appendBlock(makeLines(100, 100), b.frontier()));
        QVERIFY(a.appendBlock(makeLines(200, 100), a.frontier()));
        QCOMPARE(a.frontier(), 300ULL);
        QCOMPARE(a.search(QStringLiteral("GONETOKEN5")), (QVector<quint64>{5ULL}));

        // 外部连接把全局最老块（A 的 [0,100)）时间戳回拨 100 天，触发超龄清理
        sqlite3 *db = nullptr;
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        QCOMPARE(sqlite3_exec(db,
                              "UPDATE blocks SET start_ts_ns = 1000000000,"
                              " end_ts_ns = 1000000000 WHERE first_line = 0",
                              nullptr, nullptr, nullptr),
                 SQLITE_OK);
        sqlite3_close(db);

        // 由 b 触发清理：b 的内存索引只有 [100,200)，若按部分索引删块会错删/错算 base
        b.enforceLimits();
        QCOMPARE(b.baseLine(), 100ULL);   // 库内全局最老块 [0,100) 被删，base = 幸存首块 100
        QCOMPARE(b.frontier(), 200ULL);   // 缓存值：b 只追加过一块（库内权威 300 由重开校验）
        QCOMPARE(b.readLines(100, 3).size(), 3); // 自己的块仍可读
        a.close();
        b.close();

        // 重开：loadState 一致性校验通过，幸存区间 [100,300) 完好
        ZzColdStorage::Config cv = testConfig(path);
        cv.sessionId = QStringLiteral("session-a"); // 与块 0/2 同会话：search 过滤不遮蔽校验
        ZzColdStorage verifier(cv);
        QString error;
        QVERIFY2(verifier.open(&error), qPrintable(error));
        QCOMPARE(verifier.baseLine(), 100ULL);
        QCOMPARE(verifier.frontier(), 300ULL);
        QVERIFY(verifier.readLines(0, 10).isEmpty()); // 已清理区间
        const QVector<ZzLogLine> rest = verifier.readLines(100, 200);
        QCOMPARE(rest.size(), 200);
        for (qsizetype i = 0; i < rest.size(); ++i)
            QCOMPARE(rest[i].text, line(100 + quint64(i)).text);
        // FTS 同步删除：被删块文本不再命中（同会话幸存块 [200,300) 的文本仍可命中）
        QVERIFY(verifier.search(QStringLiteral("GONETOKEN5")).isEmpty());
        QCOMPARE(verifier.search(QStringLiteral("\"cold line\""), 300).size(), 100); // 仅块 2 的 100 行
    }
};

QTEST_GUILESS_MAIN(ZzColdStorageTest)
#include "ZzColdStorageTest.moc"
