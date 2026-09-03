#pragma once

#include <QtWidgets/QDialog>

#include "session/ZzSessionProfile.h"

class QTabWidget;
class ZzCredentialStore;
class ZzSshConfigPage;
class ZzLocalShellConfigPage;

/**
 * @brief 会话新建/编辑配置窗口：顶部协议 tab + 各协议独立配置页。
 *
 * 替代原 ZzSessionEditDialog；对外契约不变：exec() 接受后经 profile()
 * 取回完整 profile，密码/私钥口令经 ZzCredentialStore 落库只留引用。
 */
class ZzSessionConfigWindow : public QDialog
{
    Q_OBJECT
public:
    /**
     * @brief 构造新建或编辑窗口。
     * @param store 凭据库（密码/私钥口令写入或保留引用）。
     * @param profile 编辑时传入已有 profile；新建传默认构造值（id 为 null 视为新建）。
     * @param groupPathPrefix 新建时预选的分组路径。
     */
    explicit ZzSessionConfigWindow(ZzCredentialStore *store,
                                   ZzSessionProfile profile = {},
                                   const QString &groupPathPrefix = {},
                                   QWidget *parent = nullptr);

    /** @brief accept 后的表单结果（调用方写入 ZzSessionModel）。 */
    [[nodiscard]] ZzSessionProfile profile() const;

protected:
    void accept() override;
    /** @brief LanguageChange 时重设窗口级文本。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 重设窗口标题与 tab 标题（协议页各自处理页内文案）。 */
    void retranslateUi();
    /** @brief 保存密码/私钥口令到凭据库（锁定就地解锁，失败弹框）。
     *  @return 全部处理成功返回 true。 */
    bool persistCredentials(ZzSessionProfile &profile);

    ZzCredentialStore *m_store;
    ZzSessionProfile m_profile;        ///< 编辑中的工作副本
    QUuid m_originalCredentialId;      ///< 原密码引用（未改密码时保留）
    QUuid m_originalKeyPassphraseCredentialId; ///< 原私钥口令引用
    QTabWidget *m_tabs;                ///< 实为 ZzFluentUI::ZzTabWidget
    ZzSshConfigPage *m_sshPage;
    ZzLocalShellConfigPage *m_localPage;
};
