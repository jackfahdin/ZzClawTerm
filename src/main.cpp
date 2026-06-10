/**
 * @file main.cpp
 * @brief ZzClawTerm 应用入口。
 */

#include <QApplication>

#include "ui/ZzMainWindow.h"

/**
 * @brief 程序入口。
 */
int main(int argc, char* argv[])
{
    QApplication app(argc, argv);
    QApplication::setOrganizationName(QStringLiteral("ZzClawTerm"));
    QApplication::setApplicationName(QStringLiteral("ZzClawTerm"));
    QApplication::setApplicationVersion(QStringLiteral("0.1.0"));

    ZzUi::ZzMainWindow window;
    window.show();

    return app.exec();
}
