#include <QtTest/QtTest>

#include "settings/ZzAppSettings.h"

/**
 * @brief 验证全局设置：默认值、往返持久化、变更通知。
 */
class tst_ZzAppSettings : public QObject
{
    Q_OBJECT
private slots:
    void defaults()
    {
        ZzAppSettings settings(QStringLiteral("/nonexistent-dir/never-exists.ini"));
        QCOMPARE(settings.terminalType(), QStringLiteral("xterm-256color"));
        QCOMPARE(settings.encoding(), QStringLiteral("UTF-8"));
        QCOMPARE(settings.fontSize(), 12);
        QCOMPARE(settings.colorScheme(), QStringLiteral("Linux"));
        QCOMPARE(settings.historyLines(), 10000);
    }

    void roundTrip()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-test.ini"));
        QFile::remove(path);
        {
            ZzAppSettings settings(path);
            QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
            settings.setTerminalType(QStringLiteral("vt100"));
            settings.setEncoding(QStringLiteral("GBK"));
            settings.setFontSize(16);
            settings.setColorScheme(QStringLiteral("QuardCRT"));
            settings.setHistoryLines(20000);
            QCOMPARE(spy.count(), 5); // 每项变更都通知
        }
        ZzAppSettings reloaded(path);
        QCOMPARE(reloaded.terminalType(), QStringLiteral("vt100"));
        QCOMPARE(reloaded.encoding(), QStringLiteral("GBK"));
        QCOMPARE(reloaded.fontSize(), 16);
        QCOMPARE(reloaded.colorScheme(), QStringLiteral("QuardCRT"));
        QCOMPARE(reloaded.historyLines(), 20000);
        QFile::remove(path);
    }
};

QTEST_MAIN(tst_ZzAppSettings)
#include "tst_ZzAppSettings.moc"
