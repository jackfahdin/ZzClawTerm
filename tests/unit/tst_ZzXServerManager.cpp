#include <QtTest/QtTest>
#include <QDir>
#include <QFile>
#include <QTcpServer>
#include <QTemporaryDir>

#include "x11/ZzXServerManager.h"

/**
 * @brief ZzXServerManager 单元测试：display 分配、启动参数、崩溃/重启、停止释放。
 *
 * 桩 server 为 POSIX shell 脚本（记录参数到文件后 sleep / exit），因此进程类
 * 用例仅 Unix 有效，Windows 下跳过。
 */
class tst_ZzXServerManager : public QObject
{
    Q_OBJECT
private:
    QTemporaryDir m_dir;   ///< 每用例独立临时目录

    /** @brief 生成可执行桩脚本，返回其路径。 */
    QString makeStubServer(const QByteArray &body)
    {
        const QString path = m_dir.filePath(QStringLiteral("stub-xserver.sh"));
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly))
            return {};
        f.write(body);
        f.close();
        f.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                         QFileDevice::ExeOwner | QFileDevice::ReadUser |
                         QFileDevice::WriteUser | QFileDevice::ExeUser);
        return path;
    }

    /** @brief 睡眠桩：记录参数后常驻（供生命周期/参数用例）。 */
    QString makeSleepingStub(const QString &argsFile)
    {
        return makeStubServer("#!/bin/sh\n"
                              "printf '%s\\n' \"$@\" > '" + argsFile.toUtf8() + "'\n"
                              "sleep 60\n");
    }

private slots:
    /** @brief 6000 被占后应分配到 display 1（6001 端口）。 */
    void allocatesFreeDisplay()
    {
        QTcpServer blocker;
        QVERIFY(blocker.listen(QHostAddress::LocalHost, 6000));
        QCOMPARE(ZzXServerManager::allocateDisplay(), 1);
    }

    /** @brief 桩 server 收到 :N -multiwindow -clipboard -listen tcp -auth <path>。 */
    void startsWithExpectedArgs()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args.txt"));
        const QString xauth = m_dir.filePath(QStringLiteral("xauth-test"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());

        ZzXServerManager mgr;
        mgr.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&mgr, &ZzXServerManager::started);
        mgr.start(QStringLiteral("/nonexistent/vcxsrv.exe"), xauth, 7);
        // 用 QTRY 断言计数而非 spy.wait()：信号若在 stop()/start() 内同步发出，
        // Qt 6.11 的 QSignalSpy::wait() 对已记录的计数仍可能返回 false
        QTRY_VERIFY_WITH_TIMEOUT(startedSpy.count() >= 1, 5000);
        QCOMPARE(startedSpy.first().at(0).toInt(), 7);
        QVERIFY(mgr.isRunning());
        QCOMPARE(mgr.display(), 7);

        // 桩脚本落参数文件与 started 存在调度竞态，轮询等待
        QTRY_VERIFY_WITH_TIMEOUT(QFile::exists(argsFile), 5000);
        QFile f(argsFile);
        QVERIFY(f.open(QIODevice::ReadOnly));
        const QStringList args = QString::fromUtf8(f.readAll()).split(u'\n', Qt::SkipEmptyParts);
        // 精确锁定参数顺序与完整性
        QCOMPARE(args, QStringList({QStringLiteral(":7"), QStringLiteral("-multiwindow"),
                                    QStringLiteral("-clipboard"), QStringLiteral("-listen"),
                                    QStringLiteral("tcp"), QStringLiteral("-auth"), xauth}));

        // 桩走 TCP 语义端点
        const ZzXLocalEndpoint ep = mgr.localEndpoint();
        QCOMPARE(ep.host, QStringLiteral("127.0.0.1"));
        QCOMPARE(ep.port, quint16(6007));

        // stop 为异步收尾，等 stopped 再离场，避免桩进程泄漏
        QSignalSpy stoppedSpy(&mgr, &ZzXServerManager::stopped);
        mgr.stop();
        QTRY_VERIFY_WITH_TIMEOUT(stoppedSpy.count() >= 1, 5000);
#else
        QSKIP("桩脚本依赖 POSIX shell，仅 Unix 可测");
