/**
 * @file ZzLocalSession.h
 * @brief 本地终端会话: 串联 PTY、终端核心与日志引擎。
 *
 * 一个会话代表一个本地 Shell 实例。负责把 PTY 输出送入解析器,
 * 把滚出行归档进日志引擎, 并把用户输入写回 PTY。
 */

#ifndef ZZ_LOCAL_SESSION_H
#define ZZ_LOCAL_SESSION_H

#include <QByteArray>
#include <QObject>

#include <memory>

namespace ZzCore {
namespace Terminal {
class ZzTerminalCore;
}
namespace Log {
class ZzLogEngine;
}
}  // namespace ZzCore

namespace ZzSession {

class ZzLocalSessionPrivate;

/**
 * @brief 本地终端会话。
 */
class ZzLocalSession : public QObject
{
    Q_OBJECT

public:
    explicit ZzLocalSession(QObject* parent = nullptr);
    ~ZzLocalSession() override;

    /**
     * @brief 启动会话 (创建 Shell 子进程)。
     * @param cols 初始列数。
     * @param rows 初始行数。
     * @return 成功返回 true。
     */
    bool start(int cols, int rows);

    /** @brief 终端核心 (供渲染器绑定)。 */
    ZzCore::Terminal::ZzTerminalCore* terminal() const;

    /** @brief 日志引擎 (供搜索/滚动)。 */
    ZzCore::Log::ZzLogEngine* log() const;

public slots:
    /** @brief 向 Shell 写入用户输入。 */
    void sendInput(const QByteArray& data);

    /** @brief 调整终端与 PTY 尺寸。 */
    void resize(int cols, int rows);

signals:
    /** @brief Shell 进程退出。 */
    void exited(int exitCode);

    /** @brief 发生错误。 */
    void errorOccurred(const QString& message);

private:
    std::unique_ptr<ZzLocalSessionPrivate> d_ptr;
};

}  // namespace ZzSession

#endif  // ZZ_LOCAL_SESSION_H
