#include <QtCore/QCoreApplication>
#include <QtCore/QLocale>
#include <QtCore/QSettings>
#include <QtCore/QTranslator>
#include <QtTest/QtTest>

#include "settings/ZzAppSettings.h"
#include "settings/ZzLanguageManager.h"

/**
 * @brief 语言管理器：档位切换 → translator 装载/卸载 + QSettings 持久化。
 */
class tst_ZzLanguageManager : public QObject
{
    Q_OBJECT
private slots:
    void enOptionInstallsTranslator()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);

        langs.apply(ZzLanguageManager::kEn);
        QCOMPARE(langs.option(), ZzLanguageManager::kEn);
        QCOMPARE(langs.resolvedLanguage(), ZzLanguageManager::kEn);
        QCOMPARE(settings.language(), ZzLanguageManager::kEn);
        // en 翻译装载后，一个确定存在于 en.ts 的键应译出英文
        QCOMPARE(QCoreApplication::translate("ZzMenuBarService", "视图(&V)"),
                 QStringLiteral("View(&V)"));
    }

    void zhCnOptionUninstalls()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        langs.apply(ZzLanguageManager::kEn);
        langs.apply(ZzLanguageManager::kZhCn);
        QCOMPARE(langs.option(), ZzLanguageManager::kZhCn);
        QCOMPARE(langs.resolvedLanguage(), ZzLanguageManager::kZhCn);
    }

    void systemOptionResolvesByLocale()
    {
        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        QCOMPARE(langs.option(), ZzLanguageManager::kSystem);
        const QString resolved = langs.resolvedLanguage();
        QVERIFY(resolved == ZzLanguageManager::kZhCn || resolved == ZzLanguageManager::kEn);
    }

    void systemOptionOnNonChineseLocaleInstallsEnglish()
    {
        // 非中文系统 locale 下，system 档应解析为 en 并装载英文翻译
        const QLocale oldDefault;
        QLocale::setDefault(QLocale(QLocale::English, QLocale::UnitedStates));

        QTemporaryDir dir;
        ZzAppSettings settings(dir.filePath(QStringLiteral("s.ini")));
        ZzLanguageManager langs(&settings);
        langs.apply(ZzLanguageManager::kSystem);
        QCOMPARE(langs.option(), ZzLanguageManager::kSystem);
        QCOMPARE(langs.resolvedLanguage(), ZzLanguageManager::kEn);
        QCOMPARE(QCoreApplication::translate("ZzMenuBarService", "视图(&V)"),
                 QStringLiteral("View(&V)"));

        QLocale::setDefault(oldDefault);
    }
};

QTEST_MAIN(tst_ZzLanguageManager)
#include "tst_ZzLanguageManager.moc"
