#include "ZzTransportRegistry.h"

#include <utility>

#include "ZzTransportInterface.h"

ZzTransportRegistry &ZzTransportRegistry::instance()
{
    static ZzTransportRegistry registry;
    return registry;
}

bool ZzTransportRegistry::registerTransport(const QString &scheme,
                                            ZzTransportFactory factory)
{
    if (scheme.isEmpty() || !factory || m_factories.contains(scheme)) {
        return false;
    }
    m_factories.insert(scheme, std::move(factory));
    return true;
}

ZzTransportInterface *ZzTransportRegistry::create(const QString &scheme,
                                                  QObject *parent)
{
    const auto it = m_factories.constFind(scheme);
    if (it == m_factories.constEnd()) {
        return nullptr;
    }
    return it.value()(parent);
}

QStringList ZzTransportRegistry::schemes() const
{
    return m_factories.keys();
}

void ZzTransportRegistry::clear()
{
    m_factories.clear();
}
