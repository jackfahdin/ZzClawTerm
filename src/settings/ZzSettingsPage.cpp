#include "ZzSettingsPage.h"

#include <QtCore/QEvent>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QComboBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QSpinBox>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"

ZzSettingsPage::ZzSettingsPage(ZzAppSettings *settings, QWidget *parent)
    : QWidget(parent)
    , m_settings(settings)
{
    m_formLayout = new QFormLayout(this);

    // 行标签统一显式创建空 QLabel：QFormLayout 字符串重载收到空串时不建
    // 标签部件，retranslateUi 的 labelForField 反查会落空（标签随后回填）

    m_terminalTypeCombo = new QComboBox(this);
    m_terminalTypeCombo->setEditable(true);
    m_terminalTypeCombo->addItems({
        QStringLiteral("xterm-256color"), QStringLiteral("xterm"),
        QStringLiteral("vt100"), QStringLiteral("linux"),
    });
    m_terminalTypeCombo->setCurrentText(m_settings->terminalType());
    m_formLayout->addRow(new QLabel(this), m_terminalTypeCombo);

    m_encodingCombo = new QComboBox(this);
    m_encodingCombo->addItems({
        QStringLiteral("UTF-8"), QStringLiteral("GBK"),
        QStringLiteral("GB18030"), QStringLiteral("Big5"),
        QStringLiteral("Shift-JIS"), QStringLiteral("EUC-KR"),
    });
    m_encodingCombo->setCurrentText(m_settings->encoding());
    m_formLayout->addRow(new QLabel(this), m_encodingCombo);

    m_fontSizeSpin = new QSpinBox(this);
    m_fontSizeSpin->setRange(6, 32);
    m_fontSizeSpin->setSuffix(QStringLiteral(" pt"));
    m_fontSizeSpin->setValue(m_settings->fontSize());
    m_formLayout->addRow(new QLabel(this), m_fontSizeSpin);

    m_colorSchemeCombo = new QComboBox(this);
    m_colorSchemeCombo->addItems(QTermWidget::availableColorSchemes());
    m_colorSchemeCombo->setCurrentText(m_settings->colorScheme());
    m_formLayout->addRow(new QLabel(this), m_colorSchemeCombo);

    m_historyLinesSpin = new QSpinBox(this);
    m_historyLinesSpin->setRange(1000, 100000);
    m_historyLinesSpin->setSingleStep(1000);
    m_historyLinesSpin->setValue(m_settings->historyLines());
    m_formLayout->addRow(new QLabel(this), m_historyLinesSpin);

    // 凭据后端：auto（密钥环可用则用，否则 AES 文件）/ aes-file / system-keyring
    // 显示文本集中在 retranslateUi()（setItemText 保留 itemData 与当前选中）
    m_credentialBackendCombo = new QComboBox(this);
    m_credentialBackendCombo->addItem(QString(), QStringLiteral("auto"));
    m_credentialBackendCombo->addItem(QString(), QStringLiteral("aes-file"));
    m_credentialBackendCombo->addItem(QString(), QStringLiteral("system-keyring"));
    const int backendIndex =
        m_credentialBackendCombo->findData(m_settings->credentialBackend());
    m_credentialBackendCombo->setCurrentIndex(backendIndex >= 0 ? backendIndex : 0);
    m_formLayout->addRow(new QLabel(this), m_credentialBackendCombo);

    m_x11ServerCheck = new QCheckBox(this);
    m_x11ServerCheck->setChecked(m_settings->x11ServerEnabled());
    m_formLayout->addRow(new QLabel(this), m_x11ServerCheck);

    // SFTP 块大小：itemData 存字节数，0=自动（BDP 自适应，M6）
    m_sftpBlockSizeCombo = new QComboBox(this);
    m_sftpBlockSizeCombo->addItem(QString(), 0); // 文本见 retranslateUi()
    for (int kb : {64, 128, 256, 512, 1024, 2048, 4096}) {
        m_sftpBlockSizeCombo->addItem(
            kb >= 1024 ? QStringLiteral("%1 MB").arg(kb / 1024)
                       : QStringLiteral("%1 KB").arg(kb),
            kb * 1024);
    }
    const int bsIndex = m_sftpBlockSizeCombo->findData(m_settings->sftpBlockSize());
    m_sftpBlockSizeCombo->setCurrentIndex(bsIndex >= 0 ? bsIndex : 0);
    m_formLayout->addRow(new QLabel(this), m_sftpBlockSizeCombo);

    m_noteLabel = new QLabel(this);
    m_noteLabel->setWordWrap(true);
    m_formLayout->addRow(m_noteLabel);

    retranslateUi();

    // 即改即存
    // 终端类型 combo 可编辑：currentTextChanged 逐键触发会写盘 "x"/"xt"/...
    // 并引发全标签重应用；仅在下拉选择或编辑完成（回车/失焦）时提交
    const auto commitTerminalType = [this]() {
        m_settings->setTerminalType(m_terminalTypeCombo->currentText());
    };
    connect(m_terminalTypeCombo, &QComboBox::activated,
            this, commitTerminalType);
    connect(m_terminalTypeCombo->lineEdit(), &QLineEdit::editingFinished,
            this, commitTerminalType);
    connect(m_encodingCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setEncoding);
    connect(m_fontSizeSpin, &QSpinBox::valueChanged,
            m_settings, &ZzAppSettings::setFontSize);
    connect(m_colorSchemeCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setColorScheme);
    connect(m_historyLinesSpin, &QSpinBox::valueChanged,
            m_settings, &ZzAppSettings::setHistoryLines);
    connect(m_credentialBackendCombo, &QComboBox::activated, this, [this](int index) {
        m_settings->setCredentialBackend(
            m_credentialBackendCombo->itemData(index).toString());
    });
    connect(m_x11ServerCheck, &QCheckBox::toggled,
            m_settings, &ZzAppSettings::setX11ServerEnabled);
    // 用 currentIndexChanged 而非 activated：测试经 setCurrentIndex 驱动，
    // 且二者都不含程序性回显误写（setter 同值短路兜底）
    connect(m_sftpBlockSizeCombo, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_settings->setSftpBlockSize(m_sftpBlockSizeCombo->itemData(index).toInt());
    });
}

