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

#ifndef Q_OS_WIN // Windows 无 POSIX 权限语义，跳过
    /** @brief 凭据文件落盘权限必须为 0600（仅属主可读写）。 */
    void credentialFilePermissionsAreOwnerOnly()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        ZzCredentialStore store(path);
        QVERIFY(store.initialize(QStringLiteral("主密码-abc123")));

        const QFileDevice::Permissions perms = QFileInfo(path).permissions();
        QVERIFY(perms.testFlag(QFileDevice::ReadOwner));
        QVERIFY(perms.testFlag(QFileDevice::WriteOwner));
        // 组/其他用户不得有任何读、写、执行权限
        QCOMPARE(perms & (QFileDevice::ReadGroup | QFileDevice::WriteGroup
                          | QFileDevice::ExeGroup | QFileDevice::ReadOther
                          | QFileDevice::WriteOther | QFileDevice::ExeOther),
                 QFileDevice::Permissions{});
    }
#endif

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

    /** @brief 文件头 kdfIterations 超出 [1, 10000000] 判为文件损坏，拒绝解锁（防恶意文件 DoS）。 */
    void unlockRejectsOutOfRangeKdfIterations()
    {
        for (const quint32 forged : {quint32(0), quint32(10000001), quint32(0xFFFFFFFF)}) {
            QTemporaryDir dir;
            QVERIFY(dir.isValid());
            const QString path = dir.filePath(QStringLiteral("credentials.dat"));
            {
                ZzCredentialStore store(path);
                QVERIFY(store.initialize(QStringLiteral("正确密码")));
            }

            // 篡改文件头 kdfIterations（偏移 8，大端 u32）
            QFile file(path);
            QVERIFY(file.open(QIODevice::ReadWrite));
            QVERIFY(file.seek(8));
            QByteArray buf(4, Qt::Uninitialized);
            buf[0] = static_cast<char>(forged >> 24);
            buf[1] = static_cast<char>(forged >> 16);
            buf[2] = static_cast<char>(forged >> 8);
            buf[3] = static_cast<char>(forged);
            QCOMPARE(file.write(buf), qint64(4));
            file.close();

            ZzCredentialStore store(path);
            QVERIFY(!store.unlock(QStringLiteral("正确密码")));
            QVERIFY(!store.isUnlocked());
            QCOMPARE(store.errorString(), QStringLiteral("凭据文件格式非法"));
        }
    }

    /** @brief 加解密往返：写入凭据后锁定、重建实例、解锁，读回的明文必须一致（含中文与特殊字符）。 */
    void credentialRoundTrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        QUuid idAlpha;
        QUuid idBeta;
        QUuid idGamma;
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
            idAlpha = store.addCredential(QStringLiteral("root@web-01"), QStringLiteral("s3cret!@#"));
            idBeta = store.addCredential(QStringLiteral("deploy@db-01"), QStringLiteral("密码含中文与换行\n第二行"));
            idGamma = store.addCredential(QStringLiteral("空密码"), QString());
            QVERIFY(!idAlpha.isNull());
            QVERIFY(!idBeta.isNull());
            QVERIFY(!idGamma.isNull());
            // 同一实例内立即可读
            QCOMPARE(store.credential(idAlpha).value(), QStringLiteral("s3cret!@#"));
        } // 析构锁定，明文与密钥清零

        ZzCredentialStore store(path);
        QVERIFY(!store.credential(idAlpha).has_value()); // 未解锁不可读
        QVERIFY(store.unlock(QStringLiteral("主密码-xyz")));
        QCOMPARE(store.credential(idAlpha).value(), QStringLiteral("s3cret!@#"));
        QCOMPARE(store.credential(idBeta).value(), QStringLiteral("密码含中文与换行\n第二行"));
        QCOMPARE(store.credential(idGamma).value(), QString());
        QVERIFY(!store.credential(QUuid::createUuid()).has_value());
    }

    /** @brief 更新凭据后重新解锁读到新明文。 */
    void updateCredentialRoundTrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        QUuid id;
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
            id = store.addCredential(QStringLiteral("root@web-01"), QStringLiteral("old"));
            QVERIFY(!id.isNull());
            QVERIFY(store.updateCredential(id, QStringLiteral("new-密码")));
            QVERIFY(!store.updateCredential(QUuid::createUuid(), QStringLiteral("x")));
        }

        ZzCredentialStore store(path);
        QVERIFY(store.unlock(QStringLiteral("主密码-xyz")));
        QCOMPARE(store.credential(id).value(), QStringLiteral("new-密码"));
    }

    /** @brief 删除凭据后重新解锁不再可读；删除不存在的 id 返回 false。 */
    void removeCredentialRoundTrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        QUuid idKeep;
        QUuid idDrop;
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
            idKeep = store.addCredential(QStringLiteral("保留"), QStringLiteral("keep"));
            idDrop = store.addCredential(QStringLiteral("删除"), QStringLiteral("drop"));
            QVERIFY(store.removeCredential(idDrop));
            QVERIFY(!store.removeCredential(idDrop)); // 重复删除返回 false
            QVERIFY(!store.removeCredential(QUuid::createUuid()));
        }

        ZzCredentialStore store(path);
        QVERIFY(store.unlock(QStringLiteral("主密码-xyz")));
        QCOMPARE(store.credential(idKeep).value(), QStringLiteral("keep"));
        QVERIFY(!store.credential(idDrop).has_value());
    }

    /** @brief 锁定（或未解锁）状态下一切凭据操作被拒绝。 */
    void lockedStoreRejectsOperations()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        QUuid id;
        {
            ZzCredentialStore store(path);
            QVERIFY(store.initialize(QStringLiteral("主密码-xyz")));
            id = store.addCredential(QStringLiteral("root@web-01"), QStringLiteral("s3cret"));
            QVERIFY(!id.isNull());
        }

        // 未解锁
        ZzCredentialStore locked(path);
        QVERIFY(locked.addCredential(QStringLiteral("x"), QStringLiteral("y")).isNull());
        QVERIFY(!locked.errorString().isEmpty());
        QVERIFY(!locked.updateCredential(id, QStringLiteral("y")));
        QVERIFY(!locked.credential(id).has_value());
        QVERIFY(!locked.removeCredential(id));

        // 解锁后再 lock()，同样全部拒绝
        QVERIFY(locked.unlock(QStringLiteral("主密码-xyz")));
        QVERIFY(locked.credential(id).has_value());
        locked.lock();
        QVERIFY(locked.addCredential(QStringLiteral("x"), QStringLiteral("y")).isNull());
        QVERIFY(!locked.updateCredential(id, QStringLiteral("y")));
        QVERIFY(!locked.credential(id).has_value());
        QVERIFY(!locked.removeCredential(id));
    }
};

QTEST_GUILESS_MAIN(ZzCredentialStoreTest)

#include "ZzCredentialStoreTest.moc"
