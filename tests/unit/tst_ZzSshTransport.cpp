#include <QtTest/QtTest>

#include "transport/ZzSshTransport.h"

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
};

QTEST_MAIN(tst_ZzSshTransport)
#include "tst_ZzSshTransport.moc"
