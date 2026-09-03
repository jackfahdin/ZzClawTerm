#include "ZzSessionConfigWindow.h"

#include <utility>

#include <QtCore/QEvent>
#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QTabWidget>
#include <QtWidgets/QVBoxLayout>

#include <ZzFluentUI/ZzTabWidget.h>

#include "dialog/ZzLocalShellConfigPage.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "dialog/ZzSshConfigPage.h"
#include "session/ZzCredentialStore.h"

ZzSessionConfigWindow::ZzSessionConfigWindow(ZzCredentialStore *store,
                                             ZzSessionProfile profile,
                                             const QString &groupPathPrefix,
                                             QWidget *parent)
    : QDialog(parent)
    , m_store(store)
    , m_profile(std::move(profile))
    , m_originalCredentialId(m_profile.credentialId)
    , m_originalKeyPassphraseCredentialId(m_profile.keyPassphraseCredentialId)
{
    const bool isNew = m_profile.id.isNull();
    if (isNew) {
        m_profile.protocol = QStringLiteral("ssh");
        m_profile.groupPath = groupPathPrefix;
    }

    resize(760, 560);
    auto *layout = new QVBoxLayout(this);
    auto *tabs = new ZzFluentUI::ZzTabWidget(this);
    m_tabs = tabs;
    tabs->setObjectName(QStringLiteral("protocolTabWidget"));
    tabs->setDocumentMode(true);
    tabs->setMovable(false);
    tabs->setTabsClosable(false);
    m_sshPage = new ZzSshConfigPage(tabs);
    m_localPage = new ZzLocalShellConfigPage(tabs);
    tabs->addTab(m_sshPage, QString());
    tabs->addTab(m_localPage, QString());
    layout->addWidget(tabs);

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &ZzSessionConfigWindow::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);

    // 名称与协议无关：两页名称框双向镜像，任一页编辑即同步另一页
    //（两页 objectName 均为 nameEdit，镜像保证窗口级 findChild 命中任意一页
    // 的修改都对激活页生效；setText 内容相同不再发 textChanged，不会递归）
    auto *sshNameEdit =
        m_sshPage->findChild<QLineEdit *>(QStringLiteral("nameEdit"));
    auto *localNameEdit =
        m_localPage->findChild<QLineEdit *>(QStringLiteral("nameEdit"));
    if (sshNameEdit && localNameEdit) {
        connect(sshNameEdit, &QLineEdit::textChanged,
                localNameEdit, &QLineEdit::setText);
        connect(localNameEdit, &QLineEdit::textChanged,
                sshNameEdit, &QLineEdit::setText);
    }

    // 编辑：按协议预选 tab 并回填；新建：两页都给默认值（groupPathPrefix）
    m_sshPage->setProfile(m_profile);
    m_localPage->setProfile(m_profile);
    // 按是否已有已存凭据切换密码/私钥口令占位提示（「留空保留」）
    m_sshPage->setCredentialHints(!m_originalCredentialId.isNull(),
                                  !m_originalKeyPassphraseCredentialId.isNull());
    m_tabs->setCurrentIndex(
        m_profile.protocol == QStringLiteral("local") ? 1 : 0);

    retranslateUi();
}

ZzSessionProfile ZzSessionConfigWindow::profile() const
{
    return m_profile;
}

void ZzSessionConfigWindow::accept()
{
    // 只从激活 tab 收集（协议由激活 tab 决定，替代原协议下拉框）
    QWidget *active = m_tabs->currentWidget();
    ZzSessionProfile collected = m_profile; // 保留 id 与原凭据引用
    QString error;
    int pageIndex = -1;
    if (active == m_localPage) {
        if (!m_localPage->validateInputs(&error, &pageIndex)) {
            m_localPage->focusPage(pageIndex);
            QMessageBox::warning(this, tr("输入无效"), error);
            return;
        }
        m_localPage->applyTo(collected);
    } else {
        if (!m_sshPage->validateInputs(&error, &pageIndex)) {
            m_sshPage->focusPage(pageIndex);
            QMessageBox::warning(this, tr("输入无效"), error);
            return;
        }
        m_sshPage->applyTo(collected);
    }

    if (!persistCredentials(collected)) {
        return; // 凭据落库失败/用户放弃：persistCredentials 已弹框
    }
    m_profile = std::move(collected);
    QDialog::accept();
}

void ZzSessionConfigWindow::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QDialog::changeEvent(event);
}

void ZzSessionConfigWindow::retranslateUi()
{
    setWindowTitle(m_profile.id.isNull() ? tr("新建会话") : tr("编辑会话"));
    m_tabs->setTabText(0, tr("SSH"));
    m_tabs->setTabText(1, tr("本地 Shell"));
}

