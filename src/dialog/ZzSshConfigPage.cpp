#include "ZzSshConfigPage.h"

#include <QtCore/QEvent>
#include <QtCore/QItemSelectionModel>
#include <QtGui/QStandardItemModel>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QComboBox>
#include <QtWidgets/QFileDialog>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QHeaderView>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QSpinBox>
#include <QtWidgets/QSplitter>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QTableWidget>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>

#include <qtermwidget.h> // availableColorSchemes()

namespace {
/** @brief 树节点/栈页索引常量（顺序即 UI 顺序）。 */
enum ZzSshPageIndex {
    GeneralPage = 0,
    ConnectionPage = 1,
    AuthPage = 2,
    ForwardPage = 3,
    X11Page = 4,
    TerminalPage = 5,
    PageCount = 6
};
} // namespace

ZzSshConfigPage::ZzSshConfigPage(QWidget *parent)
    : QWidget(parent)
{
    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    auto *splitter = new QSplitter(this);
    layout->addWidget(splitter);

    // 左侧树：单列六节点，不可编辑、无表头、选中即切页
    m_navTree = new QTreeView(splitter);
    m_navTree->setObjectName(QStringLiteral("sshNavTree"));
    m_navTree->setHeaderHidden(true);
    m_navTree->setEditTriggers(QAbstractItemView::NoEditTriggers);
    auto *treeModel = new QStandardItemModel(0, 1, m_navTree);
    for (int i = 0; i < PageCount; ++i) {
        treeModel->appendRow(new QStandardItem); // 文本由 retranslateUi 设置
    }
    m_navTree->setModel(treeModel);
    m_navTree->setMinimumWidth(140);
    m_navTree->setMaximumWidth(200);

    m_stack = new QStackedWidget(splitter);
    splitter->setStretchFactor(1, 1);

    // —— 0 常规：名称 / 分组路径 ——
    auto *generalPage = new QWidget(this);
    auto *generalForm = new QFormLayout(generalPage);
    m_nameEdit = new QLineEdit(generalPage);
    m_nameEdit->setObjectName(QStringLiteral("nameEdit"));
    m_groupEdit = new QLineEdit(generalPage);
    generalForm->addRow(QString(), m_nameEdit);
    generalForm->addRow(QString(), m_groupEdit);
    m_stack->addWidget(generalPage);

    // —— 1 连接：主机 / 端口 / 终端类型 / 编码 / 保活间隔 ——
    auto *connectionPage = new QWidget(this);
    auto *connectionForm = new QFormLayout(connectionPage);
    m_hostEdit = new QLineEdit(connectionPage);
    m_hostEdit->setObjectName(QStringLiteral("hostEdit"));
    m_portSpin = new QSpinBox(connectionPage);
    m_portSpin->setRange(1, 65535);
    m_terminalTypeCombo = new QComboBox(connectionPage);
    m_terminalTypeCombo->setEditable(true);
    m_terminalTypeCombo->addItems({
        QStringLiteral("xterm-256color"), QStringLiteral("xterm"),
        QStringLiteral("xterm-direct"), QStringLiteral("screen"),
        QStringLiteral("linux")});
    m_encodingCombo = new QComboBox(connectionPage);
    m_encodingCombo->setEditable(true);
    m_encodingCombo->addItems({
        QStringLiteral("UTF-8"), QStringLiteral("GBK"),
        QStringLiteral("GB18030"), QStringLiteral("Big5"),
        QStringLiteral("ISO-8859-1")});
    m_keepAliveSpin = new QSpinBox(connectionPage);
    m_keepAliveSpin->setRange(0, 3600);
    m_keepAliveSpin->setSpecialValueText(QString()); // retranslateUi 设「关闭」
    connectionForm->addRow(QString(), m_hostEdit);
    connectionForm->addRow(QString(), m_portSpin);
    connectionForm->addRow(QString(), m_terminalTypeCombo);
    connectionForm->addRow(QString(), m_encodingCombo);
    connectionForm->addRow(QString(), m_keepAliveSpin);
    m_stack->addWidget(connectionPage);

    // —— 2 认证：用户名 / 认证方式 / 私钥路径+浏览 / 私钥口令 / 密码 ——
    // 迁移自 ZzSessionEditDialog.cpp:85-113（行标签经 labelForField 反查，同式）
    auto *authPage = new QWidget(this);
    auto *authForm = new QFormLayout(authPage);
    m_userEdit = new QLineEdit(authPage);
    authForm->addRow(QString(), m_userEdit);

    m_authCombo = new QComboBox(authPage);
    m_authCombo->setObjectName(QStringLiteral("authCombo"));
    m_authCombo->addItem(QString(), static_cast<int>(ZzAuthMethod::Agent));
    m_authCombo->addItem(QString(), static_cast<int>(ZzAuthMethod::PrivateKey));
    m_authCombo->addItem(QString(), static_cast<int>(ZzAuthMethod::Password));
    authForm->addRow(QString(), m_authCombo);

    // 私钥路径行：QLineEdit + 「浏览…」QPushButton 水平布局
    auto *keyPathRow = new QWidget(authPage);
    auto *keyPathLayout = new QHBoxLayout(keyPathRow);
    keyPathLayout->setContentsMargins(0, 0, 0, 0);
    m_keyPathEdit = new QLineEdit(keyPathRow);
    auto *browseButton = new QPushButton(keyPathRow);
    browseButton->setObjectName(QStringLiteral("browseKeyButton"));
    keyPathLayout->addWidget(m_keyPathEdit);
    keyPathLayout->addWidget(browseButton);
    authForm->addRow(QString(), keyPathRow);
    connect(browseButton, &QPushButton::clicked, this, [this]() {
        const QString path = QFileDialog::getOpenFileName(
            this, tr("选择私钥文件"), m_keyPathEdit->text());
        if (!path.isEmpty()) {
            m_keyPathEdit->setText(path);
        }
    });

    // 私钥口令：密文存凭据库，本页只收集新输入，由窗口层落库换引用
    m_keyPassphraseEdit = new QLineEdit(authPage);
    m_keyPassphraseEdit->setObjectName(QStringLiteral("keyPassphraseEdit"));
    m_keyPassphraseEdit->setEchoMode(QLineEdit::Password);
    authForm->addRow(QString(), m_keyPassphraseEdit);

    m_passwordEdit = new QLineEdit(authPage);
    m_passwordEdit->setObjectName(QStringLiteral("passwordEdit"));
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    authForm->addRow(QString(), m_passwordEdit);
    m_stack->addWidget(authPage);

    // —— 3 端口转发 ——
    // 迁移自 ZzSessionEditDialog.cpp:115-143（五列表 + 增删按钮）
    auto *forwardPage = new QWidget(this);
    auto *forwardLayout = new QVBoxLayout(forwardPage);
    forwardLayout->setContentsMargins(0, 0, 0, 0);
    m_forwardTable = new QTableWidget(0, 5, forwardPage);
    m_forwardTable->setObjectName(QStringLiteral("forwardTable"));
    m_forwardTable->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    m_forwardTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    forwardLayout->addWidget(m_forwardTable);
    auto *forwardButtons = new QHBoxLayout;
    auto *addButton = new QPushButton(forwardPage);
    addButton->setObjectName(QStringLiteral("addForwardButton"));
    auto *removeButton = new QPushButton(forwardPage);
    removeButton->setObjectName(QStringLiteral("removeForwardButton"));
    forwardButtons->addWidget(addButton);
    forwardButtons->addWidget(removeButton);
    forwardButtons->addStretch();
    forwardLayout->addLayout(forwardButtons);
    m_stack->addWidget(forwardPage);
    connect(addButton, &QPushButton::clicked, this, [this]() {
        appendForwardRow(ZzForwardRule{});
    });
    connect(removeButton, &QPushButton::clicked, this, [this]() {
        const int row = m_forwardTable->currentRow();
        if (row >= 0) {
            m_forwardTable->removeRow(row);
        }
    });

    // —— 4 X11 ——
    // 迁移自 ZzSessionEditDialog.cpp:145-155；嵌入勾选改为直接成员
    auto *x11Page = new QWidget(this);
    auto *x11Form = new QFormLayout(x11Page);
    // X11 转发开关：默认开启对齐 MobaXterm；Windows 走内建 X server，Linux/macOS 依赖本机 X server
    m_x11CheckBox = new QCheckBox(x11Page);
    m_x11CheckBox->setObjectName(QStringLiteral("x11CheckBox"));
    x11Form->addRow(QString(), m_x11CheckBox);
    // X11 嵌入模式（实验）：ZzXsrv 桌面嵌入会话标签页下半区；取消勾选则以独立窗口运行
    m_x11EmbedCheckBox = new QCheckBox(x11Page);
    m_x11EmbedCheckBox->setObjectName(QStringLiteral("x11EmbedCheckBox"));
    x11Form->addRow(QString(), m_x11EmbedCheckBox);
    m_stack->addWidget(x11Page);

    // —— 5 终端：配色方案 ——
    auto *terminalPage = new QWidget(this);
    auto *terminalForm = new QFormLayout(terminalPage);
    m_colorSchemeCombo = new QComboBox(terminalPage);
    m_colorSchemeCombo->addItems(QTermWidget::availableColorSchemes());
    terminalForm->addRow(QString(), m_colorSchemeCombo);
    m_stack->addWidget(terminalPage);

    connect(m_navTree->selectionModel(), &QItemSelectionModel::currentRowChanged,
            this, [this](const QModelIndex &current, const QModelIndex &) {
        if (current.isValid()) {
            m_stack->setCurrentIndex(current.row());
        }
    });

    retranslateUi();
    m_navTree->setCurrentIndex(treeModel->index(0, 0));
}

