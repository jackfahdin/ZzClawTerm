# 新建/编辑会话配置窗口 设计规格

日期：2026-09-03
状态：已批准（用户于 2026-09-03 确认设计稿 v2）

## 背景与目标

现有 `ZzSessionEditDialog`（`src/panel/`）是单一 `QFormLayout` 纵排全部字段的简陋表单，
新建与编辑共用。本特性将其替换为一个独立的会话配置窗口：顶部 `ZzTabWidget` 按协议分页，
SSH 页内左侧树形导航 + 右侧配置页，为后续串口、VNC 等协议预留结构化扩展点。

目标：
- 新建与编辑会话共用同一窗口（一套 UI，载入/回填逻辑一致）
- 布局：顶部协议 Tab（方案 A），SSH 页内「左树 + 右页」，右侧表单严格网格对齐
- 本期只实现 SSH 与本地 Shell 两个 tab；串口/VNC 仅在代码结构上留扩展点，不出占位 tab

非目标：
- 不新增 `ZzSessionProfile` 数据字段（所有 UI 字段与现有字段一一对应）
- 不实现串口/VNC 配置页
- 不改会话面板、连接流程、`ZzCredentialStore` 的任何行为

## 总体布局

```
ZzSessionConfigWindow（模态 QDialog，初始约 760×560，可调整大小）
├── ZzFluentUI::ZzTabWidget（顶部协议切换）
│   ├── tab「SSH」      → ZzSshConfigPage
│   └── tab「本地Shell」 → ZzLocalShellConfigPage
└── QDialogButtonBox（确定 / 取消）
```

协议页内部（以 SSH 为例）：

```
ZzSshConfigPage
├── 左侧：QTreeView（树形导航，六节点，单选，不可编辑）
└── 右侧：QStackedWidget（六张配置页，随树选切换）
```

右侧每张配置页统一使用 `QFormLayout`：标签列右对齐、字段列等宽拉伸；
行内带按钮的字段（如私钥路径 + 浏览…）用水平布局嵌入，保持左右边缘对齐。

## SSH 树节点与页面内容

| 树节点 | 右侧页面字段 | 对应 ZzSessionProfile 字段 |
|--------|--------------|---------------------------|
| 常规 | 名称、分组路径 | `name`、`groupPath` |
| 连接 | 主机、端口、终端类型、编码、保活间隔 | `host`、`port`、`terminalType`、`encoding`、`keepAliveIntervalSeconds` |
| 认证 | 用户名、认证方式（Agent/私钥/密码）、私钥路径+浏览、私钥口令、密码 | `userName`、`authMethod`、`privateKeyPath`、`keyPassphraseCredentialId`、`credentialId` |
| 端口转发 | 规则表格（类型/监听地址/监听端口/目标地址/目标端口）+ 增删按钮 | `portForwards` |
| X11 | X11 转发开关、嵌入模式开关 | `x11Forwarding`、`x11EmbedMode` |
| 终端 | 配色方案 | `colorSchemeName` |

「本地Shell」tab 结构相同但更简：树只有一个「常规」节点
（名称、分组路径、Shell 路径——沿用现有契约，Shell 路径存 `host` 字段）。
该页用于验证「每个协议 tab 结构独立」的扩展模式。

## 组件设计

新代码集中在 `src/dialog/`：

- `ZzSessionConfigWindow`（QDialog）
  - 构造：`(ZzCredentialStore *store, const ZzSessionProfile &existing,
    const QString &groupPathPrefix, QWidget *parent)`；`existing.id` 为空即新建
  - `ZzSessionProfile profile() const`：仅在 Accepted 后有意义，返回完整值类型
  - `accept()` 重写：校验 → 收集各协议页表单 → 凭据落库 → 写回工作副本
  - 持有 `ZzTabWidget` 与各协议页；确定时从**当前激活 tab** 收集 profile
    （协议类型由激活 tab 决定，与现有 `m_protocolCombo` 行为对应）
  - 编辑模式：构造后按 `existing.protocol` 预选中对应 tab 并回填字段；
    允许用户切 tab 改协议（与现有协议下拉可改的行为一致）
- `ZzSshConfigPage`（QWidget）：左树 + QStackedWidget；暴露
  `setProfile(const ZzSessionProfile&)` / `applyTo(ZzSessionProfile&)` /
  `validateInputs(QString *error)`，不直接接触凭据库
- `ZzLocalShellConfigPage`（QWidget）：同上接口，单节点树
- 凭据交互（密码/私钥口令 → `ZzCredentialStore::addCredential`、
  锁定时弹 `ZzMasterPasswordDialog::ensureUnlocked`、留空保留原
  credentialId）由 `ZzSessionConfigWindow::accept()` 统一处理，
  逻辑从旧对话框 `ZzSessionEditDialog::accept()` 原样迁移

校验规则（沿用现有，不变）：
- 名称非空；SSH 主机非空；本地 Shell 路径非空
- 端口转发规则逐条 `ZzForwardRule::validate()` + `validateList()` 去重
- 校验失败：切到出错字段所在 tab 与树节点，弹 `QMessageBox` 提示

调用方改动（最小）：
- `ZzSessionPanel::newSession` / `editSession`：对话框类名替换，其余不动
- 旧文件 `src/panel/ZzSessionEditDialog.{h,cpp}` 删除

## i18n

沿用项目单一路径模式：每个新类实现 `retranslateUi()`，构造末尾调用；
`changeEvent` 响应 `QEvent::LanguageChange` 重跑。树节点文本、tab 标题、
按钮文案全部走 `tr()`，英文翻译补入 `zzclawterm_en.ts`。

## 错误处理

- 凭据库锁定且用户取消解锁：放弃本次确定（留在窗口内，与现有行为一致）
- 凭据写入失败：弹错误框并中止 accept
- 窗口打开期间不持久化任何东西；只有 Accepted 才写模型与凭据库

## 测试

- 现有 `tst_ZzSessionEditDialog` 迁移为 `tst_ZzSessionConfigWindow`，保留全部场景：
  新建默认值、编辑往返（profile → UI → profile 字段无损）、校验拦截、
  凭据落库与留空保留、分组前缀回填
- 新增：tab 切换协议正确性（激活 SSH tab 产出 protocol="ssh"）、
  树节点切换右侧页、本地 Shell 页结构（单节点）
- `tst_ZzSessionPanel` 中涉及对话框的部分随调用方改名调整

## 扩展点（非本期实现）

- 新增协议 = 新建 `ZzXxxConfigPage`（实现 setProfile/applyTo/validateInputs 接口）
  + 窗口构造中一行 `addTab`
- `ZzSessionProfile` 未来扩展协议专属字段时，UI 结构无需变化
