#include "ZzSessionEditDialog.h"

#include <utility>

#include <QtCore/QEvent>
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
#include "dialog/ZzMasterPasswordDialog.h"

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
    if (isNew) {
        m_profile.protocol = QStringLiteral("ssh");
        m_profile.groupPath = groupPathPrefix;
    }

    auto *layout = new QFormLayout(this);
    m_formLayout = layout;

    // 全部用户可见文本由 retranslateUi() 统一设置（构造末尾调用，单一路径）
    m_nameEdit = new QLineEdit(m_profile.name, this);
    layout->addRow(QString(), m_nameEdit);

    m_groupEdit = new QLineEdit(m_profile.groupPath, this);
    layout->addRow(QString(), m_groupEdit);

    m_protocolCombo = new QComboBox(this);
    m_protocolCombo->addItem(QString(), QStringLiteral("ssh"));
    m_protocolCombo->addItem(QString(), QStringLiteral("local"));
    m_protocolCombo->setCurrentIndex(
        m_profile.protocol == QStringLiteral("local") ? 1 : 0);
    layout->addRow(QString(), m_protocolCombo);

    // SSH 与本地 Shell 两套字段切换
    m_hostStack = new QStackedWidget(this);
    auto *sshPage = new QWidget(this);
    auto *sshForm = new QFormLayout(sshPage);
    m_sshForm = sshForm;
    m_hostEdit = new QLineEdit(
        m_profile.protocol == QStringLiteral("local") ? QString() : m_profile.host,
        sshPage);
    m_portSpin = new QSpinBox(sshPage);
    m_portSpin->setRange(1, 65535);
    m_portSpin->setValue(m_profile.port == 0 ? 22 : m_profile.port);
    sshForm->addRow(QString(), m_hostEdit);
    sshForm->addRow(QString(), m_portSpin);
    auto *localPage = new QWidget(this);
    auto *localForm = new QFormLayout(localPage);
    m_localForm = localForm;
    m_shellEdit = new QLineEdit(
        m_profile.protocol == QStringLiteral("local") ? m_profile.host : QString(),
        localPage);
    localForm->addRow(QString(), m_shellEdit);
    m_hostStack->addWidget(sshPage);
    m_hostStack->addWidget(localPage);
    m_hostStack->setCurrentIndex(m_protocolCombo->currentIndex());
    layout->addRow(m_hostStack);
    connect(m_protocolCombo, &QComboBox::currentIndexChanged,
            m_hostStack, &QStackedWidget::setCurrentIndex);

    m_userEdit = new QLineEdit(m_profile.userName, this);
    layout->addRow(QString(), m_userEdit);

    m_authCombo = new QComboBox(this);
    m_authCombo->setObjectName(QStringLiteral("authCombo"));
    m_authCombo->addItem(QString(),
                         static_cast<int>(ZzAuthMethod::Agent));
    m_authCombo->addItem(QString(),
                         static_cast<int>(ZzAuthMethod::PrivateKey));
    m_authCombo->addItem(QString(),
                         static_cast<int>(ZzAuthMethod::Password));
    const int authIndex =
        m_authCombo->findData(static_cast<int>(m_profile.authMethod));
    m_authCombo->setCurrentIndex(authIndex >= 0 ? authIndex : 0);
    layout->addRow(QString(), m_authCombo);

    m_keyPathEdit = new QLineEdit(m_profile.privateKeyPath, this);
    layout->addRow(QString(), m_keyPathEdit);

    // 私钥口令：密文存凭据库，profile 只留 keyPassphraseCredentialId 引用
    m_keyPassphraseEdit = new QLineEdit(this);
    m_keyPassphraseEdit->setObjectName(QStringLiteral("keyPassphraseEdit"));
    m_keyPassphraseEdit->setEchoMode(QLineEdit::Password);
    layout->addRow(QString(), m_keyPassphraseEdit);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setObjectName(QStringLiteral("passwordEdit"));
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    layout->addRow(QString(), m_passwordEdit);

    // 端口转发规则表（规格 §三/§五）：五列 + 增删按钮
    auto *forwardSection = new QWidget(this);
    auto *forwardLayout = new QVBoxLayout(forwardSection);
    forwardLayout->setContentsMargins(0, 0, 0, 0);
    m_forwardTable = new QTableWidget(0, 5, forwardSection);
    m_forwardTable->setObjectName(QStringLiteral("forwardTable"));
    m_forwardTable->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    m_forwardTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    forwardLayout->addWidget(m_forwardTable);
    auto *forwardButtons = new QHBoxLayout;
    auto *addButton = new QPushButton(forwardSection);
    addButton->setObjectName(QStringLiteral("addForwardButton"));
    auto *removeButton = new QPushButton(forwardSection);
    removeButton->setObjectName(QStringLiteral("removeForwardButton"));
    forwardButtons->addWidget(addButton);
    forwardButtons->addWidget(removeButton);
    forwardButtons->addStretch();
    forwardLayout->addLayout(forwardButtons);
    layout->addRow(QString(), forwardSection);
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

    // X11 转发开关：默认开启对齐 MobaXterm；Windows 走内建 X server，Linux/macOS 依赖本机 X server
    m_x11CheckBox = new QCheckBox(this);
    m_x11CheckBox->setObjectName(QStringLiteral("x11CheckBox"));
    m_x11CheckBox->setChecked(m_profile.x11Forwarding);
    layout->addRow(QString(), m_x11CheckBox);

    // X11 嵌入模式（实验）：ZzXsrv 桌面嵌入会话标签页下半区；取消勾选则以独立窗口运行
    auto *x11EmbedCheckBox = new QCheckBox(this);
    x11EmbedCheckBox->setObjectName(QStringLiteral("x11EmbedCheckBox"));
    x11EmbedCheckBox->setChecked(m_profile.x11EmbedMode);
    layout->addRow(QString(), x11EmbedCheckBox);

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &ZzSessionEditDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addRow(buttons);

    retranslateUi();
}

