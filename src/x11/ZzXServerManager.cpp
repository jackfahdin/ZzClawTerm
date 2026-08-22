#include "ZzXServerManager.h"

#include <QHostAddress>
#include <QProcess>
#include <QTcpServer>

ZzXServerManager::ZzXServerManager(QObject *parent)
    : QObject(parent)
{
}

int ZzXServerManager::allocateDisplay()
{
    for (int n = 0; n <= 99; ++n) {
        QTcpServer probe;
        if (probe.listen(QHostAddress::LocalHost, static_cast<quint16>(6000 + n)))
            return n; // 绑定成功即空闲，probe 析构自动释放端口
    }
    return -1;
}

void ZzXServerManager::start(const QString &executablePath, const QString &xauthorityPath, int displayNum)
{
    if (m_running)
        return;

#ifdef Q_OS_WIN
    launchProcess(m_programOverride.isEmpty() ? executablePath : m_programOverride,
                  xauthorityPath, displayNum);
#else
    if (m_programOverride.isEmpty()) {
        // Unix：系统 X server 已由桌面环境运行，只解析端点不发进程
        const int parsed = parseSystemDisplay();
        m_display = parsed >= 0 ? parsed : displayNum;
        m_running = true;
        emit started(m_display);
        return;
    }
    launchProcess(m_programOverride, xauthorityPath, displayNum);
#endif
}

void ZzXServerManager::launchProcess(const QString &program, const QString &xauthorityPath, int displayNum)
{
    m_lastProgram = program;
    m_lastXauthorityPath = xauthorityPath;
    m_lastDisplay = displayNum;
    m_display = displayNum;
    m_stopping = false;

    const QStringList args = {
        QStringLiteral(":%1").arg(displayNum),
        QStringLiteral("-multiwindow"),
        QStringLiteral("-clipboard"),
        QStringLiteral("-listen"),
        QStringLiteral("tcp"),
        QStringLiteral("-auth"),
        xauthorityPath,
    };

    auto *proc = new QProcess(this);
    m_process = proc;

    connect(proc, &QProcess::started, this, [this] {
        m_running = true;
        emit started(m_display);
    });

    connect(proc, &QProcess::errorOccurred, this, [this, proc](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart)
            return;
        // 启动失败不会再发 finished，直接按崩溃处理
        m_running = false;
        if (m_process == proc)
            m_process = nullptr;
        const int d = m_display;
        m_display = -1;
        proc->deleteLater();
        emit crashed(tr("X server 启动失败（display :%1）").arg(d));
    });

    connect(proc, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
            [this, proc](int exitCode, QProcess::ExitStatus exitStatus) {
                m_running = false;
                if (m_process == proc)
                    m_process = nullptr;
                const bool intentional = m_stopping;
                m_stopping = false;
                m_display = -1;
                proc->deleteLater();
                if (intentional) {
                    emit stopped();
                } else if (exitStatus == QProcess::CrashExit || exitCode != 0) {
                    emit crashed(tr("X server 非预期退出（退出码 %1）").arg(exitCode));
                } else {
                    emit stopped();
                }
            });

    proc->start(program, args);
}

void ZzXServerManager::stop()
{
    if (!m_running && !m_process)
        return;
    m_stopping = true;
    m_running = false;
    if (m_process) {
        // finished 处理器负责复位 display 并发 stopped
        m_process->terminate();
        if (!m_process->waitForFinished(3000)) {
            m_process->kill();
            m_process->waitForFinished(1000);
        }
        return;
    }
    // Unix 无进程分支：仅复位状态
    m_stopping = false;
    m_display = -1;
    emit stopped();
}

bool ZzXServerManager::isRunning() const
{
    return m_running;
}

int ZzXServerManager::display() const
{
    return m_display;
}

void ZzXServerManager::restart()
{
    if (m_running || m_lastDisplay < 0)
        return;
    launchProcess(m_lastProgram, m_lastXauthorityPath, m_lastDisplay);
}

ZzXLocalEndpoint ZzXServerManager::localEndpoint() const
{
    ZzXLocalEndpoint ep;
#ifdef Q_OS_WIN
    ep.host = QStringLiteral("127.0.0.1");
    ep.port = static_cast<quint16>(6000 + m_display);
#else
    if (m_programOverride.isEmpty()) {
        // 复用系统 X server：走 Unix 域套接字
        ep.localSocketPath = QStringLiteral("/tmp/.X11-unix/X%1").arg(m_display);
    } else {
        ep.host = QStringLiteral("127.0.0.1");
        ep.port = static_cast<quint16>(6000 + m_display);
    }
#endif
    return ep;
}

void ZzXServerManager::setServerProgramForTesting(const QString &program)
{
    m_programOverride = program;
}

int ZzXServerManager::parseSystemDisplay()
{
    const QByteArray disp = qgetenv("DISPLAY");
    const int colon = disp.lastIndexOf(':');
    if (colon < 0)
        return -1;
    bool ok = false;
    const int n = disp.mid(colon + 1).split('.').first().toInt(&ok);
    return ok ? n : -1;
}
