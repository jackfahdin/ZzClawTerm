# 新建/编辑会话配置窗口 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 用「顶部 ZzTabWidget 协议分页 + SSH 页内左树右页」的新窗口 `ZzSessionConfigWindow` 替代单页表单 `ZzSessionEditDialog`，新建与编辑共用。

**架构：** 窗口壳（`src/dialog/ZzSessionConfigWindow`）持有 `ZzFluentUI::ZzTabWidget` 与协议页；每个协议页（`ZzSshConfigPage` / `ZzLocalShellConfigPage`）实现统一接口 `setProfile/applyTo/validateInputs`，页内左侧 `QTreeView+QStandardItemModel` 树导航 + 右侧 `QStackedWidget`。凭据落库与校验调度集中在窗口 `accept()`，逻辑从旧对话框原样迁移。

**技术栈：** C++20、Qt 6.11 Widgets、ZzFluentUI::ZzTabWidget（`ZzFluentUI/ZzTabWidget.h`，`Zz::FluentUI` 已链接）、QTest。

**规格：** `docs/superpowers/specs/2026-09-03-session-config-window-design.md`

## 文件结构

| 文件 | 职责 |
|------|------|
| 创建 `src/dialog/ZzSshConfigPage.h/.cpp` | SSH 协议页：六节点树 + 六张配置页 + 统一接口 |
| 创建 `src/dialog/ZzLocalShellConfigPage.h/.cpp` | 本地 Shell 协议页：单节点树 + 统一接口（验证扩展模式） |
| 创建 `src/dialog/ZzSessionConfigWindow.h/.cpp` | 窗口壳：tab 装配、确定/取消、校验调度、凭据落库 |
| 修改 `src/CMakeLists.txt:26-27` | 删 panel/ZzSessionEditDialog.*，加三个新文件 |
| 修改 `src/panel/ZzSessionPanel.cpp:197-218` | newSession/editSession 换用新窗口类 |
| 删除 `src/panel/ZzSessionEditDialog.h/.cpp` | 被新窗口替代 |
| 重命名 `tests/unit/tst_ZzSessionEditDialog.cpp` → `tests/unit/tst_ZzSessionConfigWindow.cpp` | 全部用例迁移 + 新增 tab/树用例 |
| 修改 `tests/CMakeLists.txt:59` | 测试目标改名 |
| 修改 `src/i18n/zzclawterm_en.ts`（构建产物再生成） | 新文案英文翻译 |

## 关键约定（全部任务遵守）

- 注释：Doxygen 风格简体中文（见 AGENTS 约定与现有代码）。
- i18n：每个新类 `retranslateUi()` 单一路径 + `changeEvent` 响应 `QEvent::LanguageChange`（参照 `ZzSessionEditDialog.cpp:166-248`）。
- objectName 保持兼容（测试依赖 findChild）：`authCombo`、`keyPassphraseEdit`、`passwordEdit`、`forwardTable`、`addForwardButton`、`removeForwardButton`、`x11CheckBox`、`x11EmbedCheckBox`。新增：`protocolTabWidget`、`sshNavTree`、`localNavTree`。
- local 会话的 Shell 路径存 `profile.host` 字段（现有契约，不变）。
- 编辑模式：构造后按 `existing.protocol` 预选对应 tab（"ssh"→0，"local"→1）并回填；允许切 tab 改协议。

---

### 任务 1：ZzSshConfigPage —— SSH 协议页（左树 + 六配置页）

**文件：**
- 创建：`src/dialog/ZzSshConfigPage.h`
- 创建：`src/dialog/ZzSshConfigPage.cpp`
- 测试：`tests/unit/tst_ZzSshConfigPage.cpp`（新建）
- 修改：`tests/CMakeLists.txt`（注册新测试）

- [ ] **步骤 1：编写失败的测试** `tests/unit/tst_ZzSshConfigPage.cpp`

