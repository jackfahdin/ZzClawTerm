#include <QtTest>
#include <QElapsedTimer>
#include <QTemporaryDir>

#include "ZzCredentialStore.h"
#include "ZzPerfRecorder.h"

namespace {

/** @brief 解锁阈值：单次解锁（含 PBKDF2 60 万次迭代）不得超过 2000ms。 */
constexpr qint64 kUnlockThresholdMs = 2000;

/** @brief 往返阈值：1000 条凭据写入 + 冷启动解锁 + 全量读取不得超过 3000ms。 */
constexpr qint64 kRoundTripThresholdMs = 3000;

/** @brief 往返测试的凭据条数。 */
constexpr int kCredentialCount = 1000;

} // namespace

/**
 * @brief 凭据加解密性能门控测试（规格 9.1：阈值失败即测试失败；仅 Release 构建有效）。
 *
 * 记录统一经 ZzPerfRecorder 写入 tests/perf/records/YYYY-MM-DD-<功能名>.json，
 * schema 与 ZzLogEngine 性能记录一致（扁平结构 + memory_mb 数值）。
 */
class ZzCredentialPerfTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 单次解锁耗时（PBKDF2 60 万次迭代 + GCM 解密校验）不超过阈值。 */
    void unlockPerformance()
    {
        if (!ZzPerfRecorder::gatingEnabled())
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

        const bool passed = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("credential-unlock"),
            QStringLiteral("单次解锁耗时（PBKDF2 60 万次迭代 + GCM 解密校验）"),
            double(kUnlockThresholdMs), double(elapsed));
        QVERIFY2(passed, qPrintable(QStringLiteral("解锁耗时 %1ms 超过阈值 %2ms")
                                        .arg(elapsed)
                                        .arg(kUnlockThresholdMs)));
    }

    /** @brief 1000 条凭据逐条加密落盘 + 冷启动解锁 + 全量读取的总耗时不超过阈值。 */
    void encryptDecryptRoundTripPerformance()
    {
        if (!ZzPerfRecorder::gatingEnabled())
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
        const bool passed = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("credential-roundtrip"),
            QStringLiteral("1000 条凭据加密落盘 + 冷启动解锁 + 全量读取总耗时"),
            double(kRoundTripThresholdMs), double(elapsed),
            QStringLiteral("ms"),
            {{QStringLiteral("credentialCount"), kCredentialCount}});
        QVERIFY2(passed, qPrintable(QStringLiteral("加解密往返耗时 %1ms 超过阈值 %2ms")
                                        .arg(elapsed)
                                        .arg(kRoundTripThresholdMs)));
    }
};

QTEST_GUILESS_MAIN(ZzCredentialPerfTest)

#include "ZzCredentialPerfTest.moc"
