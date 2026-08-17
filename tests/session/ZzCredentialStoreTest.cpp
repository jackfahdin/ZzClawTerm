#include <QtTest>
#include <QFile>
#include <QFileInfo>
#include <QTemporaryDir>

#include "ZzCredentialStore.h"

/**
 * @brief ZzCredentialStore 主密码生命周期单元测试。
 */
class ZzCredentialStoreTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 首次初始化生成凭据文件并直接处于解锁状态。 */
    void initializeCreatesFileAndUnlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        ZzCredentialStore store(path);
        QVERIFY(!store.hasMasterPassword());
        QVERIFY(!store.isUnlocked());

        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));
        QVERIFY(store.hasMasterPassword());
        QVERIFY(store.isUnlocked());
        QVERIFY(QFileInfo::exists(path));
        QVERIFY(QFileInfo(path).size() > 0);
    }

    /** @brief 已存在凭据文件时拒绝重复初始化。 */
    void initializeTwiceRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(dir.filePath(QStringLiteral("credentials.dat")));

        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));
        QVERIFY(!store.initialize(QStringLiteral("另一个密码")));
        QVERIFY(!store.errorString().isEmpty());
    }

    /** @brief 空主密码被拒绝。 */
    void initializeEmptyPasswordRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(dir.filePath(QStringLiteral("credentials.dat")));

        QVERIFY(!store.initialize(QString()));
        QVERIFY(!store.hasMasterPassword());
    }

    /** @brief 正确主密码解锁成功，密钥驻留内存直到 lock()。 */
    void unlockWithCorrectPassword()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("正确密码")));
        } // 析构即锁定

        ZzCredentialStore store(path);
        QVERIFY(!store.isUnlocked());
        QVERIFY(store.unlock(QStringLiteral("正确密码")));
        QVERIFY(store.isUnlocked());

        store.lock();
        QVERIFY(!store.isUnlocked());
    }

    /** @brief 错误主密码解锁失败（GCM tag 校验不通过），且不驻留任何密钥。 */
    void unlockWithWrongPasswordFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("正确密码")));
        }

        ZzCredentialStore store(path);
        QVERIFY(!store.unlock(QStringLiteral("错误密码")));
        QVERIFY(!store.isUnlocked());
        QVERIFY(!store.errorString().isEmpty());
    }

    /** @brief 文件内容被篡改后解锁失败（GCM 认证失败或格式非法）。 */
    void unlockWithCorruptedFileFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("正确密码")));
        }

        // 篡改文件最后一个字节（位于 GCM tag 区）
        QFile file(path);
        QVERIFY(file.open(QIODevice::ReadWrite));
        QVERIFY(file.seek(file.size() - 1));
        char byte = 0;
        QVERIFY(file.getChar(&byte));
        QVERIFY(file.seek(file.size() - 1));
        const char flipped = byte ^ 0xFF;
        QVERIFY(file.putChar(flipped));
        file.close();

        ZzCredentialStore store(path);
        QVERIFY(!store.unlock(QStringLiteral("正确密码")));
        QVERIFY(!store.isUnlocked());
    }

    /** @brief 已解锁状态下重复 unlock 被拒绝。 */
    void unlockTwiceRejected()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(dir.filePath(QStringLiteral("credentials.dat")));

        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));
        QVERIFY(!store.unlock(QStringLiteral("主密码-abc123")));
        QVERIFY(!store.errorString().isEmpty());
    }
};

QTEST_GUILESS_MAIN(ZzCredentialStoreTest)

#include "ZzCredentialStoreTest.moc"
