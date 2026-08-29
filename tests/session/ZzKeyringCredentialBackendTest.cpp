#include <QtTest>
#include <QTemporaryDir>

#include "ZzCredentialStore.h"
#include "ZzKeyringCredentialBackend.h"

/**
 * @brief 系统密钥环凭据后端测试。
 *
 * 覆盖三类路径：
 * 1. 降级路径：ZZCLAWTERM_KEYRING_DISABLE=1 模拟密钥环不可用，
 *    后端操作优雅失败（中文错误），门面 Auto 模式回退 AES 文件后端；
 * 2. 真实密钥环往返（本机 Secret Service 可达时执行，否则 QSKIP）：
 *    增/改/读/删/指定 id 写入/全量导出；
 * 3. AES 文件 → 密钥环迁移（真实密钥环可用时执行）：保留凭据 id，
 *    旧文件改名 .migrated 留档。
 *
 * @note 真实密钥环用例会向系统密钥环写入标签为 "ZzClawTerm: ..." 的条目，
 *       用例结束逐一删除（m_createdIds 兜底清理），不污染用户密钥环。
 */
class ZzKeyringCredentialBackendTest : public QObject
{
    Q_OBJECT

    /** @brief 待清理的密钥环条目 id（真实密钥环用例登记，cleanup 兜底删除）。 */
    QList<QUuid> m_createdIds;

    /** @brief 强制禁用密钥环（进入降级路径）。 */
    static void disableKeyring()
    {
        qputenv("ZZCLAWTERM_KEYRING_DISABLE", "1");
    }

    /** @brief 解除强制禁用。 */
    static void enableKeyring()
    {
        qunsetenv("ZZCLAWTERM_KEYRING_DISABLE");
    }

    /**
     * @brief macOS CI 无头 runner 识别谓词（供真实密钥环用例 slot 内 QSKIP）。
     *
     * GitHub Actions macOS runner 上 probeAvailability() 为真、addCredential 也返回
     * 非空 id，但写入的条目在 allCredentials() 中不可见（login keychain 无头行为
     * 不可靠），属运行环境缺陷而非产品缺陷；本地（有桌面会话）不受影响。
     * GitHub Actions 默认注入 CI=true，用 qEnvironmentVariableIsSet 识别。
     * 注意不能用 ZZCLAWTERM_KEYRING_DISABLE 禁用方式跳过：真实用例入口先调用
     * enableKeyring() 清除了该变量。
     * 谓词而非辅助函数内 QSKIP：QSKIP 宏的 return 只退出所在函数，slot 体仍继续
     * 执行真实钥匙环写入（Qt 6.11.1 qtestcase.h 实证）。
     */
    static bool isCiMacos()
    {
#ifdef Q_OS_MACOS
        return qEnvironmentVariableIsSet("CI");
#else
        return false;
#endif
    }

private slots:
    void cleanup()
    {
        // 兜底：删除本测试写入密钥环的全部条目
        if (!m_createdIds.isEmpty()) {
            ZzKeyringCredentialBackend backend;
            if (backend.initialize(QString())) {
                for (const QUuid &id : m_createdIds) {
                    backend.removeCredential(id);
                }
            }
            m_createdIds.clear();
        }
        enableKeyring();
    }

    /** @brief 环境变量 ZZCLAWTERM_KEYRING_DISABLE=1 强制密钥环不可用。 */
    void envVarDisablesKeyring()
    {
        disableKeyring();
        QVERIFY(!ZzKeyringCredentialBackend::probeAvailability());
        ZzKeyringCredentialBackend backend;
        QVERIFY(!backend.isAvailable());
        enableKeyring();
        QCOMPARE(ZzKeyringCredentialBackend::probeAvailability(),
                 ZzKeyringCredentialBackend::probeAvailability()); // 无副作用
    }

