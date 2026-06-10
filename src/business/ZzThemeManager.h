/**
 * @file ZzThemeManager.h
 * @brief 主题管理: 提供内置配色方案并支持切换。
 */

#ifndef ZZ_THEME_MANAGER_H
#define ZZ_THEME_MANAGER_H

#include <QObject>
#include <QString>
#include <QStringList>

#include <memory>

#include "business/ZzColorScheme.h"

namespace ZzBusiness {

class ZzThemeManagerPrivate;

/**
 * @brief 主题管理器。
 */
class ZzThemeManager : public QObject
{
    Q_OBJECT

public:
    explicit ZzThemeManager(QObject* parent = nullptr);
    ~ZzThemeManager() override;

    /** @brief 可用主题名列表。 */
    QStringList availableSchemes() const;

    /** @brief 当前配色方案。 */
    ZzColorScheme currentScheme() const;

    /** @brief 按名切换配色方案。 */
    void setScheme(const QString& name);

signals:
    /** @brief 配色方案发生变化。 */
    void schemeChanged(const ZzColorScheme& scheme);

private:
    std::unique_ptr<ZzThemeManagerPrivate> d_ptr;
};

}  // namespace ZzBusiness

#endif  // ZZ_THEME_MANAGER_H
