#include "ZzMockTransport.h"

#include <QtCore/QTimer>

ZzMockTransport::ZzMockTransport(QObject *parent)
    : ZzTransportInterface(parent)
{
}

void ZzMockTransport::open(const ZzTransportEndpoint &endpoint)
{
    ++openCallCount;
    lastEndpoint = endpoint;
    setState(State::Connecting);
    // 下一拍完成，模拟异步连接
    QTimer::singleShot(0, this, [this, host = endpoint.host]() {
        if (host == QStringLiteral("fail")) {
            setState(State::Disconnected);
            emit errorOccurred(1001, QStringLiteral("mock 连接失败"));
            return;
        }
        setState(State::Connected);
    });
}

void ZzMockTransport::write(const QByteArray &data)
{
    writtenData += data;
    if (echoEnabled && state() == State::Connected) {
        emit dataReceived(data);
    }
}

void ZzMockTransport::resize(int cols, int rows)
{
    lastCols = cols;
    lastRows = rows;
}

void ZzMockTransport::close()
{
    ++closeCallCount;
    setState(State::Disconnected);
}

void ZzMockTransport::simulateDisconnect(const QString &reason)
{
    setState(State::Disconnected);
    emit disconnected(reason);
}

void ZzMockTransport::simulateError(int code, const QString &message)
{
    setState(State::Disconnected);
    emit errorOccurred(code, message);
}

void ZzMockTransport::simulateData(const QByteArray &data)
{
    emit dataReceived(data);
}
