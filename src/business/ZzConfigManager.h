/**
 * @file ZzConfigManager.h
 * @brief 配置管理: 基于 QSettings 持久化用户偏好。
 */

#ifndef ZZ_CONFIG_MANAGER_H
#define ZZ_CONFIG_MANAGER_H

#include <QFont>
#include <QString>

#include <memory>

namespace ZzBusiness {

class ZzConfigManagerPrivate;

/**
 * @brief 配置管理器。
 */
class ZzConfigManager
{
public:
    ZzConfigManager();
    ~ZzConfigManager();

    ZzConfigManager(const ZzConfigManager&) = delete;
    ZzConfigManager& operator=(const ZzConfigManager&) = delete;

    QString themeName() const;
    void setThemeName(const QString& name);

    QFont terminalFont() const;
    void setTerminalFont(const QFont& font);

    int hotLines() const;
    void setHotLines(int lines);

    /** @brief 立即写盘。 */
    void sync();

private:
    std::unique_ptr<ZzConfigManagerPrivate> d_ptr;
};

}  // namespace ZzBusiness

#endif  // ZZ_CONFIG_MANAGER_H
