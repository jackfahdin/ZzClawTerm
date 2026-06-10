/**
 * @file ZzPtyUnixPrivate.cpp
 * @brief ZzPtyUnix 私有实现。
 */

#include "platform/ZzPtyUnixPrivate.h"

#include <QSocketNotifier>

#include <cerrno>
#include <csignal>
#include <cstdlib>
#include <cstring>

#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

// forkpty 在 Linux 位于 <pty.h>, 在 macOS/BSD 位于 <util.h>。
#if defined(ZZ_PLATFORM_MACOS)
#include <util.h>
#else
#include <pty.h>
#endif

namespace ZzPlatform {

ZzPtyUnixPrivate::~ZzPtyUnixPrivate()
{
    terminate();
}

bool ZzPtyUnixPrivate::spawn(const QString& program, const QStringList& arguments,
                             const QSize& size, QString& errorMessage)
{
    struct winsize ws{};
    ws.ws_col = static_cast<unsigned short>(size.width());
    ws.ws_row = static_cast<unsigned short>(size.height());

    pid_t pid = forkpty(&masterFd, nullptr, nullptr, &ws);
    if (pid < 0) {
        errorMessage = QStringLiteral("forkpty 失败: %1").arg(QString::fromLocal8Bit(strerror(errno)));
        return false;
    }

    if (pid == 0) {
        // 子进程: 设置环境并 exec。
        ::setenv("TERM", "xterm-256color", 1);

        const QByteArray prog = program.toLocal8Bit();
        std::vector<QByteArray> argStore;
        argStore.push_back(prog);
        for (const QString& a : arguments) {
            argStore.push_back(a.toLocal8Bit());
        }
        std::vector<char*> argv;
        argv.reserve(argStore.size() + 1);
        for (QByteArray& a : argStore) {
            argv.push_back(a.data());
        }
        argv.push_back(nullptr);

        ::execvp(prog.constData(), argv.data());
        // exec 失败。
        ::_exit(127);
    }

    // 父进程。
    childPid = pid;
    running = true;

    // master fd 设为非阻塞, 便于一次性读尽。
    const int flags = ::fcntl(masterFd, F_GETFL, 0);
    ::fcntl(masterFd, F_SETFL, flags | O_NONBLOCK);
    return true;
}

QByteArray ZzPtyUnixPrivate::readAvailable()
{
    if (masterFd < 0) {
        return {};
    }
    QByteArray out;
    char buf[8192];
    for (;;) {
        const ssize_t n = ::read(masterFd, buf, sizeof(buf));
        if (n > 0) {
            out.append(buf, static_cast<int>(n));
            if (n < static_cast<ssize_t>(sizeof(buf))) {
                break;
            }
        } else if (n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
            break;
        } else {
            // 0 (EOF) 或其它错误。
            break;
        }
    }
    return out;
}

qint64 ZzPtyUnixPrivate::writeData(const QByteArray& data)
{
    if (masterFd < 0) {
        return -1;
    }
    const ssize_t n = ::write(masterFd, data.constData(), static_cast<size_t>(data.size()));
    return static_cast<qint64>(n);
}

void ZzPtyUnixPrivate::setWindowSize(const QSize& size)
{
    if (masterFd < 0) {
        return;
    }
    struct winsize ws{};
    ws.ws_col = static_cast<unsigned short>(size.width());
    ws.ws_row = static_cast<unsigned short>(size.height());
    ::ioctl(masterFd, TIOCSWINSZ, &ws);
}

void ZzPtyUnixPrivate::terminate()
{
    if (notifier) {
        notifier->setEnabled(false);
        notifier->deleteLater();
        notifier = nullptr;
    }
    if (childPid > 0 && running) {
        ::kill(childPid, SIGHUP);
        ::waitpid(childPid, nullptr, 0);
    }
    if (masterFd >= 0) {
        ::close(masterFd);
        masterFd = -1;
    }
    childPid = -1;
    running = false;
}

int ZzPtyUnixPrivate::reapIfExited()
{
    if (childPid <= 0) {
        return -1;
    }
    int status = 0;
    const pid_t r = ::waitpid(childPid, &status, WNOHANG);
    if (r == childPid) {
        running = false;
        if (WIFEXITED(status)) {
            return WEXITSTATUS(status);
        }
        return 0;
    }
    return -1;
}

}  // namespace ZzPlatform
