#include <QtTest>
#include "x11/ZzX11Viewport.h"

class tst_ZzX11Viewport : public QObject
{
    Q_OBJECT
private slots:
    void followRectMatchesContainer()
    {
        // 容器 800x600，X 子窗口任意旧几何 → 目标矩形铺满客户区
        const QRect r = ZzX11Viewport::computeFollowRect(QSize(800, 600));
        QCOMPARE(r, QRect(0, 0, 800, 600));
    }
    void followRectZeroSize()
    {
        // 容器坍缩为 0（分屏拖到边缘）→ 矩形为 0x0，不产生负尺寸
        const QRect r = ZzX11Viewport::computeFollowRect(QSize(0, 0));
        QCOMPARE(r, QRect(0, 0, 0, 0));
    }
    void windowClassNameIsBranded()
    {
        // Qt 侧识别串必须与 ZzXsrv 窗口类名品牌化一致（任务 2）
        QCOMPARE(ZzX11Viewport::x11WindowClassName(),
                 QStringLiteral("ZzXsrv/x"));
    }
};
QTEST_MAIN(tst_ZzX11Viewport)
#include "tst_ZzX11Viewport.moc"