```cpp
#include <QtTest>
#include <QCheckBox>
#include <QComboBox>
#include <QLineEdit>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTableWidget>
#include <QTreeView>

#include "dialog/ZzSshConfigPage.h"

/**
 * @brief ZzSshConfigPage 单元测试：树导航、页面切换与 profile 往返。
 */
class tst_ZzSshConfigPage : public QObject
{
    Q_OBJECT

    static ZzSessionProfile sampleProfile()
    {
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("隧道机");
        profile.groupPath = QStringLiteral("生产/Web");
        profile.protocol = QStringLiteral("ssh");
        profile.host = QStringLiteral("10.0.0.1");
        profile.port = 2222;
        profile.userName = QStringLiteral("root");
        profile.authMethod = ZzAuthMethod::PrivateKey;
        profile.privateKeyPath = QStringLiteral("/home/u/.ssh/id_ed25519");
        profile.terminalType = QStringLiteral("xterm-256color");
        profile.encoding = QStringLiteral("UTF-8");
        profile.colorSchemeName = QStringLiteral("Linux");
        profile.keepAliveIntervalSeconds = 30;
        profile.x11Forwarding = true;
        profile.x11EmbedMode = true;
        profile.portForwards = {
            {ZzForwardRule::Type::Local, QStringLiteral("127.0.0.1"), 13306,
             QStringLiteral("db.internal"), 3306},
        };
        return profile;
    }

private slots:
    /** @brief 树有六个节点，切节点同步切 QStackedWidget 页。 */
    void treeSwitchesStackedPage()
    {
        ZzSshConfigPage page;
        auto *tree = page.findChild<QTreeView *>(QStringLiteral("sshNavTree"));
        auto *stack = page.findChild<QStackedWidget *>();
        QVERIFY(tree && stack);
        QCOMPARE(tree->model()->rowCount(), 6);
        QCOMPARE(stack->count(), 6);
        const QModelIndex index = tree->model()->index(3, 0); // 端口转发
        tree->setCurrentIndex(index);
        QCOMPARE(stack->currentIndex(), 3);
    }

    /** @brief setProfile 回填全部字段；applyTo 无损写回。 */
    void profileRoundTrip()
    {
        ZzSshConfigPage page;
        const ZzSessionProfile original = sampleProfile();
        page.setProfile(original);

        // 抽查回填：转发表一行、X11 两勾选、认证方式与端口
        auto *table = page.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        auto *x11 = page.findChild<QCheckBox *>(QStringLiteral("x11CheckBox"));
        auto *embed = page.findChild<QCheckBox *>(QStringLiteral("x11EmbedCheckBox"));
        auto *auth = page.findChild<QComboBox *>(QStringLiteral("authCombo"));
        QVERIFY(table && x11 && embed && auth);
        QCOMPARE(table->rowCount(), 1);
        QVERIFY(x11->isChecked());
        QVERIFY(embed->isChecked());
        QCOMPARE(auth->currentData().toInt(),
                 static_cast<int>(ZzAuthMethod::PrivateKey));

        ZzSessionProfile out;
        // applyTo 只覆盖 SSH 页负责的字段；id/credentialId 由窗口层保留
        out.id = original.id;
        page.applyTo(out);
        QCOMPARE(out.name, original.name);
        QCOMPARE(out.groupPath, original.groupPath);
        QCOMPARE(out.protocol, QStringLiteral("ssh"));
        QCOMPARE(out.host, original.host);
        QCOMPARE(out.port, original.port);
        QCOMPARE(out.userName, original.userName);
        QCOMPARE(out.authMethod, original.authMethod);
        QCOMPARE(out.privateKeyPath, original.privateKeyPath);
        QCOMPARE(out.terminalType, original.terminalType);
        QCOMPARE(out.encoding, original.encoding);
        QCOMPARE(out.colorSchemeName, original.colorSchemeName);
        QCOMPARE(out.keepAliveIntervalSeconds, original.keepAliveIntervalSeconds);
        QCOMPARE(out.x11Forwarding, original.x11Forwarding);
        QCOMPARE(out.x11EmbedMode, original.x11EmbedMode);
        QCOMPARE(out.portForwards.size(), 1);
        QCOMPARE(out.portForwards.at(0).targetHost, QStringLiteral("db.internal"));
    }

    /** @brief 主机为空时 validateInputs 报错并给出出错页索引（连接=1）。 */
    void validateReportsEmptyHost()
    {
        ZzSshConfigPage page;
        page.setProfile(sampleProfile());
        auto *hostEdit = page.findChild<QLineEdit *>(QStringLiteral("hostEdit"));
        QVERIFY(hostEdit);
        hostEdit->clear();
        QString error;
        int pageIndex = -1;
        QVERIFY(!page.validateInputs(&error, &pageIndex));
        QVERIFY(!error.isEmpty());
        QCOMPARE(pageIndex, 1);
    }

    /** @brief 非法转发规则（监听端口 0）报错并定位到端口转发页（索引 3）。 */
    void validateReportsInvalidForwardRule()
    {
        ZzSshConfigPage page;
        page.setProfile(sampleProfile());
        auto *table = page.findChild<QTableWidget *>(QStringLiteral("forwardTable"));
        table->item(0, 2)->setText(QStringLiteral("0"));
        QString error;
        int pageIndex = -1;
        QVERIFY(!page.validateInputs(&error, &pageIndex));
        QCOMPARE(pageIndex, 3);
    }
};

QTEST_MAIN(tst_ZzSshConfigPage)
#include "tst_ZzSshConfigPage.moc"
```

