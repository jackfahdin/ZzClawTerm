#pragma once

#include <QtCore/QJsonObject>
#include <QtCore/QString>

/**
 * @brief 性能测试记录器（规格 §9.1）。
 *
 * 每条记录写入 tests/perf/records/YYYY-MM-DD-<功能名>.json（单个 JSON 对象），
 * 包含阈值、实测值、是否通过、环境信息、git commit hash 与时间。
 *
 * 统一 schema 以 ZzLogEngine 扁平结构为准：
 * testName/threshold/unit/measured/passed + details + timestamp（UTC）
 * + environment{cpu, memory_mb, os, kernel, qtVersion, compiler, buildType, gitCommit}。
 */
class ZzPerfRecorder final
{
public:
    /**
     * @brief 记录一次性能结果（“越低越好”型门控）。
     * @param feature 功能名（进入文件名，只含字母数字与连字符）。
     * @param testName 测试项中文名。
     * @param thresholdMs 通过阈值（毫秒）。
     * @param measuredMs 实测值（毫秒）。
     * @param unit 实测值单位，默认 "ms"。
     * @param details 附加明细（如样本数、数据规模），默认空对象。
     * @return 实测值是否达标（measuredMs <= thresholdMs）。
     */
    static bool recordAndCheck(const QString &feature,
                               const QString &testName,
                               double thresholdMs,
                               double measuredMs,
                               const QString &unit = QStringLiteral("ms"),
                               const QJsonObject &details = QJsonObject());

    /** @brief 指定功能当日的记录文件绝对路径（供测试断言）。 */
    [[nodiscard]] static QString recordFilePath(const QString &feature);

    /**
     * @brief 覆盖记录根目录（仅供自检类测试使用）。
     *
     * 自检记录无性能语义，不应落盘到入库目录 tests/perf/records/ 污染仓库；
     * 测试用 QTemporaryDir 调用本函数重定向，传空串恢复默认目录。
     */
    static void setRecordsDirOverride(const QString &dir);

    /**
     * @brief 性能门控是否生效：仅 Release 构建返回 true（规格 §9.1）。
     *
     * 非 Release 构建下性能测试应 QSKIP，数字无效、不落记录。
     */
    [[nodiscard]] static constexpr bool gatingEnabled()
    {
#ifdef NDEBUG
        return true;
#else
        return false;
#endif
    }
};
