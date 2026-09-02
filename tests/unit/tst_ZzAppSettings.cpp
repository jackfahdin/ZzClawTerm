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
            QCOMPARE(spy.count(), 5); // 每项变更都通知（变化才发射）
        }
        ZzAppSettings reloaded(path);
        QCOMPARE(reloaded.terminalType(), QStringLiteral("vt100"));
        QCOMPARE(reloaded.encoding(), QStringLiteral("GBK"));
        QCOMPARE(reloaded.fontSize(), 16);
        QCOMPARE(reloaded.colorScheme(), QStringLiteral("QuardCRT"));
        QCOMPARE(reloaded.historyLines(), 20000);
        QFile::remove(path);
    }

    void sameValueDoesNotEmit()
    {
        // setter 同值短路：值未变化不发射 settingsChanged（避免无效重应用）
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-samevalue-test.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        settings.setFontSize(16); // 先落入非默认值

        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
        settings.setTerminalType(settings.terminalType());
        settings.setEncoding(settings.encoding());
        settings.setFontSize(16);
        settings.setColorScheme(settings.colorScheme());
        settings.setHistoryLines(settings.historyLines());
        QCOMPARE(spy.count(), 0);

        settings.setFontSize(20); // 真实变化仍正常发射
        QCOMPARE(spy.count(), 1);
        QFile::remove(path);
    }

    void languageRoundTrip()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-language-test.ini"));
        QFile::remove(path);
        {
            ZzAppSettings settings(path);
            QCOMPARE(settings.language(), QStringLiteral("system")); // 默认
            QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
            settings.setLanguage(QStringLiteral("en"));
            QCOMPARE(settings.language(), QStringLiteral("en"));
            QCOMPARE(spy.count(), 1);
        }
        {   // 重开持久化
            ZzAppSettings settings(path);
            QCOMPARE(settings.language(), QStringLiteral("en"));
        }
        QFile::remove(path);
    }

    void x11ServerEnabledDefaultsTrue()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-x11-default-test.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        QVERIFY(settings.x11ServerEnabled()); // M5 规格 §三决策 1：默认开
        QFile::remove(path);
    }

    void sftpBlockSizeDefaultsToAuto()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-sftp-bs-default-test.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        QCOMPARE(settings.sftpBlockSize(), 0); // 0=自动（BDP 自适应，M6）
        QFile::remove(path);
    }

    void sftpBlockSizeRoundtrip()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-sftp-bs-roundtrip-test.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
        settings.setSftpBlockSize(1024 * 1024);
        QCOMPARE(settings.sftpBlockSize(), 1024 * 1024);
        QCOMPARE(spy.count(), 1);
        settings.setSftpBlockSize(1024 * 1024); // 同值短路
        QCOMPARE(spy.count(), 1);
        QFile::remove(path);
    }

    void x11ServerEnabledRoundtrip()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-x11-roundtrip-test.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
        settings.setX11ServerEnabled(false);
        QVERIFY(!settings.x11ServerEnabled());
        QCOMPARE(spy.count(), 1);
        settings.setX11ServerEnabled(false); // 同值短路不再发射
        QCOMPARE(spy.count(), 1);
        QFile::remove(path);
    }
};

QTEST_MAIN(tst_ZzAppSettings)
#include "tst_ZzAppSettings.moc"
