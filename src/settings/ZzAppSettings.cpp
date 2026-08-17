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
    m_settings->setValue(QStringLiteral("terminal/encoding"), encoding);
    emit settingsChanged();
}

int ZzAppSettings::fontSize() const
{
    return m_settings->value(QStringLiteral("terminal/fontSize"), 12).toInt();
}

void ZzAppSettings::setFontSize(int fontSize)
{
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
    m_settings->setValue(QStringLiteral("terminal/colorScheme"), colorScheme);
    emit settingsChanged();
}

int ZzAppSettings::historyLines() const
{
    return m_settings->value(QStringLiteral("terminal/historyLines"), 10000).toInt();
}

void ZzAppSettings::setHistoryLines(int lines)
{
    m_settings->setValue(QStringLiteral("terminal/historyLines"), lines);
    emit settingsChanged();
}
