#include "ZzForwardRule.h"

#include <QtCore/QCoreApplication>

namespace {

const QString kTypeKey = QStringLiteral("type");
const QString kListenHostKey = QStringLiteral("listenHost");
const QString kListenPortKey = QStringLiteral("listenPort");
const QString kTargetHostKey = QStringLiteral("targetHost");
const QString kTargetPortKey = QStringLiteral("targetPort");

/** @brief 类型 → 显示名（describe/validateList 用，用户可见，走 i18n）。 */
QString typeDisplayName(ZzForwardRule::Type type)
{
    switch (type) {
    case ZzForwardRule::Type::Local:
        return QCoreApplication::translate("ZzForwardRule", "本地");
    case ZzForwardRule::Type::Remote:
        return QCoreApplication::translate("ZzForwardRule", "远程");
    case ZzForwardRule::Type::Dynamic:
        return QCoreApplication::translate("ZzForwardRule", "动态");
    }
    return QCoreApplication::translate("ZzForwardRule", "本地");
}

} // namespace

QString zzForwardRuleTypeToString(ZzForwardRule::Type type)
{
    switch (type) {
    case ZzForwardRule::Type::Local:   return QStringLiteral("local");
    case ZzForwardRule::Type::Remote:  return QStringLiteral("remote");
    case ZzForwardRule::Type::Dynamic: return QStringLiteral("dynamic");
    }
    return QStringLiteral("local");
}

ZzForwardRule::Type zzForwardRuleTypeFromString(const QString &text)
{
    if (text == QLatin1String("remote"))
        return ZzForwardRule::Type::Remote;
    if (text == QLatin1String("dynamic"))
        return ZzForwardRule::Type::Dynamic;
    return ZzForwardRule::Type::Local;
}

QJsonObject ZzForwardRule::toJson() const
{
    QJsonObject obj;
    obj.insert(kTypeKey, zzForwardRuleTypeToString(type));
    obj.insert(kListenHostKey, listenHost);
    obj.insert(kListenPortKey, static_cast<int>(listenPort));
    obj.insert(kTargetHostKey, targetHost);
    obj.insert(kTargetPortKey, static_cast<int>(targetPort));
    return obj;
}

ZzForwardRule ZzForwardRule::fromJson(const QJsonObject &obj)
{
    ZzForwardRule rule;
    rule.type = zzForwardRuleTypeFromString(obj.value(kTypeKey).toString());
    rule.listenHost = obj.value(kListenHostKey).toString(rule.listenHost);
    rule.listenPort = static_cast<quint16>(obj.value(kListenPortKey).toInt(rule.listenPort));
    rule.targetHost = obj.value(kTargetHostKey).toString();
    rule.targetPort = static_cast<quint16>(obj.value(kTargetPortKey).toInt(rule.targetPort));
    return rule;
}

QString ZzForwardRule::validate() const
{
    if (listenHost.trimmed().isEmpty())
        return QCoreApplication::translate("ZzForwardRule", "监听地址不能为空");
    if (listenPort == 0)
        return QCoreApplication::translate("ZzForwardRule", "监听端口必须在 1-65535 之间");
    if (type != Type::Dynamic) {
        if (targetHost.trimmed().isEmpty())
            return QCoreApplication::translate("ZzForwardRule", "本地/远程转发必须填写目标地址");
        if (targetPort == 0)
            return QCoreApplication::translate("ZzForwardRule", "目标端口必须在 1-65535 之间");
    }
    return QString();
}

QString ZzForwardRule::validateList(const QVector<ZzForwardRule> &rules)
{
    for (qsizetype i = 0; i < rules.size(); ++i) {
        for (qsizetype j = i + 1; j < rules.size(); ++j) {
            if (rules[i].type == rules[j].type
                && rules[i].listenHost == rules[j].listenHost
                && rules[i].listenPort == rules[j].listenPort) {
                return QCoreApplication::translate(
                    "ZzForwardRule", "存在重复的转发规则：%1 %2:%3")
                    .arg(typeDisplayName(rules[i].type), rules[i].listenHost)
                    .arg(rules[i].listenPort);
            }
        }
    }
    return QString();
}

QString ZzForwardRule::describe() const
{
    const QString listen = QStringLiteral("%1:%2").arg(listenHost).arg(listenPort);
    if (type == Type::Dynamic)
        return QStringLiteral("%1 %2").arg(typeDisplayName(type), listen);
    const QString target = QStringLiteral("%1:%2").arg(targetHost).arg(targetPort);
    return QStringLiteral("%1 %2 → %3").arg(typeDisplayName(type), listen, target);
}
