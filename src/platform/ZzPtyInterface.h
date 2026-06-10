/**
 * @file ZzPtyInterface.h
 * @brief 伪终端 (PTY) 抽象接口。
 *
 * 屏蔽平台差异: Windows 使用 ConPTY, Linux/macOS 使用 forkpty。
 * 上层通过本接口启动子进程、收发数据, 不关心底层实现。
 */

#ifndef ZZ_PTY_INTERFACE_H
#define ZZ_PTY_INTERFACE_H

#include <QByteArray>
#include <QObject>
#include <QSize>
#include <QString>
#include <QStringList>

namespace ZzPlatform {

/**
 * @brief 伪终端接口基类。
 *
 * 子类需实现平台特定的进程创建与读写逻辑。所有实现均以信号
 * 异步上报输出数据与进程退出事件。
 */
class ZzPtyInterface : public QObject
{
    Q_OBJECT

public:
    explicit ZzPtyInterface(QObject* parent = nullptr) : QObject(parent) {}
    ~ZzPtyInterface() override = default;

    /**
     * @brief 启动子进程 (Shell 或指定程序)。
     * @param program 可执行文件路径 (如 "/bin/bash" 或 PowerShell 路径)。
     * @param arguments 命令行参数。
     * @param initialSize 初始终端尺寸 (列, 行)。
     * @return 启动成功返回 true。
     */
    virtual bool start(const QString& program, const QStringList& arguments,
                       const QSize& initialSize) = 0;

    /**
     * @brief 向 PTY 写入数据 (用户输入)。
     * @param data 待写入字节流。
     * @return 实际写入字节数, 失败返回 -1。
     */
    virtual qint64 write(const QByteArray& data) = 0;

    /**
     * @brief 调整 PTY 窗口尺寸。
     * @param size 新尺寸 (列, 行)。
     */
    virtual void resize(const QSize& size) = 0;

    /**
     * @brief 关闭 PTY 并终止子进程。
     */
    virtual void shutdown() = 0;

    /**
     * @brief 查询子进程是否仍在运行。
     */
    virtual bool isRunning() const = 0;

signals:
    /**
     * @brief 收到子进程输出。
     * @param data 原始字节流 (可能含转义序列)。
     */
    void dataReceived(const QByteArray& data);

    /**
     * @brief 子进程已退出。
     * @param exitCode 退出码。
     */
    void processExited(int exitCode);

    /**
     * @brief 发生错误。
     * @param message 错误描述。
     */
    void errorOccurred(const QString& message);
};

}  // namespace ZzPlatform

#endif  // ZZ_PTY_INTERFACE_H
