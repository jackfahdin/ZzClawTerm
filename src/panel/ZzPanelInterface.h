#pragma once

#include <QtCore/QString>

class QWidget;

/**
 * @brief 侧栏面板抽象（规格 §2.3）：会话面板、SFTP 面板都实现它。
 *
 * 纯接口（非 QObject），面板实现类同时继承 QWidget 与本接口，
 * 由 ZzWorkspaceShell 注册进活动栏侧栏（IDE 工作区，第一期外壳替换）。
 */
class ZzPanelInterface
{
public:
    virtual ~ZzPanelInterface() = default;

    /** @brief 稳定面板标识（如 "sessions"），用于面板登记册与工作区注册。 */
    [[nodiscard]] virtual QString panelId() const = 0;

    /** @brief 面板显示标题。 */
    [[nodiscard]] virtual QString panelTitle() const = 0;

    /** @brief 面板控件本体（实现类通常返回 this）。 */
    [[nodiscard]] virtual QWidget *panelWidget() = 0;
};
