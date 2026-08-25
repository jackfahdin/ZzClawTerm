#include "ZzX11Service.h"

#include <QFileInfo>
#include <QStandardPaths>

#include "x11/ZzXServerDownloader.h"

ZzX11Service::ZzX11Service(QObject *parent)
    : QObject(parent)
    , m_manager(new ZzXServerManager(this))
{
    connect(m_manager, &ZzXServerManager::started, this, [this](int display) {
        m_starting = false;
        m_display = display;
        if (m_cookie.isEmpty()) {
            m_cookie = m_authority.generateCookie();
        }
#ifndef Q_OS_WIN
        // Unix 复用系统 X server：cookie 需并入用户授权库（桩程序测试路径跳过）
        if (m_programOverride.isEmpty()) {
            QString error;
            if (!m_authority.addToSystemAuthority(display, m_cookie, &error)) {
                m_display = -1;
                m_cookie.clear();
                emit startFailed(tr("X11 授权写入失败：%1").arg(error));
                return;
            }
        }
#endif
        emit serverStarted(display);
    });
    connect(m_manager, &ZzXServerManager::crashed, this,
            [this](const QString &message) {
                m_starting = false;
                m_display = -1;
                m_cookie.clear();
                emit serverCrashed(message);
            });
    connect(m_manager, &ZzXServerManager::stopped, this, [this] {
        m_starting = false;
        m_display = -1;
        m_cookie.clear();
        // 快速 toggle 补拉：stop() 收尾期间重开开关时，start() 会被 manager 的
        // 收尾守卫静默拒绝；此处 m_process 已清空，可正常拉起。
        // 无递归风险：Unix 无进程分支 stopped 同步发射时 m_enabled 已为 false，
        // 主动 stop() 路径 m_enabled 同样已先置 false，只有重开场景才进入。
        if (m_enabled) {
            start();
        }
    });
}

void ZzX11Service::setEnabled(bool enabled)
{
    if (m_enabled == enabled) {
        return;
    }
    m_enabled = enabled;
    if (m_enabled) {
        start();
    } else {
        stop();
    }
}

void ZzX11Service::start()
{
    if (!m_enabled || m_starting || m_manager->isRunning()) {
        return;
    }
#if defined(Q_OS_WIN)
    m_starting = true;
    if (!m_downloader) {
        m_downloader = new ZzXServerDownloader(this);
        connect(m_downloader, &ZzXServerDownloader::ready,
                this, &ZzX11Service::onDownloaderReady);
        connect(m_downloader, &ZzXServerDownloader::downloadFailed, this,
                [this](const QString &message) {
                    m_starting = false;
                    emit startFailed(tr("X11 转发不可用：%1").arg(message));
                });
    }
    m_downloader->ensureAvailable();
#else
    if (m_programOverride.isEmpty()) {
#if defined(Q_OS_LINUX)
        // 无本地 X server（纯 Wayland/无头）时提前提示并跳过
        if (qgetenv("DISPLAY").isEmpty()) {
            emit startFailed(tr("X11 转发已跳过：未检测到本地 X server（$DISPLAY 为空）"));
            return;
        }
#elif defined(Q_OS_MAC)
        if (!QFileInfo::exists(QStringLiteral("/tmp/.X11-unix"))) {
            emit startFailed(tr("X11 转发已跳过：未检测到 XQuartz（/tmp/.X11-unix 不存在）"));
            return;
        }
#endif
    }
    m_starting = true;
    m_manager->start(QString(), QString(), 0); // Unix：解析 $DISPLAY 或拉起桩程序
#endif
}

void ZzX11Service::stop()
{
    // 与 ZzXServerManager::stop 语义对齐（立即置为未运行）：display/cookie 同步复位，
    // 进程异步收尾到达的 stopped 处理器再复位一次（幂等）
    m_starting = false;
    m_display = -1;
    m_cookie.clear();
    m_manager->stop();
}

bool ZzX11Service::isRunning() const
{
    return m_manager->isRunning();
}

ZzXLocalEndpoint ZzX11Service::localEndpoint() const
{
    return m_manager->localEndpoint();
}

void ZzX11Service::setServerProgramForTesting(const QString &program)
{
    m_programOverride = program;
    m_manager->setServerProgramForTesting(program);
}

#if defined(Q_OS_WIN)
void ZzX11Service::onDownloaderReady(const QString &executablePath)
{
    if (!m_enabled) {
        m_starting = false; // 下载期间开关被关闭：放弃拉起
        return;
    }
    const int d = ZzXServerManager::allocateDisplay();
    if (d < 0) {
        m_starting = false;
        emit startFailed(tr("X11 转发不可用：无空闲 display 号"));
        return;
    }
    m_cookie = m_authority.generateCookie();
    const QString xauthPath =
        QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
        + QStringLiteral("/xserver/xauth-shared-%1").arg(d);
    if (!m_authority.writeXauthorityFile(xauthPath, d, m_cookie)) {
        m_starting = false;
        m_cookie.clear();
        emit startFailed(tr("X11 授权写入失败：%1").arg(xauthPath));
        return;
    }
    m_manager->start(executablePath, xauthPath, d);
}
#endif