#endif
    }

    /** @brief 桩非零退出 → crashed（含退出码）；restart() 再次拉起。 */
    void reportsCrashAndAllowsRestart()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args.txt"));
        const QString flagFile = m_dir.filePath(QStringLiteral("crashed-once"));
        // 两阶段桩：首次 exit 1 模拟崩溃，其后常驻
        const QString stub = makeStubServer(
            "#!/bin/sh\n"
            "printf '%s\\n' \"$@\" >> '" + argsFile.toUtf8() + "'\n"
            "if [ ! -f '" + flagFile.toUtf8() + "' ]; then touch '" + flagFile.toUtf8() + "'; exit 1; fi\n"
            "sleep 60\n");
        QVERIFY(!stub.isEmpty());

        ZzXServerManager mgr;
        mgr.setServerProgramForTesting(stub);
        QSignalSpy crashedSpy(&mgr, &ZzXServerManager::crashed);
        mgr.start(QString(), m_dir.filePath(QStringLiteral("xauth-test")), 3);
        QTRY_VERIFY_WITH_TIMEOUT(crashedSpy.count() >= 1, 5000);
        QVERIFY(!mgr.isRunning());
        QCOMPARE(mgr.display(), -1);
        QVERIFY(crashedSpy.first().at(0).toString().contains(u'1')); // 消息含退出码

        QSignalSpy startedSpy(&mgr, &ZzXServerManager::started);
        mgr.restart();
        QTRY_VERIFY_WITH_TIMEOUT(startedSpy.count() >= 1, 5000);
        QCOMPARE(startedSpy.first().at(0).toInt(), 3);
        QVERIFY(mgr.isRunning());
        QCOMPARE(mgr.display(), 3);

        QSignalSpy stoppedSpy(&mgr, &ZzXServerManager::stopped);
        mgr.stop();
        QTRY_VERIFY_WITH_TIMEOUT(stoppedSpy.count() >= 1, 5000);
#else
        QSKIP("桩脚本依赖 POSIX shell，仅 Unix 可测");
#endif
    }

    /** @brief stop() 发 stopped（非 crashed）、进程退出、display 复位释放。 */
    void stopsCleanly()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args.txt"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());

        ZzXServerManager mgr;
        mgr.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&mgr, &ZzXServerManager::started);
        mgr.start(QString(), m_dir.filePath(QStringLiteral("xauth-test")), 5);
        QTRY_VERIFY_WITH_TIMEOUT(startedSpy.count() >= 1, 5000);
        QVERIFY(mgr.isRunning());

        QSignalSpy stoppedSpy(&mgr, &ZzXServerManager::stopped);
        QSignalSpy crashedSpy(&mgr, &ZzXServerManager::crashed);
        mgr.stop();
        QTRY_VERIFY_WITH_TIMEOUT(stoppedSpy.count() >= 1, 5000);
        QCOMPARE(crashedSpy.size(), 0); // 主动停止不得误报崩溃
        QVERIFY(!mgr.isRunning());
        QCOMPARE(mgr.display(), -1);

        // display 已释放：重新 start 同一 display 可再次拉起
        QSignalSpy restartedSpy(&mgr, &ZzXServerManager::started);
        mgr.start(QString(), m_dir.filePath(QStringLiteral("xauth-test")), 5);
        QTRY_VERIFY_WITH_TIMEOUT(restartedSpy.count() >= 1, 5000);
        QVERIFY(mgr.isRunning());
        QSignalSpy stoppedSpy2(&mgr, &ZzXServerManager::stopped);
        mgr.stop();
        QTRY_VERIFY_WITH_TIMEOUT(stoppedSpy2.count() >= 1, 5000);
#else
        QSKIP("桩脚本依赖 POSIX shell，仅 Unix 可测");
#endif
    }

    /** @brief stop() 不阻塞调用线程；桩忽略 SIGTERM 时 3s 后升级 kill 收尾。 */
    void stopIsAsyncAndEscalatesToKill()
    {
#ifdef Q_OS_UNIX
        // 忽略 SIGTERM 的桩：只有升级 SIGKILL 才能退出
        const QString stub = makeStubServer("#!/bin/sh\ntrap '' TERM\nsleep 60 &\nwait\n");
        QVERIFY(!stub.isEmpty());

        ZzXServerManager mgr;
        mgr.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&mgr, &ZzXServerManager::started);
        mgr.start(QString(), m_dir.filePath(QStringLiteral("xauth-test")), 9);
        QTRY_VERIFY_WITH_TIMEOUT(startedSpy.count() >= 1, 5000);
        QTest::qWait(300); // 等桩脚本装好 trap，避免 terminate 抢在 trap 前命中

        QSignalSpy stoppedSpy(&mgr, &ZzXServerManager::stopped);
        QElapsedTimer timer;
        timer.start();
        mgr.stop();
        QVERIFY(timer.elapsed() < 1000); // 立即返回，不等进程退出
        QVERIFY(!mgr.isRunning());
        QTRY_VERIFY_WITH_TIMEOUT(stoppedSpy.count() >= 1, 6000); // 3s 后 kill 收尾
        QCOMPARE(mgr.display(), -1);
#else
        QSKIP("桩脚本依赖 POSIX shell，仅 Unix 可测");
#endif
    }
};

QTEST_MAIN(tst_ZzXServerManager)
#include "tst_ZzXServerManager.moc"
