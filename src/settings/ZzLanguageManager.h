#pragma once

#include <QtCore/QObject>
#include <QtCore/QString>

#include <memory>

class QTranslator;
class ZzAppSettings;

/**
 * @brief 界面语言管理：system（跟随系统）/ zh_CN / en 三档，即改即存。
 *
 * 简体中文为源码文案，zh_CN 档不装载任何 translator（天然回退）；
 * en 档装载 :/i18n/zzclawterm_en.qm。apply() 后 Qt 自动广播
 * QEvent::LanguageChange，各组件 retranslateUi() 响应。
 */
class ZzLanguageManager : public QObject
{
    Q_OBJECT
public:
    /** @brief 语言档位标识（与 QSettings 存储值一致）。 */
    static const inline QString kSystem = QStringLiteral("system");
    static const inline QString kZhCn = QStringLiteral("zh_CN");
    static const inline QString kEn = QStringLiteral("en");

    explicit ZzLanguageManager(ZzAppSettings *settings, QObject *parent = nullptr);
    /** @brief 析构移至 .cpp：QTranslator 为不完整类型，unique_ptr 销毁需完整类型。 */
    ~ZzLanguageManager() override;

    /** @brief 当前档位（读自设置）。 */
    [[nodiscard]] QString option() const;
    /** @brief 解析后的实际生效语言（system 依系统 locale 解析）。 */
    [[nodiscard]] QString resolvedLanguage() const;

    /** @brief 应用指定档位：切换 translator 并持久化。 */
    void apply(const QString &option);

private:
    /** @brief 档位解析：system 依 QLocale::system() 解析为 zh_CN/en。 */
    [[nodiscard]] static QString resolveOption(const QString &option);

    ZzAppSettings *m_settings;             ///< 非拥有
    std::unique_ptr<QTranslator> m_en;     ///< 装载中的 en translator（无则未装载）
};
