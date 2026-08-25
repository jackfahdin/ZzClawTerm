#include <QtTest/QtTest>
#include <QFile>
#include <QTemporaryDir>
#include <QStandardPaths>

#include "x11/ZzX11Service.h"

/**
 * @brief ZzX11Service 单元测试：开关语义、幂等拉起、共享 cookie、停用停止。
 *
 * 桩 server 为 POSIX shell 脚本（记录参数后 sleep），进程类用例仅 Unix 有效。
 */
class tst_ZzX11Service : public QObject
{
    Q_OBJECT
private:
    QTemporaryDir m_dir;

    QString makeSleepingStub(const QString &argsFile)
    {
        const QString path = m_dir.filePath(QStringLiteral("stub-xserver.sh"));
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly))
            return {};
        f.write("#!/bin/sh\n"
                "printf '%s\\n' \"$@\" > '" + argsFile.toUtf8() + "'\n"
                "sleep 60\n");
        f.close();
        f.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                         QFileDevice::ExeOwner | QFileDevice::ReadUser |
                         QFileDevice::WriteUser | QFileDevice::ExeUser);
        return path;
    }

    /** @brief 无执行权限的桩：QProcess FailedToStart，模拟 exe 被查杀/占用的启动失败。 */
    QString makeFailingStub()
    {
        const QString path = m_dir.filePath(QStringLiteral("stub-failing.sh"));
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly))
            return {};
        f.write("#!/bin/sh\nexit 1\n");
        f.close();
        f.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                         QFileDevice::ReadUser | QFileDevice::WriteUser);
        return path;
    }

    /** @brief 成功启动后立即以非零码退出的桩：模拟运行态崩溃（started 先于 finished）。 */
    QString makeCrashingStub()
    {
        const QString path = m_dir.filePath(QStringLiteral("stub-crashing.sh"));
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly))
            return {};
        f.write("#!/bin/sh\nexit 1\n");
        f.close();
        f.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                         QFileDevice::ExeOwner | QFileDevice::ReadUser |
                         QFileDevice::WriteUser | QFileDevice::ExeUser);
        return path;
    }

private slots:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true); // xauth 落测试目录，不碰真实用户目录
    }

    /** @brief 全局开关关闭时 start/ensureRunning 为空操作（规格 §七错误处理）。 */
    void disabledStartIsNoop()
    {
        ZzX11Service service;
        QVERIFY(!service.isEnabled());
        service.start();
        service.ensureRunning();
        QVERIFY(!service.isRunning());
        QCOMPARE(service.display(), -1);
    }

    /** @brief 开启即拉起（幂等），multiwindow 参数组，cookie 共享一份。 */
    void enableStartsSharedServer()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args.txt"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&service, &ZzX11Service::serverStarted);

        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(service.isRunning(), 5000);
        QCOMPARE(startedSpy.count(), 1);
        QVERIFY(service.display() >= 0);
        QVERIFY(!service.cookie().isEmpty());

        // 幂等：运行中重复 start/ensureRunning 不再拉起、不重发信号
        service.start();
        service.ensureRunning();
        service.setEnabled(true); // 同值短路
        QTest::qWait(200);
        QCOMPARE(startedSpy.count(), 1);

        // 参数组：-multiwindow（非嵌入共享形态，规格 §4.2）
        QTRY_VERIFY(QFile::exists(argsFile));
        QFile f(argsFile);
        QVERIFY(f.open(QIODevice::ReadOnly));
        const QString args = QString::fromUtf8(f.readAll());
        QVERIFY(args.contains(QStringLiteral("-multiwindow")));
        QVERIFY(!args.contains(QStringLiteral("-parent")));
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }

    /** @brief 关闭开关停止共享 server（规格 §三决策 1 允许关闭）。 */
    void disableStopsServer()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args2.txt"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(service.isRunning(), 5000);

        service.setEnabled(false);
        QTRY_VERIFY_WITH_TIMEOUT(!service.isRunning(), 8000);
        QCOMPARE(service.display(), -1);
        QVERIFY(service.cookie().isEmpty());
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }

    /** @brief 快速 toggle（开→关→立即开）后 server 最终回到运行态（审查修复：stopped 处理器补拉）。 */
    void rapidToggleRelaunches()
    {
#ifdef Q_OS_UNIX
        const QString argsFile = m_dir.filePath(QStringLiteral("args3.txt"));
        const QString stub = makeSleepingStub(argsFile);
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(service.isRunning(), 5000);

        // 关后立即重开：stop() 异步收尾未完，重开的 start() 被 manager 收尾守卫
        // 静默拒绝；收尾到达 stopped 后由处理器按 m_enabled 补拉
        service.setEnabled(false);
        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(service.isRunning(), 10000);
        QVERIFY(service.display() >= 0);
        QVERIFY(!service.cookie().isEmpty());
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }

    /** @brief 启动中崩溃（FailedToStart）：转发 serverCrashed 同时补发 startFailed，
     * 使挂起等待的 transport 走"失败不阻断会话"收尾（M5 终审修复）。 */
    void crashDuringStartingEmitsStartFailed()
    {
#ifdef Q_OS_UNIX
        const QString stub = makeFailingStub();
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&service, &ZzX11Service::serverStarted);
        QSignalSpy failedSpy(&service, &ZzX11Service::startFailed);
        QSignalSpy crashedSpy(&service, &ZzX11Service::serverCrashed);

        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(crashedSpy.count() == 1, 5000);
        QCOMPARE(startedSpy.count(), 0);
        QCOMPARE(failedSpy.count(), 1);
        QVERIFY(!service.isRunning());
        QCOMPARE(service.display(), -1);
        QVERIFY(service.cookie().isEmpty());
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }

    /** @brief 运行态崩溃只发 serverCrashed、不发 startFailed（规格：不自动热恢复）。 */
    void crashWhileRunningOnlyEmitsServerCrashed()
    {
#ifdef Q_OS_UNIX
        const QString stub = makeCrashingStub();
        QVERIFY(!stub.isEmpty());
        ZzX11Service service;
        service.setServerProgramForTesting(stub);
        QSignalSpy startedSpy(&service, &ZzX11Service::serverStarted);
        QSignalSpy failedSpy(&service, &ZzX11Service::startFailed);
        QSignalSpy crashedSpy(&service, &ZzX11Service::serverCrashed);

        service.setEnabled(true);
        QTRY_VERIFY_WITH_TIMEOUT(startedSpy.count() == 1, 5000);
        QTRY_VERIFY_WITH_TIMEOUT(crashedSpy.count() == 1, 5000);
        QCOMPARE(failedSpy.count(), 0);
        QCOMPARE(service.display(), -1);
        QVERIFY(service.cookie().isEmpty());
#else
        QSKIP("桩 server 为 POSIX 脚本，仅 Unix 有效");
#endif
    }
};

QTEST_MAIN(tst_ZzX11Service)
#include "tst_ZzX11Service.moc"
