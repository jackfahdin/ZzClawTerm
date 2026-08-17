#include <QtTest/QtTest>

#include "qtermwidget.h"
#include "ZzMockTransport.h"
#include "settings/ZzAppSettings.h"
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
};

QTEST_MAIN(tst_ZzTerminalView)
#include "tst_ZzTerminalView.moc"
