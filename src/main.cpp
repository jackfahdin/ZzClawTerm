#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <winsock2.h>
#include <windows.h>
#endif

#include <cstdlib>
#include <memory>
#include <utility>

#include <QtCore/QAbstractNativeEventFilter>
#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtGui/QContextMenuEvent>
#include <QtGui/QGuiApplication>
#include <QtGui/QMouseEvent>
#include <QtGui/QStyleHints>

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

namespace {

/**
 * @brief 第二轮诊断探针：应用级事件 + Windows 原生消息双路记录右键链路。
 *
 * 背景：Windows 真机会话树右键无菜单，且第一轮埋点显示右键按下/抬起
 * 事件未到达树控件事件过滤器。本探针用于定位断裂位置：
 * - nativeEventFilter 记录 WM_RBUTTONDOWN/UP/DBLCLK/CONTEXTMENU 及
 *   WM_NCRBUTTON* 非客户区变体。原生层静默 = 消息在 Qt wndproc 之前
 *   被消费；出现 NC 变体 = 点击被判定落在非客户区（标题栏扩展区）；
 * - eventFilter 安装在 QApplication 上，记录右键鼠标事件与 ContextMenu
 *   事件的目标控件。原生有而应用层无 = Qt 翻译层吞掉；两层都有而
 *   树控件无 = 事件在到达树之前被中间层拦截。
 * 仅诊断用途，定位后移除。
 */
class ZzDiagRightClickProbe : public QObject, public QAbstractNativeEventFilter
{
public:
    explicit ZzDiagRightClickProbe(QObject *parent = nullptr) : QObject(parent) {}

    bool nativeEventFilter(const QByteArray &eventType, void *message,
                           qintptr *result) override
    {
#ifdef Q_OS_WIN
        if (eventType != "windows_generic_MSG" || !message) {
            return false;
        }
        const auto *msg = static_cast<const MSG *>(message);
        switch (msg->message) {
        case WM_RBUTTONDOWN:
        case WM_RBUTTONUP:
        case WM_RBUTTONDBLCLK:
        case WM_CONTEXTMENU:
        case WM_NCRBUTTONDOWN:
        case WM_NCRBUTTONUP:
        case WM_NCRBUTTONDBLCLK:
            qInfo().noquote() << QStringLiteral(
                "诊断：原生鼠标消息 msg=0x%1 hwnd=%2 wParam=%3 lParam=%4")
                .arg(static_cast<qulonglong>(msg->message), 0, 16)
                .arg(reinterpret_cast<quintptr>(msg->hwnd))
                .arg(static_cast<qulonglong>(msg->wParam))
                .arg(static_cast<qlonglong>(msg->lParam));
            break;
        default:
            break;
        }
#else
        Q_UNUSED(eventType);
        Q_UNUSED(message);
#endif
        Q_UNUSED(result);
        return false;
    }

    bool eventFilter(QObject *watched, QEvent *event) override
    {
        const auto type = event->type();
        if (type == QEvent::MouseButtonPress
            || type == QEvent::MouseButtonRelease
            || type == QEvent::MouseButtonDblClick) {
            const auto *mouse = static_cast<const QMouseEvent *>(event);
            if (mouse->button() != Qt::RightButton
                && !(mouse->buttons() & Qt::RightButton)) {
                return false;
            }
            qInfo().noquote() << QStringLiteral(
                "诊断：应用层鼠标事件 type=%1 target=%2/%3 global=%4,%5")
                .arg(static_cast<int>(type))
                .arg(QString::fromLatin1(watched->metaObject()->className()))
                .arg(watched->objectName())
                .arg(mouse->globalPosition().x())
                .arg(mouse->globalPosition().y());
        } else if (type == QEvent::ContextMenu) {
            const auto *cm = static_cast<const QContextMenuEvent *>(event);
            qInfo().noquote() << QStringLiteral(
                "诊断：应用层 ContextMenu target=%1/%2 reason=%3 global=%4,%5")
                .arg(QString::fromLatin1(watched->metaObject()->className()))
                .arg(watched->objectName())
                .arg(static_cast<int>(cm->reason()))
                .arg(cm->globalPos().x())
                .arg(cm->globalPos().y());
        }
        return false;
    }
};

} // namespace

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

    // ContextMenu 触发改到按下：QWindowKit win32 会消费客户区 WM_RBUTTONUP
    // （误判进标题栏拖拽区时去吃系统菜单），而 Windows 平台 Qt 靠 release
    // 合成 ContextMenu——右键菜单因此永不到达应用（会话树右键失效，真机实测）。
    // 若后续库侧修复该拦截，可移除本开关恢复 release 触发。
    QGuiApplication::styleHints()->setContextMenuTrigger(
        Qt::ContextMenuTrigger::Press);
    qInfo().noquote() << QStringLiteral("诊断：ContextMenu 触发模式=%1")
        .arg(static_cast<int>(
            QGuiApplication::styleHints()->contextMenuTrigger()));

    // 第二轮诊断探针（右键链路断裂定位，见 ZzDiagRightClickProbe 注释）
    auto *diagProbe = new ZzDiagRightClickProbe(&application);
    application.installEventFilter(diagProbe);
    application.installNativeEventFilter(diagProbe);

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
