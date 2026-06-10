/**
 * @file ZzThemeManager.cpp
 * @brief ZzThemeManager 公开接口实现。
 */

#include "business/ZzThemeManager.h"

#include "business/ZzThemeManagerPrivate.h"

namespace ZzBusiness {

ZzThemeManager::ZzThemeManager(QObject* parent)
    : QObject(parent), d_ptr(std::make_unique<ZzThemeManagerPrivate>())
{
    const ZzColorScheme dracula = zzDraculaScheme();
    const ZzColorScheme nord = zzNordScheme();
    d_ptr->schemes.insert(dracula.name, dracula);
    d_ptr->schemes.insert(nord.name, nord);
    d_ptr->current = dracula.name;
}

ZzThemeManager::~ZzThemeManager() = default;

QStringList ZzThemeManager::availableSchemes() const
{
    return d_ptr->schemes.keys();
}

ZzColorScheme ZzThemeManager::currentScheme() const
{
    return d_ptr->schemes.value(d_ptr->current, zzDraculaScheme());
}

void ZzThemeManager::setScheme(const QString& name)
{
    if (d_ptr->schemes.contains(name) && name != d_ptr->current) {
        d_ptr->current = name;
        emit schemeChanged(currentScheme());
    }
}

}  // namespace ZzBusiness