bool ZzSessionConfigWindow::persistCredentials(ZzSessionProfile &profile)
{
    // 凭据处理只跟激活页走：本地页无认证字段，按「切离密码/公钥认证」
    // 清掉两个原引用；SSH 页按认证方式分别处理（迁移自
    // ZzSessionEditDialog.cpp:301-375，口令文本改由 SSH 页 accessors 提供）
    const bool sshActive = m_tabs->currentWidget() != m_localPage;

    // 密码：输入了新密码则写入凭据库换新引用；留空保留原引用
    if (sshActive && profile.authMethod == ZzAuthMethod::Password) {
        if (!m_sshPage->enteredPassword().isEmpty()) {
            QUuid credentialId = m_store->addCredential(
                profile.name, m_sshPage->enteredPassword());
            if (credentialId.isNull() && !m_store->isUnlocked()) {
                // 凭据库锁定：就地弹主密码框解锁（首次使用即在此初始化主密码）
                // 后重试一次——保存窗口是初始化凭据库的唯一入口（连接流程只在
                // 已有凭据引用时才解锁），不能让用户在「保存被拒」里走进死胡同
                if (ZzMasterPasswordDialog::ensureUnlocked(m_store, this)) {
                    credentialId = m_store->addCredential(
                        profile.name, m_sshPage->enteredPassword());
                }
            }
            if (credentialId.isNull()) {
                // 解锁后仍写入失败：不能静默丢密码，拒绝 accept 让用户处理
                QMessageBox::warning(this, tr("密码未保存"),
                    tr("凭据库未解锁，密码未保存。\n"
                       "请解锁凭据库后重试，或改用其他认证方式。"));
                return false;
            }
            // 新凭据落库成功后再删旧凭据，避免孤儿条目；删除失败不阻断保存
            if (!m_originalCredentialId.isNull()
                && m_originalCredentialId != credentialId) {
                m_store->removeCredential(m_originalCredentialId);
            }
            profile.credentialId = credentialId;
        } else {
            profile.credentialId = m_originalCredentialId;
        }
    } else {
        // 切离密码认证：清理旧密码凭据，避免孤儿条目（凭据库锁定时删除失败不阻断保存）
        if (!m_originalCredentialId.isNull()) {
            m_store->removeCredential(m_originalCredentialId);
        }
        profile.credentialId = QUuid();
    }

    // 私钥口令：与密码同策略——输入新口令写凭据库换引用，留空保留原引用
    if (sshActive && profile.authMethod == ZzAuthMethod::PrivateKey) {
        if (!m_sshPage->enteredKeyPassphrase().isEmpty()) {
            QUuid passphraseId = m_store->addCredential(
                profile.name + QStringLiteral(" 私钥口令"),
                m_sshPage->enteredKeyPassphrase());
            if (passphraseId.isNull() && !m_store->isUnlocked()) {
                // 同密码路径：锁定时就地解锁后重试一次（首次使用在此初始化主密码）
                if (ZzMasterPasswordDialog::ensureUnlocked(m_store, this)) {
                    passphraseId = m_store->addCredential(
                        profile.name + QStringLiteral(" 私钥口令"),
                        m_sshPage->enteredKeyPassphrase());
                }
            }
            if (passphraseId.isNull()) {
                // 解锁后仍写入失败：不能静默丢口令，拒绝 accept 让用户处理
                QMessageBox::warning(this, tr("私钥口令未保存"),
                    tr("凭据库未解锁，私钥口令未保存。\n"
                       "请解锁凭据库后重试，或留空口令。"));
                return false;
            }
            // 新口令落库成功后再删旧凭据，避免孤儿条目；删除失败不阻断保存
            if (!m_originalKeyPassphraseCredentialId.isNull()
                && m_originalKeyPassphraseCredentialId != passphraseId) {
                m_store->removeCredential(m_originalKeyPassphraseCredentialId);
            }
            profile.keyPassphraseCredentialId = passphraseId;
        } else {
            profile.keyPassphraseCredentialId = m_originalKeyPassphraseCredentialId;
        }
    } else {
        // 切离公钥认证：清理旧口令凭据，避免孤儿条目（凭据库锁定时删除失败不阻断保存）
        if (!m_originalKeyPassphraseCredentialId.isNull()) {
            m_store->removeCredential(m_originalKeyPassphraseCredentialId);
        }
        profile.keyPassphraseCredentialId = QUuid();
    }

    return true;
}
