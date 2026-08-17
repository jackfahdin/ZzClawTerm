#pragma once

#include <QtCore/QString>

class QWidget;

/**
 * @brief Dock 面板抽象（规格 §2.3）：会话面板、未来的 SFTP 面板都实现它。
 *
 * 纯接口（非 QObject），面板实现类同时继承 QDockWidget 与本接口。
 */
class ZzPanelInterface
{
public:
    virtual ~ZzPanelInterface() = default;

    /** @brief 稳定面板标识（如 "sessions"），用于布局持久化键。 */
    [[nodiscard]] virtual QString panelId() const = 0;

    /** @brief 面板显示标题。 */
    [[nodiscard]] virtual QString panelTitle() const = 0;

    /** @brief 面板控件本体（实现类通常返回 this）。 */
    [[nodiscard]] virtual QWidget *panelWidget() = 0;
};
