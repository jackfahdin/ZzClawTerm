#include <QtTest/QtTest>

#include <QtWidgets/QComboBox>
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
        page.terminalTypeCombo()->setCurrentText(QStringLiteral("vt100"));

        QCOMPARE(settings.fontSize(), 14);
        QCOMPARE(settings.encoding(), QStringLiteral("Big5"));
        QCOMPARE(settings.terminalType(), QStringLiteral("vt100"));
        QVERIFY(spy.count() >= 3);
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
