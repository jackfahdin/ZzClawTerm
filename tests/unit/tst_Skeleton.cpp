#include <QtTest/QtTest>

/**
 * @brief 骨架冒烟测试：验证构建链路与 Qt 基线版本。
 */
class tst_Skeleton : public QObject
{
    Q_OBJECT
private slots:
    /** @brief Qt 基线必须为 6.8+（规格 §十）。 */
    void qtBaseline()
    {
        QVERIFY2(QT_VERSION >= QT_VERSION_CHECK(6, 8, 0), "Qt 版本低于 6.8");
        QVERIFY2(QStringLiteral(ZZ_BUILD_TYPE).size() > 0, "缺少构建类型定义");
    }
};

QTEST_MAIN(tst_Skeleton)
#include "tst_Skeleton.moc"