void ZzSettingsPage::retranslateUi()
{
    // 行标签经 labelForField 按字段部件反查，不依赖行序号
    const auto setRowLabel = [this](QWidget *field, const QString &text) {
        if (auto *label =
                qobject_cast<QLabel *>(m_formLayout->labelForField(field))) {
            label->setText(text);
        }
    };
    setRowLabel(m_terminalTypeCombo, tr("终端类型："));
    setRowLabel(m_encodingCombo, tr("默认编码："));
    setRowLabel(m_fontSizeSpin, tr("字号："));
    setRowLabel(m_colorSchemeCombo, tr("配色方案："));
    setRowLabel(m_historyLinesSpin, tr("内存历史行数："));
    setRowLabel(m_credentialBackendCombo, tr("凭据后端："));
    setRowLabel(m_x11ServerCheck, tr("X11："));
    setRowLabel(m_sftpBlockSizeCombo, tr("SFTP 块大小："));

    m_credentialBackendCombo->setItemText(0, tr("自动（优先系统密钥环）"));
    m_credentialBackendCombo->setItemText(1, tr("AES 加密文件"));
    m_credentialBackendCombo->setItemText(2, tr("系统密钥环"));
    m_credentialBackendCombo->setToolTip(
        tr("凭据库在应用启动时构造，本项改动重启后生效。\n"
           "切换到系统密钥环不会自动迁移旧 AES 文件中的凭据（旧文件保留不删）。"));

    m_x11ServerCheck->setText(tr("启用 X server（启动时自动运行）"));
    m_x11ServerCheck->setToolTip(
        tr("关闭后停止内建 X server，新会话不再发起 X11 转发；重新开启即恢复"));

    m_sftpBlockSizeCombo->setItemText(0, tr("自动（BDP 自适应）"));
    m_sftpBlockSizeCombo->setToolTip(
        tr("手动值对高延迟链路可能更优；自动模式按链路 RTT 自适应（推荐）。\n"
           "进行中的传输不受影响，下一传输生效。"));

    m_noteLabel->setText(
        tr("改动立即生效：新标签使用新值，已打开标签实时应用字号/配色/编码。"));
}

void ZzSettingsPage::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QWidget::changeEvent(event);
}

QComboBox *ZzSettingsPage::terminalTypeCombo() const { return m_terminalTypeCombo; }
QComboBox *ZzSettingsPage::encodingCombo() const { return m_encodingCombo; }
QSpinBox *ZzSettingsPage::fontSizeSpin() const { return m_fontSizeSpin; }
QComboBox *ZzSettingsPage::colorSchemeCombo() const { return m_colorSchemeCombo; }
QSpinBox *ZzSettingsPage::historyLinesSpin() const { return m_historyLinesSpin; }
QComboBox *ZzSettingsPage::credentialBackendCombo() const { return m_credentialBackendCombo; }
QCheckBox *ZzSettingsPage::x11ServerCheck() const { return m_x11ServerCheck; }
QComboBox *ZzSettingsPage::sftpBlockSizeCombo() const { return m_sftpBlockSizeCombo; }
