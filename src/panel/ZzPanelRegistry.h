#pragma once

#include <QtCore/QList>

class ZzPanelInterface;

/**
 * @brief Dock 面板登记册（规格 §2.3）。
 *
 * v0.1 壳层对内置面板显式停靠；本登记册统一面板身份（panelId 用于布局持久化
 * 与重复注册防护），并为未来插件面板提供同一条接入路径。不获得所有权、
 * 不自动停靠；面板随窗口销毁后由调用方 clear()（ZzAppShell 析构时负责）。
 */
class ZzPanelRegistry final
{
public:
    /** @brief 进程级唯一注册表。 */
    static ZzPanelRegistry &instance();

    /**
     * @brief 注册面板（不获得所有权）。
     * @return 重复 panelId 拒绝并返回 false。
     */
    bool registerPanel(ZzPanelInterface *panel);

    /** @brief 已注册面板列表（注册顺序）。 */
    [[nodiscard]] QList<ZzPanelInterface *> panels() const;

    /** @brief 清空（仅测试使用）。 */
    void clear();

private:
    ZzPanelRegistry() = default;
    QList<ZzPanelInterface *> m_panels;
};
