/**
 * @file ZzConfigManager.cpp
 * @brief ZzConfigManager 公开接口实现。
 */

#include "business/ZzConfigManager.h"

#include "business/ZzConfigManagerPrivate.h"

namespace ZzBusiness {

ZzConfigManager::ZzConfigManager() : d_ptr(std::make_unique<ZzConfigManagerPrivate>())
{
    d_ptr->settings = std::make_unique<QSettings>(QStringLiteral("ZzClawTerm"),
                                                  QStringLiteral("ZzClawTerm"));
}

ZzConfigManager::~ZzConfigManager() = default;

QString ZzConfigManager::themeName() const
{
    return d_ptr->settings->value(QStringLiteral("appearance/theme"), QStringLiteral("Dracula"))
        .toString();
}

void ZzConfigManager::setThemeName(const QString& name)
{
    d_ptr->settings->setValue(QStringLiteral("appearance/theme"), name);
}

QFont ZzConfigManager::terminalFont() const
{
    QFont fallback(QStringLiteral("monospace"), 12);
    fallback.setStyleHint(QFont::Monospace);
    return qvariant_cast<QFont>(
        d_ptr->settings->value(QStringLiteral("appearance/font"), fallback));
}

void ZzConfigManager::setTerminalFont(const QFont& font)
{
    d_ptr->settings->setValue(QStringLiteral("appearance/font"), font);
}

int ZzConfigManager::hotLines() const
{
    return d_ptr->settings->value(QStringLiteral("log/hotLines"), 10000).toInt();
}

void ZzConfigManager::setHotLines(int lines)
{
    d_ptr->settings->setValue(QStringLiteral("log/hotLines"), lines);
}

void ZzConfigManager::sync()
{
    d_ptr->settings->sync();
}

}  // namespace ZzBusiness