- [ ] **步骤 2：注册测试并运行验证失败**

`tests/CMakeLists.txt` 在 `zz_add_qtest(tst_ZzSessionEditDialog ...)` 前加一行：

```cmake
zz_add_qtest(tst_ZzSshConfigPage unit/tst_ZzSshConfigPage.cpp)
```

运行：`cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release --target tst_ZzSshConfigPage`
预期：编译失败（`dialog/ZzSshConfigPage.h` 不存在）

- [ ] **步骤 3：编写头文件** `src/dialog/ZzSshConfigPage.h`

```cpp
#pragma once

#include <QtWidgets/QWidget>

#include "session/ZzSessionProfile.h"

class QCheckBox;
class QComboBox;
class QFormLayout;
class QLineEdit;
class QSpinBox;
class QStackedWidget;
class QTableWidget;
class QTreeView;

/**
 * @brief SSH 协议配置页：左侧树形导航 + 右侧 QStackedWidget 配置页。
 *
 * 六个树节点（栈页索引一致）：0 常规 / 1 连接 / 2 认证 / 3 端口转发 /
 * 4 X11 / 5 终端。不接触凭据库；凭据文本经 accessors 暴露给窗口层处理。
 */
class ZzSshConfigPage : public QWidget
{
    Q_OBJECT
public:
    explicit ZzSshConfigPage(QWidget *parent = nullptr);

    /** @brief 按 profile 回填全部字段（新建传默认值亦可）。 */
    void setProfile(const ZzSessionProfile &profile);
    /** @brief 把表单内容写回 profile（不触碰 id 与两个 credentialId）。 */
    void applyTo(ZzSessionProfile &profile) const;
    /**
     * @brief 校验表单。
     * @param error 输出错误文案（已翻译）。
     * @param pageIndex 输出出错字段所在栈页索引（用于窗口层聚焦）。
     * @return 全部合法返回 true。
     */
    [[nodiscard]] bool validateInputs(QString *error, int *pageIndex) const;

    /** @brief 用户新输入的密码（空串=未输入，窗口层据此保留原引用）。 */
    [[nodiscard]] QString enteredPassword() const;
    /** @brief 用户新输入的私钥口令（空串=未输入）。 */
    [[nodiscard]] QString enteredKeyPassphrase() const;
    /** @brief 切换到指定栈页并选中对应树节点（校验失败聚焦用）。 */
    void focusPage(int pageIndex);

protected:
    /** @brief LanguageChange 时重设全部文本。 */
    void changeEvent(QEvent *event) override;

private:
    /** @brief 重设全部用户可见文本（构造与 LanguageChange 共用单一路径）。 */
    void retranslateUi();
    /** @brief 用给定规则列表填充规则表。 */
    void populateForwardTable(const QVector<ZzForwardRule> &rules);
    /** @brief 向表格追加一行（默认值或给定规则）。 */
    void appendForwardRow(const ZzForwardRule &rule);
    /** @brief 从表格读出规则列表（未校验）。 */
    [[nodiscard]] QVector<ZzForwardRule> rulesFromTable() const;

    QTreeView *m_navTree;
    QStackedWidget *m_stack;
    // 0 常规
    QLineEdit *m_nameEdit;
    QLineEdit *m_groupEdit;
    // 1 连接
    QLineEdit *m_hostEdit;
    QSpinBox *m_portSpin;
    QComboBox *m_terminalTypeCombo;
    QComboBox *m_encodingCombo;
    QSpinBox *m_keepAliveSpin;
    // 2 认证
    QLineEdit *m_userEdit;
    QComboBox *m_authCombo;
    QLineEdit *m_keyPathEdit;
    QLineEdit *m_keyPassphraseEdit;
    QLineEdit *m_passwordEdit;
    // 3 端口转发
    QTableWidget *m_forwardTable;
    // 4 X11
    QCheckBox *m_x11CheckBox;
    QCheckBox *m_x11EmbedCheckBox;
    // 5 终端
    QComboBox *m_colorSchemeCombo;
};
```

