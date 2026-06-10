/**
 * @file ZzPtyWindows.cpp
 * @brief ZzPtyWindows 公开接口实现。
 */

#include "platform/ZzPtyWindows.h"

#include "platform/ZzPtyWindowsPrivate.h"

#if defined(ZZ_PLATFORM_WINDOWS)

namespace ZzPlatform {

ZzPtyWindows::ZzPtyWindows(QObject* parent)
    : ZzPtyInterface(parent), d_ptr(std::make_unique<ZzPtyWindowsPrivate>())
{
}

ZzPtyWindows::~ZzPtyWindows() = default;

bool ZzPtyWindows::start(const QString& program, const QStringList& arguments,
                         const QSize& initialSize)
{
    QString error;
    if (!d_ptr->spawn(program, arguments, initialSize, error)) {
        emit errorOccurred(error);
        return false;
    }

    d_ptr->reader = new ZzPtyReaderThread(d_ptr->outputRead, this);
    connect(d_ptr->reader, &ZzPtyReaderThread::chunkRead, this,
            &ZzPtyWindows::dataReceived, Qt::QueuedConnection);
    connect(d_ptr->reader, &QThread::finished, this, [this]() { emit processExited(0); });
    d_ptr->reader->start();
    return true;
}

qint64 ZzPtyWindows::write(const QByteArray& data)
{
    return d_ptr->writeData(data);
}

void ZzPtyWindows::resize(const QSize& size)
{
    d_ptr->setWindowSize(size);
}

void ZzPtyWindows::shutdown()
{
    d_ptr->terminate();
}

bool ZzPtyWindows::isRunning() const
{
    return d_ptr->running;
}

}  // namespace ZzPlatform

#else   // 非 Windows: 提供空翻译单元避免链接错误。
namespace ZzPlatform {
}
#endif  // ZZ_PLATFORM_WINDOWS
