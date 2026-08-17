#include "ZzTerminalView.h"

#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QStandardPaths>
#include <QtCore/QStringConverter>
#include <QtWidgets/QVBoxLayout>

#include "qtermwidget.h"
#include "log/ZzLogEngine.h"
#include "settings/ZzAppSettings.h"
#include "terminal/ZzScrollbackBridge.h"

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

ZzScrollbackBridge *ZzTerminalView::scrollbackBridge() const
{
    return m_scrollbackBridge;
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

void ZzTerminalView::enableScrollback(const QString &sessionId)
{
    if (m_scrollbackBridge) {
        return; // 每会话只建一次
    }
    // 温层文件落在应用配置目录下按会话 ID 区分（QUuid 字符串，文件名安全）
    const QString dirPath =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        + QStringLiteral("/scrollback");
    QDir().mkpath(dirPath);
    const QString warmPath = dirPath + QLatin1Char('/') + sessionId
                             + QStringLiteral(".warm");
    // 新会话显示层历史基线恒从 0 起（QTermWidget 侧绝对行号口径），温层必须同步
    // 从空开始；否则跨重启恢复的引擎行号会让读回命中上一会话的行（错行）
    QFile::remove(warmPath);
    ZzLogEngine::Config config;
    config.warmFilePath = warmPath;
    auto *engine = new ZzLogEngine(config, this);
    m_scrollbackBridge = new ZzScrollbackBridge(m_term, engine, this);
    // 降级 → 状态栏提示（经 errorOccurred 同一路径到 ZzTabManager::statusMessage）
    connect(m_scrollbackBridge, &ZzScrollbackBridge::degraded, this,
            [this](const QString &reason) {
                emit errorOccurred(QStringLiteral("滚动历史已降级为内存模式：%1")
                                       .arg(reason));
            });
    // open 必须最后调用：温层打开失败时它会同步发射 degradedToMemoryOnly，
    // 先于桥与提示链路接线调用会丢失该降级提示（规格 §八）
    engine->open();
}
