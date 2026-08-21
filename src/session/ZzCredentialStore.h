#pragma once

#include "ZzCredentialBackend.h"

/**
 * @brief 凭据存储门面：敏感信息的唯一出入口，按后端模式转发到具体实现。
 *
 * v0.2 起 ZzCredentialStore 从「AES-256-GCM 加密文件」的具体实现变为门面：
 * - BackendMode::AesFile：ZzAesFileCredentialBackend（v0.1 行为，PBKDF2 主密码
 *   + AES-256-GCM 文件整体加密，QSaveFile 原子落盘）；
 * - BackendMode::SystemKeyring：ZzKeyringCredentialBackend（Windows Credential
 *   Manager / macOS Keychain / Linux libsecret，系统托管解锁，无主密码）；
 * - BackendMode::Auto（默认）：系统密钥环可用则用，否则回退 AES 文件。
 * 应用层只持有本门面，切换后端对会话面板、编辑对话框、解锁框透明。
 *
 * 数据迁移约定（AES 文件 → 系统密钥环）：
 * 切换后端不会自动迁移也不会删除旧凭据文件（至少不丢数据）。旧文件中的凭据
 * 在密钥环后端下不可见；如需迁移，解锁本门面后调用
 * migrateLegacyAesFileToKeyring(主密码)：逐条按原 id 写入密钥环（会话中的
 * credentialId 引用保持有效），成功后旧文件改名 credentials.dat.migrated 留档。
 * 不迁移也可随时在设置中切回 aes-file 模式继续使用旧凭据。
 */
class ZzCredentialStore : public ZzCredentialBackend
{
    Q_OBJECT

public:
    /**
     * @brief 凭据后端模式（与 ZzAppSettings::credentialBackend 的取值一一对应）。
     */
    enum class BackendMode {
        Auto,         ///< 自动：系统密钥环可用则用，否则 AES 文件
        AesFile,      ///< AES-256-GCM 加密文件
        SystemKeyring ///< 系统密钥环
    };
    Q_ENUM(BackendMode)

    /**
     * @brief 构造 AES 文件后端的凭据存储（兼容 v0.1 调用形态）。
     * @param filePath 凭据文件路径（测试注入临时路径）。
     * @param parent Qt 父对象。
     */
    explicit ZzCredentialStore(const QString &filePath, QObject *parent = nullptr);

    /**
     * @brief 按后端模式构造凭据存储。
     * @param mode 后端模式；Auto 时按当前环境解析（密钥环可用→密钥环，否则 AES 文件）。
     * @param filePath 凭据文件路径（AES 文件后端使用；密钥环模式下仅用于迁移定位）。
     * @param parent Qt 父对象。
     */
    explicit ZzCredentialStore(BackendMode mode, const QString &filePath,
                               QObject *parent = nullptr);

    /** @brief 析构时锁定（清零内存中的密钥与明文凭据）。 */
    ~ZzCredentialStore() override;

    /**
     * @brief 默认凭据文件路径（平台配置目录下的 credentials.dat）。
     * @return 绝对路径。
     */
    static QString defaultFilePath();

    /**
     * @brief 后端模式 ↔ 设置串（"auto" / "aes-file" / "system-keyring"）。
     */
    static QString backendModeToString(BackendMode mode);
    static BackendMode backendModeFromString(const QString &text);

    /**
     * @brief 当前实际生效的后端模式（Auto 已解析为具体后端）。
     */
    BackendMode activeBackendMode() const;

    /**
     * @brief 把旧 AES 文件中的全部凭据迁移到系统密钥环（保留原凭据 id）。
     *
     * 仅当前后端为系统密钥环且 filePath 对应的旧文件存在时有意义。
     * 迁移成功后旧文件改名为 "<filePath>.migrated" 留档（改名失败不影响迁移结果，
     * 重复迁移按同 id 覆盖，幂等）。
     * @param masterPassword 旧 AES 文件的主密码。
     * @return 成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    bool migrateLegacyAesFileToKeyring(const QString &masterPassword);

    // ---- 以下接口全部转发到当前后端，语义见 ZzCredentialBackend ----
    QString backendId() const override;
    bool isAvailable() const override;
    bool requiresMasterPassword() const override;
    bool hasMasterPassword() const override;
    bool initialize(const QString &masterPassword) override;
    bool unlock(const QString &masterPassword) override;
    void lock() override;
    bool isUnlocked() const override;
    QUuid addCredential(const QString &name, const QString &secret) override;
    bool putCredential(const QUuid &credentialId, const QString &name,
                       const QString &secret) override;
    bool updateCredential(const QUuid &credentialId, const QString &secret) override;
    std::optional<QString> credential(const QUuid &credentialId) const override;
    bool removeCredential(const QUuid &credentialId) override;
    QList<ZzCredentialEntry> allCredentials() const override;
    QString errorString() const override;

private:
    QString m_filePath;              ///< 凭据文件路径（AES 后端存储 / 迁移定位）
    BackendMode m_activeMode;        ///< 实际生效的后端模式
    ZzCredentialBackend *m_backend;  ///< 当前后端，this 为父
    mutable QString m_errorString;   ///< 门面自身（迁移等）的错误信息，优先于后端错误返回
};
