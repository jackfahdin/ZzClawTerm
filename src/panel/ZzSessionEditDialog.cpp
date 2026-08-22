#include "ZzSessionEditDialog.h"

#include <utility>

#include <QtWidgets/QComboBox>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QHeaderView>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QSpinBox>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QTableWidget>
#include <QtWidgets/QVBoxLayout>
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
    , m_originalKeyPassphraseCredentialId(m_profile.keyPassphraseCredentialId)
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
    m_authCombo->setObjectName(QStringLiteral("authCombo"));
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

    // 私钥口令：密文存凭据库，profile 只留 keyPassphraseCredentialId 引用
    m_keyPassphraseEdit = new QLineEdit(this);
    m_keyPassphraseEdit->setObjectName(QStringLiteral("keyPassphraseEdit"));
    m_keyPassphraseEdit->setEchoMode(QLineEdit::Password);
    m_keyPassphraseEdit->setPlaceholderText(
        m_originalKeyPassphraseCredentialId.isNull()
            ? QStringLiteral("私钥口令（无口令留空）")
            : QStringLiteral("留空保留已保存的口令"));
    layout->addRow(QStringLiteral("私钥口令："), m_keyPassphraseEdit);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_passwordEdit->setPlaceholderText(
        m_originalCredentialId.isNull()
            ? QStringLiteral("登录密码")
            : QStringLiteral("留空保留已保存的密码"));
    layout->addRow(QStringLiteral("密码："), m_passwordEdit);

    // 端口转发规则表（规格 §三/§五）：五列 + 增删按钮
    auto *forwardSection = new QWidget(this);
    auto *forwardLayout = new QVBoxLayout(forwardSection);
    forwardLayout->setContentsMargins(0, 0, 0, 0);
    m_forwardTable = new QTableWidget(0, 5, forwardSection);
    m_forwardTable->setObjectName(QStringLiteral("forwardTable"));
    m_forwardTable->setHorizontalHeaderLabels({
        QStringLiteral("类型"), QStringLiteral("监听地址"), QStringLiteral("监听端口"),
        QStringLiteral("目标地址"), QStringLiteral("目标端口")});
    m_forwardTable->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    m_forwardTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    forwardLayout->addWidget(m_forwardTable);
    auto *forwardButtons = new QHBoxLayout;
    auto *addButton = new QPushButton(QStringLiteral("添加"), forwardSection);
    addButton->setObjectName(QStringLiteral("addForwardButton"));
    auto *removeButton = new QPushButton(QStringLiteral("删除"), forwardSection);
    removeButton->setObjectName(QStringLiteral("removeForwardButton"));
    forwardButtons->addWidget(addButton);
    forwardButtons->addWidget(removeButton);
    forwardButtons->addStretch();
    forwardLayout->addLayout(forwardButtons);
    layout->addRow(QStringLiteral("端口转发："), forwardSection);
    populateForwardTable();
    connect(addButton, &QPushButton::clicked, this, [this]() {
        appendForwardRow(ZzForwardRule{});
    });
    connect(removeButton, &QPushButton::clicked, this, [this]() {
        const int row = m_forwardTable->currentRow();
        if (row >= 0) {
            m_forwardTable->removeRow(row);
        }
    });

    // X11 转发开关（实验性）：Windows 走内建 X server，Linux/macOS 依赖本机 X server
    m_x11CheckBox = new QCheckBox(QStringLiteral("X11 转发（实验性）"), this);
    m_x11CheckBox->setObjectName(QStringLiteral("x11CheckBox"));
    m_x11CheckBox->setToolTip(QStringLiteral(
        "Windows 端首次使用将下载内建 X server；Linux/macOS 需本机 X server / XQuartz"));
    m_x11CheckBox->setChecked(m_profile.x11Forwarding);
    layout->addRow(QStringLiteral("图形转发："), m_x11CheckBox);

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
    m_profile.x11Forwarding = m_x11CheckBox->isChecked();

    // 端口转发规则：逐条校验 + 列表去重，非法禁止保存（规格 §五）
    const QVector<ZzForwardRule> rules = rulesFromTable();
    for (const ZzForwardRule &rule : rules) {
        const QString error = rule.validate();
        if (!error.isEmpty()) {
            QMessageBox::warning(this, QStringLiteral("转发规则无效"), error);
            return;
        }
    }
    const QString dupError = ZzForwardRule::validateList(rules);
    if (!dupError.isEmpty()) {
        QMessageBox::warning(this, QStringLiteral("转发规则无效"), dupError);
        return;
    }
    m_profile.portForwards = rules;

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
            // 新凭据落库成功后再删旧凭据，避免孤儿条目；删除失败不阻断保存
            if (!m_originalCredentialId.isNull()
                && m_originalCredentialId != credentialId) {
                m_store->removeCredential(m_originalCredentialId);
            }
            m_profile.credentialId = credentialId;
        } else {
            m_profile.credentialId = m_originalCredentialId;
        }
    } else {
        // 切离密码认证：清理旧密码凭据，避免孤儿条目（凭据库锁定时删除失败不阻断保存）
        if (!m_originalCredentialId.isNull()) {
            m_store->removeCredential(m_originalCredentialId);
        }
        m_profile.credentialId = QUuid();
    }

    // 私钥口令：与密码同策略——输入新口令写凭据库换引用，留空保留原引用
    if (m_profile.authMethod == ZzAuthMethod::PrivateKey) {
        if (!m_keyPassphraseEdit->text().isEmpty()) {
            const QUuid passphraseId = m_store->addCredential(
                m_profile.name + QStringLiteral(" 私钥口令"),
                m_keyPassphraseEdit->text());
            if (passphraseId.isNull()) {
                // 凭据库锁定/写入失败：不能静默丢口令，拒绝 accept 让用户处理
                QMessageBox::warning(this, QStringLiteral("私钥口令未保存"),
                    QStringLiteral("凭据库未解锁，私钥口令未保存。\n"
                                   "请解锁凭据库后重试，或留空口令。"));
                return;
            }
            // 新口令落库成功后再删旧凭据，避免孤儿条目；删除失败不阻断保存
            if (!m_originalKeyPassphraseCredentialId.isNull()
                && m_originalKeyPassphraseCredentialId != passphraseId) {
                m_store->removeCredential(m_originalKeyPassphraseCredentialId);
            }
            m_profile.keyPassphraseCredentialId = passphraseId;
        } else {
            m_profile.keyPassphraseCredentialId = m_originalKeyPassphraseCredentialId;
        }
    } else {
        // 切离公钥认证：清理旧口令凭据，避免孤儿条目（凭据库锁定时删除失败不阻断保存）
        if (!m_originalKeyPassphraseCredentialId.isNull()) {
            m_store->removeCredential(m_originalKeyPassphraseCredentialId);
        }
        m_profile.keyPassphraseCredentialId = QUuid();
    }

    QDialog::accept();
}

