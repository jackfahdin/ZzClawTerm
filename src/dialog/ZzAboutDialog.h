#pragma once

#include <QtWidgets/QDialog>

/**
 * @brief 关于对话框：应用图标、名称、版本、构建类型、git 修订、Qt 版本、仓库链接。
 */
class ZzAboutDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ZzAboutDialog(QWidget *parent = nullptr);

    /** @brief 重设全部文本（LanguageChange 时调用）。 */
    void retranslateUi();

    /** @brief 版本信息单行文本（测试观察口）：含版本/构建类型/git 修订。 */
    [[nodiscard]] QString versionLine() const;

protected:
    void changeEvent(QEvent *event) override;

private:
    QString m_versionLine;
};
