#include "ZzPerfRecorder.h"

#include <QtCore/QDateTime>
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

QString ZzPerfRecorder::recordFilePath(const QString &feature)
{
    const QString date =
        QDateTime::currentDateTimeUtc().toString(QStringLiteral("yyyy-MM-dd"));
    return QStringLiteral("%1/%2-%3.json")
        .arg(QStringLiteral(ZZ_PERF_RECORDS_DIR), date, feature);
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
    QFile file(path);
    if (file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        file.write(QJsonDocument(entry).toJson(QJsonDocument::Indented));
    return passed;
}
