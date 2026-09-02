# ZzClawTerm 应用图标三平台适配设计

- **日期**：2026-09-02
- **状态**：已批准（方案 A：源图 + 生成脚本 + 生成物全部入库）
- **需求方决策**：运行时窗口图标与原生图标一起做（qrc 嵌入 + `setWindowIcon`）

## 1. 背景

用户提供 `ZzClawTerm.png`（1254×1254，圆角方形蓝色 Z 爪标 + 终端 `>_` 元素）作为应用图标源图。当前状态：

- Linux：`scripts/package-linux.sh:28` 内嵌 base64 占位 PNG，注释标明待正式图标替换；desktop 文件 `Icon=zzclawterm` 已就位
- Windows：`src/CMakeLists.txt:97` `WIN32_EXECUTABLE TRUE`，无 .rc、无图标资源
- macOS：`src/CMakeLists.txt:98` `MACOSX_BUNDLE TRUE`，无 Info.plist、无 .icns
- 两个打包脚本（`package-windows.ps1`、`package-macos.sh`）均未引用图标

## 2. 方案

源图、生成脚本、全部生成物都入库；构建机无需 Python/PIL。

## 3. 资产布局

```
resources/icons/
├── source/ZzClawTerm.png      # 1254×1254 源图（原样入库）
├── generated/
│   ├── zzclawterm.ico         # Windows 多尺寸：16/24/32/48/64/128/256
│   ├── zzclawterm.icns        # macOS：16/32/64/128/256/512/1024 + @2x 映射
│   └── png/{16,24,32,48,64,128,256,512}.png   # Linux hicolor
└── appicon.qrc                # 运行时窗口图标（嵌 generated/png/256.png）
scripts/generate-icons.py      # PIL 一键再生成（Pillow ≥ 10）
```

生成规则：Lanczos 重采样；所有派生物由脚本从 `source/ZzClawTerm.png` 产出，禁止手工编辑生成物。

## 4. 分平台集成

### 4.1 Windows

- 新增 `src/windows/zzclawterm.rc`：`IDI_ICON1 ICON "zzclawterm.ico"`，ico 路径指向 `resources/icons/generated/`
- .rc 加入 `qt_add_executable(ZzClawTerm ...)` 源列表（MSVC/MinGW 均支持，CMake 自动处理 RC 编译）
- `package-windows.ps1` 不改：windeployqt 自动携带 exe 资源
- 效果：资源管理器缩略图、任务栏、标题栏、Alt+Tab

### 4.2 macOS

- 最小 `src/macos/Info.plist`：`CFBundleIconFile=zzclawterm.icns`、 bundle id `com.zzclawterm.app`、版本由 CMake 注入
- `set_target_properties`：`MACOSX_BUNDLE_ICON_FILE zzclawterm.icns`；icns 文件设 `MACOSX_PACKAGE_LOCATION Resources` 打进 `Contents/Resources`
- icns 在 Linux 上用 `generate-icons.py` 手工打包容器（icns = magic `icns` + 条目表 + PNG 载荷；条目类型 icp4/icp5/icp6/ic07/ic08/ic09/ic10/ic11/ic12/ic13/ic14，对应 16/32/64/128/256/512/1024 及 @2x 映射）
- 效果：Dock、Finder、Launchpad、Cmd+Tab

### 4.3 Linux

- `package-linux.sh`：删除 base64 占位段，改为从 `resources/icons/generated/png/` 复制 16/24/32/48/64/128/256/512 到 `$APPDIR/usr/share/icons/hicolor/<size>x<size>/apps/zzclawterm.png`（linuxdeploy 自动收集 desktop 文件声明的图标）
- desktop 文件不变（`Icon=zzclawterm` 已正确）

### 4.4 运行时窗口图标（全平台）

- `appicon.qrc` 注册 `generated/png/256.png` 为 `:/icons/zzclawterm.png`
- qrc 经 `qt_add_resources` 挂到 `ZzClawTerm` 目标
- `main.cpp` 在 `QApplication` 构造后调用 `app.setWindowIcon(QIcon(QStringLiteral(":/icons/zzclawterm.png")))`
- 必要性：Wayland 下任务栏图标只能来自应用侧；其余平台与原生图标一致，无副作用

## 5. 验证

- [ ] `python3 scripts/generate-icons.py` 幂等重跑，产物字节一致
- [ ] PIL 回读解析 .ico（断言 7 个尺寸齐全）与 .icns（断言条目表类型集合）
- [ ] Linux 全量构建通过，48/48 ctest 绿
- [ ] AppImage 打包后 `usr/share/icons/hicolor/` 各尺寸存在
- [ ] Windows 验收清单（用户真机）：资源管理器缩略图 / 任务栏 / 标题栏图标正确
- [ ] macOS 验收清单（后续真机）：Dock / Finder 图标正确

## 6. 兼容性约束

- 不改 `ZzClawTerm` 目标现有属性语义，仅追加
- 源图文件名/路径固定，生成脚本以此为准
- 不引入 Python3 为构建依赖：脚本只在需要再生成时手工运行

## 7. 范围外

- Qt 安装包/在线安装器图标
- SVG 可变图标（hicolor 无 scalable 需求）
- `dist/` 下既有旧 AppImage 重打包
