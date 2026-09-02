#pragma once

#include <QtWidgets/QDialog>

class QFormLayout;
class QLabel;
class QLineEdit;
class ZzCredentialStore;

/**
 * @brief 主密码解锁框（规格 §七：缺主密码弹解锁框；§6.2：首次启动设主密码）。
 *
 * 两种形态：凭据库尚无主密码时引导设置并确认两次；已有主密码时输入解锁。
 */
class ZzMasterPasswordDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ZzMasterPasswordDialog(ZzCredentialStore *store,
                                    QWidget *parent = nullptr);

    /**
     * @brief 确保凭据库已解锁：未解锁则弹窗（模态），成功或用户取消后返回。
     * @return true 表示已解锁可继续取凭据。
     */
    static bool ensureUnlocked(ZzCredentialStore *store, QWidget *parent = nullptr);

    /**
     * @brief 纯逻辑：设置（首次）或验证主密码。供自动化测试与对话框复用。
     * @param password 用户输入；空串视为取消，返回 false。
     * @return 操作是否成功。
     */
    static bool ensureStoreReady(ZzCredentialStore *store, const QString &password);

protected:
    /** @brief LanguageChange 时重设全部文本。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 集中重设全部用户可见文本（构造时同样调用，单一路径）。 */
    void retranslateUi();

    ZzCredentialStore *m_store;
    bool m_firstRun;            ///< 首次设置主密码形态（决定标题/提示/确认行）
    QFormLayout *m_formLayout;  ///< 行标签经 labelForField 反查重设
    QLabel *m_hintLabel;
    QLineEdit *m_passwordEdit;
    QLineEdit *m_confirmEdit;   ///< 仅首次设置时可见
};
