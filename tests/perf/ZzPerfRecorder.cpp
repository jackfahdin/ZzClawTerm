#include "ZzPerfRecorder.h"

#include <QtCore/QDateTime>
#include <QtCore/QDebug>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QJsonDocument>
#include <QtCore/QSysInfo>

namespace {

/** @brief 读取物理内存总量（MB），无法获取返回 -1。 */
qint64 zzTotalMemoryMB()
{
#ifdef Q_OS_LINUX
    QFile meminfo(QStringLiteral("/proc/meminfo"));
    if (meminfo.open(QIODevice::ReadOnly)) {
        const QByteArray content = meminfo.readAll();
        const int begin = content.indexOf("MemTotal:");
        if (begin >= 0) {
            const int end = content.indexOf('\n', begin);
            const QByteArray line = content.mid(begin, end - begin);
            // 格式：MemTotal:       16384000 kB（标签与数值之间有多个空格）
            return QString::fromLatin1(line).split(' ', Qt::SkipEmptyParts)
                       .value(1).toLongLong() / 1024;
        }
    }
#endif
    return -1; ///< Windows/macOS 暂无采集实现，记录为 -1
}

/** @brief 组装环境信息对象（规格 §9.1 要求项，统一 schema）。 */
QJsonObject zzEnvironment()
{
    QJsonObject env;
    env.insert(QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture());
    env.insert(QStringLiteral("memory_mb"), double(zzTotalMemoryMB()));
    env.insert(QStringLiteral("os"), QSysInfo::prettyProductName());
    env.insert(QStringLiteral("kernel"), QSysInfo::kernelVersion());
    env.insert(QStringLiteral("qtVersion"), QString::fromLatin1(qVersion()));
    env.insert(QStringLiteral("compiler"), QStringLiteral(ZZ_COMPILER_DESCRIPTION));
    env.insert(QStringLiteral("buildType"), QStringLiteral(ZZ_BUILD_TYPE));
    env.insert(QStringLiteral("gitCommit"), QStringLiteral(ZZ_GIT_REVISION));
    return env;
}

} // namespace

namespace {

/** @brief 记录根目录覆盖（仅测试自检用，见 setRecordsDirOverride）。 */
QString g_recordsDirOverride;

} // namespace

void ZzPerfRecorder::setRecordsDirOverride(const QString &dir)
{
    g_recordsDirOverride = dir;
}

QString ZzPerfRecorder::recordFilePath(const QString &feature)
{
    const QString date =
        QDateTime::currentDateTimeUtc().toString(QStringLiteral("yyyy-MM-dd"));
    const QString root = g_recordsDirOverride.isEmpty()
        ? QStringLiteral(ZZ_PERF_RECORDS_DIR) : g_recordsDirOverride;
    return QStringLiteral("%1/%2-%3.json").arg(root, date, feature);
}

bool ZzPerfRecorder::recordAndCheck(const QString &feature,
                                    const QString &testName,
                                    double thresholdMs,
                                    double measuredMs,
                                    const QString &unit,
                                    const QJsonObject &details)
{
    const bool passed = measuredMs <= thresholdMs;

    QJsonObject entry;
    entry.insert(QStringLiteral("testName"), testName);
    entry.insert(QStringLiteral("threshold"), thresholdMs);
    entry.insert(QStringLiteral("unit"), unit);
    entry.insert(QStringLiteral("measured"), measuredMs);
    entry.insert(QStringLiteral("passed"), passed);
    entry.insert(QStringLiteral("environment"), zzEnvironment());
    entry.insert(QStringLiteral("details"), details);
    entry.insert(QStringLiteral("timestamp"),
                 QDateTime::currentDateTimeUtc().toString(Qt::ISODate));

    const QString path = recordFilePath(feature);
    QDir().mkpath(QFileInfo(path).absolutePath());

    // 同日同功能文件为单个 JSON 对象：覆盖写回（与 ZzLogEngine 记录格式一致）
    // 落盘失败必须让测试失败（规格 §9.1 兜底），否则性能记录静默丢失
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        qWarning("ZzPerfRecorder: 无法写入性能记录 %ls：%ls",
                 qUtf16Printable(path), qUtf16Printable(file.errorString()));
        return false;
    }
    if (file.write(QJsonDocument(entry).toJson(QJsonDocument::Indented)) < 0) {
        qWarning("ZzPerfRecorder: 性能记录写入失败 %ls：%ls",
                 qUtf16Printable(path), qUtf16Printable(file.errorString()));
        return false;
    }
    return passed;
}
