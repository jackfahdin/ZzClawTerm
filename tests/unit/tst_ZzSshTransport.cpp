#include <QtTest/QtTest>

#include "transport/ZzSshTransport.h"
#include "x11/ZzX11Service.h"

/**
 * @brief 验证 SSH 适配器的错误透传与状态机（成功路径由计划 01 的 Docker 集成测试覆盖）。
 */
class tst_ZzSshTransport : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void connectionRefusedEmitsError()
    {
        ZzSshTransport transport;
        QSignalSpy errorSpy(&transport, &ZzTransportInterface::errorOccurred);

        ZzTransportEndpoint endpoint;
        endpoint.host = QStringLiteral("127.0.0.1");
        endpoint.port = 1; // 基本必然无人监听的端口
        endpoint.user = QStringLiteral("nobody");
        transport.open(endpoint);

        QCOMPARE(transport.state(), ZzTransportInterface::State::Connecting);
        QTRY_VERIFY_WITH_TIMEOUT(errorSpy.count() >= 1, 10000);
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }

    void writeBeforeConnectedIsSafe()
    {
        // 未连接时 write/resize/close 不得崩溃
        ZzSshTransport transport;
        transport.write("x");
        transport.resize(80, 24);
        transport.close();
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }

    void x11ServiceInjectionRoundtrip()
    {
        // M5：共享门面经 ZzTabManager 注入，适配器只观察不拥有
        ZzSshTransport transport;
        QVERIFY(transport.x11Service() == nullptr);
        ZzX11Service service;
        transport.setX11Service(&service);
        QCOMPARE(transport.x11Service(), &service);
    }

    void closeWithX11AndNoServiceIsSafe()
    {
        // 实际覆盖：x11Forwarding=true 时连接失败（端口必拒，onConnected 不触发，
        // 因此不会走到"未启用跳过"分支）+ close 不崩溃、状态正常回收
        ZzSshTransport transport;
        ZzTransportEndpoint endpoint;
        endpoint.host = QStringLiteral("127.0.0.1");
        endpoint.port = 1;
        endpoint.user = QStringLiteral("nobody");
        endpoint.x11Forwarding = true;
        transport.open(endpoint);
        transport.close();
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }
};

QTEST_MAIN(tst_ZzSshTransport)
#include "tst_ZzSshTransport.moc"
