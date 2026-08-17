#include <QtTest/QtTest>

#include "ZzPerfRecorder.h"
#include "settings/ZzAppSettings.h"

/**
 * @brief 性能门控：设置读写往返 1000 次。阈值 500ms（Release）。
 */
class tst_PerfSettings : public QObject
{
    Q_OBJECT
private slots:
    void settingsReadWriteThroughput()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-perf.ini"));
        QFile::remove(path);

        QElapsedTimer timer;
        timer.start();
        {
            ZzAppSettings settings(path);
            for (int i = 0; i < 1000; ++i) {
                settings.setFontSize(10 + (i % 20));
                (void)settings.fontSize();
                (void)settings.terminalType();
            }
        }
        const double elapsed = static_cast<double>(timer.elapsed());
        QFile::remove(path);

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("app-settings"),
            QStringLiteral("设置读写往返 1000 次"), 500.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 500ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfSettings)
#include "tst_PerfSettings.moc"
