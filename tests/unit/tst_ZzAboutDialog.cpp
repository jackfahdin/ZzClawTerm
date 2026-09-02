#include <QtTest>
#include <QLabel>

#include "dialog/ZzAboutDialog.h"

/**
 * @brief ZzAboutDialog 单元测试：版本信息行与窗口标题。
 */
class tst_ZzAboutDialog : public QObject
{
    Q_OBJECT

private slots:
    /** @brief versionLine() 非空且含版本号、构建类型与 git 修订前 7 位。 */
    void versionLineContainsVersionBuildAndRevision()
    {
        ZzAboutDialog dlg;
        const QString line = dlg.versionLine();
        QVERIFY(!line.isEmpty());
        QVERIFY(line.contains(QStringLiteral("0.1.0")));
        QVERIFY(line.contains(QString::fromLatin1(ZZ_BUILD_TYPE)));
        QVERIFY(line.contains(QString::fromLatin1(ZZ_GIT_REVISION).left(7)));
    }

    /** @brief 窗口标题为 tr 键值（测试无 translator，回退源文本）。 */
    void windowTitleIsTranslateKey()
    {
        ZzAboutDialog dlg;
        QCOMPARE(dlg.windowTitle(), dlg.tr("关于 ZzClawTerm"));
    }

    /** @brief 版本行在界面上的 aboutVersionLabel 同步展示。 */
    void versionLabelShowsVersionLine()
    {
        ZzAboutDialog dlg;
        auto *label = dlg.findChild<QLabel *>(QStringLiteral("aboutVersionLabel"));
        QVERIFY(label);
        QCOMPARE(label->text(), dlg.versionLine());
    }

    /** @brief 正文含应用名标签 aboutNameLabel（规格：关于对话框展示应用名）。 */
    void nameLabelShowsAppName()
    {
        ZzAboutDialog dlg;
        auto *label = dlg.findChild<QLabel *>(QStringLiteral("aboutNameLabel"));
        QVERIFY(label);
        QCOMPARE(label->text(), QStringLiteral("ZzClawTerm"));
    }
};

QTEST_MAIN(tst_ZzAboutDialog)
#include "tst_ZzAboutDialog.moc"
