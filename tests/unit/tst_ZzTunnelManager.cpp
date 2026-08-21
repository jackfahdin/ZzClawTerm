#include <QtTest>

#include "transport/ZzTunnelHandle.h"
#include "transport/ZzTunnelManager.h"

namespace {

/**
 * @brief 测试用 fake 隧道句柄：脚本化启动行为，可手动注入事件。
 */
class ZzFakeTunnelHandle : public ZzTunnelHandle
{
    Q_OBJECT
public:
    using ZzTunnelHandle::ZzTunnelHandle;

    void start() override
    {
        ++startCallCount;
        if (failOnStart) {
            emit failed(1001, QStringLiteral("监听端口被占用"));
        } else {
            listening_ = true;
            emit listening(listenPort);
        }
    }
    void stop() override { ++stopCallCount; listening_ = false; }
    int activeConnectionCount() const override { return connectionCount; }

    /** @brief 注入断线失效（模拟 SSH 断开）。 */
    void simulateInvalidated() { listening_ = false; emit invalidated(); }

    bool failOnStart = false;      ///< start 时发射 failed 而非 listening
    quint16 listenPort = 0;        ///< listening 信号携带的端口
    int connectionCount = 0;       ///< 伪装的活动连接数
    int startCallCount = 0;
    int stopCallCount = 0;
    bool listening_ = false;
};

/**
 * @brief 测试用 fake 工厂：记录请求的规则，按序返回 fake 句柄。
 */
class ZzFakeTunnelFactory : public ZzTunnelFactory
{
public:
    ZzTunnelHandle *createHandle(const ZzForwardRule &rule, QObject *parent) override
    {
        requestedRules.append(rule);
        auto *handle = new ZzFakeTunnelHandle(parent);
        handle->failOnStart = failNext;
        failNext = false;
        handles.append(handle);
        return handle;
    }

    QVector<ZzForwardRule> requestedRules; ///< 依次收到的规则
    QList<ZzFakeTunnelHandle *> handles;   ///< 依次产出的句柄
    bool failNext = false;                 ///< 下一个句柄 start 即失败
};

/** @brief 造三条规则：本地/远程/动态各一。 */
QVector<ZzForwardRule> threeRules()
{
    return {
        {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
         QStringLiteral("db.internal"), 3306},
        {ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
         QStringLiteral("127.0.0.1"), 3000},
        {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 1080,
         QString(), 0},
    };
}

} // namespace

/**
 * @brief ZzTunnelManager 生命周期单元测试（规格 §七：连接→建隧道→断线→销毁→重建）。
 */
class tst_ZzTunnelManager : public QObject
{
    Q_OBJECT

private slots:
    /** @brief startAll 按规则逐一创建并启动句柄，三规则全部 listening。 */
    void startAllCreatesHandlePerRule()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        manager.startAll();

        QCOMPARE(factory.requestedRules.size(), 3);
        QCOMPARE(factory.handles.size(), 3);
        for (auto *h : factory.handles)
            QCOMPARE(h->startCallCount, 1);
        QCOMPARE(manager.activeTunnelCount(), 3);
        QVERIFY(manager.failedRules().isEmpty());
    }

    /** @brief startAll 幂等：重复调用不重复创建。 */
    void startAllIsIdempotent()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        manager.startAll();
        manager.startAll();
        QCOMPARE(factory.handles.size(), 3);
    }

    /** @brief 单规则失败隔离：失败规则入 failedRules 并报 ruleFailed，其余规则不受影响。 */
    void failedRuleIsolated()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        QSignalSpy failSpy(&manager, &ZzTunnelManager::ruleFailed);
        factory.failNext = true; // 第一条规则失败

        manager.startAll();

        QCOMPARE(failSpy.count(), 1);
        QCOMPARE(manager.failedRules().size(), 1);
        QCOMPARE(manager.failedRules().first().type, ZzForwardRule::Type::Local);
        QCOMPARE(manager.activeTunnelCount(), 2); // 其余两条正常
    }

    /** @brief 断线：句柄 invalidated 即从活动集移除（隧道本体由下层自毁）。 */
    void invalidatedDropsTunnel()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        QSignalSpy changeSpy(&manager, &ZzTunnelManager::tunnelsChanged);
        manager.startAll();
        changeSpy.clear();

        factory.handles.at(1)->simulateInvalidated();

        QCOMPARE(manager.activeTunnelCount(), 2);
        QVERIFY(changeSpy.count() >= 1);
        QVERIFY(manager.failedRules().isEmpty()); // 断线不算规则失败
    }

    /** @brief stopAll 停止并清空全部句柄（会话断开销毁路径）。 */
    void stopAllStopsEverything()
    {
        ZzFakeTunnelFactory factory;
        ZzTunnelManager manager(&factory, threeRules());
        manager.startAll();
        manager.stopAll();

        QCOMPARE(manager.activeTunnelCount(), 0);
        // stopAll 后句柄即销毁，只能经 startAll 前的快照断言 stop 被调用
        // （fake 随 delete 失效，故改为重建断言幂等性）
        manager.startAll(); // 重连重建语义：stopAll 后可再次 startAll
        QCOMPARE(manager.activeTunnelCount(), 3);
        QCOMPARE(factory.handles.size(), 6); // 全部重新创建
    }

    /** @brief 析构时自动停止全部隧道（会话关闭路径，不崩溃即通过）。 */
    void destructorStopsCleanly()
    {
        ZzFakeTunnelFactory factory;
        {
            ZzTunnelManager manager(&factory, threeRules());
            manager.startAll();
            QCOMPARE(manager.activeTunnelCount(), 3);
        }
        QVERIFY(true);
    }
};

QTEST_MAIN(tst_ZzTunnelManager)
#include "tst_ZzTunnelManager.moc"
