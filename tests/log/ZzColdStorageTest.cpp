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
};

QTEST_GUILESS_MAIN(ZzColdStorageTest)
#include "ZzColdStorageTest.moc"
