/**
 * @file ZzMainWindowPrivate.h
 * @brief ZzMainWindow 私有实现声明 (Pimpl)。
 */

#ifndef ZZ_MAIN_WINDOW_PRIVATE_H
#define ZZ_MAIN_WINDOW_PRIVATE_H

#include <memory>

#include "business/ZzConfigManager.h"

class QTabWidget;
class QLabel;

namespace ZzBusiness {
class ZzThemeManager;
}

namespace ZzUi {

class ZzTerminalWidget;
class ZzSearchBar;

/**
 * @brief 主窗口内部状态与子控件。
 */
class ZzMainWindowPrivate
{
public:
    QTabWidget* tabs = nullptr;             ///< 标签页容器
    ZzSearchBar* searchBar = nullptr;       ///< 搜索栏
    QLabel* statusLabel = nullptr;          ///< 状态栏行数标签
    ZzBusiness::ZzThemeManager* themes = nullptr;  ///< 主题管理器 (QObject 子对象)
    std::unique_ptr<ZzBusiness::ZzConfigManager> config;  ///< 配置管理器
};

}  // namespace ZzUi

#endif  // ZZ_MAIN_WINDOW_PRIVATE_H
