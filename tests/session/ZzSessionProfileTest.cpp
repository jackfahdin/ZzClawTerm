#include <QtTest>
#include <QVariant>

#include "ZzForwardRule.h"
#include "ZzSessionProfile.h"

/**
 * @brief ZzSessionProfile 序列化单元测试。
 */
class ZzSessionProfileTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 全字段非默认值，序列化后反序列化必须完全相等。 */
    void serializationRoundTrip()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("生产 Web-01");
        profile.groupPath = QStringLiteral("生产环境/Web 服务器");
        profile.protocol = QStringLiteral("local");
        profile.host = QStringLiteral("10.0.0.1");
        profile.port = 2222;
        profile.userName = QStringLiteral("deploy");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/zz/.ssh/id_ed25519");
        profile.credentialId = QUuid::createUuid();
        profile.terminalType = QStringLiteral("xterm");
        profile.encoding = QStringLiteral("GBK");
        profile.colorSchemeName = QStringLiteral("Solarized Dark");
        profile.keepAliveIntervalSeconds = 30;

        const ZzSessionProfile restored = ZzSessionProfile::fromJson(profile.toJson());
        QVERIFY(restored == profile);
    }

    /** @brief 空 JSON 对象反序列化时所有字段取默认值（含计划 04 冻结契约字段）。 */
    void fromJsonUsesDefaults()
    {
        const ZzSessionProfile profile = ZzSessionProfile::fromJson(QJsonObject());
        QVERIFY(profile.id.isNull());
        QVERIFY(profile.name.isEmpty());
        QVERIFY(profile.groupPath.isEmpty());
        QCOMPARE(profile.protocol, QStringLiteral("ssh"));
        QVERIFY(profile.host.isEmpty());
        QCOMPARE(profile.port, quint16(22));
        QVERIFY(profile.userName.isEmpty());
        QCOMPARE(profile.authMethod, ZzAuthMethod::Agent);
        QVERIFY(profile.privateKeyPath.isEmpty());
        QVERIFY(profile.credentialId.isNull());
        QVERIFY(profile.terminalType.isEmpty()); // 空串 = 跟随全局设置
        QCOMPARE(profile.encoding, QStringLiteral("UTF-8"));
        QVERIFY(profile.colorSchemeName.isEmpty());
        QCOMPARE(profile.keepAliveIntervalSeconds, 0);
    }

    /** @brief 三种认证方式序列化为字符串后均可无损还原。 */
    void authMethodStringRoundTrip()
    {
        for (ZzAuthMethod method : {ZzAuthMethod::Agent, ZzAuthMethod::PrivateKey, ZzAuthMethod::Password}) {
            ZzSessionProfile profile;
            profile.authMethod = method;
            const ZzSessionProfile restored = ZzSessionProfile::fromJson(profile.toJson());
            QCOMPARE(restored.authMethod, method);
        }
    }

    /** @brief 计划 04 冻结契约：Q_DECLARE_METATYPE 生效，可装入 QVariant 往返。 */
    void metatypeUsableInVariant()
    {
        QVERIFY(qMetaTypeId<ZzSessionProfile>() != QMetaType::UnknownType);

        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("契约检查");
        const QVariant variant = QVariant::fromValue(profile);
        QVERIFY(variant.canConvert<ZzSessionProfile>());
        QVERIFY(variant.value<ZzSessionProfile>() == profile);
    }

    /** @brief portForwards 字段序列化往返（规格 §五配置格式）。 */
    void portForwardsRoundTrip()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("带隧道");
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
             QString(), 0},
        };
        const ZzSessionProfile restored = ZzSessionProfile::fromJson(profile.toJson());
        QVERIFY(restored == profile); // operator== 全字段含 portForwards
    }

    /** @brief 旧版 sessions.json 无 portForwards 字段：默认空列表（version 仍为 1 兼容）。 */
    void portForwardsDefaultsToEmpty()
    {
        const ZzSessionProfile profile = ZzSessionProfile::fromJson(QJsonObject());
        QVERIFY(profile.portForwards.isEmpty());
    }
};

QTEST_GUILESS_MAIN(ZzSessionProfileTest)

#include "ZzSessionProfileTest.moc"
