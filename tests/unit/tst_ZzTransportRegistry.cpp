#include <QtTest/QtTest>

#include "transport/ZzTransportInterface.h"
#include "transport/ZzTransportRegistry.h"
#include "ZzMockTransport.h"

/**
 * @brief 验证传输工厂注册表：注册/创建/重复注册拒绝/未知协议返回空（规格 §2.3）。
 */
class tst_ZzTransportRegistry : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        // 每个用例前清空注册表，避免用例间互相污染
        ZzTransportRegistry::instance().clear();
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void registerAndCreate()
    {
        auto &registry = ZzTransportRegistry::instance();
        QVERIFY(registry.registerTransport(QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
        QVERIFY(registry.schemes().contains(QStringLiteral("mock")));

        std::unique_ptr<ZzTransportInterface> transport(
            registry.create(QStringLiteral("mock")));
        QVERIFY(transport != nullptr);
        QCOMPARE(transport->state(), ZzTransportInterface::State::Disconnected);
    }

    void duplicateRegisterRejected()
    {
        auto &registry = ZzTransportRegistry::instance();
        QVERIFY(registry.registerTransport(QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
        QVERIFY(!registry.registerTransport(QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
        QVERIFY(!registry.registerTransport(QString(),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
    }

    void unknownSchemeReturnsNull()
    {
        std::unique_ptr<ZzTransportInterface> transport(
            ZzTransportRegistry::instance().create(QStringLiteral("no-such-scheme")));
        QVERIFY(transport == nullptr);
    }
};

QTEST_MAIN(tst_ZzTransportRegistry)
#include "tst_ZzTransportRegistry.moc"
