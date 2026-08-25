#include <QtTest/QtTest>

#include <QtWidgets/QCheckBox>
#include <QtWidgets/QComboBox>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QSpinBox>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzSettingsPage.h"

/**
 * @brief 验证设置页：表单回显当前值、修改即写存储、配色选项来自 QTermWidget。
 */
class tst_ZzSettingsPage : public QObject
{
    Q_OBJECT
private:
    QString m_path;

private slots:
    void init()
    {
        m_path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settingspage-test.ini"));
        QFile::remove(m_path);
    }

    void reflectsCurrentSettings()
    {
        ZzAppSettings settings(m_path);
        settings.setFontSize(20);
        settings.setEncoding(QStringLiteral("GBK"));

        ZzSettingsPage page(&settings);
        QCOMPARE(page.fontSizeSpin()->value(), 20);
        QCOMPARE(page.encodingCombo()->currentText(), QStringLiteral("GBK"));
    }

    void editingWritesThrough()
    {
        ZzAppSettings settings(m_path);
        ZzSettingsPage page(&settings);
        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);

        page.fontSizeSpin()->setValue(14);
        page.encodingCombo()->setCurrentText(QStringLiteral("Big5"));

        QCOMPARE(settings.fontSize(), 14);
        QCOMPARE(settings.encoding(), QStringLiteral("Big5"));
        QCOMPARE(spy.count(), 2);

        // 可编辑终端类型 combo：逐键输入不写盘（避免 "x"/"xt"/... 中间态落盘）
        QTest::keyClicks(page.terminalTypeCombo()->lineEdit(),
                         QStringLiteral("xterm"));
        QCOMPARE(settings.terminalType(), QStringLiteral("xterm-256color"));
        QCOMPARE(spy.count(), 2);

        // 编辑完成（回车）才提交写盘
        QTest::keyClick(page.terminalTypeCombo()->lineEdit(), Qt::Key_Return);
        QCOMPARE(settings.terminalType(),
                 page.terminalTypeCombo()->currentText());
        QVERIFY(spy.count() > 2);
    }

    void x11ServerCheckBoxReflectsAndWrites()
    {
        ZzAppSettings settings(m_path);
        ZzSettingsPage page(&settings);
        auto *box = page.x11ServerCheck();
        QVERIFY(box && box->isChecked());      // 默认开
        box->setChecked(false);                // toggled 即写设置
        QVERIFY(!settings.x11ServerEnabled());
    }

    void sftpBlockSizeComboReflectsAndWrites()
    {
        ZzAppSettings settings(m_path);
        ZzSettingsPage page(&settings);
        auto *combo = page.sftpBlockSizeCombo();
        QVERIFY(combo);
        QCOMPARE(combo->currentData().toInt(), 0); // 默认"自动"
        const int idx = combo->findData(1024 * 1024);
        QVERIFY(idx >= 0);
        combo->setCurrentIndex(idx);              // currentIndexChanged 即写
        QCOMPARE(settings.sftpBlockSize(), 1024 * 1024);
    }

    void colorSchemesComeFromTerminal()
    {
        ZzAppSettings settings(m_path);
        ZzSettingsPage page(&settings);
        QCOMPARE(page.colorSchemeCombo()->count(),
                 QTermWidget::availableColorSchemes().count());
        QVERIFY(page.colorSchemeCombo()->count() > 0);
    }
};

QTEST_MAIN(tst_ZzSettingsPage)
#include "tst_ZzSettingsPage.moc"