    /** @brief 降级路径：不可用的密钥环后端一切操作优雅失败并给出中文错误。 */
    void unavailableBackendFailsGracefully()
    {
        disableKeyring();
        ZzKeyringCredentialBackend backend;
        QVERIFY(!backend.isAvailable());
        QVERIFY(!backend.requiresMasterPassword());

        QVERIFY(!backend.initialize(QString())); // 不可用即初始化失败
        QCOMPARE(backend.errorString(), QStringLiteral("系统密钥环不可用"));
        QVERIFY(!backend.isUnlocked());

        QVERIFY(backend.addCredential(QStringLiteral("n"), QStringLiteral("s")).isNull());
        QCOMPARE(backend.errorString(), QStringLiteral("凭据存储未解锁"));

        QVERIFY(!backend.updateCredential(QUuid::createUuid(), QStringLiteral("s")));
        QVERIFY(!backend.credential(QUuid::createUuid()).has_value());
        QVERIFY(!backend.removeCredential(QUuid::createUuid()));
        QVERIFY(backend.allCredentials().isEmpty());
    }

    /** @brief 门面 Auto 模式：密钥环被禁用时回退 AES 文件后端，功能完整可用。 */
    void autoModeFallsBackToAesFile()
    {
        disableKeyring();
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        ZzCredentialStore store(ZzCredentialStore::BackendMode::Auto, path);
        QCOMPARE(store.activeBackendMode(), ZzCredentialStore::BackendMode::AesFile);
        QCOMPARE(store.backendId(), QStringLiteral("aes-file"));
        QVERIFY(store.requiresMasterPassword());

        // AES 后端完整往返：初始化 → 增 → 锁 → 解锁 → 读 → 删
        QVERIFY(store.initialize(QStringLiteral("主密码")));
        const QUuid id = store.addCredential(QStringLiteral("root@web-01"),
                                             QStringLiteral("秘密-1"));
        QVERIFY(!id.isNull());
        store.lock();
        QVERIFY(store.unlock(QStringLiteral("主密码")));
        QCOMPARE(store.credential(id).value(), QStringLiteral("秘密-1"));
        QVERIFY(store.removeCredential(id));
    }

    /** @brief 门面强制密钥环模式：密钥环被禁用时后端报告不可用（不静默换后端）。 */
    void forcedKeyringModeReportsUnavailable()
    {
        disableKeyring();
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzCredentialStore store(ZzCredentialStore::BackendMode::SystemKeyring,
                                dir.filePath(QStringLiteral("credentials.dat")));
        QCOMPARE(store.activeBackendMode(), ZzCredentialStore::BackendMode::SystemKeyring);
        QVERIFY(!store.isAvailable());
        QVERIFY(!store.unlock(QString()));
        QCOMPARE(store.errorString(), QStringLiteral("系统密钥环不可用"));
    }

    /** @brief 后端模式串 ↔ 枚举互转。 */
    void backendModeStringRoundTrip()
    {
        using Mode = ZzCredentialStore::BackendMode;
        QCOMPARE(ZzCredentialStore::backendModeFromString(QStringLiteral("auto")), Mode::Auto);
        QCOMPARE(ZzCredentialStore::backendModeFromString(QStringLiteral("aes-file")),
                 Mode::AesFile);
        QCOMPARE(ZzCredentialStore::backendModeFromString(QStringLiteral("system-keyring")),
                 Mode::SystemKeyring);
        QCOMPARE(ZzCredentialStore::backendModeFromString(QStringLiteral("未知")),
                 Mode::Auto); // 未识别回退 Auto
        QCOMPARE(ZzCredentialStore::backendModeToString(Mode::SystemKeyring),
                 QStringLiteral("system-keyring"));
    }