- [ ] **步骤 4：编写实现** `src/dialog/ZzSshConfigPage.cpp`

结构与要点（迁移来源均指 `src/panel/ZzSessionEditDialog.cpp`，迁移时只改宿主与成员名，逻辑不动）：

```cpp
#include "ZzSshConfigPage.h"

#include <QtCore/QEvent>
#include <QtCore/QStandardItemModel>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QComboBox>
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
```

构造（骨架，布局即规格线框）：

```cpp
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
    // 私钥路径行：QLineEdit + 「浏览…」QPushButton 水平布局，QFileDialog::getOpenFileName
    // objectName 保持：authCombo / keyPassphraseEdit / passwordEdit
    // keyPassphraseEdit、passwordEdit 均 setEchoMode(QLineEdit::Password)

    // —— 3 端口转发 ——
    // 迁移自 ZzSessionEditDialog.cpp:115-143（五列表 + 增删按钮 +
    // populateForwardTable/appendForwardRow/rulesFromTable 三个方法原样迁移，
    // 仅 populateForwardTable 改接收参数版签名，见头文件）

    // —— 4 X11 ——
    // 迁移自 ZzSessionEditDialog.cpp:145-155；x11EmbedCheckBox 由 findChild
    // 反查改为直接成员 m_x11EmbedCheckBox（新代码不留历史包袱）

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
    m_navTree->setCurrentIndex(treeModel->index(0, 0));

    retranslateUi();
}
```

`setProfile`（回填，逐字段）：

```cpp
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
```

`applyTo` / `validateInputs`：

```cpp
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
```

`focusPage`：

```cpp
void ZzSshConfigPage::focusPage(int pageIndex)
{
    if (pageIndex < 0 || pageIndex >= PageCount) {
        return;
    }
    m_navTree->setCurrentIndex(m_navTree->model()->index(pageIndex, 0));
    m_stack->setCurrentIndex(pageIndex);
}
```

其余方法：`enteredPassword()`/`enteredKeyPassphrase()` 返回对应 QLineEdit 文本；`populateForwardTable/appendForwardRow/rulesFromTable` 从 `ZzSessionEditDialog.cpp:380-430` 原样迁移；`retranslateUi()` 覆盖全部行标签/树节点文本/占位符/工具提示（迁移 `:166-240` 对应项并补新字段：连接页五行、终端页「配色方案：」、保活间隔「秒；0 为关闭」）；`changeEvent` 同旧式。

- [ ] **步骤 5：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release --target tst_ZzSshConfigPage && QT_QPA_PLATFORM=offscreen ./build/linux-gcc-release/tests/tst_ZzSshConfigPage`
预期：4 用例全 PASS

- [ ] **步骤 6：Commit**

```bash
git add src/dialog/ZzSshConfigPage.h src/dialog/ZzSshConfigPage.cpp \
        tests/unit/tst_ZzSshConfigPage.cpp tests/CMakeLists.txt