void ZzSessionEditDialog::retranslateUi()
{
    setWindowTitle(m_profile.id.isNull() ? tr("新建会话") : tr("编辑会话"));

    // QFormLayout 行标签按字段部件反查（labelForField）
    const auto setRowLabel = [](QFormLayout *form, QWidget *field,
                                const QString &text) {
        if (auto *label = qobject_cast<QLabel *>(form->labelForField(field))) {
            label->setText(text);
        }
    };
    setRowLabel(m_formLayout, m_nameEdit, tr("名称："));
    setRowLabel(m_formLayout, m_groupEdit, tr("分组路径："));
    setRowLabel(m_formLayout, m_protocolCombo, tr("协议："));
    setRowLabel(m_sshForm, m_hostEdit, tr("主机："));
    setRowLabel(m_sshForm, m_portSpin, tr("端口："));
    setRowLabel(m_localForm, m_shellEdit, tr("Shell 程序："));
    setRowLabel(m_formLayout, m_userEdit, tr("用户名："));
    setRowLabel(m_formLayout, m_authCombo, tr("认证方式："));
    setRowLabel(m_formLayout, m_keyPathEdit, tr("私钥路径："));
    setRowLabel(m_formLayout, m_keyPassphraseEdit, tr("私钥口令："));
    setRowLabel(m_formLayout, m_passwordEdit, tr("密码："));
    // 端口转发区的字段部件是包裹容器（规则表的父部件）
    setRowLabel(m_formLayout, m_forwardTable->parentWidget(), tr("端口转发："));
    setRowLabel(m_formLayout, m_x11CheckBox, tr("图形转发："));

    m_groupEdit->setPlaceholderText(tr("如：生产环境/Web 服务器"));
    m_shellEdit->setPlaceholderText(tr("留空使用系统默认 shell"));
    m_keyPathEdit->setPlaceholderText(tr("私钥路径（公钥认证）"));
    m_keyPassphraseEdit->setPlaceholderText(
        m_originalKeyPassphraseCredentialId.isNull()
            ? tr("私钥口令（无口令留空）")
            : tr("留空保留已保存的口令"));
    m_passwordEdit->setPlaceholderText(
        m_originalCredentialId.isNull()
            ? tr("登录密码")
            : tr("留空保留已保存的密码"));

    m_protocolCombo->setItemText(0, tr("SSH"));
    m_protocolCombo->setItemText(1, tr("本地 Shell"));
    m_authCombo->setItemText(0, tr("SSH Agent"));
    m_authCombo->setItemText(1, tr("公钥文件"));
    m_authCombo->setItemText(2, tr("密码"));

    m_forwardTable->setHorizontalHeaderLabels({
        tr("类型"), tr("监听地址"), tr("监听端口"),
        tr("目标地址"), tr("目标端口")});
    // 已有规则行的类型下拉同样跟随语言切换
    for (int row = 0; row < m_forwardTable->rowCount(); ++row) {
        if (auto *typeCombo = qobject_cast<QComboBox *>(
                m_forwardTable->cellWidget(row, 0))) {
            typeCombo->setItemText(0, tr("本地 -L"));
            typeCombo->setItemText(1, tr("远程 -R"));
            typeCombo->setItemText(2, tr("动态 -D"));
        }
    }
    if (auto *addButton =
            findChild<QPushButton *>(QStringLiteral("addForwardButton"))) {
        addButton->setText(tr("添加"));
    }
    if (auto *removeButton =
            findChild<QPushButton *>(QStringLiteral("removeForwardButton"))) {
        removeButton->setText(tr("删除"));
    }

    m_x11CheckBox->setText(tr("X11 转发"));
    m_x11CheckBox->setToolTip(tr(
        "Windows 端首次使用将下载内建 X server；Linux/macOS 需本机 X server / XQuartz"));
    if (auto *embedCheck =
            findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"))) {
        embedCheck->setText(tr("嵌入标签页显示（实验；否则独立窗口）"));
        embedCheck->setToolTip(tr(
            "仅 Windows 生效：X11 桌面嵌入会话标签页内；取消勾选则 X 程序以独立窗口显示"));
    }
}

