/**
 * @file ZzPtyUnix.cpp
 * @brief ZzPtyUnix 公开接口实现。
 */

#include "platform/ZzPtyUnix.h"

#include <QSocketNotifier>

#include "platform/ZzPtyUnixPrivate.h"

namespace ZzPlatform {

ZzPtyUnix::ZzPtyUnix(QObject* parent)
    : ZzPtyInterface(parent), d_ptr(std::make_unique<ZzPtyUnixPrivate>())
{
}

ZzPtyUnix::~ZzPtyUnix() = default;

bool ZzPtyUnix::start(const QString& program, const QStringList& arguments,
                      const QSize& initialSize)
{
    QString error;
    if (!d_ptr->spawn(program, arguments, initialSize, error)) {
        emit errorOccurred(error);
        return false;
    }

    d_ptr->notifier = new QSocketNotifier(d_ptr->masterFd, QSocketNotifier::Read, this);
    connect(d_ptr->notifier, &QSocketNotifier::activated, this, &ZzPtyUnix::onReadyRead);
    return true;
}

qint64 ZzPtyUnix::write(const QByteArray& data)
{
    return d_ptr->writeData(data);
}

void ZzPtyUnix::resize(const QSize& size)
{
    d_ptr->setWindowSize(size);
}

void ZzPtyUnix::shutdown()
{
    d_ptr->terminate();
}

bool ZzPtyUnix::isRunning() const
{
    return d_ptr->running;
}

void ZzPtyUnix::onReadyRead()
{
    const QByteArray data = d_ptr->readAvailable();
    if (!data.isEmpty()) {
        emit dataReceived(data);
    }

    const int exitCode = d_ptr->reapIfExited();
    if (exitCode >= 0 || (!d_ptr->running)) {
        if (d_ptr->notifier) {
            d_ptr->notifier->setEnabled(false);
        }
        emit processExited(exitCode < 0 ? 0 : exitCode);
    }
}

}  // namespace ZzPlatform
