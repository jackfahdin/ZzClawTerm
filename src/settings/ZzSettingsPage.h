#pragma once

#include <QtWidgets/QWidget>

class QCheckBox;
class QComboBox;
class QFormLayout;
class QLabel;
class QSpinBox;
class ZzAppSettings;

/**
 * @brief 全局设置页（规格 §七）：终端类型、编码、字号、配色、内存历史行数。
 *
 * 即改即存：控件变更直接写 ZzAppSettings 并触发 settingsChanged，
 * 不需要"保存"按钮。作为框架页面挂在导航 Footer（任务 14 装配）。
 */
class ZzSettingsPage : public QWidget
{
    Q_OBJECT
public:
    explicit ZzSettingsPage(ZzAppSettings *settings, QWidget *parent = nullptr);

    // ---- 测试观察口 ----
    [[nodiscard]] QComboBox *terminalTypeCombo() const;
    [[nodiscard]] QComboBox *encodingCombo() const;
    [[nodiscard]] QSpinBox *fontSizeSpin() const;
    [[nodiscard]] QComboBox *colorSchemeCombo() const;
    [[nodiscard]] QSpinBox *historyLinesSpin() const;
    [[nodiscard]] QComboBox *credentialBackendCombo() const;
    [[nodiscard]] QCheckBox *x11ServerCheck() const;
    [[nodiscard]] QComboBox *sftpBlockSizeCombo() const;

protected:
    /** @brief LanguageChange 时重设全部文本（该页长驻，必须响应）。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 集中重设全部用户可见文本（构造时同样调用，单一路径）。 */
    void retranslateUi();

    ZzAppSettings *m_settings;
    QFormLayout *m_formLayout;
    QComboBox *m_terminalTypeCombo;
    QComboBox *m_encodingCombo;
    QSpinBox *m_fontSizeSpin;
    QComboBox *m_colorSchemeCombo;
    QSpinBox *m_historyLinesSpin;
    QComboBox *m_credentialBackendCombo;
    QCheckBox *m_x11ServerCheck;
    QComboBox *m_sftpBlockSizeCombo;
    QLabel *m_noteLabel;
};