void ZzSshConfigPage::setProfile(const ZzSessionProfile &profile)
{
    m_nameEdit->setText(profile.name);
    m_groupEdit->setText(profile.groupPath);
    m_hostEdit->setText(profile.protocol == QStringLiteral("local")
                            ? QString() : profile.host);
    m_portSpin->setValue(profile.port == 0 ? 22 : profile.port);
    m_terminalTypeCombo->setCurrentText(
        profile.terminalType.isEmpty()
            ? QStringLiteral("xterm-256color") : profile.terminalType);
    m_encodingCombo->setCurrentText(
        profile.encoding.isEmpty() ? QStringLiteral("UTF-8") : profile.encoding);
    m_keepAliveSpin->setValue(profile.keepAliveIntervalSeconds);
    m_userEdit->setText(profile.userName);
    const int authIndex = m_authCombo->findData(static_cast<int>(profile.authMethod));
    m_authCombo->setCurrentIndex(authIndex >= 0 ? authIndex : 0);
    m_keyPathEdit->setText(profile.privateKeyPath);
    m_keyPassphraseEdit->clear(); // 凭据明文永不回填，只保留占位提示
    m_passwordEdit->clear();
    populateForwardTable(profile.portForwards);
    m_x11CheckBox->setChecked(profile.x11Forwarding);
    m_x11EmbedCheckBox->setChecked(profile.x11EmbedMode);
    const int schemeIndex = m_colorSchemeCombo->findText(profile.colorSchemeName);
    if (schemeIndex >= 0) {
        m_colorSchemeCombo->setCurrentIndex(schemeIndex);
    }
}

