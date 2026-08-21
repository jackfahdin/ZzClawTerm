#include <QtTest>

#include <ZzSshConnection.h>

#include "transport/ZzSshTunnelHandle.h"

/**
 * @brief ZzSshTunnelHandle / ZzSshTunnelFactory 单元测试。
 *
 * 注：createTunnel 任意连接状态可创建（本地 QTcpServer 先行监听）；
 * createForwardListener 仅 Connected 可用，未连接返回 nullptr（工厂记为创建失败）。
 */
class tst_ZzSshTunnelHandle : public QObject
{
    Q_OBJECT

private slots:
    /** @brief 工厂按类型路由：Local/Dynamic 出隧道句柄，Remote 未连接返回 nullptr。 */
    void factoryRoutesByRuleType()
    {
        ZzSshConnection conn; // 未连接：隧道可建，监听器不可建
        ZzSshTunnelFactory factory(&conn);

        ZzTunnelHandle *local = factory.createHandle(
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 0,
             QStringLiteral("db.internal"), 3306}, this);
        QVERIFY(local != nullptr);

        ZzTunnelHandle *dynamic = factory.createHandle(
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 0,
             QString(), 0}, this);
        QVERIFY(dynamic != nullptr);

        ZzTunnelHandle *remote = factory.createHandle(
            {ZzForwardRule::Type::Remote, QStringLiteral("0.0.0.0"), 8080,
             QStringLiteral("127.0.0.1"), 3000}, this);
        QVERIFY(remote == nullptr); // 未连接时监听器不可创建
    }

    /** @brief 句柄 start/stop 委托到隧道：本地隧道真实监听（端口 0=系统分配）。 */
    void handleDelegatesStartStop()
    {
        ZzSshConnection conn;
        ZzSshTunnelFactory factory(&conn);
        ZzTunnelHandle *handle = factory.createHandle(
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 0,
             QStringLiteral("127.0.0.1"), 22}, this);
        QVERIFY(handle != nullptr);

        QSignalSpy listenSpy(handle, &ZzTunnelHandle::listening);
        handle->start();
        // QTcpServer 本地监听为同步路径，信号可能已发射
        QVERIFY(listenSpy.count() == 1 || listenSpy.wait(3000));
        QVERIFY(listenSpy.first().at(0).toUInt() > 0); // 实际绑定端口

        handle->stop(); // 幂等不崩溃
        handle->stop();
        QCOMPARE(handle->activeConnectionCount(), 0);
    }

    /** @brief 句柄销毁级联销毁被包装隧道（setParent 所有权语义）。 */
    void handleOwnsTunnel()
    {
        auto *conn = new ZzSshConnection;
        ZzSshTunnelFactory factory(conn);
        ZzTunnelHandle *handle = factory.createHandle(
            {ZzForwardRule::Type::Dynamic, QStringLiteral("127.0.0.1"), 0,
             QString(), 0}, nullptr);
        QVERIFY(handle != nullptr);
        QVERIFY(handle->parent() == nullptr);
        delete handle; // 隧道随之销毁（隧道已是句柄子对象）；不崩溃即通过
        delete conn;
    }
};

QTEST_MAIN(tst_ZzSshTunnelHandle)
#include "tst_ZzSshTunnelHandle.moc"
