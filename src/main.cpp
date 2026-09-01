#include <cstdlib>
#include <memory>
#include <utility>

#include <QtCore/QCoreApplication>

#include <ZzFluentUI/ZzNavigationPlacement.h>
#include <ZzPureTools/ZzApplicationBuilder.h>
#include <ZzPureTools/ZzApplicationWindow.h>
#include <ZzPureTools/ZzNavigationNode.h>
#include <ZzPureTools/ZzPageLifetimePolicy.h>
#include <ZzPureTools/ZzPageRegistration.h>
#include <ZzPureTools/ZzPureApplication.h>
#include <ZzPureTools/ZzRouteId.h>
#include <ZzPureTools/ZzWorkspaceShell.h>
#include <ZzWindowKit/ZzWindowKitBootstrap.h>

#include "ZzAppShell.h"
#include "ZzClawTermModule.h"
#include "transport/ZzLocalPtyTransport.h"
#include "transport/ZzSshTransport.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 应用入口：框架 bootstrap → 注册传输协议 → 装配页面/导航/窗口回调。
 */
int main(int argc, char *argv[])
{
    const auto bootstrap = ZzWindowKit::ZzWindowKitBootstrap::prepare();
    if (!bootstrap) {
        return EXIT_FAILURE;
    }

    ZzPureTools::ZzPureApplication application(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("ZzClawTerm"));
    QCoreApplication::setOrganizationName(QStringLiteral("ZzClaw"));

    // 内置传输协议注册（规格 §2.3：与未来第三方插件同一条注册路径）
    auto &transports = ZzTransportRegistry::instance();
    transports.registerTransport(QStringLiteral("ssh"),
        [](QObject *parent) -> ZzTransportInterface * {
            return new ZzSshTransport(parent);
        });
    transports.registerTransport(QStringLiteral("local"),
        [](QObject *parent) -> ZzTransportInterface * {
            return new ZzLocalPtyTransport(parent);
        });

    ZzAppShell shell;

    ZzPureTools::ZzApplicationBuilder builder;
    if (!builder.addModule(std::make_unique<ZzClawTermModule>())) {
        return EXIT_FAILURE;
    }

    const ZzPureTools::ZzRouteId terminalRoute(QStringLiteral("terminal"));
    const ZzPureTools::ZzRouteId settingsRoute(QStringLiteral("settings"));

    ZzPureTools::ZzPageRegistration terminalPage;
    terminalPage.routeId = terminalRoute;
    terminalPage.lifetime = ZzPureTools::ZzPageLifetimePolicy::Persistent;
    terminalPage.factory = [&shell](QWidget *pageParent) {
        return shell.createTerminalPage(pageParent);
    };
    if (!builder.addPage(std::move(terminalPage))) {
        return EXIT_FAILURE;
    }

    ZzPureTools::ZzPageRegistration settingsPage;
    settingsPage.routeId = settingsRoute;
    settingsPage.lifetime = ZzPureTools::ZzPageLifetimePolicy::Persistent;
    settingsPage.factory = [&shell](QWidget *pageParent) {
        return shell.createSettingsPage(pageParent);
    };
    if (!builder.addPage(std::move(settingsPage))) {
        return EXIT_FAILURE;
    }

    ZzPureTools::ZzNavigationNode terminalNode{
        terminalRoute, QStringLiteral("ZzClawTerm"),
        QStringLiteral("Terminal"), {}};
    if (!builder.addNavigationNode(std::move(terminalNode))) {
        return EXIT_FAILURE;
    }
    ZzPureTools::ZzNavigationNode settingsNode{
        settingsRoute, QStringLiteral("ZzClawTerm"),
        QStringLiteral("Settings"), {}};
    settingsNode.placement = ZzFluentUI::ZzNavigationPlacement::Footer;
    if (!builder.addNavigationNode(std::move(settingsNode))) {
        return EXIT_FAILURE;
    }

    if (!builder.setInitialRoute(terminalRoute)
        || !builder.setWindowSetupCallback(
            [&shell](ZzPureTools::ZzApplicationWindow &window) {
                return shell.assemble(window);
            })
        || !builder.build(application)) {
        return EXIT_FAILURE;
    }

    // 框架窗口初始化在 assemble() 之后还会执行 refreshTranslations()，把窗口与
    // Fluent 标题栏标题重置为库默认「ZzPureTools」（ZzApplicationWindowPrivate），
    // 覆盖装配期设置的标题。build 完成后重放一次标题刷新夺回设置（setApplicationTitle
    // 无条件触发 refreshTitle，幂等）。
    shell.workspaceShell()->setApplicationTitle(
        QStringLiteral("ZzClawTerm"));

    return application.exec();
}
