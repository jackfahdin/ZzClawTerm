#include <QtTest/QtTest>

#include "qtermwidget.h"
#include "log/ZzLogEngine.h"
#include "terminal/ZzScrollbackBridge.h"

/**
 * @brief 验证滚动历史桥：输出逐行进引擎、读回透传、降级信号透传（规格 §5.4/§八）。
 *
 * 读回口径与 QTermWidget::setHistoryProvider 契约一致：返回绝对行号
 * [beforeLine - maxLines, beforeLine) 窗口内的行文本，旧→新顺序（经引擎
 * getLines 夹取到当前可读窗口）。
 */
class tst_ZzScrollbackBridge : public QObject
{
    Q_OBJECT
private slots:
    void appendsLinesToEngine()
    {
        QTermWidget term;
        ZzLogEngine engine(ZzLogEngine::Config{}); // 温层路径为空 → 纯内存模式
        QVERIFY(engine.open());
        ZzScrollbackBridge bridge(&term, &engine);

        // dupDisplayOutput 按行吐 UTF-8 文本
        emit term.dupDisplayOutput("total 0\n", 8);
        emit term.dupDisplayOutput("drwxr-xr-x  2 root root 4096\n", 28);
        QCoreApplication::processEvents();

        QCOMPARE(engine.totalLines(), 2ULL);
        QCOMPARE(bridge.readOlderLines(2, 10),
                 QStringList({QStringLiteral("total 0"),
                              QStringLiteral("drwxr-xr-x  2 root root 4096")}));
    }

    void readBackDelegates()
    {
        QTermWidget term;
        ZzLogEngine engine(ZzLogEngine::Config{});
        QVERIFY(engine.open());
        engine.appendLine({QStringLiteral("line-1"), QByteArray()});
        engine.appendLine({QStringLiteral("line-2"), QByteArray()});
        engine.appendLine({QStringLiteral("line-3"), QByteArray()});
        ZzScrollbackBridge bridge(&term, &engine);
        // [beforeLine - maxLines, beforeLine) = 绝对行号 {1, 2}
        QCOMPARE(bridge.readOlderLines(3, 2),
                 QStringList({QStringLiteral("line-2"),
                              QStringLiteral("line-3")}));
    }

    void degradationForwards()
    {
        QTermWidget term;
        ZzLogEngine engine(ZzLogEngine::Config{});
        QVERIFY(engine.open());
        ZzScrollbackBridge bridge(&term, &engine);
        QSignalSpy spy(&bridge, &ZzScrollbackBridge::degraded);
        bridge.simulateDegradationForTest(QStringLiteral("磁盘空间不足"));
        QCOMPARE(spy.count(), 1);
        QVERIFY(spy.first().at(0).toString().contains(QStringLiteral("磁盘")));
    }
};

QTEST_MAIN(tst_ZzScrollbackBridge)
#include "tst_ZzScrollbackBridge.moc"