void ZzSessionEditDialog::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QDialog::changeEvent(event);
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
        m_hostEdit->setPlaceholderText(tr("主机不能为空"));
        return;
    }
    if (m_nameEdit->text().trimmed().isEmpty()) {
        m_nameEdit->setPlaceholderText(tr("名称不能为空"));
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
    // 嵌入勾选项按 objectName 反查（选项属 X11 附加设置，未单设成员）
    if (auto *embedCheck = findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"))) {
        m_profile.x11EmbedMode = embedCheck->isChecked();
    }

    // 端口转发规则：逐条校验 + 列表去重，非法禁止保存（规格 §五）
    const QVector<ZzForwardRule> rules = rulesFromTable();
    for (const ZzForwardRule &rule : rules) {
        const QString error = rule.validate();
        if (!error.isEmpty()) {
            QMessageBox::warning(this, tr("转发规则无效"), error);
            return;
        }
    }
    const QString dupError = ZzForwardRule::validateList(rules);
    if (!dupError.isEmpty()) {
        QMessageBox::warning(this, tr("转发规则无效"), dupError);
        return;
    }
    m_profile.portForwards = rules;

    // 密码：输入了新密码则写入凭据库换新引用；留空保留原引用
    if (m_profile.authMethod == ZzAuthMethod::Password) {
        if (!m_passwordEdit->text().isEmpty()) {
            QUuid credentialId = m_store->addCredential(
                m_profile.name, m_passwordEdit->text());
            if (credentialId.isNull() && !m_store->isUnlocked()) {
                // 凭据库锁定：就地弹主密码框解锁（首次使用即在此初始化主密码）
                // 后重试一次——保存对话框是初始化凭据库的唯一入口（连接流程只在
                // 已有凭据引用时才解锁），不能让用户在「保存被拒」里走进死胡同
                if (ZzMasterPasswordDialog::ensureUnlocked(m_store, this)) {
                    credentialId = m_store->addCredential(
                        m_profile.name, m_passwordEdit->text());
                }
            }
            if (credentialId.isNull()) {
                // 解锁后仍写入失败：不能静默丢密码，拒绝 accept 让用户处理
                QMessageBox::warning(this, tr("密码未保存"),
                    tr("凭据库未解锁，密码未保存。\n"
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
            QUuid passphraseId = m_store->addCredential(
                m_profile.name + QStringLiteral(" 私钥口令"),
                m_keyPassphraseEdit->text());
            if (passphraseId.isNull() && !m_store->isUnlocked()) {
                // 同密码路径：锁定时就地解锁后重试一次（首次使用在此初始化主密码）
                if (ZzMasterPasswordDialog::ensureUnlocked(m_store, this)) {
                    passphraseId = m_store->addCredential(
                        m_profile.name + QStringLiteral(" 私钥口令"),
                        m_keyPassphraseEdit->text());
                }
            }
            if (passphraseId.isNull()) {
                // 解锁后仍写入失败：不能静默丢口令，拒绝 accept 让用户处理
                QMessageBox::warning(this, tr("私钥口令未保存"),
                    tr("凭据库未解锁，私钥口令未保存。\n"
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
    typeCombo->addItem(tr("本地 -L"), static_cast<int>(ZzForwardRule::Type::Local));
    typeCombo->addItem(tr("远程 -R"), static_cast<int>(ZzForwardRule::Type::Remote));
    typeCombo->addItem(tr("动态 -D"), static_cast<int>(ZzForwardRule::Type::Dynamic));
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
