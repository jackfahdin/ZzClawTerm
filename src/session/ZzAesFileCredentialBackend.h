#pragma once

#include "ZzCredentialBackend.h"

#include <QByteArray>
#include <QList>

/**
 * @brief AES-256-GCM 加密文件凭据后端（ZzCredentialBackend 实现之一）。
 *
 * 凭据保存在 credentials.dat（平台配置目录），文件整体加密：
 * PBKDF2-HMAC-SHA256 从主密码派生 32 字节密钥（60 万次迭代），
 * OpenSSL EVP 接口执行 AES-256-GCM 加解密。
 * 首次启动 initialize() 设主密码；unlock() 一次后密钥驻留内存；
 * lock()（或析构）用 OPENSSL_cleanse 清零密钥。锁定状态下一切凭据操作被拒绝。
 * GCM tag 提供完整性认证：主密码错误或文件被篡改都会导致 unlock 失败。
 *
 * 本类由 v0.1 的 ZzCredentialStore 原样迁移而来，行为与文件格式不变。
 */
class ZzAesFileCredentialBackend : public ZzCredentialBackend
{
    Q_OBJECT

public:
    /**
     * @brief 构造 AES 文件凭据后端。
     * @param filePath 凭据文件路径（测试注入临时路径）。
     * @param parent Qt 父对象。
     */
    explicit ZzAesFileCredentialBackend(const QString &filePath, QObject *parent = nullptr);

    /** @brief 析构时锁定（清零内存中的密钥与明文凭据）。 */
    ~ZzAesFileCredentialBackend() override;

    /**
     * @brief 默认凭据文件路径（平台配置目录下的 credentials.dat）。
     * @return 绝对路径。
     */
    static QString defaultFilePath();

    QString backendId() const override;
    bool isAvailable() const override;          // 恒 true：本地文件后端无外部依赖
    bool requiresMasterPassword() const override; // 恒 true
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
    /**
     * @brief 用内存中的密钥把全部凭据加密并原子落盘。
     * @return 成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    bool persist() const;

    QString m_filePath;                 ///< 凭据文件路径
    QByteArray m_key;                   ///< 派生密钥，仅解锁期间驻留内存
    QByteArray m_salt;                  ///< PBKDF2 盐（随文件头读写）
    quint32 m_kdfIterations = 600000;   ///< PBKDF2 迭代次数
    QList<ZzCredentialEntry> m_credentials; ///< 解锁期间的明文凭据
    mutable QString m_errorString;      ///< 最近一次失败的错误信息
};
