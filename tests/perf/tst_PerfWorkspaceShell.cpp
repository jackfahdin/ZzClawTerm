#include <QtTest/QtTest>

#include <QtWidgets/QMainWindow>

#include <ZzPureTools/ZzWorkspaceShell.h>

#include "ZzAppShell.h"
#include "ZzPerfRecorder.h"
#include "settings/ZzAppSettings.h"

/**
 * @brief 性能门控：IDE 工作区外壳装配与布局往返（第一期外壳替换）。
 *
 * 三项指标（均离屏 Release 测量，规格 §9.1）：
 *  - assemble() 总耗时（ZzWorkspaceShell 创建 + 侧栏工厂注册 + 中央挂载）< 500ms
 *  - 会话面板延迟工厂首开耗时（等价点击活动栏「会话」首次实例化）< 300ms
 *  - 布局 saveLayout + restoreLayout 往返耗时 < 50ms
 */
class tst_PerfWorkspaceShell : public QObject
{
    Q_OBJECT
private:
    QString m_dir;
    bool m_originalX11Enabled = false; ///< initTestCase 保存的用户开关原值

    /** @brief 重建隔离配置目录并装配一套工作区（计时外用）。 */
    ZzAppShell *assembleFresh(QMainWindow &window)
    {
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
        auto *shell = new ZzAppShell(m_dir);
        auto assembled = shell->assemble(window);
        if (!assembled) {
            qWarning("工作区装配失败：%ls",
                     qUtf16Printable(assembled.error().technicalMessage()));
            delete shell;
            return nullptr;
        }
        return shell;
    }

private slots:
    void initTestCase()
    {
        // 设置写盘隔离到测试模式目录（早于 ZzAppSettings::instance() 首次调用）
        QStandardPaths::setTestModeEnabled(true);
        // 基线隔离：共享 X server 不启动（同 tst_ZzAppShell，避免沙盒外副作用）
        m_originalX11Enabled = ZzAppSettings::instance().x11ServerEnabled();
        ZzAppSettings::instance().setX11ServerEnabled(false);
        m_dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-perf-workspace"));
    }

    void cleanupTestCase()
    {
        ZzAppSettings::instance().setX11ServerEnabled(m_originalX11Enabled);
        QDir(m_dir).removeRecursively();
    }

    void workspaceAssemblyLatency()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        QMainWindow window;
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
        ZzAppShell shell(m_dir); // 构造不计时（会话模型/凭据库加载在装配链路之外）

        QElapsedTimer timer;
        timer.start();
        auto assembled = shell.assemble(window);
        QVERIFY(assembled);
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("zzshell-assembly"),
            QStringLiteral("IDE 工作区外壳 assemble() 总耗时"),
            500.0, elapsed,
            QStringLiteral("ms"),
            QJsonObject{{QStringLiteral("samples"), 1}});
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 500ms").arg(elapsed)));
    }

    void sessionPanelFirstOpenLatency()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        QMainWindow window;
        std::unique_ptr<ZzAppShell> shell(assembleFresh(window));
        QVERIFY(shell != nullptr);

        QElapsedTimer timer;
        timer.start();
        // 首开 = 延迟工厂首次调用：创建会话面板内容并挂入侧栏
        auto shown = shell->workspaceShell()->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sessions")));
        QVERIFY(shown);
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("zzshell-session-panel-first-open"),
            QStringLiteral("会话面板延迟工厂首开耗时"),
            300.0, elapsed,
            QStringLiteral("ms"),
            QJsonObject{{QStringLiteral("samples"), 1}});
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 300ms").arg(elapsed)));
    }

    void layoutSaveRestoreRoundtrip()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        // 源工作区：实例化两侧栏面板后保存布局字节
        QMainWindow sourceWindow;
        std::unique_ptr<ZzAppShell> source(assembleFresh(sourceWindow));
        QVERIFY(source != nullptr);
        QVERIFY(source->workspaceShell()->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sessions"))));
        QVERIFY(source->workspaceShell()->showPanel(
            ZzPureTools::ZzWorkspacePanelId(QStringLiteral("sftp"))));
        auto saved = source->workspaceShell()->saveLayout();
        QVERIFY(saved);
        const QByteArray state = std::move(saved).value();
        QVERIFY(!state.isEmpty());

        // 目标工作区：全新装配后恢复同一布局字节（真实往返）
        QMainWindow targetWindow;
        std::unique_ptr<ZzAppShell> target(assembleFresh(targetWindow));
        QVERIFY(target != nullptr);

        QElapsedTimer timer;
        timer.start();
        auto restored = target->workspaceShell()->restoreLayout(state);
        const double elapsed = static_cast<double>(timer.elapsed());
        QVERIFY(restored);

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("zzshell-layout-roundtrip"),
            QStringLiteral("工作区布局 saveLayout+restoreLayout 往返耗时"),
            50.0, elapsed,
            QStringLiteral("ms"),
            QJsonObject{{QStringLiteral("layoutBytes"), state.size()},
                        {QStringLiteral("samples"), 1}});
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 50ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfWorkspaceShell)
#include "tst_PerfWorkspaceShell.moc"
