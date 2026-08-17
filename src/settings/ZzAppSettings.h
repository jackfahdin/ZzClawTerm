#pragma once

#include <QtCore/QObject>
#include <QtCore/QString>

class QSettings;

/**
 * @brief 全局设置存储（规格 §七）：默认终端类型、编码、字号、配色、热层内存行数。
 *
 * 生产环境用 instance()（INI 文件落在 QStandardPaths::AppConfigLocation）；
 * 测试用显式路径构造。任何字段变更发射 settingsChanged()，已打开标签实时应用。
 */
class ZzAppSettings : public QObject
{
    Q_OBJECT
public:
    /** @brief 以指定 INI 路径构造（测试与自定义场景）。 */
    explicit ZzAppSettings(const QString &filePath, QObject *parent = nullptr);

    /** @brief 生产单例：路径为 <AppConfigLocation>/settings.ini。 */
    static ZzAppSettings &instance();

    /** @brief 默认终端类型（TERM 值），默认 "xterm-256color"。 */
    [[nodiscard]] QString terminalType() const;
    void setTerminalType(const QString &terminalType);

    /** @brief 默认字符编码名（如 "UTF-8"、"GBK"），默认 "UTF-8"。 */
    [[nodiscard]] QString encoding() const;
    void setEncoding(const QString &encoding);

    /** @brief 默认字号（pt），默认 12。 */
    [[nodiscard]] int fontSize() const;
    void setFontSize(int fontSize);

    /** @brief 默认配色方案名（QTermWidget::availableColorSchemes 之一），默认 "Linux"。 */
    [[nodiscard]] QString colorScheme() const;
    void setColorScheme(const QString &colorScheme);

    /** @brief 终端内存历史行数上限（ZzLogEngine 热层之外的屏幕侧缓存），默认 10000。 */
    [[nodiscard]] int historyLines() const;
    void setHistoryLines(int lines);

signals:
    /** @brief 任一字段变更后发射；UI 层收到后实时应用到已打开标签。 */
    void settingsChanged();

private:
    QSettings *m_settings;
};
