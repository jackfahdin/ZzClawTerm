#include <QtTest>
#include <QDate>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSysInfo>
#include <QTemporaryDir>

#include "ZzCredentialStore.h"

namespace {

/** @brief 解锁阈值：单次解锁（含 PBKDF2 60 万次迭代）不得超过 2000ms。 */
constexpr qint64 kUnlockThresholdMs = 2000;

/** @brief 往返阈值：1000 条凭据写入 + 冷启动解锁 + 全量读取不得超过 3000ms。 */
constexpr qint64 kRoundTripThresholdMs = 3000;

/** @brief 往返测试的凭据条数。 */
constexpr int kCredentialCount = 1000;

/**
 * @brief 读取物理内存总量描述（尽力而为，非 Linux 平台返回 unknown）。
 * @return 如 "16254000 kB"。
 */
QString totalMemoryString()
{
#ifdef Q_OS_LINUX
    QFile file(QStringLiteral("/proc/meminfo"));
    if (file.open(QIODevice::ReadOnly)) {
        const QString content = QString::fromLatin1(file.readAll());
        const QRegularExpression re(QStringLiteral("MemTotal:\\s*(\\d+\\s*kB)"));
        const QRegularExpressionMatch match = re.match(content);
        if (match.hasMatch())
            return match.captured(1);
    }
#endif
    return QStringLiteral("unknown");
}

/**
 * @brief 把一条性能记录写入 tests/perf/records/YYYY-MM-DD-<记录名>.json（规格 9.1）。
 * @param recordName 记录名（文件名后缀）。
 * @param thresholdMs 通过阈值（毫秒）。
 * @param measuredMs 实测值（毫秒）。
 * @param passed 是否达标。
 * @return 写入成功返回 true。
 */
bool writeRecord(const QString &recordName, qint64 thresholdMs, qint64 measuredMs, bool passed)
{
    QJsonObject env;
    env.insert(QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture());
    env.insert(QStringLiteral("memory"), totalMemoryString());
    env.insert(QStringLiteral("os"), QSysInfo::prettyProductName());
    env.insert(QStringLiteral("qtVersion"), QStringLiteral(QT_VERSION_STR));
    env.insert(QStringLiteral("compiler"),
               QStringLiteral(ZZ_COMPILER_ID) + QLatin1Char(' ') + QStringLiteral(ZZ_COMPILER_VERSION));
    env.insert(QStringLiteral("buildType"), QStringLiteral(ZZ_BUILD_TYPE));

    QJsonObject root;
    root.insert(QStringLiteral("testItem"), recordName);
    root.insert(QStringLiteral("thresholdMs"), thresholdMs);
    root.insert(QStringLiteral("measuredMs"), measuredMs);
    root.insert(QStringLiteral("passed"), passed);
    root.insert(QStringLiteral("environment"), env);
    root.insert(QStringLiteral("gitCommit"), QStringLiteral(ZZ_GIT_COMMIT_HASH));
    root.insert(QStringLiteral("timestamp"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));

    QDir dir(QStringLiteral(ZZ_RECORDS_DIR));
    if (!dir.exists() && !dir.mkpath(QStringLiteral(".")))
        return false;

    const QString path = dir.filePath(QDate::currentDate().toString(QStringLiteral("yyyy-MM-dd"))
                                      + QLatin1Char('-') + recordName + QStringLiteral(".json"));
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    file.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    return true;
}

} // namespace

/**
 * @brief 凭据加解密性能门控测试（规格 9.1：阈值失败即测试失败；仅 Release 构建有效）。
 */
class ZzCredentialPerfTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 单次解锁耗时（PBKDF2 60 万次迭代 + GCM 解密校验）不超过阈值。 */
    void unlockPerformance()
    {
        if (QStringLiteral(ZZ_BUILD_TYPE) != QLatin1String("Release"))
            QSKIP("性能数字仅 Release 构建有效（规格 9.1），当前构建跳过阈值判定");

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("PerfMaster-密码")));
        }

        qint64 elapsed = 0;
        {
            ZzCredentialStore store(path);
            QElapsedTimer timer;
            timer.start();
            QVERIFY(store.unlock(QStringLiteral("PerfMaster-密码")));
            elapsed = timer.elapsed();
        }

        const bool passed = elapsed <= kUnlockThresholdMs;
        QVERIFY(writeRecord(QStringLiteral("credential-unlock"), kUnlockThresholdMs, elapsed, passed));
        QVERIFY2(passed, qPrintable(QStringLiteral("解锁耗时 %1ms 超过阈值 %2ms")
                                        .arg(elapsed)
                                        .arg(kUnlockThresholdMs)));
    }

    /** @brief 1000 条凭据逐条加密落盘 + 冷启动解锁 + 全量读取的总耗时不超过阈值。 */
    void encryptDecryptRoundTripPerformance()
    {
        if (QStringLiteral(ZZ_BUILD_TYPE) != QLatin1String("Release"))
            QSKIP("性能数字仅 Release 构建有效（规格 9.1），当前构建跳过阈值判定");

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));
        const QString secret = QStringLiteral("perf-secret-0123456789-abcdefghijklmnopqrstuvwxyz-中文填充数据");

        QElapsedTimer timer;
        timer.start();

        QList<QUuid> ids;
        ids.reserve(kCredentialCount);
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("PerfMaster-密码")));
            for (int i = 0; i < kCredentialCount; ++i) {
                const QUuid id = store.addCredential(QStringLiteral("cred-%1").arg(i), secret);
                QVERIFY(!id.isNull());
                ids.append(id);
            }
        }
        {
            ZzCredentialStore store(path);
            QVERIFY(store.unlock(QStringLiteral("PerfMaster-密码")));
            for (const QUuid &id : ids)
                QCOMPARE(store.credential(id).value(), secret);
        }

        const qint64 elapsed = timer.elapsed();
        const bool passed = elapsed <= kRoundTripThresholdMs;
        QVERIFY(writeRecord(QStringLiteral("credential-roundtrip"), kRoundTripThresholdMs, elapsed, passed));
        QVERIFY2(passed, qPrintable(QStringLiteral("加解密往返耗时 %1ms 超过阈值 %2ms")
                                        .arg(elapsed)
                                        .arg(kRoundTripThresholdMs)));
    }
};

QTEST_GUILESS_MAIN(ZzCredentialPerfTest)

#include "ZzCredentialPerfTest.moc"
