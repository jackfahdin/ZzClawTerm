/**
 * @file ZzPtyUnixPrivate.h
 * @brief ZzPtyUnix 的私有实现声明 (Pimpl)。
 */

#ifndef ZZ_PTY_UNIX_PRIVATE_H
#define ZZ_PTY_UNIX_PRIVATE_H

#include <QByteArray>
#include <QSize>
#include <QString>
#include <QStringList>

#include <sys/types.h>

class QSocketNotifier;

namespace ZzPlatform {

/**
 * @brief ZzPtyUnix 内部状态与底层系统调用封装。
 */
class ZzPtyUnixPrivate
{
public:
    ZzPtyUnixPrivate() = default;
    ~ZzPtyUnixPrivate();

    /**
     * @brief 通过 forkpty 启动子进程。
     * @return 成功返回 true, 失败时 errorMessage 含原因。
     */
    bool spawn(const QString& program, const QStringList& arguments, const QSize& size,
               QString& errorMessage);

    /** @brief 从 master fd 读取全部可用数据。 */
    QByteArray readAvailable();

    /** @brief 向 master fd 写入数据。 */
    qint64 writeData(const QByteArray& data);

    /** @brief 设置 PTY 窗口尺寸 (TIOCSWINSZ)。 */
    void setWindowSize(const QSize& size);

    /** @brief 关闭 fd 并终止子进程。 */
    void terminate();

    /** @brief 回收已退出子进程, 返回退出码 (-1 表示仍在运行)。 */
    int reapIfExited();

    int masterFd = -1;            ///< PTY 主端文件描述符
    pid_t childPid = -1;          ///< 子进程 PID
    QSocketNotifier* notifier = nullptr;  ///< 读就绪通知器
    bool running = false;         ///< 子进程是否运行中
};

}  // namespace ZzPlatform

#endif  // ZZ_PTY_UNIX_PRIVATE_H
