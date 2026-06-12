/**
 * @file ZzPtyFactory.cpp
 * @brief PTY 工厂实现。
 */

#include "platform/ZzPtyFactory.h"

#include <QProcessEnvironment>

#if defined(ZZ_PLATFORM_WINDOWS)
#include "platform/ZzPtyWindows.h"
#else
#include "platform/ZzPtyUnix.h"
#endif

namespace ZzPlatform {

std::unique_ptr<ZzPtyInterface> ZzPtyFactory::create(QObject* parent)
{
#if defined(ZZ_PLATFORM_WINDOWS)
    return std::make_unique<ZzPtyWindows>(parent);
#else
    return std::make_unique<ZzPtyUnix>(parent);
#endif
}

QString ZzPtyFactory::defaultShell()
{
#if defined(ZZ_PLATFORM_WINDOWS)
    const QString systemRoot =
        QProcessEnvironment::systemEnvironment().value(QStringLiteral("SystemRoot"),
                                                       QStringLiteral("C:\\Windows"));
    // [诊断] 临时改用 cmd.exe 以区分 PowerShell/PSReadLine 与 ConPTY 输入问题。
    return systemRoot + QStringLiteral("\\System32\\cmd.exe");
#elif defined(ZZ_PLATFORM_MACOS)
    return QStringLiteral("/bin/zsh");
#else
    const QString shell = QProcessEnvironment::systemEnvironment().value(QStringLiteral("SHELL"));
    return shell.isEmpty() ? QStringLiteral("/bin/bash") : shell;
#endif
}

}  // namespace ZzPlatform
