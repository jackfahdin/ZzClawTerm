#include "ZzSettingsPage.h"

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
    auto *layout = new QFormLayout(this);

    m_terminalTypeCombo = new QComboBox(this);
    m_terminalTypeCombo->setEditable(true);
    m_terminalTypeCombo->addItems({
        QStringLiteral("xterm-256color"), QStringLiteral("xterm"),
        QStringLiteral("vt100"), QStringLiteral("linux"),
    });
    m_terminalTypeCombo->setCurrentText(m_settings->terminalType());
    layout->addRow(QStringLiteral("终端类型："), m_terminalTypeCombo);

    m_encodingCombo = new QComboBox(this);
    m_encodingCombo->addItems({
        QStringLiteral("UTF-8"), QStringLiteral("GBK"),
        QStringLiteral("GB18030"), QStringLiteral("Big5"),
        QStringLiteral("Shift-JIS"), QStringLiteral("EUC-KR"),
    });
    m_encodingCombo->setCurrentText(m_settings->encoding());
    layout->addRow(QStringLiteral("默认编码："), m_encodingCombo);

    m_fontSizeSpin = new QSpinBox(this);
    m_fontSizeSpin->setRange(6, 32);
    m_fontSizeSpin->setSuffix(QStringLiteral(" pt"));
    m_fontSizeSpin->setValue(m_settings->fontSize());
    layout->addRow(QStringLiteral("字号："), m_fontSizeSpin);

    m_colorSchemeCombo = new QComboBox(this);
    m_colorSchemeCombo->addItems(QTermWidget::availableColorSchemes());
    m_colorSchemeCombo->setCurrentText(m_settings->colorScheme());
    layout->addRow(QStringLiteral("配色方案："), m_colorSchemeCombo);

    m_historyLinesSpin = new QSpinBox(this);
    m_historyLinesSpin->setRange(1000, 100000);
    m_historyLinesSpin->setSingleStep(1000);
    m_historyLinesSpin->setValue(m_settings->historyLines());
    layout->addRow(QStringLiteral("内存历史行数："), m_historyLinesSpin);

    // 凭据后端：auto（密钥环可用则用，否则 AES 文件）/ aes-file / system-keyring
    m_credentialBackendCombo = new QComboBox(this);
    m_credentialBackendCombo->addItem(QStringLiteral("自动（优先系统密钥环）"),
                                      QStringLiteral("auto"));
    m_credentialBackendCombo->addItem(QStringLiteral("AES 加密文件"),
                                      QStringLiteral("aes-file"));
    m_credentialBackendCombo->addItem(QStringLiteral("系统密钥环"),
                                      QStringLiteral("system-keyring"));
    const int backendIndex =
        m_credentialBackendCombo->findData(m_settings->credentialBackend());
    m_credentialBackendCombo->setCurrentIndex(backendIndex >= 0 ? backendIndex : 0);
    m_credentialBackendCombo->setToolTip(
        QStringLiteral("凭据库在应用启动时构造，本项改动重启后生效。\n"
                       "切换到系统密钥环不会自动迁移旧 AES 文件中的凭据（旧文件保留不删）。"));
    layout->addRow(QStringLiteral("凭据后端："), m_credentialBackendCombo);

    m_x11ServerCheck = new QCheckBox(QStringLiteral("启用 X server（启动时自动运行）"), this);
    m_x11ServerCheck->setChecked(m_settings->x11ServerEnabled());
    m_x11ServerCheck->setToolTip(QStringLiteral(
        "关闭后停止内建 X server，新会话不再发起 X11 转发；重新开启即恢复"));
    layout->addRow(QStringLiteral("X11："), m_x11ServerCheck);

    auto *note = new QLabel(
        QStringLiteral("改动立即生效：新标签使用新值，已打开标签实时应用字号/配色/编码。"),
        this);
    note->setWordWrap(true);
    layout->addRow(note);

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
}

QComboBox *ZzSettingsPage::terminalTypeCombo() const { return m_terminalTypeCombo; }
QComboBox *ZzSettingsPage::encodingCombo() const { return m_encodingCombo; }
QSpinBox *ZzSettingsPage::fontSizeSpin() const { return m_fontSizeSpin; }
QComboBox *ZzSettingsPage::colorSchemeCombo() const { return m_colorSchemeCombo; }
QSpinBox *ZzSettingsPage::historyLinesSpin() const { return m_historyLinesSpin; }
QComboBox *ZzSettingsPage::credentialBackendCombo() const { return m_credentialBackendCombo; }
QCheckBox *ZzSettingsPage::x11ServerCheck() const { return m_x11ServerCheck; }
