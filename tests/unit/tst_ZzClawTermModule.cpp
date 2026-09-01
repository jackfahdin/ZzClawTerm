#include <chrono>

#include <QtTest/QtTest>

#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QFile>

#include <ZzCore/ZzApplicationPaths.h>

#include <ZzLog/ZzLog.h>

#include "ZzClawTermModule.h"

/**
 * @brief 应用模块日志接入：start() 初始化 ZzLog 文件 sink 并桥接 Qt 消息，
 *        stop() 卸载桥并关闭运行时（幂等）。
 */
class tst_ZzClawTermModule : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase()
    {
        // 隔离写盘 + 固定应用/组织名（模块按 QCoreApplication 名称生成日志目录）
        QStandardPaths::setTestModeEnabled(true);
        QCoreApplication::setOrganizationName(QStringLiteral("ZzClaw"));
        QCoreApplication::setApplicationName(QStringLiteral("ZzClawTerm"));
    }

    void descriptorIsStable()
    {
        ZzClawTermModule module;
        QCOMPARE(module.descriptor().id.value(),
                 QStringLiteral("com.zzclawterm.app"));
        QCOMPARE(module.descriptor().version, QStringLiteral("0.1.0"));
        QVERIFY(module.descriptor().dependencies.isEmpty());
    }

    void startBridgesQtMessagesToFileAndStopCloses()
    {
        ZzClawTermModule module;
        QVERIFY(module.start());
        QVERIFY(ZzLog::isInitialized());

        // Qt 消息经桥落入文件 sink（Windows 无控制台场景的诊断通路）
        qWarning("zzclawterm-module-probe-message");
        QVERIFY(ZzLog::flushAndWait(std::chrono::seconds(2)));

        const ZzCore::ZzApplicationPaths paths(
            QStringLiteral("ZzClaw"), QStringLiteral("ZzClawTerm"));
        const QString logPath = QDir(paths.logDirectory())
            .filePath(QStringLiteral("ZzClawTerm.log"));
        QVERIFY2(QFile::exists(logPath), qPrintable(logPath));
        QFile f(logPath);
        QVERIFY(f.open(QIODevice::ReadOnly | QIODevice::Text));
        const QByteArray content = f.readAll();
        QVERIFY(content.contains("zzclawterm-module-probe-message"));

        module.stop();
        QVERIFY(!ZzLog::isInitialized());
        // stop 幂等：重复调用安全
        module.stop();
    }

    void doubleStartIsRejected()
    {
        ZzClawTermModule module;
        QVERIFY(module.start());
        QVERIFY(!module.start());
        module.stop();
    }
};

QTEST_MAIN(tst_ZzClawTermModule)
#include "tst_ZzClawTermModule.moc"
