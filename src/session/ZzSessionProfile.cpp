#include "ZzSessionProfile.h"

#include <QJsonArray>

namespace {

const QString kIdKey = QStringLiteral("id");
const QString kNameKey = QStringLiteral("name");
const QString kGroupPathKey = QStringLiteral("groupPath");
const QString kProtocolKey = QStringLiteral("protocol");
const QString kHostKey = QStringLiteral("host");
const QString kPortKey = QStringLiteral("port");
const QString kUserNameKey = QStringLiteral("userName");
const QString kAuthMethodKey = QStringLiteral("authMethod");
const QString kPrivateKeyPathKey = QStringLiteral("privateKeyPath");
const QString kCredentialIdKey = QStringLiteral("credentialId");
const QString kTerminalTypeKey = QStringLiteral("terminalType");
const QString kEncodingKey = QStringLiteral("encoding");
const QString kColorSchemeNameKey = QStringLiteral("colorSchemeName");
const QString kKeepAliveKey = QStringLiteral("keepAliveIntervalSeconds");
const QString kPortForwardsKey = QStringLiteral("portForwards");

/**
 * @brief 认证方式转 JSON 字符串。
 */
QString authMethodToString(ZzAuthMethod method)
{
    switch (method) {
    case ZzAuthMethod::Agent:      return QStringLiteral("agent");
    case ZzAuthMethod::PrivateKey: return QStringLiteral("privateKey");
    case ZzAuthMethod::Password:   return QStringLiteral("password");
    }
    return QStringLiteral("agent");
}

/**
 * @brief JSON 字符串转认证方式；无法识别时回退为 Agent。
 */
ZzAuthMethod authMethodFromString(const QString &text)
{
    if (text == QLatin1String("privateKey"))
        return ZzAuthMethod::PrivateKey;
    if (text == QLatin1String("password"))
        return ZzAuthMethod::Password;
    return ZzAuthMethod::Agent;
}

} // namespace

QJsonObject ZzSessionProfile::toJson() const
{
    QJsonObject obj;
    obj.insert(kIdKey, id.toString(QUuid::WithoutBraces));
    obj.insert(kNameKey, name);
    obj.insert(kGroupPathKey, groupPath);
    obj.insert(kProtocolKey, protocol);
    obj.insert(kHostKey, host);
    obj.insert(kPortKey, static_cast<int>(port));
    obj.insert(kUserNameKey, userName);
    obj.insert(kAuthMethodKey, authMethodToString(authMethod));
    obj.insert(kPrivateKeyPathKey, privateKeyPath);
    obj.insert(kCredentialIdKey, credentialId.toString(QUuid::WithoutBraces));
    obj.insert(kTerminalTypeKey, terminalType);
    obj.insert(kEncodingKey, encoding);
    obj.insert(kColorSchemeNameKey, colorSchemeName);
    obj.insert(kKeepAliveKey, keepAliveIntervalSeconds);
    QJsonArray forwards;
    for (const ZzForwardRule &rule : portForwards)
        forwards.append(rule.toJson());
    obj.insert(kPortForwardsKey, forwards);
    return obj;
}

ZzSessionProfile ZzSessionProfile::fromJson(const QJsonObject &obj)
{
    ZzSessionProfile profile;
    profile.id = QUuid::fromString(obj.value(kIdKey).toString()); // 非法串得 null QUuid
    profile.name = obj.value(kNameKey).toString();
    profile.groupPath = obj.value(kGroupPathKey).toString();
    profile.protocol = obj.value(kProtocolKey).toString(profile.protocol);
    profile.host = obj.value(kHostKey).toString();
    profile.port = static_cast<quint16>(obj.value(kPortKey).toInt(profile.port));
    profile.userName = obj.value(kUserNameKey).toString();
    profile.authMethod = authMethodFromString(obj.value(kAuthMethodKey).toString());
    profile.privateKeyPath = obj.value(kPrivateKeyPathKey).toString();
    profile.credentialId = QUuid::fromString(obj.value(kCredentialIdKey).toString());
    profile.terminalType = obj.value(kTerminalTypeKey).toString(profile.terminalType);
    profile.encoding = obj.value(kEncodingKey).toString(profile.encoding);
    profile.colorSchemeName = obj.value(kColorSchemeNameKey).toString();
    profile.keepAliveIntervalSeconds = obj.value(kKeepAliveKey).toInt(profile.keepAliveIntervalSeconds);
    const QJsonArray forwards = obj.value(kPortForwardsKey).toArray();
    for (const auto &v : forwards)
        profile.portForwards.append(ZzForwardRule::fromJson(v.toObject()));
    return profile;
}
