#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "qtermwidget.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 端到端连接流程（mock 传输）：profile → 标签 → 连接 → 双向字节流（规格 §七/§九）。
 *
 * 注：简报样例按规划稿写成 profile.id=QString/profile.user；计划 03 实际交付的
 * ZzSessionProfile 契约（id 为 QUuid、用户名字段为 userName）为准，此处已适配，
 * 仅字段名/取值方式不同，断言与简报一致。
 */
class tst_ZzConnectFlow : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void doubleClickToByteStream()
    {
        ZzTabManager tabs;
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("流程机");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("10.0.0.1");
        profile.userName = QStringLiteral("deploy");

        // 等价于会话面板双击：面板发 connectRequested(profile) → openSession
        tabs.openSession(profile);

        auto *view = tabs.viewAt(0);
        QVERIFY(view != nullptr);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        // 映射正确：host/user 进入 endpoint
        QCOMPARE(mock->lastEndpoint.host, QStringLiteral("10.0.0.1"));
        QCOMPARE(mock->lastEndpoint.user, QStringLiteral("deploy"));

        // 双向字节流：键盘输入抵达传输，远端输出不崩溃
        emit view->termWidget()->sendData("pwd\n", 4);
        QCoreApplication::processEvents();
        QCOMPARE(mock->writtenData, QByteArray("pwd\n"));
        mock->simulateData("/home/deploy\r\n");
        QCoreApplication::processEvents();
    }

    void connectFailureKeepsTabWithError()
    {
        ZzTabManager tabs;
        QSignalSpy msgSpy(&tabs, &ZzTabManager::statusMessage);
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("连不上");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("fail"); // mock 约定：host==fail 触发失败

        tabs.openSession(profile);
        auto *view = tabs.viewAt(0);
        QVERIFY(view != nullptr);
        QTRY_COMPARE(view->transportState(),
                     ZzTransportInterface::State::Disconnected);
        // 连接失败保留标签（规格 §八），错误横幅由任务 13 断言
        QCOMPARE(tabs.count(), 1);
        QTRY_VERIFY(msgSpy.count() >= 1);
    }

    void masterPasswordUnlockLogic()
    {
        // 主密码解锁的纯逻辑部分（对话框本身依赖人工交互，不进自动化）
        const QString dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-cred-flow"));
        // 清掉上次运行残留，保证用例可重复执行且结果确定
        QDir(dir).removeRecursively();
        QDir().mkpath(dir);
        ZzCredentialStore store(dir + QStringLiteral("/credentials.dat"), this);
        QVERIFY(ZzMasterPasswordDialog::ensureStoreReady(&store,
                QStringLiteral("正确密码")));
        QVERIFY(store.isUnlocked());
        QVERIFY(!ZzMasterPasswordDialog::ensureStoreReady(&store, QString()));
    }
};

QTEST_MAIN(tst_ZzConnectFlow)
#include "tst_ZzConnectFlow.moc"
