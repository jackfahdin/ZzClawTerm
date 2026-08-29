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

    /** @brief 行结束符：真实终端 Enter 产生 \r；ConPTY 上 PowerShell PSReadLine
     *  只认 \r 提交行，\n 仅是行内字符（命令不会执行）。Unix PTY 用 \n。
     */
    static QByteArray lineEnding()
    {
#if defined(Q_OS_WIN)
        return QByteArrayLiteral("\r");
#else
        return QByteArrayLiteral("\n");
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

        // 写命令。注意断言要区分 PTY 输入回显与命令真实输出：回显只出现 1 次
        // 标记，命令执行后输出再出现 1 次；若只用 contains 判定，回显即可
        // 让用例假绿（九跑 Windows 上 openEchoClose 即因此误判通过）。
        QByteArray received;
        connect(&transport, &ZzTransportInterface::dataReceived,
                this, [&received](const QByteArray &data) { received += data; });
        transport.write(QByteArrayLiteral("echo zz-pty-ok") + lineEnding());
        QTRY_VERIFY_WITH_TIMEOUT(received.count("zz-pty-ok") >= 2, 5000);

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

        transport.write(QByteArrayLiteral("exit") + lineEnding());
        QTRY_VERIFY_WITH_TIMEOUT(disconnectSpy.count() >= 1, 5000);
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }
};

QTEST_MAIN(tst_ZzLocalPtyTransport)
#include "tst_ZzLocalPtyTransport.moc"
