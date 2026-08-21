#pragma once

#include <QList>
#include <QObject>
#include <QString>
#include <QUuid>
#include <optional>

/**
 * @brief 凭据条目导出结构：解锁状态下的明文视图，仅供跨后端迁移使用。
 */
struct ZzCredentialEntry {
    QUuid id;       ///< 凭据 id（ZzSessionProfile 以其引用凭据）
    QString name;   ///< 显示名
    QString secret; ///< 明文密码
};

/**
 * @brief 凭据存储后端抽象接口：敏感信息的唯一出入口（规格 §6.2 的抽象化）。
 *
 * v0.2 起凭据存储拆分为「接口 + 多后端」：
 * - ZzAesFileCredentialBackend：AES-256-GCM 加密文件（PBKDF2 主密码派生密钥）；
 * - ZzKeyringCredentialBackend：系统密钥环（Windows Credential Manager /
 *   macOS Keychain / Linux libsecret）。
 * 应用层经 ZzCredentialStore 门面按设置选择后端，对上层透明。
 *
 * 契约约定（所有后端一致）：
 * - 锁定状态下一切凭据操作被拒绝，错误原因见 errorString()；
 * - 凭据以 QUuid 引用，后端各自决定持久化形态；
 * - 错误信息统一简体中文。
 */
class ZzCredentialBackend : public QObject
{
    Q_OBJECT

public:
    using QObject::QObject;

    /** @brief 后端标识（"aes-file" / "system-keyring"），供日志与设置展示。 */
    virtual QString backendId() const = 0;

    /**
     * @brief 后端当前是否可用。
     *
     * AES 文件后端恒可用；系统密钥环后端取决于编译支持与运行时服务
     * （如 Linux 的 Secret Service 守护进程）是否可达。
     */
    virtual bool isAvailable() const = 0;

    /**
     * @brief 该后端是否需要主密码。
     *
     * AES 文件后端返回 true；系统密钥环由操作系统托管解锁，返回 false。
     * UI 层据此决定是否弹主密码对话框。
     */
    virtual bool requiresMasterPassword() const = 0;

    /** @brief 是否已完成首次初始化（AES 文件后端：凭据文件存在）。 */
    virtual bool hasMasterPassword() const = 0;

    /**
     * @brief 首次初始化，成功后直接处于解锁状态。
     * @param masterPassword 主密码；不需要主密码的后端忽略该参数。
     * @return 成功返回 true；错误原因见 errorString()。
     */
    virtual bool initialize(const QString &masterPassword) = 0;

    /**
     * @brief 解锁：成功后凭据可读写在内存/密钥环中进行。
     * @param masterPassword 主密码；不需要主密码的后端忽略该参数。
     * @return 成功返回 true；错误原因见 errorString()。
     */
    virtual bool unlock(const QString &masterPassword) = 0;

    /** @brief 锁定：清零内存中的密钥与明文凭据。幂等。 */
    virtual void lock() = 0;

    /** @brief 当前是否处于解锁状态。 */
    virtual bool isUnlocked() const = 0;

    /**
     * @brief 新增凭据（id 由后端生成）。
     * @param name 凭据显示名（如 "root@web-01"）。
     * @param secret 明文密码。
     * @return 成功返回凭据 id；失败返回 null QUuid，错误原因见 errorString()。
     */
    virtual QUuid addCredential(const QString &name, const QString &secret) = 0;

    /**
     * @brief 按指定 id 写入凭据（已存在则覆盖），供跨后端迁移保留引用使用。
     * @return 成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    virtual bool putCredential(const QUuid &credentialId, const QString &name,
                               const QString &secret) = 0;

    /**
     * @brief 更新凭据明文。
     * @return 成功返回 true；id 不存在或未解锁返回 false，错误原因见 errorString()。
     */
    virtual bool updateCredential(const QUuid &credentialId, const QString &secret) = 0;

    /**
     * @brief 读取凭据明文。
     * @return 找到且已解锁返回明文，否则返回 std::nullopt。
     */
    virtual std::optional<QString> credential(const QUuid &credentialId) const = 0;

    /**
     * @brief 删除凭据。
     * @return 成功返回 true；id 不存在或未解锁返回 false，错误原因见 errorString()。
     */
    virtual bool removeCredential(const QUuid &credentialId) = 0;

    /**
     * @brief 导出全部凭据（明文），仅供迁移到另一后端使用。
     * @return 解锁状态下返回全部条目；锁定状态返回空列表。
     */
    virtual QList<ZzCredentialEntry> allCredentials() const = 0;

    /** @brief 最近一次失败的错误信息（简体中文）。 */
    virtual QString errorString() const = 0;
};
