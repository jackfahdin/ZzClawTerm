#include "ZzTerminalView.h"

#include <QtCore/QStringConverter>
#include <QtWidgets/QVBoxLayout>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"

ZzTerminalView::ZzTerminalView(QWidget *parent)
    : QWidget(parent)
{
    m_term = new QTermWidget(this, this);
    m_term->setScrollBarPosition(QTermWidget::ScrollBarRight);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(m_term, 1);

    // 终端 → 传输（键盘输入方向）
    connect(m_term, &QTermWidget::sendData, this,
            [this](const char *data, int size) {
                if (m_transport) {
                    m_transport->write(QByteArray(data, size));
                }
            });
    // 终端尺寸 → 传输 + 状态栏
    connect(m_term, &QTermWidget::termSizeChange, this,
            [this](int lines, int columns) {
                if (m_transport) {
                    m_transport->resize(columns, lines);
                }
                emit sizeChanged(columns, lines);
            });

    applySettings(ZzAppSettings::instance());
}

void ZzTerminalView::setTransport(ZzTransportInterface *transport)
{
    if (m_transport) {
        disconnect(m_transport, nullptr, this, nullptr);
    }
    m_transport = transport;
    if (!m_transport) {
        return;
    }
    // 传输 → 终端（远端输出方向）
    connect(m_transport, &ZzTransportInterface::dataReceived, this,
            [this](const QByteArray &data) {
                m_term->recvData(data.constData(), data.size());
            });
    connect(m_transport, &ZzTransportInterface::stateChanged, this,
            [this](ZzTransportInterface::State state) {
                emit stateChanged(state);
            });
    connect(m_transport, &ZzTransportInterface::errorOccurred, this,
            [this](int, const QString &message) {
                emit errorOccurred(message);
            });
    connect(m_transport, &ZzTransportInterface::disconnected, this,
            [this](const QString &reason) { emit disconnected(reason); });
}

ZzTransportInterface *ZzTerminalView::transport() const
{
    return m_transport;
}

QTermWidget *ZzTerminalView::termWidget() const
{
    return m_term;
}

void ZzTerminalView::openEndpoint(const ZzTransportEndpoint &endpoint)
{
    m_lastEndpoint = endpoint;
    if (m_transport) {
        m_transport->open(endpoint);
    }
}

QString ZzTerminalView::encoding() const
{
    return m_encoding;
}

ZzTransportInterface::State ZzTerminalView::transportState() const
{
    return m_transport ? m_transport->state()
                       : ZzTransportInterface::State::Disconnected;
}

void ZzTerminalView::applySettings(const ZzAppSettings &settings)
{
    QFont font = m_term->getTerminalFont();
    font.setPointSize(settings.fontSize());
    m_term->setTerminalFont(font);

    m_encoding = settings.encoding();
    const auto encoding =
        QStringConverter::encodingForName(settings.encoding().toUtf8().constData());
    if (encoding) {
        m_term->setTextCodec(QStringEncoder(*encoding));
    }

    if (QTermWidget::availableColorSchemes().contains(settings.colorScheme())) {
        m_term->setColorScheme(settings.colorScheme());
    }
    m_term->setHistorySize(settings.historyLines());
}
