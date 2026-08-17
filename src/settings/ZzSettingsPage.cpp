#include "ZzSettingsPage.h"

#include <QtWidgets/QComboBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLabel>
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

    auto *note = new QLabel(
        QStringLiteral("改动立即生效：新标签使用新值，已打开标签实时应用字号/配色/编码。"),
        this);
    note->setWordWrap(true);
    layout->addRow(note);

    // 即改即存
    connect(m_terminalTypeCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setTerminalType);
    connect(m_encodingCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setEncoding);
    connect(m_fontSizeSpin, &QSpinBox::valueChanged,
            m_settings, &ZzAppSettings::setFontSize);
    connect(m_colorSchemeCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setColorScheme);
    connect(m_historyLinesSpin, &QSpinBox::valueChanged,
            m_settings, &ZzAppSettings::setHistoryLines);
}

QComboBox *ZzSettingsPage::terminalTypeCombo() const { return m_terminalTypeCombo; }
QComboBox *ZzSettingsPage::encodingCombo() const { return m_encodingCombo; }
QSpinBox *ZzSettingsPage::fontSizeSpin() const { return m_fontSizeSpin; }
QComboBox *ZzSettingsPage::colorSchemeCombo() const { return m_colorSchemeCombo; }
QSpinBox *ZzSettingsPage::historyLinesSpin() const { return m_historyLinesSpin; }
