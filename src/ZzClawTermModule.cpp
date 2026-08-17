#include "ZzClawTermModule.h"

ZzPureTools::ZzModuleDescriptor ZzClawTermModule::descriptor() const
{
    return ZzPureTools::ZzModuleDescriptor{
        ZzPureTools::ZzModuleId(QStringLiteral("com.zzclawterm.app")),
        QStringLiteral("0.1.0"),
        {}};
}

ZzCore::ZzResult<void> ZzClawTermModule::start()
{
    m_started = true;
    return ZzCore::ZzResult<void>::success();
}

void ZzClawTermModule::requestStop() noexcept
{
    m_started = false;
}

void ZzClawTermModule::stop() noexcept
{
    m_started = false;
}
