#include <QtTest/QtTest>
#include <QStandardPaths>

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
        // ConPTY 要求 shell 路径为绝对路径（ptyqt 显式校验），
        // 与 ZzLocalPtyTransport 生产默认一致；findExecutable 兜底解析。
        const QString found = QStandardPaths::findExecutable(QStringLiteral("powershell.exe"));
        return found.isEmpty()
            ? QStringLiteral("C:/Windows/system32/WindowsPowerShell/v1.0/powershell.exe")
            : found;
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
        // 失败自解释：把传输层错误（如 ptyqt ConPTY 启动失败原因）打进 QTest 日志
        connect(&transport, &ZzTransportInterface::errorOccurred, this,
                [](int code, const QString &msg) { qWarning() << "errorOccurred:" << code << msg; });

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
        connect(&transport, &ZzTransportInterface::errorOccurred, this,
                [](int code, const QString &msg) { qWarning() << "errorOccurred:" << code << msg; });

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
