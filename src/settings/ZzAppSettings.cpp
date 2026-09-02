#include "ZzAppSettings.h"

#include <QtCore/QSettings>
#include <QtCore/QStandardPaths>

ZzAppSettings::ZzAppSettings(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_settings(new QSettings(filePath, QSettings::IniFormat, this))
{
}

ZzAppSettings &ZzAppSettings::instance()
{
    static ZzAppSettings settings(
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        + QStringLiteral("/settings.ini"));
    return settings;
}

QString ZzAppSettings::terminalType() const
{
    return m_settings->value(QStringLiteral("terminal/type"),
                             QStringLiteral("xterm-256color")).toString();
}

void ZzAppSettings::setTerminalType(const QString &terminalType)
{
    if (this->terminalType() == terminalType) {
        return; // 同值短路：值未变化不发射，避免放大无效重应用
    }
    m_settings->setValue(QStringLiteral("terminal/type"), terminalType);
    emit settingsChanged();
}

QString ZzAppSettings::encoding() const
{
    return m_settings->value(QStringLiteral("terminal/encoding"),
                             QStringLiteral("UTF-8")).toString();
}

void ZzAppSettings::setEncoding(const QString &encoding)
{
    if (this->encoding() == encoding) {
        return;
    }
    m_settings->setValue(QStringLiteral("terminal/encoding"), encoding);
    emit settingsChanged();
}

int ZzAppSettings::fontSize() const
{
    return m_settings->value(QStringLiteral("terminal/fontSize"), 12).toInt();
}

void ZzAppSettings::setFontSize(int fontSize)
{
    if (this->fontSize() == fontSize) {
        return;
    }
    m_settings->setValue(QStringLiteral("terminal/fontSize"), fontSize);
    emit settingsChanged();
}

QString ZzAppSettings::colorScheme() const
{
    return m_settings->value(QStringLiteral("terminal/colorScheme"),
                             QStringLiteral("Linux")).toString();
}

void ZzAppSettings::setColorScheme(const QString &colorScheme)
{
    if (this->colorScheme() == colorScheme) {
        return;
    }
    m_settings->setValue(QStringLiteral("terminal/colorScheme"), colorScheme);
    emit settingsChanged();
}

int ZzAppSettings::historyLines() const
{
    return m_settings->value(QStringLiteral("terminal/historyLines"), 10000).toInt();
}

void ZzAppSettings::setHistoryLines(int lines)
{
    if (historyLines() == lines) {
        return;
    }
    m_settings->setValue(QStringLiteral("terminal/historyLines"), lines);
    emit settingsChanged();
}

QString ZzAppSettings::credentialBackend() const
{
    return m_settings->value(QStringLiteral("credential/backend"),
                             QStringLiteral("auto")).toString();
}

void ZzAppSettings::setCredentialBackend(const QString &backend)
{
    if (credentialBackend() == backend) {
        return;
    }
    m_settings->setValue(QStringLiteral("credential/backend"), backend);
    emit settingsChanged();
}

bool ZzAppSettings::x11ServerEnabled() const
{
    return m_settings->value(QStringLiteral("x11/serverEnabled"), true).toBool();
}

void ZzAppSettings::setX11ServerEnabled(bool enabled)
{
    if (x11ServerEnabled() == enabled) {
        return;
    }
    m_settings->setValue(QStringLiteral("x11/serverEnabled"), enabled);
    emit settingsChanged();
}

QString ZzAppSettings::language() const
{
    return m_settings->value(QStringLiteral("app/language"),
                             QStringLiteral("system")).toString();
}

void ZzAppSettings::setLanguage(const QString &language)
{
    if (this->language() == language) {
        return;
    }
    m_settings->setValue(QStringLiteral("app/language"), language);
    emit settingsChanged();
}

int ZzAppSettings::sftpBlockSize() const
{
    return m_settings->value(QStringLiteral("sftp/blockSize"), 0).toInt();
}

void ZzAppSettings::setSftpBlockSize(int bytes)
{
    if (sftpBlockSize() == bytes) {
        return;
    }
    m_settings->setValue(QStringLiteral("sftp/blockSize"), bytes);
    emit settingsChanged();
}

QByteArray ZzAppSettings::workspaceLayout() const
{
    return m_settings->value(QStringLiteral("workspace/layout")).toByteArray();
}

void ZzAppSettings::setWorkspaceLayout(const QByteArray &layout)
{
    if (workspaceLayout() == layout) {
        return;
    }
    m_settings->setValue(QStringLiteral("workspace/layout"), layout);
    emit settingsChanged();
}
