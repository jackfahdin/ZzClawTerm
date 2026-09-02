#include "ZzTerminalView.h"

#include <atomic>

#include <QtCore/QDir>
#include <QtCore/QEvent>
#include <QtCore/QStandardPaths>
#include <QtCore/QStringConverter>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>
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

    // 标签内错误横幅：默认隐藏（规格 §八：错误走标签内提示，不弹窗轰炸）
    m_errorBanner = new QWidget(this);
    m_errorBanner->setObjectName(QStringLiteral("zzErrorBanner"));
    m_errorBanner->setStyleSheet(
        QStringLiteral("#zzErrorBanner { background: #4a2b2b; color: #ffd7d7; }"));
    auto *bannerLayout = new QHBoxLayout(m_errorBanner);
    bannerLayout->setContentsMargins(8, 4, 8, 4);
    m_errorLabel = new QLabel(m_errorBanner);
    m_errorLabel->setWordWrap(true);
    m_retryButton = new QPushButton(m_errorBanner);
    m_retryButton->setObjectName(QStringLiteral("zzRetryButton"));
    bannerLayout->addWidget(m_errorLabel, 1);
    bannerLayout->addWidget(m_retryButton);
    m_errorBanner->hide();
    connect(m_retryButton, &QPushButton::clicked, this, [this]() {
        hideErrorBanner();
        if (m_transport) {
            m_transport->open(m_lastEndpoint); // 用记忆的参数重试
        }
    });
    layout->addWidget(m_errorBanner);
    layout->addWidget(m_term, 1);

    retranslateUi();

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
                if (state == ZzTransportInterface::State::Connected) {
                    hideErrorBanner();
                }
                emit stateChanged(state);
            });
    connect(m_transport, &ZzTransportInterface::errorOccurred, this,
            [this](int, const QString &message) {
                showErrorBanner(message);
                emit errorOccurred(message);
            });
    connect(m_transport, &ZzTransportInterface::disconnected, this,
            [this](const QString &reason) { emit disconnected(reason); });
    connect(m_transport, &ZzTransportInterface::tunnelCountChanged, this,
            &ZzTerminalView::tunnelCountChanged);
    connect(m_transport, &ZzTransportInterface::statusNotice, this,
            &ZzTerminalView::statusNotice);
}

ZzTransportInterface *ZzTerminalView::transport() const
{
    return m_transport;
}

QWidget *ZzTerminalView::errorBanner() const { return m_errorBanner; }
QLabel *ZzTerminalView::errorLabel() const { return m_errorLabel; }
QPushButton *ZzTerminalView::retryButton() const { return m_retryButton; }

void ZzTerminalView::retranslateUi()
{
    m_retryButton->setText(tr("重试"));
}

void ZzTerminalView::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QWidget::changeEvent(event);
}

void ZzTerminalView::showErrorBanner(const QString &message)
{
    m_errorLabel->setText(message);
    m_errorBanner->show();
}

void ZzTerminalView::hideErrorBanner()
{
    m_errorBanner->hide();
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
    // 温层文件落在应用配置目录下按会话 ID 区分（QUuid 字符串，文件名安全）。
    // 同一 profile 可同时开多个标签（sessionId 相同），必须追加每标签唯一后缀，
    // 否则后开标签与先开标签共用同一温层文件路径，互相干扰
    static std::atomic<int> s_warmSequence = 0;
    const QString dirPath =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        + QStringLiteral("/scrollback");
    QDir().mkpath(dirPath);
    const QString warmPath = dirPath + QLatin1Char('/') + sessionId
                             + QLatin1Char('-')
                             + QString::number(s_warmSequence.fetch_add(1))
                             + QStringLiteral(".warm");
    // 温层残留文件不再由本层预删：冷层模式下引擎 open() 会按文件头游标完成
    // 崩溃恢复续传后删除残留并创建全新空温层；冷层不可用时引擎降级为温层模式
    // 并自行删除残留全新开始（与 v0.1 行为一致，新会话显示层行号仍从 0 起）
    ZzLogEngine::Config config;
    config.warmFilePath = warmPath;
    config.coldDbPath = dirPath + QStringLiteral("/cold.db"); // 全局单库：所有会话共享
    config.sessionId = sessionId;                             // 会话 profile id（冷层行归属）
    auto *engine = new ZzLogEngine(config, this);
    m_scrollbackBridge = new ZzScrollbackBridge(m_term, engine, this);
    // 降级 → 状态栏提示（经 errorOccurred 同一路径到 ZzTabManager::statusMessage）
    connect(m_scrollbackBridge, &ZzScrollbackBridge::degraded, this,
            [this](const QString &reason) {
                emit errorOccurred(tr("滚动历史已降级为内存模式：%1")
                                       .arg(reason));
            });
    // 冷层降级 → 状态栏提示（与温层降级同一路径，不打断终端）
    connect(engine, &ZzLogEngine::degradedToWarmOnly, this,
            [this](const QString &reason) {
                emit errorOccurred(tr("滚动历史已降级为温层模式：%1")
                                       .arg(reason));
            });
    // open 必须最后调用：温层打开失败时它会同步发射 degradedToMemoryOnly，
    // 先于桥与提示链路接线调用会丢失该降级提示（规格 §八）
    engine->open();
}