void ZzSshConfigPage::applyTo(ZzSessionProfile &profile) const
{
    profile.name = m_nameEdit->text().trimmed();
    profile.groupPath = m_groupEdit->text().trimmed();
    profile.protocol = QStringLiteral("ssh");
    profile.host = m_hostEdit->text().trimmed();
    profile.port = static_cast<quint16>(m_portSpin->value());
    profile.terminalType = m_terminalTypeCombo->currentText().trimmed();
    profile.encoding = m_encodingCombo->currentText().trimmed();
    profile.keepAliveIntervalSeconds = m_keepAliveSpin->value();
    profile.userName = m_userEdit->text().trimmed();
    profile.authMethod =
        static_cast<ZzAuthMethod>(m_authCombo->currentData().toInt());
    profile.privateKeyPath = m_keyPathEdit->text().trimmed();
    profile.portForwards = rulesFromTable();
    profile.x11Forwarding = m_x11CheckBox->isChecked();
    profile.x11EmbedMode = m_x11EmbedCheckBox->isChecked();
    profile.colorSchemeName = m_colorSchemeCombo->currentText();
}

bool ZzSshConfigPage::validateInputs(QString *error, int *pageIndex) const
{
    if (m_nameEdit->text().trimmed().isEmpty()) {
        *error = tr("名称不能为空");
        *pageIndex = GeneralPage;
        return false;
    }
    if (m_hostEdit->text().trimmed().isEmpty()) {
        *error = tr("主机不能为空");
        *pageIndex = ConnectionPage;
        return false;
    }
    const QVector<ZzForwardRule> rules = rulesFromTable();
    for (const ZzForwardRule &rule : rules) {
        const QString ruleError = rule.validate();
        if (!ruleError.isEmpty()) {
            *error = ruleError;
            *pageIndex = ForwardPage;
            return false;
        }
    }
    const QString dupError = ZzForwardRule::validateList(rules);
    if (!dupError.isEmpty()) {
        *error = dupError;
        *pageIndex = ForwardPage;
        return false;
    }
    return true;
}

QString ZzSshConfigPage::enteredPassword() const
{
    return m_passwordEdit->text();
}

QString ZzSshConfigPage::enteredKeyPassphrase() const
{
    return m_keyPassphraseEdit->text();
}

void ZzSshConfigPage::focusPage(int pageIndex)
{
    if (pageIndex < 0 || pageIndex >= PageCount) {
        return;
    }
    m_navTree->setCurrentIndex(m_navTree->model()->index(pageIndex, 0));
    m_stack->setCurrentIndex(pageIndex);
}

