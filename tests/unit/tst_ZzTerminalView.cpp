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
     */
    void scrollbackDegradationPromptsOnOpen()
    {
        QStandardPaths::setTestModeEnabled(true); // 隔离用户真实配置目录
        const QString sessionId = QStringLiteral("test-degrade-open");
        const QString warmPath =
            QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
            + QStringLiteral("/scrollback/") + sessionId + QStringLiteral(".warm");
        // 预置同名目录占位 → 温层按文件打开必然失败 → open() 同步降级
        QVERIFY(QDir().mkpath(warmPath));

        ZzTerminalView view;
        QSignalSpy spy(&view, &ZzTerminalView::errorOccurred);
        view.enableScrollback(sessionId);
        QCOMPARE(spy.count(), 1);
        QVERIFY(spy.first().at(0).toString().contains(QStringLiteral("降级")));

        QVERIFY(QDir(warmPath).removeRecursively()); // 清理占位目录
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
