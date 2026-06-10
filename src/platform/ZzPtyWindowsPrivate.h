/**
 * @file ZzPtyWindowsPrivate.h
 * @brief ZzPtyWindows 私有实现声明 (Pimpl) 与读取线程。
 */

#ifndef ZZ_PTY_WINDOWS_PRIVATE_H
#define ZZ_PTY_WINDOWS_PRIVATE_H

#include <QByteArray>
#include <QSize>
#include <QString>
#include <QStringList>
#include <QThread>

#if defined(ZZ_PLATFORM_WINDOWS)

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

namespace ZzPlatform {

/**
 * @brief ConPTY 输出读取线程。
 *
 * 阻塞式 ReadFile 输出管道, 每读到一块数据发射 chunkRead 信号
 * (队列连接到主线程)。
 */
class ZzPtyReaderThread : public QThread
{
    Q_OBJECT

public:
    explicit ZzPtyReaderThread(HANDLE outputRead, QObject* parent = nullptr)
        : QThread(parent), m_outputRead(outputRead)
    {
    }

signals:
    /** @brief 读到一块输出数据。 */
    void chunkRead(const QByteArray& data);

protected:
    void run() override;

private:
    HANDLE m_outputRead;
};

/**
 * @brief ZzPtyWindows 内部状态与 ConPTY 封装。
 */
class ZzPtyWindowsPrivate
{
public:
    ZzPtyWindowsPrivate() = default;
    ~ZzPtyWindowsPrivate();

    /** @brief 创建管道与伪控制台并启动子进程。 */
    bool spawn(const QString& program, const QStringList& arguments, const QSize& size,
               QString& errorMessage);

    /** @brief 写入输入管道。 */
    qint64 writeData(const QByteArray& data);

    /** @brief 调整伪控制台尺寸。 */
    void setWindowSize(const QSize& size);

    /** @brief 终止子进程并释放资源。 */
    void terminate();

    HPCON pseudoConsole = nullptr;        ///< 伪控制台句柄
    HANDLE inputWrite = INVALID_HANDLE_VALUE;   ///< 输入管道写端
    HANDLE outputRead = INVALID_HANDLE_VALUE;   ///< 输出管道读端
    PROCESS_INFORMATION processInfo{};    ///< 子进程信息
    STARTUPINFOEXW startupInfo{};         ///< 扩展启动信息
    ZzPtyReaderThread* reader = nullptr;  ///< 输出读取线程
    bool running = false;                 ///< 子进程是否运行中

private:
    /** @brief 构造带伪控制台属性的 STARTUPINFOEX。 */
    bool prepareStartupInfo(QString& errorMessage);
};

}  // namespace ZzPlatform

#endif  // ZZ_PLATFORM_WINDOWS
#endif  // ZZ_PTY_WINDOWS_PRIVATE_H