git commit -m "feat(会话): SSH 协议配置页（左树导航 + 六配置页）"
```

---

### 任务 2：ZzLocalShellConfigPage + ZzSessionConfigWindow 窗口壳

**文件：**
- 创建：`src/dialog/ZzLocalShellConfigPage.h/.cpp`
- 创建：`src/dialog/ZzSessionConfigWindow.h/.cpp`
- 测试：重命名 `tests/unit/tst_ZzSessionEditDialog.cpp` → `tests/unit/tst_ZzSessionConfigWindow.cpp` 并改造
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：迁移并改造窗口级测试**

`git mv tests/unit/tst_ZzSessionEditDialog.cpp tests/unit/tst_ZzSessionConfigWindow.cpp`，改造点：
- include 改 `dialog/ZzSessionConfigWindow.h`，类名/构造改 `ZzSessionConfigWindow`
- 现有 11 个用例全部保留：对话框级 `findChild` 能递归找到 tab 页内控件（objectName 兼容），断言不变
- 新增两个用例：

```cpp
    /** @brief tab 切换决定产出协议：激活本地 tab 时 protocol=="local"。 */
    void activeTabDeterminesProtocol()
    {
        auto store = makeStore();
        ZzSessionConfigWindow dlg(store.get());
        auto *tabs = dlg.findChild<QTabWidget *>(QStringLiteral("protocolTabWidget"));
        QVERIFY(tabs);
        QCOMPARE(tabs->count(), 2);
        tabs->setCurrentIndex(1); // 本地 Shell
        auto *shellEdit = dlg.findChild<QLineEdit *>(QStringLiteral("shellEdit"));
        QVERIFY(shellEdit);
        shellEdit->setText(QStringLiteral("/bin/bash"));
        auto *nameEdit = dlg.findChild<QLineEdit *>(QStringLiteral("nameEdit"));
        nameEdit->setText(QStringLiteral("本机"));
        QSignalSpy finishSpy(&dlg, &QDialog::finished);
        clickOk(dlg);
        QCOMPARE(finishSpy.count(), 1);
        QCOMPARE(dlg.profile().protocol, QStringLiteral("local"));
        QCOMPARE(dlg.profile().host, QStringLiteral("/bin/bash"));
    }

    /** @brief 编辑 local 会话：构造后预选本地 tab。 */
    void editLocalProfilePreselectsLocalTab()
    {
        auto store = makeStore();
        ZzSessionProfile profile;
        profile.id = QUuid::createUuid();
        profile.name = QStringLiteral("本机");
        profile.protocol = QStringLiteral("local");
        profile.host = QStringLiteral("/bin/zsh");
        ZzSessionConfigWindow dlg(store.get(), profile);
        auto *tabs = dlg.findChild<QTabWidget *>(QStringLiteral("protocolTabWidget"));
        QCOMPARE(tabs->currentIndex(), 1);
        auto *shellEdit = dlg.findChild<QLineEdit *>(QStringLiteral("shellEdit"));
        QCOMPARE(shellEdit->text(), QStringLiteral("/bin/zsh"));
    }
```

注意：`nameEdit`/`shellEdit` 两个 objectName 需在协议页实现时补上（任务 1 的 `m_nameEdit` 与此任务的 shell 编辑框）。

- [ ] **步骤 2：运行验证失败**

`tests/CMakeLists.txt` 把 `zz_add_qtest(tst_ZzSessionEditDialog unit/tst_ZzSessionEditDialog.cpp)` 改为：

```cmake
zz_add_qtest(tst_ZzSessionConfigWindow unit/tst_ZzSessionConfigWindow.cpp)
```

运行构建：预期编译失败（`dialog/ZzSessionConfigWindow.h` 不存在）

- [ ] **步骤 3：编写 ZzLocalShellConfigPage**

`src/dialog/ZzLocalShellConfigPage.h`：与 `ZzSshConfigPage` 同接口（`setProfile/applyTo/validateInputs/focusPage`），成员：

```cpp
    QTreeView *m_navTree;    // objectName localNavTree，仅一个「常规」节点
    QStackedWidget *m_stack; // 一页：名称 / 分组路径 / Shell 程序
    QLineEdit *m_nameEdit;   // objectName nameEdit
    QLineEdit *m_groupEdit;
    QLineEdit *m_shellEdit;  // objectName shellEdit
```

实现要点：`applyTo` 写 `protocol="local"`、`host=shellEdit 文本`；`validateInputs` 只查名称非空（Shell 路径留空=系统默认，现有行为）；树/栈结构照任务 1 同款（单节点）。

- [ ] **步骤 4：编写 ZzSessionConfigWindow**

`src/dialog/ZzSessionConfigWindow.h`：

```cpp
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
```

`src/dialog/ZzSessionConfigWindow.cpp` 要点：

```cpp
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

    // 编辑：按协议预选 tab 并回填；新建：两页都给默认值（groupPathPrefix）
    m_sshPage->setProfile(m_profile);
    m_localPage->setProfile(m_profile);
    m_tabs->setCurrentIndex(
        m_profile.protocol == QStringLiteral("local") ? 1 : 0);

    retranslateUi();
}
```

`accept()`：

```cpp
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
```

`persistCredentials`：从 `ZzSessionEditDialog.cpp:301-375` 原样迁移密码与私钥口令两段（新密码→`addCredential`→锁定就地 `ZzMasterPasswordDialog::ensureUnlocked` 重试→失败弹框返回 false→成功后删旧凭据；留空→保留原引用；切离对应认证→删旧凭据清引用）。差异：密码/口令文本从 `m_sshPage->enteredPassword()` / `enteredKeyPassphrase()` 取，仅当激活页是 SSH 页时处理；激活本地页时按「切离密码/公钥认证」分支清掉两个原引用。

`retranslateUi()`：`setWindowTitle(isNew ? tr("新建会话") : tr("编辑会话"))`、`m_tabs->setTabText(0, tr("SSH"))`、`setTabText(1, tr("本地 Shell"))`。

- [ ] **步骤 5：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release --target tst_ZzSessionConfigWindow && QT_QPA_PLATFORM=offscreen ./build/linux-gcc-release/tests/tst_ZzSessionConfigWindow`
预期：13 用例全 PASS（11 迁移 + 2 新增）

