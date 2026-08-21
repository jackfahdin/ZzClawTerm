#pragma once

#include "ZzCredentialBackend.h"

/**
 * @brief 系统密钥环凭据后端（ZzCredentialBackend 实现之一）。
 *
 * 按平台选用系统凭据设施，凭据以 id 为键直接写入系统密钥环：
 * - Windows：Credential Manager（wincred，目标名 "ZzClawTerm/<id>"）；
 * - macOS：Keychain（generic password，service=ZzClawTerm，account=<id>）；
 * - Linux：libsecret（Secret Service API，属性 id + app=ZzClawTerm）。
 *
 * 密钥环由操作系统托管解锁（登录会话解锁），因此本后端不需要主密码：
 * initialize()/unlock() 忽略密码参数，只要服务可达即进入解锁状态；
 * lock() 仅翻转内存中的解锁标记，密钥环内容不受影响。
 *
 * 优雅降级：编译期未找到平台设施（如 Linux 缺 libsecret 开发包）或运行时
 * 服务不可达时，isAvailable() 返回 false，一切操作以中文错误信息失败；
 * 环境变量 ZZCLAWTERM_KEYRING_DISABLE=1 可强制禁用本后端（测试与应急回退）。
 * 上层（ZzCredentialStore 门面）据此回退 AES 文件后端。
 */
class ZzKeyringCredentialBackend : public ZzCredentialBackend
{
    Q_OBJECT

public:
    explicit ZzKeyringCredentialBackend(QObject *parent = nullptr);

    /**
     * @brief 探测系统密钥环是否可用（编译支持 + 未被环境变量禁用 + 服务可达）。
     *
     * Linux 下每次真实探测都会经 secret_service_get_sync 新建代理、完成一次
     * D-Bus 握手（libsecret 不替调用方缓存），开销不小；因此本函数在首次探测
     * 成功后做进程内缓存，失败则不缓存（服务恢复后下次调用重试）。
     * 无 D-Bus 会话总线或守护进程时快速失败。
     */
    static bool probeAvailability();

    QString backendId() const override;           // "system-keyring"
    bool isAvailable() const override;
    bool requiresMasterPassword() const override; // 恒 false
    bool hasMasterPassword() const override;      // 可用即视为已就绪
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
     * @brief 操作前检查：后端可用且已解锁。
     * @return 可进行凭据操作返回 true；否则设置错误信息并返回 false。
     */
    bool ensureReady() const;

    bool m_unlocked = false;        ///< 解锁标记（密钥环本身由 OS 托管）
    mutable QString m_errorString;  ///< 最近一次失败的错误信息
};
