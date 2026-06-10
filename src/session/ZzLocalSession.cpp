/**
 * @file ZzLocalSession.cpp
 * @brief ZzLocalSession 公开接口实现。
 */

#include "session/ZzLocalSession.h"

#include <QDateTime>
#include <QDir>
#include <QSize>
#include <QStandardPaths>

#include "core/log/ZzLogEngine.h"
#include "core/terminal/ZzTerminalCore.h"
#include "platform/ZzPtyFactory.h"
#include "session/ZzLocalSessionPrivate.h"

namespace ZzSession {

namespace {
/** @brief 返回本次会话的日志数据库路径 (位于应用数据目录)。 */
QString sessionDbPath()
{
    QString base = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    if (base.isEmpty()) {
        base = QDir::homePath() + QStringLiteral("/.zzclawterm");
    }
    const QString logDir = base + QStringLiteral("/logs");
    QDir().mkpath(logDir);
    const QString stamp = QDateTime::currentDateTime().toString(QStringLiteral("yyyyMMdd_hhmmss"));
    return logDir + QStringLiteral("/session_%1.db").arg(stamp);
}
}  // namespace

ZzLocalSession::ZzLocalSession(QObject* parent)
    : QObject(parent), d_ptr(std::make_unique<ZzLocalSessionPrivate>())
{
    d_ptr->terminal = new ZzCore::Terminal::ZzTerminalCore(80, 24, this);
    d_ptr->log = new ZzCore::Log::ZzLogEngine(this);

    // 滚出屏幕的行归档进日志引擎。
    connect(d_ptr->terminal, &ZzCore::Terminal::ZzTerminalCore::lineArchived, d_ptr->log,
            &ZzCore::Log::ZzLogEngine::appendLine);
}

ZzLocalSession::~ZzLocalSession()
{
    if (d_ptr->pty) {
        d_ptr->pty->shutdown();
    }
}

bool ZzLocalSession::start(int cols, int rows)
{
    d_ptr->log->open(sessionDbPath());
    d_ptr->terminal->resize(cols, rows);

    d_ptr->pty = ZzPlatform::ZzPtyFactory::create(this);
    connect(d_ptr->pty.get(), &ZzPlatform::ZzPtyInterface::dataReceived, d_ptr->terminal,
            &ZzCore::Terminal::ZzTerminalCore::processInput);
    connect(d_ptr->pty.get(), &ZzPlatform::ZzPtyInterface::processExited, this,
            &ZzLocalSession::exited);
    connect(d_ptr->pty.get(), &ZzPlatform::ZzPtyInterface::errorOccurred, this,
            &ZzLocalSession::errorOccurred);

    const QString shell = ZzPlatform::ZzPtyFactory::defaultShell();
    return d_ptr->pty->start(shell, {}, QSize(cols, rows));
}

ZzCore::Terminal::ZzTerminalCore* ZzLocalSession::terminal() const
{
    return d_ptr->terminal;
}

ZzCore::Log::ZzLogEngine* ZzLocalSession::log() const
{
    return d_ptr->log;
}

void ZzLocalSession::sendInput(const QByteArray& data)
{
    if (d_ptr->pty) {
        d_ptr->pty->write(data);
    }
}

void ZzLocalSession::resize(int cols, int rows)
{
    d_ptr->terminal->resize(cols, rows);
    if (d_ptr->pty) {
        d_ptr->pty->resize(QSize(cols, rows));
    }
}

}  // namespace ZzSession
