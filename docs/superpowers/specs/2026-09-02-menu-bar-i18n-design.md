# ZzClawTerm 标题栏菜单 + 完整 i18n 设计

- **日期**：2026-09-02
- **状态**：已批准（用户确认：标题栏嵌入菜单、完整 i18n 迁移）

## 1. 目标

应用加入菜单体系，功能不再依赖侧边栏右键：

- 会话菜单：新建会话
- 视图菜单：主题（跟随系统/浅色/深色/高对比）、终端主题（精选配色 + 更多方案入口）、语言（跟随系统/简体中文/English）
- 帮助菜单：关于对话框、打开日志目录、GitHub 仓库链接
- 中间允许后续插入新菜单，位置不定死

同时完成应用全量 i18n 设施与文案迁移（用户选择"完整 i18n 迁移"档）。

## 2. 架构

```
ZzAppShell::assemble()
  └─ ZzMenuBarService（新增，src/menu/）
       ├─ 拿 ZzApplicationWindow::titleBar()->menuBar() 装配三个 QMenu
       ├─ 动作转发：shell 现有能力 / ZzSessionPanel / themeController / ZzLanguageManager
       └─ retranslate()：LanguageChange 时重设全部菜单/动作文本
```

要点：

- 框架标题栏自带 `menuBar()`（`ZzFluentTitleBar.h:131`），支持 Compact/Adaptive 自动折叠（窄窗变汉堡按钮），无需自建菜单栏控件
- 菜单装配发生在 `setWindowSetupCallback` 的 `assemble()` 内（官方通道，见库范例 `ZzExampleWindowShellPrivate.cpp:177-205`）
- 新菜单插入方式：后续用 `menuBar()->insertMenu(before, menu)` 或追加，结构上不锁定位置

## 3. 会话菜单

| 项 | 行为 | 复用 |
|---|---|---|
| 新建会话 | 弹 `ZzSessionEditDialog`，接受后落库 | `ZzSessionPanel::newSession`（`src/panel/ZzSessionPanel.cpp:197`） |

- 会话面板是延迟加载工厂（`assemble()` :106-122 注册的 LeftPrimary 面板），菜单点击时：若面板未物化，先经 `ZzWorkspaceShell` 触发会话面板显示（物化），再转发新建动作
- 实现方式：`ZzSessionPanel` 增 public 方法（如 `requestNewSession()`），`ZzAppShell` 暴露转发槽供菜单 connect
- 快捷键：Ctrl+Shift+N

## 4. 视图菜单

### 4.1 主题子菜单（勾选组 QActionGroup）

| 项 | 动作 |
|---|---|
| 跟随系统 / 浅色 / 深色 / 高对比 | `ZzPureApplication::themeController()->setMode(ZzThemeMode::{System,Light,Dark,HighContrast})` |

- 初始化勾选状态从 `themeController()->mode()` 读
- 框架已在 `ZzApplicationWindowPrivate.cpp:99-111` 接线 `themeModeRequested → setMode` 并回写标题栏主题按钮，菜单与标题栏按钮经同一 controller 天然同步

### 4.2 终端主题子菜单（勾选组）

- 精选约 12 个方案：Linux、Solarized、SolarizedLight、Dracula、nord、tokyonight、Catppuccin 四口味、Tango、Ubuntu、Builtin Dark/Light（实际清单以实现时 `availableColorSchemes()` 校验存在性为准，不存在的条目跳过并记 warning）
- 勾选状态与 `ZzAppSettings::colorScheme()`（`src/settings/ZzAppSettings.cpp:64-76`，键 `terminal/colorScheme`，默认 "Linux"）双向同步；选择后走 `ZzAppSettings::setColorScheme` → 既有 `settingsChanged` 广播链路（`src/ZzAppShell.cpp:388-396`）
- 尾部「更多方案…」：跳转设置页路由（`ZzRouteId("settings")`）