    /** @brief 迁移守卫：非密钥环后端 / 无旧文件 / 未解锁时迁移失败且不丢数据。 */
    void migrationGuards()
    {
        disableKeyring();
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        // AES 后端下调用迁移：失败，文件原样保留
        ZzCredentialStore aesStore(ZzCredentialStore::BackendMode::AesFile, path);
        QVERIFY(aesStore.initialize(QStringLiteral("pw")));
        const QUuid id = aesStore.addCredential(QStringLiteral("a"), QStringLiteral("s"));
        QVERIFY(!id.isNull());
        QVERIFY(!aesStore.migrateLegacyAesFileToKeyring(QStringLiteral("pw")));
        QVERIFY(QFileInfo::exists(path));
        QCOMPARE(aesStore.credential(id).value(), QStringLiteral("s"));

        // 密钥环后端但不可用：初始化失败导致未解锁，迁移失败
        ZzCredentialStore keyringStore(ZzCredentialStore::BackendMode::SystemKeyring, path);
        QVERIFY(!keyringStore.isUnlocked());
        QVERIFY(!keyringStore.migrateLegacyAesFileToKeyring(QStringLiteral("pw")));
        QVERIFY(QFileInfo::exists(path)); // 旧文件不动
    }

    /** @brief 真实密钥环往返：增/读/改/指定 id 写入/全量导出/删。不可用则跳过。 */
    void realKeyringRoundTrip()
    {
        enableKeyring();
        if (isCiMacos()) {
            QSKIP("macOS 无头 CI runner 钥匙串条目写入后不可见（运行环境缺陷，非产品缺陷）");
        }
        if (!ZzKeyringCredentialBackend::probeAvailability()) {
            QSKIP("系统密钥环不可用（无 Secret Service 守护进程），跳过真实往返");
        }

        ZzKeyringCredentialBackend backend;
        QVERIFY(backend.isAvailable());
        QVERIFY(!backend.requiresMasterPassword());
        QVERIFY(backend.hasMasterPassword());
        QVERIFY(backend.unlock(QString()));

        // 增 → 读
        const QUuid id = backend.addCredential(QStringLiteral("测试条目"),
                                               QStringLiteral("口令-中文-123"));
        QVERIFY2(!id.isNull(), qPrintable(backend.errorString()));
        m_createdIds.append(id);
        QCOMPARE(backend.credential(id).value(), QStringLiteral("口令-中文-123"));

        // 改 → 读（名称标签应保留）
        QVERIFY(backend.updateCredential(id, QStringLiteral("新口令")));
        QCOMPARE(backend.credential(id).value(), QStringLiteral("新口令"));

        // 指定 id 写入（迁移路径用的 putCredential）
        const QUuid fixedId = QUuid::createUuid();
        QVERIFY(backend.putCredential(fixedId, QStringLiteral("迁移条目"),
                                      QStringLiteral("迁移口令")));
        m_createdIds.append(fixedId);
        QCOMPARE(backend.credential(fixedId).value(), QStringLiteral("迁移口令"));

        // 全量导出包含两条
        const QList<ZzCredentialEntry> all = backend.allCredentials();
        QVERIFY(all.size() >= 2);
        bool foundFixed = false;
        for (const ZzCredentialEntry &entry : all) {
            if (entry.id == fixedId) {
                foundFixed = true;
                QCOMPARE(entry.name, QStringLiteral("迁移条目"));
                QCOMPARE(entry.secret, QStringLiteral("迁移口令"));
            }
        }
        QVERIFY(foundFixed);

        // 更新不存在 id → 失败
        QVERIFY(!backend.updateCredential(QUuid::createUuid(), QStringLiteral("x")));

        // 删 → 读不到；重复删 → 失败
        QVERIFY(backend.removeCredential(id));
        m_createdIds.removeAll(id);
        QVERIFY(!backend.credential(id).has_value());
        QVERIFY(!backend.removeCredential(id));

        // 锁定后操作被拒绝
        backend.lock();
        QVERIFY(backend.addCredential(QStringLiteral("n"), QStringLiteral("s")).isNull());
        QVERIFY(backend.unlock(QString())); // 重新解锁供 cleanup 删除 fixedId
    }