void ZzSessionEditDialog::populateForwardTable()
{
    for (const ZzForwardRule &rule : m_profile.portForwards) {
        appendForwardRow(rule);
    }
}

void ZzSessionEditDialog::appendForwardRow(const ZzForwardRule &rule)
{
    const int row = m_forwardTable->rowCount();
    m_forwardTable->insertRow(row);

    auto *typeCombo = new QComboBox(m_forwardTable);
    typeCombo->addItem(QStringLiteral("本地 -L"), static_cast<int>(ZzForwardRule::Type::Local));
    typeCombo->addItem(QStringLiteral("远程 -R"), static_cast<int>(ZzForwardRule::Type::Remote));
    typeCombo->addItem(QStringLiteral("动态 -D"), static_cast<int>(ZzForwardRule::Type::Dynamic));
    const int typeIndex = typeCombo->findData(static_cast<int>(rule.type));
    typeCombo->setCurrentIndex(typeIndex >= 0 ? typeIndex : 0);
    m_forwardTable->setCellWidget(row, 0, typeCombo);

    m_forwardTable->setItem(row, 1, new QTableWidgetItem(rule.listenHost));
    m_forwardTable->setItem(row, 2, new QTableWidgetItem(
        rule.listenPort ? QString::number(rule.listenPort) : QString()));
    m_forwardTable->setItem(row, 3, new QTableWidgetItem(rule.targetHost));
    m_forwardTable->setItem(row, 4, new QTableWidgetItem(
        rule.targetPort ? QString::number(rule.targetPort) : QString()));
}

QVector<ZzForwardRule> ZzSessionEditDialog::rulesFromTable() const
{
    QVector<ZzForwardRule> rules;
    for (int row = 0; row < m_forwardTable->rowCount(); ++row) {
        auto *typeCombo = qobject_cast<QComboBox *>(m_forwardTable->cellWidget(row, 0));
        ZzForwardRule rule;
        rule.type = static_cast<ZzForwardRule::Type>(typeCombo->currentData().toInt());
        const auto *listenHostItem = m_forwardTable->item(row, 1);
        const auto *listenPortItem = m_forwardTable->item(row, 2);
        const auto *targetHostItem = m_forwardTable->item(row, 3);
        const auto *targetPortItem = m_forwardTable->item(row, 4);
        rule.listenHost = listenHostItem ? listenHostItem->text().trimmed() : QString();
        // 非法数字/空串 → 0，交由 validate() 拒绝（quint16 超界由 toUInt 失败兜底）
        bool ok = false;
        const uint listenPort = listenPortItem ? listenPortItem->text().toUInt(&ok) : 0;
        rule.listenPort = (ok && listenPort <= 65535) ? static_cast<quint16>(listenPort) : 0;
        rule.targetHost = targetHostItem ? targetHostItem->text().trimmed() : QString();
        const uint targetPort = targetPortItem ? targetPortItem->text().toUInt(&ok) : 0;
        rule.targetPort = (ok && targetPort <= 65535) ? static_cast<quint16>(targetPort) : 0;
        rules.append(rule);
    }
    return rules;
}
