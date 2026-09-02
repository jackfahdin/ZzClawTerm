#include "ZzMasterPasswordDialog.h"

#include <QtCore/QEvent>
#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>

#include "session/ZzCredentialStore.h"

ZzMasterPasswordDialog::ZzMasterPasswordDialog(ZzCredentialStore *store,
                                               QWidget *parent)
    : QDialog(parent)
    , m_store(store)
    , m_firstRun(!store->hasMasterPassword())
{
    m_formLayout = new QFormLayout(this);
    m_hintLabel = new QLabel(this);
    m_formLayout->addRow(m_hintLabel);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_formLayout->addRow(QString(), m_passwordEdit);

    m_confirmEdit = new QLineEdit(this);
    m_confirmEdit->setEchoMode(QLineEdit::Password);
    if (m_firstRun) {
        m_formLayout->addRow(QString(), m_confirmEdit);
    } else {
        m_confirmEdit->hide();
    }

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, [this]() {
        if (m_firstRun && m_passwordEdit->text() != m_confirmEdit->text()) {
            m_confirmEdit->clear();
            m_confirmEdit->setPlaceholderText(tr("两次输入不一致"));
            return;
        }
        if (ensureStoreReady(m_store, m_passwordEdit->text())) {
            accept();
        } else {
            m_passwordEdit->clear();
            m_passwordEdit->setPlaceholderText(tr("密码错误或为空"));
        }
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    m_formLayout->addRow(buttons);

    retranslateUi();
}

void ZzMasterPasswordDialog::retranslateUi()
{
    setWindowTitle(m_firstRun ? tr("设置主密码")
                              : tr("解锁凭据库"));
    m_hintLabel->setText(m_firstRun
        ? tr("首次使用凭据存储，请设置主密码（AES-256-GCM 加密，规格 §6.2）：")
        : tr("请输入主密码解锁凭据库："));
    // 行标签经 labelForField 按字段部件反查；确认密码行仅首次设置形态存在
    const auto setRowLabel = [this](QWidget *field, const QString &text) {
        if (auto *label =
                qobject_cast<QLabel *>(m_formLayout->labelForField(field))) {
            label->setText(text);
        }
    };
    setRowLabel(m_passwordEdit, tr("主密码："));
    if (m_firstRun) {
        setRowLabel(m_confirmEdit, tr("确认密码："));
    }
}

void ZzMasterPasswordDialog::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QDialog::changeEvent(event);
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