    /** @brief AES 文件 → 密钥环迁移：id 保留、旧文件改名留档、错误主密码不动数据。 */
    void realKeyringMigration()
    {
        enableKeyring();
        if (isCiMacos()) {
            QSKIP("macOS 无头 CI runner 钥匙串条目写入后不可见（运行环境缺陷，非产品缺陷）");
        }
        if (!ZzKeyringCredentialBackend::probeAvailability()) {
            QSKIP("系统密钥环不可用，跳过迁移用例");
        }

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        // 旧 AES 文件：两条凭据
        QUuid id1, id2;
        {
            ZzCredentialStore legacy(ZzCredentialStore::BackendMode::AesFile, path);
            QVERIFY(legacy.initialize(QStringLiteral("旧主密码")));
            id1 = legacy.addCredential(QStringLiteral("root@web-01"), QStringLiteral("密码甲"));
            id2 = legacy.addCredential(QStringLiteral("key 口令"), QStringLiteral("口令乙"));
            QVERIFY(!id1.isNull() && !id2.isNull());
        }

        ZzCredentialStore store(ZzCredentialStore::BackendMode::SystemKeyring, path);
        QVERIFY(store.initialize(QString()));

        // 错误主密码：迁移失败，旧文件原样保留
        QVERIFY(!store.migrateLegacyAesFileToKeyring(QStringLiteral("错误密码")));
        QVERIFY(QFileInfo::exists(path));
        QVERIFY(!store.errorString().isEmpty());

        // 正确主密码：迁移成功
        QVERIFY2(store.migrateLegacyAesFileToKeyring(QStringLiteral("旧主密码")),
                 qPrintable(store.errorString()));
        m_createdIds.append(id1);
        m_createdIds.append(id2);

        // 凭据 id 保留，引用（ZzSessionProfile::credentialId）迁移后仍有效
        QCOMPARE(store.credential(id1).value(), QStringLiteral("密码甲"));
        QCOMPARE(store.credential(id2).value(), QStringLiteral("口令乙"));

        // 旧文件改名留档（不丢数据）
        QVERIFY(!QFileInfo::exists(path));
        QVERIFY(QFileInfo::exists(path + QStringLiteral(".migrated")));

        // 幂等：旧文件已改名，再迁移报"无可迁移数据"
        QVERIFY(!store.migrateLegacyAesFileToKeyring(QStringLiteral("旧主密码")));
    }

    /**
     * @brief 回归：门面 errorString() 不得被陈旧迁移错误遮蔽——迁移失败后的
     *        后端操作失败必须返回该操作自身的错误信息。
     */
    void facadeErrorStringNotStale()
    {
        enableKeyring();
        if (isCiMacos()) {
            QSKIP("macOS 无头 CI runner 钥匙串条目写入后不可见（运行环境缺陷，非产品缺陷）");
        }
        if (!ZzKeyringCredentialBackend::probeAvailability()) {
            QSKIP("系统密钥环不可用，跳过 errorString 回归用例");
        }

        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("credentials.dat"));

        // 造一个旧 AES 文件，让密钥环门面迁移时以错误主密码失败一次
        {
            ZzCredentialStore legacy(ZzCredentialStore::BackendMode::AesFile, path);
            QVERIFY(legacy.initialize(QStringLiteral("正确密码")));
        }
        ZzCredentialStore store(ZzCredentialStore::BackendMode::SystemKeyring, path);
        QVERIFY(store.initialize(QString()));
        QVERIFY(!store.migrateLegacyAesFileToKeyring(QStringLiteral("错误密码")));
        QCOMPARE(store.errorString(), QStringLiteral("主密码错误或凭据文件已损坏"));

        // 之后的后端操作失败必须返回自身错误，而非上一条迁移错误
        QVERIFY(!store.removeCredential(QUuid::createUuid()));
        QVERIFY(store.errorString().startsWith(QStringLiteral("凭据不存在：")));
    }
};

QTEST_GUILESS_MAIN(ZzKeyringCredentialBackendTest)

#include "ZzKeyringCredentialBackendTest.moc"
