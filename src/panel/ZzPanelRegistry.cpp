#include "ZzPanelRegistry.h"

#include "ZzPanelInterface.h"

ZzPanelRegistry &ZzPanelRegistry::instance()
{
    static ZzPanelRegistry registry;
    return registry;
}

bool ZzPanelRegistry::registerPanel(ZzPanelInterface *panel)
{
    if (panel == nullptr) {
        return false;
    }
    for (const ZzPanelInterface *existing : m_panels) {
        if (existing->panelId() == panel->panelId()) {
            return false;
        }
    }
    m_panels.append(panel);
    return true;
}

QList<ZzPanelInterface *> ZzPanelRegistry::panels() const
{
    return m_panels;
}

void ZzPanelRegistry::clear()
{
    m_panels.clear();
}
