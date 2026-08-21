#include "ZzMasterPasswordDialog.h"

#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>

#include "session/ZzCredentialStore.h"

ZzMasterPasswordDialog::ZzMasterPasswordDialog(ZzCredentialStore *store,
                                               QWidget *parent)
    : QDialog(parent)
    , m_store(store)
{
    const bool firstRun = !store->hasMasterPassword();
    setWindowTitle(firstRun ? QStringLiteral("设置主密码")
                            : QStringLiteral("解锁凭据库"));

    auto *layout = new QFormLayout(this);
    auto *hint = new QLabel(firstRun
        ? QStringLiteral("首次使用凭据存储，请设置主密码（AES-256-GCM 加密，规格 §6.2）：")
        : QStringLiteral("请输入主密码解锁凭据库："), this);
    layout->addRow(hint);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    layout->addRow(QStringLiteral("主密码："), m_passwordEdit);

    m_confirmEdit = new QLineEdit(this);
    m_confirmEdit->setEchoMode(QLineEdit::Password);
    if (firstRun) {
        layout->addRow(QStringLiteral("确认密码："), m_confirmEdit);
    } else {
        m_confirmEdit->hide();
    }

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, [this, firstRun]() {
        if (firstRun && m_passwordEdit->text() != m_confirmEdit->text()) {
            m_confirmEdit->clear();
            m_confirmEdit->setPlaceholderText(QStringLiteral("两次输入不一致"));
            return;
        }
        if (ensureStoreReady(m_store, m_passwordEdit->text())) {
            accept();
        } else {
            m_passwordEdit->clear();
            m_passwordEdit->setPlaceholderText(QStringLiteral("密码错误或为空"));
        }
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addRow(buttons);
}

bool ZzMasterPasswordDialog::ensureUnlocked(ZzCredentialStore *store,
                                            QWidget *parent)
{
    if (store->isUnlocked()) {
        return true;
    }
    // 系统密钥环等无主密码后端：由 OS 托管解锁，不弹主密码框
    if (!store->requiresMasterPassword()) {
        return store->hasMasterPassword() ? store->unlock(QString())
                                          : store->initialize(QString());
    }
    ZzMasterPasswordDialog dialog(store, parent);
    return dialog.exec() == QDialog::Accepted;
}

bool ZzMasterPasswordDialog::ensureStoreReady(ZzCredentialStore *store,
                                              const QString &password)
{
    if (password.isEmpty()) {
        return false; // 用户取消
    }
    if (!store->hasMasterPassword()) {
        // 计划 03 交付的凭据库契约：首次设置走 initialize()，成功后即解锁
        return store->initialize(password);
    }
    return store->unlock(password);
}
