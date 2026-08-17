#include <QtCore/QTimer>
#include <QtWidgets/QApplication>
#include <QtWidgets/QMainWindow>

/**
 * @brief 应用入口（骨架版，任务 14 替换为框架完整装配）。
 */
int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    QMainWindow window;
    window.setWindowTitle(QStringLiteral("ZzClawTerm"));
    // 冒烟模式：设置环境变量后启动即退出，供 CI 快速验证可执行能跑起来
    if (qEnvironmentVariableIsSet("ZZCLAWTERM_SMOKE_QUIT")) {
        QTimer::singleShot(0, &app, &QApplication::quit);
    }
    window.show();
    return app.exec();
}
