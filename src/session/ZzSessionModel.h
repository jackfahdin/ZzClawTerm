#pragma once

#include "ZzSessionProfile.h"

#include <QObject>
#include <QList>
#include <QString>
#include <QStringList>
#include <QUuid>
#include <optional>

/**
 * @brief 会话配置档案模型，负责会话的内存管理、增删改查与 JSON 持久化。
 *
 * 纯 Qt Core 后端类，不依赖 Widgets。持久化路径经构造函数注入；
 * 生产代码使用 defaultFilePath()（平台配置目录下的 sessions.json），
 * 测试注入临时目录路径。每次数据变更后发射 sessionsChanged() 信号。
 * 分组用路径字符串表示（如 "生产环境/Web 服务器"），重命名分组即改字符串前缀。
 */
class ZzSessionModel : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 构造会话模型。
     * @param filePath 持久化 JSON 文件路径。
     * @param parent Qt 父对象。
     */
    explicit ZzSessionModel(const QString &filePath, QObject *parent = nullptr);

    /**
     * @brief 默认持久化路径（Linux ~/.config/ZzClawTerm/sessions.json，
     *        Windows %APPDATA%/ZzClawTerm/sessions.json，
     *        macOS ~/Library/Application Support/ZzClawTerm/sessions.json）。
     * @return 平台配置目录下的 sessions.json 绝对路径。
     */
    static QString defaultFilePath();

    /**
     * @brief 从磁盘加载会话。文件不存在时视为空模型并返回 true（首次启动）。
     * @return 加载成功返回 true；文件存在但内容非法返回 false，错误原因见 errorString()。
     */
    bool load();

    /**
     * @brief 原子保存到磁盘（QSaveFile），父目录不存在时自动创建。
     * @return 保存成功返回 true；失败返回 false，错误原因见 errorString()。
     */
    bool save() const;

    /** @brief 返回全部会话（按添加顺序）。 */
    QList<ZzSessionProfile> allSessions() const;

    /**
     * @brief 按 id 查询会话。
     * @param id addSession 返回的会话 id。
     * @return 找到返回会话副本，否则返回 std::nullopt。
     */
    std::optional<ZzSessionProfile> session(const QUuid &id) const;

    /**
     * @brief 添加会话。id 为 null 时自动生成；id 冲突时拒绝。
     * @param profile 会话档案（id 字段可为 null）。
     * @return 成功返回该会话的 id；失败返回 null QUuid，错误原因见 errorString()。
     */
    QUuid addSession(ZzSessionProfile profile);

    /**
     * @brief 按 id 整体更新会话。
     * @param profile 新档案，id 字段必须指向已存在的会话。
     * @return 成功返回 true；id 不存在返回 false，错误原因见 errorString()。
     */
    bool updateSession(const ZzSessionProfile &profile);

    /**
     * @brief 按 id 删除会话。
     * @param id 会话 id。
     * @return 成功返回 true；id 不存在返回 false，错误原因见 errorString()。
     * @note 不级联删除 ZzCredentialStore 中对应的凭据（由上层按需处理）。
     */
    bool removeSession(const QUuid &id);

    /**
     * @brief 返回全部非空分组路径（去重、字典序排序）。
     * @return 分组路径列表，元素为完整路径字符串（如 "生产环境/Web 服务器"）。
     * @note 只返回实际被会话使用的路径，不从嵌套路径推导父分组；
     *       UI 构建分组树时自行按 '/' 拆分归并。
     */
    QStringList allGroupPaths() const;

    /**
     * @brief 返回直接位于指定分组的会话（不含子分组中的会话）。
     * @param groupPath 分组路径；空串不是合法分组，返回空列表。
     * @return 会话列表（按添加顺序）。
     */
    QList<ZzSessionProfile> sessionsInGroup(const QString &groupPath) const;

    /**
     * @brief 重命名分组：groupPath 等于 oldPath 或以 "oldPath/" 为前缀的会话，前缀改写为 newPath。
     * @param oldPath 原分组路径，不能为空。
     * @param newPath 新分组路径，不能为空、不能与 oldPath 相同、不能是 oldPath 的子路径。
     * @return 参数非法返回 false；无匹配会话视为幂等成功返回 true。
     */
    bool renameGroup(const QString &oldPath, const QString &newPath);

    /**
     * @brief 删除分组：级联删除 groupPath 等于该路径或以 "groupPath/" 为前缀的全部会话。
     * @param groupPath 分组路径，不能为空。
     * @return 删除了至少一个会话返回 true；路径为空或无匹配返回 false。
     */
    bool removeGroup(const QString &groupPath);

    /** @brief 最近一次失败的错误信息（简体中文）。 */
    QString errorString() const;

signals:
    /** @brief 会话数据发生任何增删改后发射。 */
    void sessionsChanged();

private:
    QList<ZzSessionProfile> m_sessions; ///< 内存中的全部会话
    QString m_filePath;                 ///< 持久化文件路径
    mutable QString m_errorString;      ///< 最近一次失败的错误信息
};
