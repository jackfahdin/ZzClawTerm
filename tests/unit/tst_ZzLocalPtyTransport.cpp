#include <QtTest/QtTest>

#include "transport/ZzLocalPtyTransport.h"

/**
 * @brief 验证本地 PTY 传输：起 shell、收发回显、resize、关闭与被动断开。
 */
class tst_ZzLocalPtyTransport : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    /** @brief 按平台取默认 shell（与生产代码同一套规则）。 */
    static QString platformShell()
    {
#if defined(Q_OS_WIN)
        return QStringLiteral("powershell.exe");
#else
        return QString::fromLocal8Bit(qgetenv("SHELL")).isEmpty()
            ? QStringLiteral("/bin/sh")
            : QString::fromLocal8Bit(qgetenv("SHELL"));
#endif
    }

    void openEchoClose()
    {
        ZzLocalPtyTransport transport;
        QSignalSpy stateSpy(&transport, &ZzTransportInterface::stateChanged);

        ZzTransportEndpoint endpoint;
        endpoint.localShell = true;
        endpoint.shellProgram = platformShell();
        endpoint.cols = 80;
        endpoint.rows = 24;
        transport.open(endpoint);

        QTRY_VERIFY_WITH_TIMEOUT(
            transport.state() == ZzTransportInterface::State::Connected, 5000);
        QVERIFY(stateSpy.count() >= 2); // Disconnected→Connecting→Connected

        // 写命令，应读到回显输出（PTY 默认回显 + 命令输出）
        QByteArray received;
        connect(&transport, &ZzTransportInterface::dataReceived,
                this, [&received](const QByteArray &data) { received += data; });
        transport.write("echo zz-pty-ok\n");
        QTRY_VERIFY_WITH_TIMEOUT(received.contains("zz-pty-ok"), 5000);

        transport.resize(100, 40);
        transport.close();
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }

    void shellExitEmitsDisconnected()
    {
        ZzLocalPtyTransport transport;
        QSignalSpy disconnectSpy(&transport, &ZzTransportInterface::disconnected);

        ZzTransportEndpoint endpoint;
        endpoint.localShell = true;
        endpoint.shellProgram = platformShell();
        transport.open(endpoint);
        QTRY_VERIFY_WITH_TIMEOUT(
            transport.state() == ZzTransportInterface::State::Connected, 5000);

        transport.write("exit\n");
        QTRY_VERIFY_WITH_TIMEOUT(disconnectSpy.count() >= 1, 5000);
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }
};

QTEST_MAIN(tst_ZzLocalPtyTransport)
#include "tst_ZzLocalPtyTransport.moc"
