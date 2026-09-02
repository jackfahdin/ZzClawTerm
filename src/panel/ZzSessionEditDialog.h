#pragma once

#include <QtWidgets/QDialog>

#include "session/ZzSessionProfile.h"

class QComboBox;
class QCheckBox;
class QFormLayout;
class QLineEdit;
class QSpinBox;
class QStackedWidget;
class QTableWidget;
class ZzCredentialStore;

/**
 * @brief 会话新建/编辑对话框（规格 §七右键菜单的载体）。
 *
 * 表单字段与 ZzSessionProfile 一一对应；密码不存 profile，
 * 经 ZzCredentialStore 加密存储、profile 只留 credentialId 引用（规格 §6.2）。
 */
class ZzSessionEditDialog : public QDialog
{
    Q_OBJECT
public:
    /**
     * @brief 构造新建或编辑对话框。
     * @param store 凭据库（authMethod==Password 时写入/保留密码引用）。
     * @param profile 编辑时传入已有 profile；新建传默认构造值（id 为 null 视为新建）。
     * @param groupPathPrefix 新建时预选的分组路径（在分组项上右键新建）。
     */
    explicit ZzSessionEditDialog(ZzCredentialStore *store,
                                 ZzSessionProfile profile = {},
                                 const QString &groupPathPrefix = {},
                                 QWidget *parent = nullptr);

    /** @brief 表单当前内容（accept 后由调用方写入 ZzSessionModel）。 */
    [[nodiscard]] ZzSessionProfile profile() const;

protected:
    void accept() override;
    /** @brief LanguageChange 时重设全部文本。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 重设全部用户可见文本（构造与 LanguageChange 共用单一路径）。 */
    void retranslateUi();
    /** @brief 用 m_profile.portForwards 填充规则表。 */
    void populateForwardTable();
    /** @brief 向表格追加一行（默认值或给定规则）。 */
    void appendForwardRow(const ZzForwardRule &rule);
    /** @brief 从表格读出规则列表（未校验）。 */
    QVector<ZzForwardRule> rulesFromTable() const;

    ZzCredentialStore *m_store;
    ZzSessionProfile m_profile;        ///< 编辑中的工作副本
    QUuid m_originalCredentialId;      ///< 原密码引用（未改密码时保留）
    QUuid m_originalKeyPassphraseCredentialId; ///< 原私钥口令引用（未改口令时保留）

    QLineEdit *m_nameEdit;
    QLineEdit *m_groupEdit;
    QComboBox *m_protocolCombo;
    QStackedWidget *m_hostStack;       ///< ssh 页 / local 页
    QFormLayout *m_formLayout = nullptr;  ///< 主表单（retranslateUi 按字段反查行标签）
    QFormLayout *m_sshForm = nullptr;     ///< ssh 页表单（主机/端口行标签）
    QFormLayout *m_localForm = nullptr;   ///< local 页表单（Shell 程序行标签）
    QLineEdit *m_hostEdit;
    QSpinBox *m_portSpin;
    QLineEdit *m_shellEdit;            ///< local 会话的 shell 路径
    QLineEdit *m_userEdit;
    QComboBox *m_authCombo;            ///< Agent / PrivateKey / Password
    QLineEdit *m_keyPathEdit;
    QLineEdit *m_keyPassphraseEdit;    ///< 仅输入新私钥口令；留空=保留原引用
    QLineEdit *m_passwordEdit;         ///< 仅输入新密码；留空=保留原引用
    QTableWidget *m_forwardTable = nullptr; ///< 端口转发规则表
    QCheckBox *m_x11CheckBox = nullptr;  ///< X11 转发开关（默认开启，对齐 MobaXterm）
};
