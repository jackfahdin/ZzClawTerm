#include "ZzSessionEditDialog.h"

#include <utility>

#include <QtWidgets/QComboBox>
#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QSpinBox>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QWidget>

#include "session/ZzCredentialStore.h"

ZzSessionEditDialog::ZzSessionEditDialog(ZzCredentialStore *store,
                                         ZzSessionProfile profile,
                                         const QString &groupPathPrefix,
                                         QWidget *parent)
    : QDialog(parent)
    , m_store(store)
    , m_profile(std::move(profile))
    , m_originalCredentialId(m_profile.credentialId)
{
    const bool isNew = m_profile.id.isNull();
    setWindowTitle(isNew ? QStringLiteral("新建会话") : QStringLiteral("编辑会话"));
    if (isNew) {
        m_profile.protocol = QStringLiteral("ssh");
        m_profile.groupPath = groupPathPrefix;
    }

    auto *layout = new QFormLayout(this);

    m_nameEdit = new QLineEdit(m_profile.name, this);
    layout->addRow(QStringLiteral("名称："), m_nameEdit);

    m_groupEdit = new QLineEdit(m_profile.groupPath, this);
    m_groupEdit->setPlaceholderText(QStringLiteral("如：生产环境/Web 服务器"));
    layout->addRow(QStringLiteral("分组路径："), m_groupEdit);

    m_protocolCombo = new QComboBox(this);
    m_protocolCombo->addItem(QStringLiteral("SSH"), QStringLiteral("ssh"));
    m_protocolCombo->addItem(QStringLiteral("本地 Shell"), QStringLiteral("local"));
    m_protocolCombo->setCurrentIndex(
        m_profile.protocol == QStringLiteral("local") ? 1 : 0);
    layout->addRow(QStringLiteral("协议："), m_protocolCombo);

    // SSH 与本地 Shell 两套字段切换
    m_hostStack = new QStackedWidget(this);
    auto *sshPage = new QWidget(this);
    auto *sshForm = new QFormLayout(sshPage);
    m_hostEdit = new QLineEdit(
        m_profile.protocol == QStringLiteral("local") ? QString() : m_profile.host,
        sshPage);
    m_portSpin = new QSpinBox(sshPage);
    m_portSpin->setRange(1, 65535);
    m_portSpin->setValue(m_profile.port == 0 ? 22 : m_profile.port);
    sshForm->addRow(QStringLiteral("主机："), m_hostEdit);
    sshForm->addRow(QStringLiteral("端口："), m_portSpin);
    auto *localPage = new QWidget(this);
    auto *localForm = new QFormLayout(localPage);
    m_shellEdit = new QLineEdit(
        m_profile.protocol == QStringLiteral("local") ? m_profile.host : QString(),
        localPage);
    m_shellEdit->setPlaceholderText(QStringLiteral("留空使用系统默认 shell"));
    localForm->addRow(QStringLiteral("Shell 程序："), m_shellEdit);
    m_hostStack->addWidget(sshPage);
    m_hostStack->addWidget(localPage);
    m_hostStack->setCurrentIndex(m_protocolCombo->currentIndex());
    layout->addRow(m_hostStack);
    connect(m_protocolCombo, &QComboBox::currentIndexChanged,
            m_hostStack, &QStackedWidget::setCurrentIndex);

    m_userEdit = new QLineEdit(m_profile.userName, this);
    layout->addRow(QStringLiteral("用户名："), m_userEdit);

    m_authCombo = new QComboBox(this);
    m_authCombo->addItem(QStringLiteral("SSH Agent"),
                         static_cast<int>(ZzAuthMethod::Agent));
    m_authCombo->addItem(QStringLiteral("公钥文件"),
                         static_cast<int>(ZzAuthMethod::PrivateKey));
    m_authCombo->addItem(QStringLiteral("密码"),
                         static_cast<int>(ZzAuthMethod::Password));
    const int authIndex =
        m_authCombo->findData(static_cast<int>(m_profile.authMethod));
    m_authCombo->setCurrentIndex(authIndex >= 0 ? authIndex : 0);
    layout->addRow(QStringLiteral("认证方式："), m_authCombo);

    m_keyPathEdit = new QLineEdit(m_profile.privateKeyPath, this);
    m_keyPathEdit->setPlaceholderText(QStringLiteral("私钥路径（公钥认证）"));
    layout->addRow(QStringLiteral("私钥路径："), m_keyPathEdit);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_passwordEdit->setPlaceholderText(
        m_originalCredentialId.isNull()
            ? QStringLiteral("登录密码")
            : QStringLiteral("留空保留已保存的密码"));
    layout->addRow(QStringLiteral("密码："), m_passwordEdit);

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &ZzSessionEditDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addRow(buttons);
}

ZzSessionProfile ZzSessionEditDialog::profile() const
{
    return m_profile;
}

void ZzSessionEditDialog::accept()
{
    const bool isLocal =
        m_protocolCombo->currentData().toString() == QStringLiteral("local");
    if (!isLocal && m_hostEdit->text().trimmed().isEmpty()) {
        m_hostEdit->setPlaceholderText(QStringLiteral("主机不能为空"));
        return;
    }
    if (m_nameEdit->text().trimmed().isEmpty()) {
        m_nameEdit->setPlaceholderText(QStringLiteral("名称不能为空"));
        return;
    }

    m_profile.name = m_nameEdit->text().trimmed();
    m_profile.groupPath = m_groupEdit->text().trimmed();
    m_profile.protocol = m_protocolCombo->currentData().toString();
    // 契约约定：local 会话的 shell 程序路径存于 host 字段
    m_profile.host = isLocal ? m_shellEdit->text().trimmed()
                             : m_hostEdit->text().trimmed();
    m_profile.port = static_cast<quint16>(m_portSpin->value());
    m_profile.userName = m_userEdit->text().trimmed();
    m_profile.authMethod =
        static_cast<ZzAuthMethod>(m_authCombo->currentData().toInt());
    m_profile.privateKeyPath = m_keyPathEdit->text().trimmed();

    // 密码：输入了新密码则写入凭据库换新引用；留空保留原引用
    if (m_profile.authMethod == ZzAuthMethod::Password) {
        if (!m_passwordEdit->text().isEmpty()) {
            const QUuid credentialId = m_store->addCredential(
                m_profile.name, m_passwordEdit->text());
            if (credentialId.isNull()) {
                // 凭据库锁定/写入失败：不能静默丢密码，拒绝 accept 让用户处理
                QMessageBox::warning(this, QStringLiteral("密码未保存"),
                    QStringLiteral("凭据库未解锁，密码未保存。\n"
                                   "请解锁凭据库后重试，或改用其他认证方式。"));
                return;
            }
            m_profile.credentialId = credentialId;
        } else {
            m_profile.credentialId = m_originalCredentialId;
        }
    } else {
        m_profile.credentialId = QUuid();
    }

    QDialog::accept();
}
