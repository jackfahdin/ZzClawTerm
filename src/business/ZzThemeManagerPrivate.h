/**
 * @file ZzThemeManagerPrivate.h
 * @brief ZzThemeManager 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_THEME_MANAGER_PRIVATE_H
#define ZZ_THEME_MANAGER_PRIVATE_H

#include <QHash>
#include <QString>

#include "business/ZzColorScheme.h"

namespace ZzBusiness {

/**
 * @brief 主题管理器内部状态。
 */
class ZzThemeManagerPrivate
{
public:
    QHash<QString, ZzColorScheme> schemes;  ///< 名称 → 配色
    QString current;                        ///< 当前主题名
};

}  // namespace ZzBusiness

#endif  // ZZ_THEME_MANAGER_PRIVATE_H
