/**
 * @file ZzConfigManagerPrivate.h
 * @brief ZzConfigManager 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_CONFIG_MANAGER_PRIVATE_H
#define ZZ_CONFIG_MANAGER_PRIVATE_H

#include <QSettings>

#include <memory>

namespace ZzBusiness {

/**
 * @brief 配置管理器内部状态。
 */
class ZzConfigManagerPrivate
{
public:
    std::unique_ptr<QSettings> settings;  ///< 底层 QSettings
};

}  // namespace ZzBusiness

#endif  // ZZ_CONFIG_MANAGER_PRIVATE_H
