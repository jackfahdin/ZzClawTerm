#include <QtTest/QtTest>

#include "ZzPerfRecorder.h"

/**
 * @brief 验证性能记录器：JSON 字段齐全（统一 schema）、阈值判定。
 *
 * 统一 schema 以 ZzLogEngine 扁平结构为准：testName/threshold/unit/measured/passed
 * + environment{cpu, memory_mb, os, kernel, qtVersion, compiler, buildType, gitCommit}
 * + timestamp（UTC）+ details。
 */
class tst_PerfRecorder : public QObject
{
    Q_OBJECT
private slots:
    void recordWritesJson()
    {
        // 自检记录无性能语义，落盘到临时目录而非入库目录，避免污染仓库
        QTemporaryDir recordsDir;
        QVERIFY(recordsDir.isValid());
        ZzPerfRecorder::setRecordsDirOverride(recordsDir.path());

        // 本用例自身即一次真实记录（功能名固定为基建自检）
        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("perf-infra-selfcheck"),
            QStringLiteral("记录器自检"), 1000.0, 1.0);
        QVERIFY(ok);

        const QString path = ZzPerfRecorder::recordFilePath(
            QStringLiteral("perf-infra-selfcheck"));
        QVERIFY2(path.startsWith(recordsDir.path()), qPrintable(path));
        QFile file(path);
        QVERIFY2(file.exists(), qPrintable(path));
        QVERIFY(file.open(QIODevice::ReadOnly));
        const QJsonObject entry =
            QJsonDocument::fromJson(file.readAll()).object();
        QVERIFY(!entry.isEmpty());

        QCOMPARE(entry.value(QStringLiteral("testName")).toString(),
                 QStringLiteral("记录器自检"));
        QCOMPARE(entry.value(QStringLiteral("threshold")).toDouble(), 1000.0);
        QCOMPARE(entry.value(QStringLiteral("measured")).toDouble(), 1.0);
        QCOMPARE(entry.value(QStringLiteral("unit")).toString(),
                 QStringLiteral("ms"));
        QCOMPARE(entry.value(QStringLiteral("passed")).toBool(), true);
        QVERIFY(entry.value(QStringLiteral("details")).isObject());
        QVERIFY(!entry.value(QStringLiteral("timestamp")).toString().isEmpty());

        const QJsonObject env =
            entry.value(QStringLiteral("environment")).toObject();
        for (const char *key : {"cpu", "memory_mb", "os", "kernel",
                                "qtVersion", "compiler", "buildType", "gitCommit"}) {
            QVERIFY2(env.contains(QLatin1String(key)), key);
        }
        // memory_mb 为数值（MB），不是 "31942880 kB" 这类字符串
        QVERIFY(env.value(QStringLiteral("memory_mb")).isDouble());
        QVERIFY(!env.value(QStringLiteral("gitCommit")).toString().isEmpty());

        ZzPerfRecorder::setRecordsDirOverride(QString()); // 恢复默认目录
    }

    void thresholdViolationReturnsFalse()
    {
        // 同 recordWritesJson：自检记录落临时目录，不污染入库目录
        QTemporaryDir recordsDir;
        QVERIFY(recordsDir.isValid());
        ZzPerfRecorder::setRecordsDirOverride(recordsDir.path());

        QVERIFY(!ZzPerfRecorder::recordAndCheck(
            QStringLiteral("perf-infra-selfcheck-violation"),
            QStringLiteral("阈值违例自检"), 10.0, 999.0));

        ZzPerfRecorder::setRecordsDirOverride(QString()); // 恢复默认目录
    }
};

QTEST_MAIN(tst_PerfRecorder)
#include "tst_PerfRecorder.moc"