- [ ] **步骤 6：Commit**

```bash
git add src/dialog/ZzLocalShellConfigPage.h src/dialog/ZzLocalShellConfigPage.cpp \
        src/dialog/ZzSessionConfigWindow.h src/dialog/ZzSessionConfigWindow.cpp \
        tests/unit/tst_ZzSessionConfigWindow.cpp tests/CMakeLists.txt
git commit -m "feat(会话): 会话配置窗口壳（协议 tab）与本地 Shell 配置页"
```

---

### 任务 3：接线替换、删旧文件、翻译与全量回归

**文件：**
- 修改：`src/CMakeLists.txt:26-27`
- 修改：`src/panel/ZzSessionPanel.cpp:197-218`（及文件头 include）
- 删除：`src/panel/ZzSessionEditDialog.h`、`src/panel/ZzSessionEditDialog.cpp`
- 修改：`src/i18n/zzclawterm_en.ts`
- 修改：`tests/unit/tst_ZzSessionPanel.cpp`（若引用旧对话框类名）

- [ ] **步骤 1：替换调用方与构建清单**

`src/CMakeLists.txt`：删 `panel/ZzSessionEditDialog.h/.cpp` 两行，加：

```cmake
    dialog/ZzSshConfigPage.h
    dialog/ZzSshConfigPage.cpp
    dialog/ZzLocalShellConfigPage.h
    dialog/ZzLocalShellConfigPage.cpp
    dialog/ZzSessionConfigWindow.h
    dialog/ZzSessionConfigWindow.cpp
```

`src/panel/ZzSessionPanel.cpp`：include 改 `dialog/ZzSessionConfigWindow.h`；`newSession`/`editSession` 内 `ZzSessionEditDialog dialog(...)` 改 `ZzSessionConfigWindow dialog(...)`（构造签名一致，其余不动）。

`src/panel/ZzSessionPanel.h`：若前置声明/注释提到旧类名，同步更新。

检查其他引用：`grep -rn "ZzSessionEditDialog" src/ tests/` 应只剩测试迁移前的历史引用（tst_ZzSessionPanel 若有则改名替换）。

- [ ] **步骤 2：删除旧对话框文件**

```bash
git rm src/panel/ZzSessionEditDialog.h src/panel/ZzSessionEditDialog.cpp
```

- [ ] **步骤 3：全量构建 + 运行全部测试**

运行：`cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release -j$(nproc) && cd build/linux-gcc-release && ctest --output-on-failure -LE perf`
预期：全绿（含两个新测试目标）

- [ ] **步骤 4：更新英文翻译**

运行构建后执行（qt_add_translations 管线）：
`cmake --build --preset linux-gcc-release --target update_translations 2>/dev/null || cmake --build --preset linux-gcc-release --target all_translations`
打开 `src/i18n/zzclawterm_en.ts`，为新增条目（树节点「常规/连接/认证/端口转发/X11/终端」、tab「本地 Shell」、新字段标签、错误文案等）补英文译文，消除全部 `type="unfinished"`。重跑 `tst_ZzLanguageManager` 与 `tst_ZzSessionConfigWindow` 验证。

- [ ] **步骤 5：Commit**

```bash
git add -A
git commit -m "refactor(会话): 会话配置窗口替代旧单页表单并接入面板

ZzSessionPanel 新建/编辑入口换用 ZzSessionConfigWindow；删除
ZzSessionEditDialog；补充新文案英文翻译。"
```

---

## 自检记录

- 规格覆盖：布局（任务 1/2）、六节点内容（任务 1）、本地页（任务 2）、编辑预选 tab（任务 2 构造+用例）、校验聚焦（focusPage + accept）、凭据落库（persistCredentials）、调用方最小改动（任务 3）、i18n（各任务 retranslateUi + 任务 3 步骤 4）、测试迁移与新增（任务 1/2）、删除旧文件（任务 3）。
- 新增 UI 字段（终端类型/编码/保活/配色）对应 profile 已有字段，不违反「不新增数据字段」。
- objectName 兼容表保证 11 个旧用例无需改断言。
