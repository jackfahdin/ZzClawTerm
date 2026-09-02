#include "ZzLanguageManager.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QLocale>
#include <QtCore/QTranslator>

#include "ZzAppSettings.h"

ZzLanguageManager::ZzLanguageManager(ZzAppSettings *settings, QObject *parent)
    : QObject(parent), m_settings(settings)
{
}

ZzLanguageManager::~ZzLanguageManager()
{
    // 先卸载再销毁：QCoreApplication 持有的是裸指针，悬挂会在下次翻译查找时崩溃
    if (m_en) {
        QCoreApplication::removeTranslator(m_en.get());
    }
}

QString ZzLanguageManager::option() const
{
    return m_settings->language();
}

QString ZzLanguageManager::resolvedLanguage() const
{
    return resolveOption(option());
}

QString ZzLanguageManager::resolveOption(const QString &option)
{
    if (option == kZhCn || option == kEn) {
        return option;
    }
    // system：中文系统用源文案（zh_CN 档），其余用英文翻译
    return QLocale::system().language() == QLocale::Chinese ? kZhCn : kEn;
}

void ZzLanguageManager::apply(const QString &option)
{
    // 先卸载：zh_CN 与切换路径共用「卸载即回退源文案」
    if (m_en) {
        QCoreApplication::removeTranslator(m_en.get());
        m_en.reset();
    }
    // 基于解析后的语言判定装载：apply 最后才写设置，
    // resolvedLanguage() 读的是旧值，故对传入 option 直接解析
    if (resolveOption(option) == kEn) {
        auto translator = std::make_unique<QTranslator>();
        if (translator->load(QStringLiteral(":/i18n/zzclawterm_en.qm"))) {
            QCoreApplication::installTranslator(translator.get());
            m_en = std::move(translator);
        }
        // qm 缺失：保持未装载状态，界面回退源文案，不报错
    }
    m_settings->setLanguage(option);
}