### 4.3 语言子菜单（勾选组）

| 项 | 行为 |
|---|---|
| 跟随系统 | `QLocale::system()` 解析 → zh_CN 用中文，其余用英文 |
| 简体中文 / English | 直接装载对应 qm |

- 即改即存：QSettings 键 `app/language`，默认 `system`
- 切换实现：`ZzLanguageManager::apply(locale)` → `installTranslator`/`removeTranslator` → Qt 自动发 `LanguageChange` 事件 → 各 retranslate 钩子刷新
- 失败回退：qm 缺失或翻译缺失时显示源文本（源文案即简体中文，天然兜底）

## 5. 帮助菜单

| 项 | 行为 |
|---|---|
| 关于 ZzClawTerm | 新组件 `src/dialog/ZzAboutDialog`：应用名、版本（v0.1）、构建类型 `ZZ_BUILD_TYPE`、git 修订 `ZZ_GIT_REVISION`、Qt 运行时版本、图标、仓库链接 |
| 打开日志目录 | `QDesktopServices::openUrl(QUrl::fromLocalFile(日志目录))`，日志路径与 ZzLog 初始化处一致（`QStandardPaths::AppDataLocation/logs`） |
| GitHub 仓库 | `QDesktopServices::openUrl` 打开 `https://github.com/jackfahdin/ZzClawTerm` |

## 6. 完整 i18n 迁移

### 6.1 设施

- 新组件 `src/settings/ZzLanguageManager`（单例或 shell 持有）：语言选项枚举、持久化、`apply()`、`current()`、当前实际生效语言查询
- 翻译产物：`resources/i18n/zzclawterm_zh_CN.ts` / `zzclawterm_en.ts` → lrelease → qm 经 qrc 嵌入（`:/i18n/`）
- CMake：`find_package(Qt6 COMPONENTS LinguistTools)` 已在多数 Qt 安装中可用；若不可用则脚本化 lrelease 调用，qm 文件入库

### 6.2 迁移范围

- `src/` 全部用户可见中文 `QStringLiteral`/`tr` 缺失文案（面板、对话框、设置页、状态栏、标签标题等）→ `tr()`/`QCoreApplication::translate`
- 各 widget 增加 `retranslateUi()`（命名沿用 Qt 惯例）+ `changeEvent` 响应 `QEvent::LanguageChange`
- `ZzMenuBarService` 的菜单与动作文本集中 retranslate
- 库侧文案（ZzPureToolsPro/ZzTermWidget）不迁移，库自管（qtermwidget 自带 40 语言 ts 按 locale 自动装载）

### 6.3 不变量

- 源文案保持简体中文（源码可读性 + 兜底）
- `lupdate` 可重复跑且不丢翻译；`zh_CN.ts` 内容为源文本同值

## 7. 测试

- `ZzLanguageManager` 单测：三种选项切换 → translator 装载/卸载正确；QSettings 往返持久化；缺失 qm 时不崩且回退
- `ZzMenuBarService` 单测（offscreen）：三菜单存在、子菜单项数、勾选组互斥、主题动作触发 `themeController()->setMode`、终端主题动作触发 `setColorScheme`
- i18n 冒烟：装载 en 后抽查菜单「视图」= "View"，「新建会话」= "New Session"
- 全量回归：48+ 既有 ctest 保持绿

## 8. 兼容性约束

- 不改库代码（不动 `third_party/`）
- 菜单文本全部经 `tr()`，但首次运行时外观与现状一致（中文）
- 既有 `terminal/colorScheme` QSettings 键不变

## 9. 范围外

- 命令面板（Ctrl+Shift+P）、菜单快捷键体系（仅新建会话给一个）
- SFTP 面板以外的第三方库文案迁移
- 多窗口菜单（等 ZzSplitWorkspace P0 落地后再议）
- 侧栏面板标题的动态语言刷新（标题注册时被框架缓存；重启或重新注册面板后生效）