void ZzSshConfigPage::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QWidget::changeEvent(event);
}

void ZzSshConfigPage::retranslateUi()
{
    // 树节点文本（顺序即 ZzSshPageIndex）
    auto *treeModel = qobject_cast<QStandardItemModel *>(m_navTree->model());
    const QStringList pageNames = {
        tr("常规"), tr("连接"), tr("认证"),
        tr("端口转发"), tr("X11"), tr("终端")};
    for (int i = 0; i < PageCount && i < pageNames.size(); ++i) {
        if (auto *item = treeModel->item(i, 0)) {
            item->setText(pageNames.at(i));
        }
    }

    // QFormLayout 行标签按字段部件反查（labelForField，同 ZzSessionEditDialog 旧式）
    const auto setRowLabel = [](QFormLayout *form, QWidget *field,
                                const QString &text) {
        if (auto *label = qobject_cast<QLabel *>(form->labelForField(field))) {
            label->setText(text);
        }
    };
    auto *generalForm =
        qobject_cast<QFormLayout *>(m_stack->widget(GeneralPage)->layout());
    setRowLabel(generalForm, m_nameEdit, tr("名称："));
    setRowLabel(generalForm, m_groupEdit, tr("分组路径："));

    auto *connectionForm =
        qobject_cast<QFormLayout *>(m_stack->widget(ConnectionPage)->layout());
    setRowLabel(connectionForm, m_hostEdit, tr("主机："));
    setRowLabel(connectionForm, m_portSpin, tr("端口："));
    setRowLabel(connectionForm, m_terminalTypeCombo, tr("终端类型："));
    setRowLabel(connectionForm, m_encodingCombo, tr("编码："));
    setRowLabel(connectionForm, m_keepAliveSpin, tr("保活间隔："));

    auto *authForm =
        qobject_cast<QFormLayout *>(m_stack->widget(AuthPage)->layout());
    setRowLabel(authForm, m_userEdit, tr("用户名："));
    setRowLabel(authForm, m_authCombo, tr("认证方式："));
    // 私钥路径行的字段部件是包裹容器（行编辑框 + 浏览按钮的父部件）
    setRowLabel(authForm, m_keyPathEdit->parentWidget(), tr("私钥路径："));
    setRowLabel(authForm, m_keyPassphraseEdit, tr("私钥口令："));
    setRowLabel(authForm, m_passwordEdit, tr("密码："));

    auto *x11Form =
        qobject_cast<QFormLayout *>(m_stack->widget(X11Page)->layout());
    setRowLabel(x11Form, m_x11CheckBox, tr("图形转发："));
    setRowLabel(x11Form, m_x11EmbedCheckBox, tr("显示方式："));

    auto *terminalForm =
        qobject_cast<QFormLayout *>(m_stack->widget(TerminalPage)->layout());
    setRowLabel(terminalForm, m_colorSchemeCombo, tr("配色方案："));

    m_groupEdit->setPlaceholderText(tr("如：生产环境/Web 服务器"));
    m_keyPathEdit->setPlaceholderText(tr("私钥路径（公钥认证）"));
    m_keyPassphraseEdit->setPlaceholderText(tr("私钥口令（无口令留空）"));
    m_passwordEdit->setPlaceholderText(tr("登录密码"));

    m_keepAliveSpin->setSpecialValueText(tr("关闭"));
    m_keepAliveSpin->setSuffix(tr(" 秒"));
    m_keepAliveSpin->setToolTip(tr("保活间隔，单位秒；0 为关闭"));

    m_authCombo->setItemText(0, tr("SSH Agent"));
    m_authCombo->setItemText(1, tr("公钥文件"));
    m_authCombo->setItemText(2, tr("密码"));

    if (auto *browseButton =
            findChild<QPushButton *>(QStringLiteral("browseKeyButton"))) {
        browseButton->setText(tr("浏览…"));
    }

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
    m_x11EmbedCheckBox->setText(tr("嵌入标签页显示（实验；否则独立窗口）"));
    m_x11EmbedCheckBox->setToolTip(tr(
        "仅 Windows 生效：X11 桌面嵌入会话标签页内；取消勾选则 X 程序以独立窗口显示"));
}

void ZzSshConfigPage::populateForwardTable(const QVector<ZzForwardRule> &rules)
{
    m_forwardTable->setRowCount(0);
    for (const ZzForwardRule &rule : rules) {
        appendForwardRow(rule);
    }
}

void ZzSshConfigPage::appendForwardRow(const ZzForwardRule &rule)
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

QVector<ZzForwardRule> ZzSshConfigPage::rulesFromTable() const
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
