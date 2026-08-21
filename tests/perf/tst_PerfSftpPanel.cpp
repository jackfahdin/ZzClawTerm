#include <QtTest/QtTest>

#include "ZzMockSftpOps.h"
#include "ZzPerfRecorder.h"
#include "panel/ZzSftpPanel.h"

/**
 * @brief 性能门控：SFTP 面板列举 2 万条目大目录并分批填充满模型。
 *        阈值 3000ms（Release），覆盖目录模型分批填充路径（传输吞吐
 *        由库侧流水线保障，不在面板侧测量）。
 */
class tst_PerfSftpPanel : public QObject
{
    Q_OBJECT
private slots:
    void largeDirFill()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        ZzSftpPanel panel;
        panel.resize(640, 480);
        panel.show();

        constexpr int kEntries = 20000;
        QList<ZzSftpFileInfo> entries;
        entries.reserve(kEntries);
        for (int i = 0; i < kEntries; ++i) {
            ZzSftpFileInfo info;
            info.name = QStringLiteral("file-%1.dat").arg(i, 5, 10, QLatin1Char('0'));
            info.size = i * 1024;
            info.permissions = LIBSSH2_SFTP_S_IFREG | 0644;
            info.mtime = 1700000000 + i;
            entries.append(info);
        }

        ZzMockSftpOps mock; // 无父：面板不接管
        mock.dirReply = entries;
        panel.attachOpsForTesting(&mock);

        QElapsedTimer timer;
        timer.start();
        mock.simulateOpened(); // → listDir("/") → dirListed → 分批填充
        QTRY_VERIFY_WITH_TIMEOUT(panel.visibleEntryCount() == kEntries, 30000);
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("sftp-panel-large-dir-fill"),
            QStringLiteral("SFTP 面板 2 万条目目录分批填充"), 3000.0, elapsed,
            QStringLiteral("ms"),
            QJsonObject{{QStringLiteral("entries"), kEntries},
                        {QStringLiteral("batchSize"), 500}});
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 3000ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfSftpPanel)
#include "tst_PerfSftpPanel.moc"
