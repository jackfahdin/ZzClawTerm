#pragma once

#include <QJsonObject>
#include <QMetaType>
#include <QString>
#include <QUuid>

/**
 * @brief 会话认证方式。
 */
enum class ZzAuthMethod {
    Agent,      ///< 使用 SSH agent 认证
    PrivateKey, ///< 使用私钥文件认证
    Password    ///< 使用密码认证（密码密文存于 ZzCredentialStore，此处仅保存引用）
};

/**
 * @brief 会话配置档案（纯值类型）。
 *
 * 描述一个会话的全部静态配置。密码等敏感信息不保存在此，
 * 仅通过 credentialId 引用 ZzCredentialStore 中的条目。
 * 分组用路径字符串表示（如 "生产环境/Web 服务器"），空串表示未分组。
 * id / protocol / credentialId 字段与 Q_DECLARE_METATYPE 为计划 04 冻结契约。
 */
struct ZzSessionProfile {
    QUuid id;                       ///< 全局唯一标识（由 ZzSessionModel::addSession 生成；null 表示未分配）
    QString name;                   ///< 会话显示名称
    QString groupPath;              ///< 分组路径，空串表示未分组
    QString protocol = QStringLiteral("ssh"); ///< 协议类型："ssh"（SSH 远程会话）或 "local"（本地 shell）
    QString host;                   ///< 主机地址（IP 或域名；protocol == "local" 时忽略）
    quint16 port = 22;              ///< 端口号（1-65535）
    QString userName;               ///< 登录用户名
    ZzAuthMethod authMethod = ZzAuthMethod::Agent; ///< 认证方式
    QString privateKeyPath;         ///< 私钥文件路径（authMethod == PrivateKey 时有效）
    QUuid credentialId;             ///< 密码引用（authMethod == Password 时有效；null 表示无）
    QString terminalType;           ///< 终端类型（TERM），空串表示跟随全局设置（ZzAppSettings::terminalType）
    QString encoding = QStringLiteral("UTF-8");              ///< 字符编码
    QString colorSchemeName;        ///< 配色方案名，空串表示使用全局默认
    int keepAliveIntervalSeconds = 0; ///< keepalive 间隔（秒），0 表示禁用

    /**
     * @brief 序列化为 JSON 对象。
     * @return 包含全部字段的 JSON 对象（QUuid 以 WithoutBraces 字符串落盘）。
     */
    QJsonObject toJson() const;

    /**
     * @brief 从 JSON 对象反序列化。
     * @param obj 由 toJson() 产出的 JSON 对象；缺失或非法字段使用默认值。
     * @return 还原后的会话配置档案。
     */
    static ZzSessionProfile fromJson(const QJsonObject &obj);

    /** @brief 全字段相等比较。 */
    bool operator==(const ZzSessionProfile &other) const = default;
};

Q_DECLARE_METATYPE(ZzSessionProfile)
