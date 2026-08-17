#include <QtTest/QtTest>

#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>

#include "ZzMockTransport.h"
#include "terminal/ZzTerminalView.h"

/**
 * @brief 验证标签内错误横幅：出错显示、重试重连、连通自动隐藏（规格 §八）。
 */
class tst_ZzErrorUi : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void errorShowsBannerWithRetry()
    {
        ZzTerminalView view;
        view.resize(640, 480);
        view.show(); // 横幅可见性断言需要父子均 show
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        view.openEndpoint(ZzTransportEndpoint{});
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
        QVERIFY(!view.errorBanner()->isVisible());

        transport->simulateError(1001, QStringLiteral("认证失败"));
        QVERIFY(view.errorBanner()->isVisible());
        QVERIFY(view.errorLabel()->text().contains(QStringLiteral("认证失败")));

        // 点重试 → 用记忆的参数重新 open
        QTest::mouseClick(view.retryButton(), Qt::LeftButton);
        QCOMPARE(transport->openCallCount, 2);
        QVERIFY(!view.errorBanner()->isVisible()); // 点击即收起
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
    }

    void bannerHidesOnReconnect()
    {
        ZzTerminalView view;
        view.resize(640, 480);
        view.show();
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        transport->simulateError(1002, QStringLiteral("网络不可达"));
        QVERIFY(view.errorBanner()->isVisible());

        view.openEndpoint(ZzTransportEndpoint{});
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
        QVERIFY(!view.errorBanner()->isVisible());
    }
};

QTEST_MAIN(tst_ZzErrorUi)
#include "tst_ZzErrorUi.moc"
