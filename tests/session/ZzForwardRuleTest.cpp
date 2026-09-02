#include <QtTest>

#include "ZzForwardRule.h"

/**
 * @brief ZzForwardRule 序列化与校验单元测试（规格 §五）。
 */
class ZzForwardRuleTest : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 三种类型全字段序列化后反序列化必须完全相等。 */
    void serializationRoundTrip()
    {
        const QVector<ZzForwardRule> rules = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
            {ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
             QStringLiteral("127.0.0.1"), 3000},
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
             QString(), 0},
        };
        for (const ZzForwardRule &rule : rules) {
            QCOMPARE(ZzForwardRule::fromJson(rule.toJson()), rule);
        }
    }

    /** @brief 空 JSON 反序列化取默认值（本地转发、127.0.0.1、端口 0）。 */
    void fromJsonUsesDefaults()
    {
        const ZzForwardRule rule = ZzForwardRule::fromJson(QJsonObject());
        QCOMPARE(rule.type, ZzForwardRule::Type::Local);
        QCOMPARE(rule.listenHost, QStringLiteral("127.0.0.1"));
        QCOMPARE(rule.listenPort, quint16(0));
        QVERIFY(rule.targetHost.isEmpty());
        QCOMPARE(rule.targetPort, quint16(0));
    }

    /** @brief 类型字符串往返；无法识别的字符串回退 Local。 */
    void typeStringRoundTrip()
    {
        QCOMPARE(zzForwardRuleTypeToString(ZzForwardRule::Type::Local), QStringLiteral("local"));
        QCOMPARE(zzForwardRuleTypeToString(ZzForwardRule::Type::Remote), QStringLiteral("remote"));
        QCOMPARE(zzForwardRuleTypeToString(ZzForwardRule::Type::Dynamic), QStringLiteral("dynamic"));
        QCOMPARE(zzForwardRuleTypeFromString(QStringLiteral("remote")), ZzForwardRule::Type::Remote);
        QCOMPARE(zzForwardRuleTypeFromString(QStringLiteral("垃圾")), ZzForwardRule::Type::Local);
    }

    /** @brief 三种类型的合法规则均通过校验（validate 返回空串）。 */
    void validateAcceptsValidRules()
    {
        QVERIFY((ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                               QStringLiteral("db.internal"), 3306}.validate().isEmpty()));
        QVERIFY((ZzForwardRule{ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
                               QStringLiteral("127.0.0.1"), 3000}.validate().isEmpty()));
        // Dynamic 无目标地址，属合法（规格 §五）
        QVERIFY((ZzForwardRule{ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
                               QString(), 0}.validate().isEmpty()));
    }

    /** @brief 非法规则逐类拒绝：端口 0、缺目标、空监听地址。 */
    void validateRejectsInvalidRules()
    {
        // 监听端口 0
        QVERIFY((!ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 0,
                                QStringLiteral("db.internal"), 3306}.validate().isEmpty()));
        // Local/Remote 缺目标地址
        QVERIFY((!ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                                QString(), 3306}.validate().isEmpty()));
        QVERIFY((!ZzForwardRule{ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
                                QString(), 3000}.validate().isEmpty()));
        // Local/Remote 目标端口 0
        QVERIFY((!ZzForwardRule{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                                QStringLiteral("db.internal"), 0}.validate().isEmpty()));
        // 空监听地址
        QVERIFY((!ZzForwardRule{ZzForwardRule::Type::Dynamic, QString(), 1080,
                                QString(), 0}.validate().isEmpty()));
    }

    /** @brief 列表级校验：同 (type, listenHost, listenPort) 不允许重复。 */
    void validateListDetectsDuplicate()
    {
        const ZzForwardRule a{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("db.internal"), 3306};
        const ZzForwardRule b{ZzForwardRule::Type::Remote, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("127.0.0.1"), 3000}; // 类型不同，不算重复
        const ZzForwardRule c{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                              QStringLiteral("other.host"), 22}; // 与 a 同三元组
        QVERIFY((ZzForwardRule::validateList({a, b}).isEmpty()));
        QVERIFY((!ZzForwardRule::validateList({a, c}).isEmpty()));
    }

    /** @brief 规则描述串供状态栏提示使用。 */
    void describeFormatsReadable()
    {
        const ZzForwardRule local{ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
                                  QStringLiteral("db.internal"), 3306};
        QCOMPARE(local.describe(),
                 QCoreApplication::translate("ZzForwardRule", "本地")
                     + QStringLiteral(" 127.0.0.1:13306 → db.internal:3306"));
        const ZzForwardRule dynamic{ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
                                    QString(), 0};
        QCOMPARE(dynamic.describe(),
                 QCoreApplication::translate("ZzForwardRule", "动态")
                     + QStringLiteral(" 127.0.0.1:1080"));
    }
};

QTEST_GUILESS_MAIN(ZzForwardRuleTest)

#include "ZzForwardRuleTest.moc"
