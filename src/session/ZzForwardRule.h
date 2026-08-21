#pragma once

#include <QJsonObject>
#include <QString>
#include <QVector>

/**
 * @brief 端口转发规则（纯值类型，规格 §五）。
 *
 * 绑定在会话 profile 上：连接成功后自动启动，断线随会话销毁，重连自动重建。
 * 三种类型：Local（-L）、Remote（-R）、Dynamic（-D SOCKS5）。
 */
struct ZzForwardRule {
    /** @brief 转发类型。 */
    enum class Type {
        Local,   ///< 本地转发：监听本地端口 → 固定目标
        Remote,  ///< 远程转发：服务端监听 → 本地目标
        Dynamic  ///< 动态转发：本地 SOCKS5 入口，目标由握手解析
    };

    Type type = Type::Local;                            ///< 转发类型
    QString listenHost = QStringLiteral("127.0.0.1");   ///< 监听地址
    quint16 listenPort = 0;                             ///< 监听端口（1-65535）
    QString targetHost;                                 ///< 目标地址（Local/Remote 必填；Dynamic 忽略）
    quint16 targetPort = 0;                             ///< 目标端口（Local/Remote 必填；Dynamic 忽略）

    /**
     * @brief 序列化为 JSON 对象。
     * @return 包含全部字段的 JSON 对象（type 以字符串落盘）。
     */
    QJsonObject toJson() const;

    /**
     * @brief 从 JSON 对象反序列化。
     * @param obj 由 toJson() 产出的 JSON 对象；缺失或非法字段使用默认值。
     * @return 还原后的转发规则。
     */
    static ZzForwardRule fromJson(const QJsonObject &obj);

    /**
     * @brief 单条规则校验（规格 §五）。
     * @return 合法返回空串；否则返回中文错误描述。
     * @note 端口字段为 quint16，天然不超 65535；此处拒绝 0 与缺目标。
     */
    QString validate() const;

    /**
     * @brief 列表级校验：同 (type, listenHost, listenPort) 不允许重复（规格 §五）。
     * @param rules 待校验规则列表。
     * @return 无冲突返回空串；否则返回首个冲突的中文描述。
     */
    static QString validateList(const QVector<ZzForwardRule> &rules);

    /**
     * @brief 一行可读描述（状态栏/日志提示用）。
     * @return 如「本地 127.0.0.1:13306 → db.internal:3306」「动态 127.0.0.1:1080」。
     */
    QString describe() const;

    /** @brief 全字段相等比较。 */
    bool operator==(const ZzForwardRule &other) const = default;
};

/**
 * @brief 转发类型转 JSON 字符串（"local"/"remote"/"dynamic"）。
 */
QString zzForwardRuleTypeToString(ZzForwardRule::Type type);

/**
 * @brief JSON 字符串转转发类型；无法识别时回退 Local。
 */
ZzForwardRule::Type zzForwardRuleTypeFromString(const QString &text);
