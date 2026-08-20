#include "ZzColdStorage.h"

#include <QFile>
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
};

QTEST_GUILESS_MAIN(ZzColdStorageTest)
#include "ZzColdStorageTest.moc"
