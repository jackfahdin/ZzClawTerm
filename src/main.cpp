/**
 * @file main.cpp
 * @brief ZzClawTerm 应用入口。
 */

#include <QApplication>
#include <QKeyEvent>
#include <QTimer>

#include "ui/ZzMainWindow.h"
#include "ui/ZzTerminalWidget.h"

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

    if (qEnvironmentVariableIsSet("ZZ_SELFTEST")) {
        QTimer::singleShot(2500, [&window]() {
            auto* term = window.findChild<ZzUi::ZzTerminalWidget*>();
            if (!term) {
                return;
            }
            const QString cmd = QStringLiteral(
                "echo zzselftest > C:\\Users\\guomaojie\\AppData\\Local\\Temp\\zzmark.txt\r");
            for (const QChar ch : cmd) {
                if (ch == QLatin1Char('\r')) {
                    QKeyEvent e(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier,
                                QStringLiteral("\r"));
                    QApplication::sendEvent(term, &e);
                } else {
                    QKeyEvent e(QEvent::KeyPress, 0, Qt::NoModifier, QString(ch));
                    QApplication::sendEvent(term, &e);
                }
            }
        });
    }

    return app.exec();
}
