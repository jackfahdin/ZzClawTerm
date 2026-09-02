#include "ZzLocalPtyTransport.h"

#include <QtCore/QDir>
#include <QtCore/QIODevice>
#include <QtCore/QProcess>
#include <QtCore/QProcessEnvironment>

#include "ptyqt.h"
#include "iptyprocess.h"

namespace {

/** @brief 按平台取默认本地 shell（与 ZzTermWidget example 同一套规则）。 */
QString zzDefaultShell()
{
#if defined(Q_OS_WIN)
    return QStringLiteral("c:\\Windows\\system32\\WindowsPowerShell\\v1.0\\powershell.exe");
#else
    const QString fromEnv = QString::fromLocal8Bit(qgetenv("SHELL"));
    return fromEnv.isEmpty() ? QStringLiteral("/bin/sh") : fromEnv;
#endif
}

} // namespace

ZzLocalPtyTransport::ZzLocalPtyTransport(QObject *parent)
    : ZzTransportInterface(parent)
{
}

ZzLocalPtyTransport::~ZzLocalPtyTransport()
{
    close();
}

void ZzLocalPtyTransport::open(const ZzTransportEndpoint &endpoint)
{
    if (state() != State::Disconnected) {
        return;
    }
    setState(State::Connecting);
    m_closing = false;

    m_pty.reset(PtyQt::createPtyProcess());
    const QString shell = endpoint.shellProgram.isEmpty()
        ? zzDefaultShell()
        : endpoint.shellProgram;

    // PTY 输出 → dataReceived（终端显示方向）
    connect(m_pty->notifier(), &QIODevice::readyRead, this, [this]() {
        const QByteArray data = m_pty->readAll();
        if (!data.isEmpty()) {
            emit dataReceived(data);
        }
    });
    // 子进程退出 → 被动断开。
    // 注意：ptyqt 各后端通知机制不一致——Windows ConPty 由 ptyqt 主动发射
    // notifier 的 aboutToClose；Unix 后端的 notifier 是内部 ShellProcess
    // （QProcess 子类），子进程退出只发 QProcess::finished，不发 aboutToClose，
    // 且 Unix 后端不维护 exitCode。两条路径共用同一处理，靠状态守卫去重。
    auto handleShellExit = [this]() {
        if (!m_pty || state() == State::Disconnected) {
            return;
        }
        const QByteArray rest = m_pty->readAll();
        if (!rest.isEmpty()) {
            emit dataReceived(rest);
        }
        if (!m_closing) {
            setState(State::Disconnected);
            emit disconnected(tr("本地 shell 已退出（退出码 %1）")
                                  .arg(m_pty->exitCode()));
        }
    };
    connect(m_pty->notifier(), &QIODevice::aboutToClose, this, handleShellExit);
    if (auto *proc = qobject_cast<QProcess *>(m_pty->notifier())) {
        connect(proc, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished),
                this, std::move(handleShellExit));
    }

    if (!m_pty->startProcess(shell, {}, QDir::homePath(),
                             QProcessEnvironment::systemEnvironment().toStringList(),
                             static_cast<qint16>(endpoint.cols),
                             static_cast<qint16>(endpoint.rows))) {
        const QString reason = m_pty->lastError();
        m_pty.reset();
        setState(State::Disconnected);
        emit errorOccurred(2001, tr("启动本地 shell 失败：%1").arg(reason));
        return;
    }
    setState(State::Connected);
}

void ZzLocalPtyTransport::write(const QByteArray &data)
{
    if (m_pty && state() == State::Connected) {
        m_pty->write(data);
    }
}

void ZzLocalPtyTransport::resize(int cols, int rows)
{
    if (m_pty) {
        m_pty->resize(static_cast<qint16>(cols), static_cast<qint16>(rows));
    }
}

void ZzLocalPtyTransport::close()
{
    if (!m_pty) {
        setState(State::Disconnected);
        return;
    }
    m_closing = true;
    m_pty->kill();
    m_pty.reset();
    setState(State::Disconnected);
}
