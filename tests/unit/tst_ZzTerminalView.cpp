#include <QtTest/QtTest>

#include "qtermwidget.h"
#include "ZzMockTransport.h"
#include "settings/ZzAppSettings.h"
#include "terminal/ZzScrollbackBridge.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 验证终端视图胶水：双向字节流、尺寸转发、设置应用、编码查询。
 */
class tst_ZzTerminalView : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void bidirectionalByteStream()
    {
        ZzTerminalView view;
        auto *transport = new ZzMockTransport(&view);
        transport->echoEnabled = false; // 回显关掉，分辨两个方向
        view.setTransport(transport);

        ZzTransportEndpoint endpoint;
        transport->open(endpoint);
        QTRY_VERIFY(transport->state() == ZzTransportInterface::State::Connected);

        // 远端 → 终端：不应崩溃，视图保持存活即通过（像素内容属 ZzTermWidget 测试域）
        transport->simulateData("hello remote\r\n");
        QCoreApplication::processEvents();

        // 终端 → 远端：模拟键盘输入
        emit view.termWidget()->sendData("ls\n", 3);
        QCoreApplication::processEvents();
        QCOMPARE(transport->writtenData, QByteArray("ls\n"));
    }

    void sizeForwardsToTransport()
    {
        ZzTerminalView view;
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        QSignalSpy sizeSpy(&view, &ZzTerminalView::sizeChanged);

        // QTermWidget::termSizeChange(lines, columns) → transport->resize(cols, rows)
        emit view.termWidget()->termSizeChange(40, 100);
        QCOMPARE(transport->lastCols, 100);
        QCOMPARE(transport->lastRows, 40);
        QCOMPARE(sizeSpy.count(), 1);
        QCOMPARE(sizeSpy.first().at(0).toInt(), 100);
        QCOMPARE(sizeSpy.first().at(1).toInt(), 40);
    }

    void applyGlobalSettings()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-view-settings.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        settings.setFontSize(18);
        settings.setEncoding(QStringLiteral("GBK"));
        settings.setHistoryLines(5000);
        if (QTermWidget::availableColorSchemes().contains(QStringLiteral("QuardCRT"))) {
            settings.setColorScheme(QStringLiteral("QuardCRT"));
        }

        ZzTerminalView view;
        view.applySettings(settings);
        QCOMPARE(view.termWidget()->getTerminalFont().pointSize(), 18);
        QCOMPARE(view.termWidget()->historySize(), 5000);
        QCOMPARE(view.encoding(), QStringLiteral("GBK"));
        QFile::remove(path);
    }

    void statePassthrough()
    {
        ZzTerminalView view;
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        QSignalSpy stateSpy(&view, &ZzTerminalView::stateChanged);

        transport->open(ZzTransportEndpoint{});
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
        QCOMPARE(stateSpy.count(), 2); // Connecting + Connected

        transport->simulateDisconnect(QStringLiteral("对端关闭"));
        QCOMPARE(view.transportState(), ZzTransportInterface::State::Disconnected);
    }

    /**
     * @brief 回归：enableScrollback 时温层 open 失败（同步降级）必须提示用户（规格 §八）。
     *        曾因 open() 先于桥/提示链路接线而丢失该信号。
     *        冷层接线后目录只读会让冷层与温层 open 双双失败：先报冷层降级
     *        （degradedToWarmOnly），再报温层降级（degradedToMemoryOnly，
     *        最终状态为纯内存模式，末条提示为准）。
     */
    void scrollbackDegradationPromptsOnOpen()
    {
        QStandardPaths::setTestModeEnabled(true); // 隔离用户真实配置目录
        // 温层文件名带每标签唯一后缀，无法预知精确路径；改为把 scrollback
        // 目录置为只读，任何温层文件创建都会失败 → open() 同步降级
        const QString dirPath =
            QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
            + QStringLiteral("/scrollback");
        QVERIFY(QDir().mkpath(dirPath));
        QVERIFY(QFile::setPermissions(
            dirPath, QFileDevice::ReadOwner | QFileDevice::ExeOwner));

        ZzTerminalView view;
        QSignalSpy spy(&view, &ZzTerminalView::errorOccurred);
        view.enableScrollback(QStringLiteral("test-degrade-open"));
        QCOMPARE(spy.count(), 2); // 冷层降级 + 温层降级各一条
        QVERIFY(spy.at(0).at(0).toString().contains(QStringLiteral("温层模式")));
        QVERIFY(spy.at(1).at(0).toString().contains(QStringLiteral("内存模式")));

        // 恢复写权限，避免影响同进程后续用例
        QVERIFY(QFile::setPermissions(
            dirPath,
            QFileDevice::ReadOwner | QFileDevice::WriteOwner
                | QFileDevice::ExeOwner));
    }

    /**
     * @brief 回归：同 sessionId 双开标签时温层文件必须每标签唯一。
     *        曾共用 <sessionId>.warm，后开标签的 QFile::remove 会截断
     *        先开标签正在使用的温层。
     */
    void scrollbackWarmFilesAreUniquePerTab()
    {
        QStandardPaths::setTestModeEnabled(true);
        const QString sessionId = QStringLiteral("test-double-open");
        const QString dirPath =
            QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
            + QStringLiteral("/scrollback");
        // 清掉历史残留，保证计数断言确定
        for (const QString &file :
             QDir(dirPath).entryList({sessionId + QStringLiteral("-*.warm")},
                                     QDir::Files)) {
            QFile::remove(dirPath + QLatin1Char('/') + file);
        }

        ZzTerminalView view1;
        view1.enableScrollback(sessionId);
        ZzTerminalView view2; // 同 sessionId 同时存活（同 profile 双开）
        view2.enableScrollback(sessionId);

        const QStringList warmFiles =
            QDir(dirPath).entryList({sessionId + QStringLiteral("-*.warm")},
                                    QDir::Files);
        QCOMPARE(warmFiles.size(), 2);
    }

    /**
     * @brief 回归：跨重启重开同一会话时温层重置，读回不错位。
     *        显示层历史基线每会话从 0 起，引擎若恢复上一会话行号，provider
     *        读回会命中旧行；enableScrollback 必须截断温层重新对齐。
     */
    void scrollbackRestartsWithFreshLineNumbers()
    {
        QStandardPaths::setTestModeEnabled(true);
        const QString sessionId = QStringLiteral("test-restart-align");

        // 第一次会话：写入超过热层容量（默认 1 万）的行，强制归档进温层
        {
            ZzTerminalView view;
            view.enableScrollback(sessionId);
            for (int i = 0; i < 11000; ++i) {
                const QByteArray line =
                    QStringLiteral("old-line-%1\n").arg(i).toUtf8();
                emit view.termWidget()->dupDisplayOutput(line.constData(),
                                                         line.size());
            }
            QVERIFY(view.scrollbackBridge()->totalLines() > 0);
        } // 视图析构 → 引擎 flush，温层落盘

        // 第二次会话（等价重启后重开）：行号从 0 重新对齐，不恢复旧行
        ZzTerminalView view2;
        view2.enableScrollback(sessionId);
        ZzScrollbackBridge *bridge = view2.scrollbackBridge();
        QVERIFY(bridge);
        QCOMPARE(bridge->totalLines(), qint64(0));
        emit view2.termWidget()->dupDisplayOutput("fresh-line\n", 10);
        QCOMPARE(bridge->totalLines(), qint64(1));
        QCOMPARE(bridge->readOlderLines(1, 1),
                 QStringList({QStringLiteral("fresh-line")}));
    }
};

QTEST_MAIN(tst_ZzTerminalView)
#include "tst_ZzTerminalView.moc"
