#pragma once

#include <QtWidgets/QWidget>

#include "session/ZzSessionProfile.h"

class QCheckBox;
class QComboBox;
class QFormLayout;
class QLineEdit;
class QSpinBox;
class QStackedWidget;
class QTableWidget;
class QTreeView;

/**
 * @brief SSH 协议配置页：左侧树形导航 + 右侧 QStackedWidget 配置页。
 *
 * 六个树节点（栈页索引一致）：0 常规 / 1 连接 / 2 认证 / 3 端口转发 /
 * 4 X11 / 5 终端。不接触凭据库；凭据文本经 accessors 暴露给窗口层处理。
 */
class ZzSshConfigPage : public QWidget
{
    Q_OBJECT
public:
    explicit ZzSshConfigPage(QWidget *parent = nullptr);

    /** @brief 按 profile 回填全部字段（新建传默认值亦可）。 */
    void setProfile(const ZzSessionProfile &profile);
    /** @brief 把表单内容写回 profile（不触碰 id 与两个 credentialId）。 */
    void applyTo(ZzSessionProfile &profile) const;
    /**
     * @brief 校验表单。
     * @param error 输出错误文案（已翻译）。
     * @param pageIndex 输出出错字段所在栈页索引（用于窗口层聚焦）。
     * @return 全部合法返回 true。
     */
    [[nodiscard]] bool validateInputs(QString *error, int *pageIndex) const;

    /** @brief 用户新输入的密码（空串=未输入，窗口层据此保留原引用）。 */
    [[nodiscard]] QString enteredPassword() const;
    /** @brief 用户新输入的私钥口令（空串=未输入）。 */
    [[nodiscard]] QString enteredKeyPassphrase() const;
    /** @brief 切换到指定栈页并选中对应树节点（校验失败聚焦用）。 */
    void focusPage(int pageIndex);

protected:
    /** @brief LanguageChange 时重设全部文本。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 重设全部用户可见文本（构造与 LanguageChange 共用单一路径）。 */
    void retranslateUi();
    /** @brief 用给定规则列表填充规则表。 */
    void populateForwardTable(const QVector<ZzForwardRule> &rules);
    /** @brief 向表格追加一行（默认值或给定规则）。 */
    void appendForwardRow(const ZzForwardRule &rule);
    /** @brief 从表格读出规则列表（未校验）。 */
    [[nodiscard]] QVector<ZzForwardRule> rulesFromTable() const;

    QTreeView *m_navTree;
    QStackedWidget *m_stack;
    // 0 常规
    QLineEdit *m_nameEdit;
    QLineEdit *m_groupEdit;
    // 1 连接
    QLineEdit *m_hostEdit;
    QSpinBox *m_portSpin;
    QComboBox *m_terminalTypeCombo;
    QComboBox *m_encodingCombo;
    QSpinBox *m_keepAliveSpin;
    // 2 认证
    QLineEdit *m_userEdit;
    QComboBox *m_authCombo;
    QLineEdit *m_keyPathEdit;
    QLineEdit *m_keyPassphraseEdit;
    QLineEdit *m_passwordEdit;
    // 3 端口转发
    QTableWidget *m_forwardTable;
    // 4 X11
    QCheckBox *m_x11CheckBox;
    QCheckBox *m_x11EmbedCheckBox;
    // 5 终端
    QComboBox *m_colorSchemeCombo;
};
