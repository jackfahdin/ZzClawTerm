#include "ZzSessionModel.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSaveFile>
#include <QStandardPaths>

namespace {

const QString kVersionKey = QStringLiteral("version");
const QString kSessionsKey = QStringLiteral("sessions");
constexpr int kFormatVersion = 1;

} // namespace

ZzSessionModel::ZzSessionModel(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_filePath(filePath)
{
}

QString ZzSessionModel::defaultFilePath()
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return dir + QStringLiteral("/sessions.json");
}

bool ZzSessionModel::load()
{
    QFile file(m_filePath);
    if (!file.exists()) {
        // 首次启动：文件不存在视为空模型
        m_sessions.clear();
        return true;
    }
    if (!file.open(QIODevice::ReadOnly)) {
        m_errorString = QStringLiteral("无法打开会话文件：%1").arg(file.errorString());
        return false;
    }
    const QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
    if (!doc.isObject()) {
        m_errorString = QStringLiteral("会话文件格式非法（不是 JSON 对象）");
        return false;
    }
    const QJsonArray array = doc.object().value(kSessionsKey).toArray();
    QList<ZzSessionProfile> loaded;
    loaded.reserve(array.size());
    for (const QJsonValue &value : array) {
        if (value.isObject())
            loaded.append(ZzSessionProfile::fromJson(value.toObject()));
    }
    m_sessions = loaded;
    return true;
}

bool ZzSessionModel::save() const
{
    const QFileInfo info(m_filePath);
    if (!info.dir().exists() && !QDir().mkpath(info.absolutePath())) {
        m_errorString = QStringLiteral("无法创建会话目录：%1").arg(info.absolutePath());
        return false;
    }

    QJsonArray array;
    for (const ZzSessionProfile &profile : m_sessions)
        array.append(profile.toJson());

    QJsonObject root;
    root.insert(kVersionKey, kFormatVersion);
    root.insert(kSessionsKey, array);

    QSaveFile file(m_filePath);
    if (!file.open(QIODevice::WriteOnly)) {
        m_errorString = QStringLiteral("无法写入会话文件：%1").arg(file.errorString());
        return false;
    }
    file.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    if (!file.commit()) {
        m_errorString = QStringLiteral("会话文件落盘失败：%1").arg(file.errorString());
        return false;
    }
    return true;
}

QList<ZzSessionProfile> ZzSessionModel::allSessions() const
{
    return m_sessions;
}

std::optional<ZzSessionProfile> ZzSessionModel::session(const QUuid &id) const
{
    for (const ZzSessionProfile &profile : m_sessions) {
        if (profile.id == id)
            return profile;
    }
    return std::nullopt;
}

QUuid ZzSessionModel::addSession(ZzSessionProfile profile)
{
    if (profile.id.isNull())
        profile.id = QUuid::createUuid();
    if (session(profile.id).has_value()) {
        m_errorString = QStringLiteral("会话 id 已存在：%1").arg(profile.id.toString(QUuid::WithoutBraces));
        return QUuid();
    }
    m_sessions.append(profile);
    emit sessionsChanged();
    return profile.id;
}

bool ZzSessionModel::updateSession(const ZzSessionProfile &profile)
{
    for (qsizetype i = 0; i < m_sessions.size(); ++i) {
        if (m_sessions[i].id == profile.id) {
            m_sessions[i] = profile;
            emit sessionsChanged();
            return true;
        }
    }
    m_errorString = QStringLiteral("会话不存在：%1").arg(profile.id.toString(QUuid::WithoutBraces));
    return false;
}

bool ZzSessionModel::removeSession(const QUuid &id)
{
    for (qsizetype i = 0; i < m_sessions.size(); ++i) {
        if (m_sessions[i].id == id) {
            m_sessions.removeAt(i);
            emit sessionsChanged();
            return true;
        }
    }
    m_errorString = QStringLiteral("会话不存在：%1").arg(id.toString(QUuid::WithoutBraces));
    return false;
}

QStringList ZzSessionModel::allGroupPaths() const
{
    QStringList paths;
    for (const ZzSessionProfile &profile : m_sessions) {
        if (!profile.groupPath.isEmpty() && !paths.contains(profile.groupPath))
            paths.append(profile.groupPath);
    }
    paths.sort();
    return paths;
}

QList<ZzSessionProfile> ZzSessionModel::sessionsInGroup(const QString &groupPath) const
{
    QList<ZzSessionProfile> result;
    if (groupPath.isEmpty())
        return result; // 空路径不是合法分组，返回空列表
    for (const ZzSessionProfile &profile : m_sessions) {
        if (profile.groupPath == groupPath)
            result.append(profile);
    }
    return result;
}

bool ZzSessionModel::renameGroup(const QString &oldPath, const QString &newPath)
{
    if (oldPath.isEmpty() || newPath.isEmpty() || oldPath == newPath) {
        m_errorString = QStringLiteral("分组重命名参数非法（空路径或与原路径相同）");
        return false;
    }
    if (newPath.startsWith(oldPath + QLatin1Char('/'))) {
        m_errorString = QStringLiteral("不能将分组重命名为自身的子分组");
        return false;
    }

    bool changed = false;
    const QString oldPrefix = oldPath + QLatin1Char('/');
    for (ZzSessionProfile &profile : m_sessions) {
        if (profile.groupPath == oldPath) {
            profile.groupPath = newPath;
            changed = true;
        } else if (profile.groupPath.startsWith(oldPrefix)) {
            profile.groupPath = newPath + profile.groupPath.mid(oldPath.size());
            changed = true;
        }
    }
    if (changed)
        emit sessionsChanged();
    return true; // 无匹配分组视为幂等成功
}

bool ZzSessionModel::removeGroup(const QString &groupPath)
{
    if (groupPath.isEmpty()) {
        m_errorString = QStringLiteral("不能删除空分组路径");
        return false;
    }

    const QString prefix = groupPath + QLatin1Char('/');
    const qsizetype before = m_sessions.size();
    m_sessions.removeIf([&](const ZzSessionProfile &profile) {
        return profile.groupPath == groupPath || profile.groupPath.startsWith(prefix);
    });
    if (m_sessions.size() == before) {
        m_errorString = QStringLiteral("分组不存在或为空：%1").arg(groupPath);
        return false;
    }
    emit sessionsChanged();
    return true;
}

QString ZzSessionModel::errorString() const
{
    return m_errorString;
}
