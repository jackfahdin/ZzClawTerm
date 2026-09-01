#include "ZzClawTermModule.h"

#include <chrono>
#include <utility>

#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QString>

#include <ZzCore/ZzApplicationPaths.h>
#include <ZzCore/ZzError.h>
#include <ZzCore/ZzErrorCode.h>

#include <ZzLog/ZzLog.h>

ZzPureTools::ZzModuleDescriptor ZzClawTermModule::descriptor() const
{
    return ZzPureTools::ZzModuleDescriptor{
        ZzPureTools::ZzModuleId(QStringLiteral("com.zzclawterm.app")),
        QStringLiteral("0.1.0"),
        {}};
}

ZzCore::ZzResult<void> ZzClawTermModule::start()
{
    if (m_started || m_ownsLogRuntime) {
        return ZzCore::ZzResult<void>::failure(ZzCore::ZzError(
            ZzCore::ZzErrorCode::InvalidState,
            QStringLiteral("application module is already started")));
    }

    // 日志目录：框架应用路径约定的数据目录 logs 子目录（Windows 真机无控制台，
    // qDebug/qWarning 默认不可见；接入后 Qt 消息与库侧日志（如连接计时）统一落盘）。
    const ZzCore::ZzApplicationPaths paths(
        QCoreApplication::organizationName().isEmpty()
            ? QStringLiteral("ZzClaw")
            : QCoreApplication::organizationName(),
        QCoreApplication::applicationName().isEmpty()
            ? QStringLiteral("ZzClawTerm")
            : QCoreApplication::applicationName());
    auto ensured = paths.ensureDirectories();
    if (!ensured) {
        return ZzCore::ZzResult<void>::failure(ZzCore::ZzError(
            ZzCore::ZzErrorCode::Backend,
            QStringLiteral("failed to create application directories"),
            ensured.error().technicalMessage()));
    }
    const QString logPath = QDir(paths.logDirectory())
        .filePath(QStringLiteral("ZzClawTerm.log"));

    ZzLog::ZzLogConfig config;
    config.loggerName = "ZzClawTerm";
    config.console.enabled = true;
    config.file.enabled = true;
    config.file.path = QFileInfo(logPath).filesystemAbsoluteFilePath();
    config.file.level = ZzLog::ZzLogLevel::Debug;
    config.file.async = true;
    // 滚动与大小上限：显式写出（与 ZzLog 默认值一致，10 MiB × 5 个轮转文件）
    config.file.maxFileSize = std::size_t{10} * 1024 * 1024;
    config.file.maxFiles = 5;

    const auto logResult = ZzLog::initialize(std::move(config));
    if (!logResult) {
        return ZzCore::ZzResult<void>::failure(ZzCore::ZzError(
            ZzCore::ZzErrorCode::Backend,
            QStringLiteral("failed to initialize ZzLog"),
            QString::fromUtf8(logResult.message)));
    }
    m_ownsLogRuntime = true;

    // Qt 消息桥：不链式调用旧处理器（Windows GUI 子系统无控制台，旧处理器
    // 输出无处可去；链式开启反而可能经其他处理器重复落盘）。
    const auto bridgeResult = m_logBridge.install();
    if (!bridgeResult) {
        ZzLog::shutdown();
        m_ownsLogRuntime = false;
        return bridgeResult;
    }

    ZzLog::writeText(ZzLog::ZzLogLevel::Info, "ZzClawTerm application module started");
    m_started = true;
    return ZzCore::ZzResult<void>::success();
}

void ZzClawTermModule::requestStop() noexcept
{
    m_started = false;
}

void ZzClawTermModule::stop() noexcept
{
    if (!m_started && !m_ownsLogRuntime) {
        return;
    }
    try {
        if (m_logBridge.isInstalled()) {
            static_cast<void>(m_logBridge.uninstall());
        }
        if (m_ownsLogRuntime) {
            static_cast<void>(
                ZzLog::flushAndWait(std::chrono::seconds(2)));
        }
    } catch (...) { // NOLINT(bugprone-empty-catch) stop() 必须 noexcept。
        // 停止期间日志收尾失败不阻断应用退出。
    }
    if (m_ownsLogRuntime) {
        ZzLog::shutdown();
    }
    m_ownsLogRuntime = false;
    m_started = false;
}
