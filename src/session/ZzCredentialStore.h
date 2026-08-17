#pragma once

#include <QObject>
#include <QByteArray>
#include <QList>
#include <QString>
#include <QUuid>
#include <optional>

/**
 * @brief 凭据存储：AES-256-GCM 加密 + 主密码，敏感信息的唯一出入口。
 *
 * 凭据保存在 credentials.dat（平台配置目录），文件整体加密：
 * PBKDF2-HMAC-SHA256 从主密码派生 32 字节密钥（60 万次迭代），
 * OpenSSL EVP 接口执行 AES-256-GCM 加解密。
 * 首次启动 initialize() 设主密码；unlock() 一次后密钥驻留内存；
 * lock()（或析构）用 OPENSSL_cleanse 清零密钥。锁定状态下一切凭据操作被拒绝。
 * GCM tag 提供完整性认证：主密码错误或文件被篡改都会导致 unlock 失败。
 *
 * v0.2 可将系统密钥环实现为同一抽象的另一后端，应用层无感。
 */
class ZzCredentialStore : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 构造凭据存储。
     * @param filePath 凭据文件路径（测试注入临时路径）。
     * @param parent Qt 父对象。
     */
    explicit ZzCredentialStore(const QString &filePath, QObject *parent = nullptr);

    /** @brief 析构时锁定（清零内存中的密钥与明文凭据）。 */
    ~ZzCredentialStore() override;

    /**
     * @brief 默认凭据文件路径（平台配置目录下的 credentials.dat）。
     * @return 绝对路径。
     */
    static QString defaultFilePath();

    /** @brief 是否已设置过主密码（凭据文件存在）。 */
    bool hasMasterPassword() const;

    /**
     * @brief 首次初始化：设置主密码并创建凭据文件，成功后直接处于解锁状态。
     * @param masterPassword 主密码，不能为空。
     * @return 成功返回 true；文件已存在或密码为空返回 false，错误原因见 errorString()。
     */
    bool initialize(const QString &masterPassword);

    /**
     * @brief 用主密码解锁：派生密钥、解密并校验凭据文件，成功后密钥驻留内存。
     * @param masterPassword 主密码。
     * @return 成功返回 true；主密码错误、文件损坏或已处于解锁状态返回 false，
     *         错误原因见 errorString()。
     */
    bool unlock(const QString &masterPassword);

    /** @brief 锁定：清零内存中的密钥与全部明文凭据。幂等。 */
    void lock();

    /** @brief 当前是否处于解锁状态。 */
    bool isUnlocked() const;

    /**
     * @brief 新增凭据。
     * @param name 凭据显示名（如 "root@web-01"）。
     * @param secret 明文密码（仅内存中短暂存在，落盘前整体加密）。
     * @return 成功返回凭据 id（QUuid，供 ZzSessionProfile::credentialId 引用）；
     *         失败返回 null QUuid，错误原因见 errorString()。锁定状态下必然失败。
     */
    QUuid addCredential(const QString &name, const QString &secret);

    /**
     * @brief 更新凭据明文。
     * @param credentialId addCredential 返回的 id。
     * @param secret 新明文密码。
     * @return 成功返回 true；id 不存在或未解锁返回 false，错误原因见 errorString()。
     */
    bool updateCredential(const QUuid &credentialId, const QString &secret);

    /**
     * @brief 读取凭据明文。
     * @param credentialId addCredential 返回的 id。
     * @return 找到且已解锁返回明文，否则返回 std::nullopt。
     */
    std::optional<QString> credential(const QUuid &credentialId) const;

    /**
     * @brief 删除凭据。
     * @param credentialId addCredential 返回的 id。
     * @return 成功返回 true；id 不存在或未解锁返回 false，错误原因见 errorString()。
     */
    bool removeCredential(const QUuid &credentialId);

    /** @brief 最近一次失败的错误信息（简体中文）。 */
    QString errorString() const;

private:
    /**
     * @brief 内存中的凭据条目（明文，仅在解锁期间存在）。
     */
    struct Credential {
        QUuid id;       ///< 凭据 id
        QString name;   ///< 显示名
        QString secret; ///< 明文密码
    };

    /**
     * @brief 用内存中的密钥把全部凭据加密并原子落盘。
     * @return 成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    bool persist() const;

    QString m_filePath;                 ///< 凭据文件路径
    QByteArray m_key;                   ///< 派生密钥，仅解锁期间驻留内存
    QByteArray m_salt;                  ///< PBKDF2 盐（随文件头读写）
    quint32 m_kdfIterations = 600000;   ///< PBKDF2 迭代次数
    QList<Credential> m_credentials;    ///< 解锁期间的明文凭据
    mutable QString m_errorString;      ///< 最近一次失败的错误信息
};
