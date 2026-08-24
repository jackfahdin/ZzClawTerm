# M4b（ZzXsrv 裁剪品牌化 + rootful 嵌入）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** ZzXsrv 裁剪 + 品牌化（ZzXsrv.exe）+ `-parent <HWND>` rootful 嵌入魔改并发布 zz2；主仓库新增 ZzX11Viewport 容器与嵌入装配，X 应用窗口显示在会话标签页内。

**架构：** ZzXsrv 侧（仓库 `third_party/vcxsrv`，remote `zzxsrv`）：从 mhmake 构建图裁剪 mesa/xlaunch/apps/plink（不删源码），品牌化改名，winprocarg.c 新增 `-parent` 参数、wincreatewnd.c 边界窗口支持 WS_CHILD 嵌入，CI 加嵌入冒烟后发 tag `zz-21.1.16.1-2`。主仓库侧：`ZzX11Viewport`（QWidget 容器，resize 时 EnumChildWindows + SetWindowPos 零 IPC 跟随）挂在标签页级分割区，`ZzTransportEndpoint` 新增父窗口句柄通路，`ZzXServerManager::startEmbedded` 以 `-parent <hwnd> -screen <W>x<H>` 启动 server。

**技术栈：** C（xorg，C89）、C++20/Qt 6.10、mhmake、GitHub Actions windows-2022 + WSL、PowerShell。

**规格：** `docs/superpowers/specs/2026-08-24-x11-m4b-zzxsrv-embed-design.md`（用户已批准）。

**前置事实（调研已实证，执行时不必重新调研）：**

- 裁剪锚点：`xorg-server/makefile:74-85`（EXTRASTOBUILD，xlaunch:75 / xkbcomp:76 保留 / apps:77-82 / plink:83 / swrastwgl_dri:84 / dxtn:85）与 `:92-93`（mesalib 全量构建引入点）
- **mesa 坑**：`xorg-server/glx/makefile:52` 把 mesalib glapi 源码编进 server glx 库、`hw/xwin/glx/makefile:3` include mesalib 头——mesa 裁剪只动构建产物（92-93 的 swrast_dri.dll + EXTRASTOBUILD 84-85），**不动 mesalib 源码树与 glx 库**；若链接失败立即回退该步并在报告记录
- `-parent` 参数模式：`winprocarg.c:223` `ddxProcessArgument`，`IS_OPTION` 宏（:220），带值参数范式 `-screen`（:284-306：校验 `i+1<argc` → `atoi/strtoul(argv[i+1])` → return 消耗数）；全局变量放 `winglobals.h`（extern，:77-80 有范式）+ `winglobals.c`（定义，:94-96 有范式）
- 嵌入窗口点：`wincreatewnd.c` 样式设置 :151-174（基础样式 = `WS_OVERLAPPED|WS_SYSMENU|WS_MINIMIZEBOX`，WS_OVERLAPPED 实为 0）、`CreateWindowExA` :291-305（parent 参数在 :298 `(HWND) NULL`）
- 窗口类名/标题宏：`hw/xwin/winwindow.h:49-55`（`WINDOW_CLASS "VcXsrv/x"` 等，品牌化 + Qt 侧识别都要用）
- exe 名决定点：`xorg-server/makefile:54-60`（`TTYAPP=vcxsrv` / `WINAPP=vcxsrv`）；RC 版本块 `hw/xwin/XWin.rc:51-56`；nsi 在 `xorg-server/installer/vcxsrv-64.nsi`（改名点 :21 NAME、:30 OutFile、:33 InstallDir、:73 Section 名、:102 exe File、:154-161 注册表、:217-220/:310-315 快捷方式、:256 卸载 Delete）；`packageall.sh` 引用点 :13-84 与 :5/:48；`noadmin.patch` 对 4 个 nsi 做 OutFile/InstallDir/权限变换（改名后需同步）
- **manifest**：`hw/xwin/XWin.exe.manifest:16-20` 只有旧式 `<dpiAware>true</dpiAware>`，且 `XWin.rc:154` 的 manifest 嵌入行**当前是注释**——本计划启用嵌入并升 PerMonitorV2
- 主仓库：`ZzXServerManager::launchProcess` 参数构造 `src/x11/ZzXServerManager.cpp:52-60`（`-multiwindow -clipboard -listen tcp -auth`）；装配链路 `ZzSshTransport.cpp:307-342` onX11ServerReady；`ZzTransportEndpoint` 在 `src/transport/`；标签页结构 `ZzTabManager(QTabWidget) → ZzSplitContainer(QSplitter 树，叶子 ZzTerminalView)`；profile 序列化模式 `src/session/ZzSessionProfile.cpp:22/71/98`（k*Key + insert + value().toXxx(缺省)）；X11 勾选在 `src/panel/ZzSessionEditDialog.cpp:153-159`（回写 :197）；exe 名常量在 `ZzXServerDownloader.cpp:18`（kExeName 唯一实体点）；测试桩模式 `tests/unit/tst_ZzXServerManager.cpp:22-42`（makeStubServer + setServerProgramForTesting）
- ZzXsrv CI 现状（M4a）：workflow `.github/workflows/build.yml`，全量约 26 分钟，含回环监听冒烟与产物核验；构建入口 `wsl -d Ubuntu-24.04 bash -lc "./buildall.sh 1 5 R"`

**全局约束：**

- commit message：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详细说明
- ZzXsrv 侧推送 `zzxsrv` remote 已预授权；`origin`（上游）绝不推；主仓库只 commit 不 push（用户确认后控制者统一推）
- xorg 代码 C89、TAB 缩进；主仓库类名 Zz 前缀、文件名=类名、Doxygen 简体中文注释
- 主仓库回归门禁：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release`（基线 43 项 + 本计划新增），跑完 `git checkout -- tests/perf/records/` 恢复覆写并清理新生成的未跟踪 `2026-08-*.json`
- **时间预算纪律**（CI 任务）：轮询不空等；每轮结果立即写报告；预算将尽把 run-id/卡点/下一步写进报告后 DONE_WITH_CONCERNS

**文件结构总览：**

ZzXsrv 侧（third_party/vcxsrv）：
- 修改：`xorg-server/makefile`（裁剪 + exe 改名）
- 修改：`xorg-server/hw/xwin/winglobals.h/.c`（`g_hwndParent` 全局）
- 修改：`xorg-server/hw/xwin/winprocarg.c`（`-parent` 解析）
- 修改：`xorg-server/hw/xwin/wincreatewnd.c`（WS_CHILD 嵌入分支）
- 修改：`xorg-server/hw/xwin/winwindow.h`（类名/标题宏品牌化）
- 修改：`xorg-server/hw/xwin/XWin.rc`（版本信息品牌化 + manifest 嵌入启用）
- 修改：`xorg-server/hw/xwin/XWin.exe.manifest`（PerMonitorV2）
- 修改：`xorg-server/installer/vcxsrv-64.nsi` → 改名 `zzxsrv-64.nsi`（品牌化 + 删段）；`packageall.sh`、`noadmin.patch`、`mkzip-64.sh`（同步）
- 创建：`NOTICE`、`README.md` 改写
- 修改：`.github/workflows/build.yml`（产物核验更新、嵌入冒烟、双模式监听、权限收缩、交叉核验、失败日志上传）

主仓库侧：
- 创建：`src/x11/ZzX11Viewport.h/.cpp`（+Private）（嵌入容器）
- 修改：`src/x11/ZzXServerManager.h/.cpp`（`startEmbedded`）
- 修改：`src/x11/ZzXServerDownloader.h/.cpp`（常量 zz2 + kExeName）
- 修改：`src/transport/ZzTransportEndpoint.h`（父窗口句柄字段）、`src/transport/ZzSshTransport.cpp`（装配 startEmbedded）
- 修改：`src/session/ZzSessionProfile.h/.cpp`（`x11EmbedMode` 字段）
- 修改：`src/panel/ZzSessionEditDialog.cpp`（嵌入选项）
- 修改：`src/tab/ZzTabManager.cpp`（标签页装配 viewport + endpoint 填充）
- 测试：`tests/unit/tst_ZzX11Viewport.cpp`（新）、`tests/unit/tst_ZzXServerManager.cpp`（嵌入参数用例）、`tests/session/ZzSessionProfileTest.cpp`（字段 round-trip）、`tests/unit/tst_ZzSessionEditDialog.cpp`（选项断言）

---

### 任务 1：ZzXsrv 裁剪（构建图移除 + 产物核验更新）

**文件：**
- 修改：`third_party/vcxsrv/xorg-server/makefile:74-85,92-93`（EXTRASTOBUILD 与 mesalib 引入点）
- 修改：`third_party/vcxsrv/.github/workflows/build.yml`（产物核验步骤：已裁文件不存在断言；冒烟运行时布局段去掉 swrast 拷贝）

**说明：** 裁剪 = 从构建图移除，不删源码目录（保持上游 rebase 最小冲突）。分两个 commit：先无风险项（apps/plink/xlaunch），后 mesa 产物项（swrast_dri/swrastwgl_dri/dxtn），mesa 若引发 glx 链接失败立即回退第二个 commit 并在报告记录（glx 库保留为代价）。

- [ ] **步骤 1：无风险裁剪（commit 1）**

`xorg-server/makefile` 的 EXTRASTOBUILD（:74-85）删除以下行：:75（xlaunch）、:77-82（apps/xcalc、xclock、xwininfo、xhost、xrdb、xauth）、:83（tools/plink）。保留 :76（xkbcomp，运行时必需）、:84-85（留给步骤 2）。删除后形如：

```makefile
EXTRASTOBUILD =  \
 ..\xkbcomp\$(NOSERVOBJDIR)\xkbcomp.exe \
 hw\xwin\swrastwgl_dri\$(NOSERVOBJDIR)\swrastwgl_dri.dll \
 ..\dxtn\$(NOSERVOBJDIR)\dxtn.dll
```

注意检查 `EXTRASTOBUILDDIRS` 的派生（:89 `load_makefile $(EXTRASTOBUILDDIRS:...)`）随动即可，无需手改。

- [ ] **步骤 2：mesa 产物裁剪（commit 2）**

删 EXTRASTOBUILD 的 :84-85（swrastwgl_dri、dxtn）与 :92-93 两行（mesalib 全量构建引入 + all 依赖）：

```makefile
# 删除：
load_makefile $(MHMAKECONF)\mesalib\src\makefile MAKESERVER=0 DEBUG=$(DEBUG)
all: $(MHMAKECONF)\mesalib\src\$(NOSERVOBJDIR)\swrast_dri.lib $(MHMAKECONF)\mesalib\src\$(NOSERVOBJDIR)\swrast_dri.dll
```

**不碰** `xorg-server/glx/makefile`、`hw/xwin/glx/`、`dxtn/`、`mesalib/` 源码树。

- [ ] **步骤 3：CI 产物核验与冒烟布局同步**

`.github/workflows/build.yml`：
- 产物核验步骤追加已裁文件不存在断言（防裁剪回退）：

```powershell
$cut = @('swrast_dri.dll','swrastwgl_dri.dll','dxtn.dll','xlaunch.exe','plink.exe','xcalc.exe','xclock.exe','xwininfo.exe','xhost.exe','xrdb.exe','xauth.exe')
foreach ($f in $cut) {
  $hit = Get-ChildItem vcxsrv -Recurse -Filter $f -ErrorAction SilentlyContinue |
         Where-Object { $_.FullName -match 'obj64' }
  if ($hit) { throw "cut artifact still built: $($hit.FullName)" }
}
```

- 「冒烟：监听地址断言」步骤的运行时布局段（原 :213 附近）删除 `swrast_dri.dll` 的 Copy-Item 行（产物已不存在）。

- [ ] **步骤 4：commit ×2 + push + CI 验证**

```bash
cd third_party/vcxsrv
git add xorg-server/makefile
git commit -m "build: 裁剪 xlaunch/apps/plink 出构建图

- EXTRASTOBUILD 删除 xlaunch、xcalc/xclock/xwininfo/xhost/xrdb/xauth、
  plink 条目（M4b 裁剪清单无风险项）
- xkbcomp 保留（server 运行时必调）；mesa 产物留下一 commit 处理
- 只从构建图移除不删源码，保持与上游 rebase 最小冲突面"
git commit -am "build: 裁剪 mesa 软渲染产物（swrast_dri/swrastwgl_dri/dxtn）

- 删除 mesalib 全量构建引入（makefile:92-93）与 EXTRASTOBUILD 余项
- mesalib 源码树与 glx 库保留（glapi 编进 glx 是硬依赖，见 M4b 计划）
- CI 产物核验同步：已裁文件不存在断言 + 冒烟布局去 swrast 拷贝"
git push zzxsrv master
gh run list --repo jackfahdin/ZzXsrv --limit 2
```

CI 绿（含新断言通过）为完成。若 mesa commit 导致 glx 链接失败：`git revert` 该 commit 推送，报告记录「glx 硬依赖 mesa 产物，裁剪范围收缩为无风险项」，任务仍算完成（交付物 = 安全裁剪集）。**把裁剪后全量构建时长记进报告。**

---

### 任务 2：ZzXsrv 品牌化（ZzXsrv.exe + NOTICE/README）

**文件：**
- 修改：`third_party/vcxsrv/xorg-server/makefile:54-60`（产物名）
- 修改：`third_party/vcxsrv/xorg-server/hw/xwin/XWin.rc:51-56`（版本信息）
- 修改：`third_party/vcxsrv/xorg-server/hw/xwin/winwindow.h:49-55`（类名/标题宏）
- 重命名+修改：`xorg-server/installer/vcxsrv-64.nsi` → `zzxsrv-64.nsi`；同步修改 `vcxsrv.nsi`/`vcxsrv-debug.nsi`/`vcxsrv-64-debug.nsi`（保留 32 位与 debug 脚本一致性）或按 YAGNI 只维护 64 位 release 脚本并在 packageall.sh 去掉其余调用
- 修改：`xorg-server/installer/packageall.sh`、`noadmin.patch`、`mkzip-64.sh`
- 创建：`third_party/vcxsrv/NOTICE`
- 修改：`third_party/vcxsrv/README.md`
- 修改：`.github/workflows/build.yml`（产物核验 exe 名、冒烟 exe 名、release 资产 glob）

**说明：** 只做 64 位 Release 路径的品牌化（32 位/debug 脚本在 packageall.sh 的调用同步去掉，nsi 旧文件删除——我们的发布形态只有 64 位 Release）。窗口类名改 `ZzXsrv/x`——**这是 Qt 侧 EnumChildWindows 的识别依据，任务 5 要用同一个字符串**。

- [ ] **步骤 1：产物名与代码侧品牌化**

`xorg-server/makefile:54-60`：`TTYAPP=vcxsrv` → `TTYAPP=ZzXsrv`、`WINAPP=vcxsrv` → `WINAPP=ZzXsrv`。
`XWin.rc:51-56`：`"VcXsrv windows xserver"` → `"ZzXsrv windows xserver"`、`"VcXsrv"` → `"ZzXsrv"`（InternalName/ProductName）、`"vcxsrv.exe"` → `"ZzXsrv.exe"`（OriginalFilename）、LegalCopyright 改为 `"https://github.com/jackfahdin/ZzXsrv"`。
`winwindow.h:49-55`：`WINDOW_CLASS "VcXsrv/x"` → `"ZzXsrv/x"`，标题宏中 `PROJECT_NAME` 相关字符串品牌化（保持宏结构，只改字符串字面量）。

- [ ] **步骤 2：安装脚本品牌化**

`git mv xorg-server/installer/vcxsrv-64.nsi xorg-server/installer/zzxsrv-64.nsi`，逐行改：:21 `!define NAME "ZzXsrv"`、:30 OutFile `zzxsrv-64.${VERSION}.installer.exe`、:33 InstallDir `$PROGRAMFILES64\ZzXsrv`、:39 注册表键、:45 FileDescription、:73 Section 名、:102 `File "..\obj64\servrelease\ZzXsrv.exe"`、:154-161 注册表、快捷方式段、:256 卸载 Delete 名；**删除**：xlaunch File 行（:123）与 .xlaunch 文件关联段（:168-191）、plink 行（:124）、swrast_dri/swrastwgl_dri/dxtn 行（:125-127）、apps 六件套行（:106-116 除 xkbcomp 外）。`noadmin.patch`：改为对 `zzxsrv-64.nsi` 的等价变换（InstallDir `C:\ZzXsrv`、RequestExecutionLevel user、SetShellVarContext current、OutFile 加 .noadmin）。`packageall.sh`：删 32 位/debug 调用，x64 段 exe 名改 ZzXsrv.exe、nsi 文件名改 zzxsrv-64.nsi。`mkzip-64.sh`：exe 名与 OutFile 同步（版本号遗留 21.1.10.0 顺手改 21.1.16.1）。

- [ ] **步骤 3：NOTICE 与 README**

`NOTICE`（新建）：

```text
ZzXsrv
Copyright (c) 2026 ZzClawTerm Project

本产品基于 VcXsrv（https://github.com/marchaesen/vcxsrv）修改，
VcXsrv 基于 X.Org X Server。各源文件头部的 MIT/X11 许可声明
全部保留，许可以各源文件头为准。

与上游的差异：
1. xtrans 默认仅绑定回环地址（ZZXSRV_LISTEN_ANY=1 恢复全网卡）
2. 新增 -parent <HWND> 启动参数，rootful 窗口可嵌入外部容器
3. 裁剪：mesa 软渲染 OpenGL、xlaunch、apps/*、plink、Xv/XDMCP/record
4. 品牌化为 ZzXsrv
tools/mhmake 为 GPLv3，仅构建期使用，不随产品分发。
```

`README.md` 顶部改写为 fork 自述（与上游差异四点 + 构建走 GitHub Actions CI + 发布物说明），上游原文保留在 `README.upstream.md`（git mv 后新建 README.md）。

- [ ] **步骤 4：CI 同步 + commit + push + 验证**

build.yml：产物核验 `vcxsrv.exe` → `ZzXsrv.exe`（含 Test-Path 与 upload-artifact 路径）；冒烟步骤 exe 名同步；release 资产 glob 与 noadmin 断言里的文件名同步（`zzxsrv-64.*.installer.noadmin.exe`）。commit（`feat: 品牌化为 ZzXsrv` + 详述四点改动）+ push + CI 绿（产物断言 ZzXsrv.exe 存在）为完成。

---

### 任务 3：`-parent` 嵌入魔改 + manifest + CI 嵌入冒烟

**文件：**
- 修改：`third_party/vcxsrv/xorg-server/hw/xwin/winglobals.h`、`winglobals.c`
- 修改：`third_party/vcxsrv/xorg-server/hw/xwin/winprocarg.c`
- 修改：`third_party/vcxsrv/xorg-server/hw/xwin/wincreatewnd.c:151-174,290-305`
- 修改：`third_party/vcxsrv/xorg-server/hw/xwin/XWin.exe.manifest:16-20`、`XWin.rc:154`
- 修改：`.github/workflows/build.yml`（新增嵌入冒烟步骤）

- [ ] **步骤 1：全局变量与参数解析**

`winglobals.h`（仿 :77-80 范式）追加：

```c
/* ZzXsrv: -parent <HWND> 指定的嵌入父窗口（NULL = 独立窗口模式） */
extern HWND g_hwndParent;
```

`winglobals.c`（仿 :94-96 范式）追加：

```c
HWND g_hwndParent = NULL;
```

`winprocarg.c` 的 `ddxProcessArgument` 内（建议插在 `-clipboard` 块 :700 之前）追加（仿 `-screen` :284-306 范式）：

```c
    /*
     * ZzXsrv: look for the '-parent' argument
     */
    if (IS_OPTION("-parent")) {
        /* Check for required value */
        if (i + 1 >= argc) {
            ErrorF("win: -parent requires a HWND (decimal) argument\n");
            return 0;
        }

        g_hwndParent = (HWND) (UINT_PTR) strtoul(argv[i + 1], NULL, 10);
        if (g_hwndParent == NULL || !IsWindow(g_hwndParent)) {
            ErrorF("win: -parent 0x%p is not a valid window, exiting\n",
                   g_hwndParent);
            /* 无效句柄记日志并退出：不静默退化为独立窗口（行为可预期） */
            TermProcess(1);
        }

        /* Indicate that we have processed two arguments */
        return 2;
    }
```

（`TermProcess` 是 xorg 的退出原语；若该处不可见，用 `exit(1)` 并在报告注明偏差。）

- [ ] **步骤 2：边界窗口嵌入分支**

`wincreatewnd.c` 的 `winCreateBoundingWindowWindowed` 内：

样式设置段（:151-174 之后）追加嵌入分支：

```c
    /* ZzXsrv: embedded mode - child window of the -parent container */
    if (g_hwndParent != NULL) {
        RECT rc;
        GetClientRect(g_hwndParent, &rc);
        dwWindowStyle = WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS;
        iPosX = 0;
        iPosY = 0;
        iWidth = rc.right - rc.left;
        iHeight = rc.bottom - rc.top;
    }
```

（确认 `iPosX/iPosY/iWidth/iHeight` 为该函数内既有局部变量名，按实际名字对齐；嵌入分支要放在既有样式逻辑之后覆盖。）

`CreateWindowExA` 调用（:291-305）的 parent 参数：

```c
                             g_hwndParent,   /* ZzXsrv: parent window (NULL = rootful standalone) */
```

并在函数头部 include 区确认 `winglobals.h` 已包含（没有则补 `#include "winglobals.h"`）。

- [ ] **步骤 3：manifest 启用 + PerMonitorV2**

`XWin.exe.manifest:18` 的 `<dpiAware>true</dpiAware>` 改为：

```xml
<dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
<dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">permonitorv2,permonitor</dpiAwareness>
```

`XWin.rc:154` 取消注释启用 manifest 嵌入：

```c
CREATEPROCESS_MANIFEST_RESOURCE_ID RT_MANIFEST "XWin.exe.manifest"
```

- [ ] **步骤 4：CI 嵌入冒烟步骤**

build.yml 的「冒烟：监听地址断言」之后追加（沿用该步骤的运行时布局铺设，exe 名为 ZzXsrv.exe）：

```powershell
      - name: 冒烟：-parent 嵌入断言
        shell: pwsh
        run: |
          $serv = "$PWD\vcxsrv\xorg-server\obj64\servrelease"
          # 运行时布局（与监听冒烟同）：xkbdata/fonts/xkbcomp/XKeysymDB 已在上一步铺好则复用
          Add-Type -AssemblyName System.Windows.Forms
          $form = New-Object System.Windows.Forms.Form
          $form.Width = 800; $form.Height = 600
          $form.Show(); $form.Hide()   # 创建句柄但不展示
          $hwnd = $form.Handle
          $p = Start-Process -FilePath "$serv\ZzXsrv.exe" `
               -ArgumentList ':98','-parent',([string]$hwnd),'-screen','0','800x600','-ac','-listen','tcp' `
               -WorkingDirectory $serv -PassThru
          Start-Sleep -Seconds 10
          if ($p.HasExited) {
            if (Test-Path "$serv\XWin.log") { Get-Content "$serv\XWin.log" -Tail 60 }
            throw "ZzXsrv 嵌入模式过早退出，代码 $($p.ExitCode)"
          }
          # 断言父窗口下存在 WS_CHILD 子窗口（类名 ZzXsrv/x）
          $sig = '[DllImport("user32.dll")] public static extern System.IntPtr FindWindowEx(System.IntPtr p, System.IntPtr c, string cls, string title);'
          $u32 = Add-Type -MemberDefinition $sig -Name U32 -Namespace Win32 -PassThru
          $child = $u32::FindWindowEx($hwnd, [System.IntPtr]::Zero, 'ZzXsrv/x', $null)
          if ($child -eq [System.IntPtr]::Zero) {
            if (Test-Path "$serv\XWin.log") { Get-Content "$serv\XWin.log" -Tail 60 }
            throw "no child window of class ZzXsrv/x under parent"
          }
          $listen = netstat -ano | Select-String '127\.0\.0\.1:6098\s+.*LISTENING'
          if (-not $listen) { throw "embedded server not listening on 127.0.0.1:6098" }
          Stop-Process -Id $p.Id -Force
          $form.Dispose()
```

注意：`-screen` 的真实参数格式以 winprocarg.c:284-324 的解析为准（可能为 `-screen 0 800x600` 三参）；若 FindWindowEx 找不到（窗口类名带屏号后缀），改为 `EnumChildWindows` 枚举任意子窗口即算通过，报告注明。

- [ ] **步骤 5：commit + push + CI 验证**

commit（`feat: 新增 -parent <HWND> rootful 嵌入支持` + 详述参数解析/WS_CHILD 分支/manifest PMv2/嵌入冒烟）+ push + CI 绿（嵌入冒烟通过）为完成。若 runner 会话限制导致嵌入冒烟起不来（参考 M4a GUI 风险），步骤改 `continue-on-error: true` 并在报告记录，改以人工验收兜底——但 M4a 已实证 vcxsrv 可在 runner 运行，预期可行。

---

### 任务 4：CI 增强 + tag `zz-21.1.16.1-2` 发布

**文件：**
- 修改：`third_party/vcxsrv/.github/workflows/build.yml`

- [ ] **步骤 1：四项 CI 增强**

同一 workflow 内：
1. **监听双模式**：既有「冒烟：监听地址断言」步骤复制为两段矩阵式运行——第一段默认（现逻辑），第二段 `env: ZZXSRV_LISTEN_ANY: '1'`，断言 `0.0.0.0:6099` 必须存在（旁路语义）
2. **build job 权限收缩**：workflow 顶部（`on:` 之后）加 `permissions: contents: read`；release job 保留自身 `contents: write`
3. **release 交叉核验**：release job 收集步骤追加——对每个 `.exe` 重算 `Get-FileHash` 与同名 `.sha256` 内容比对，不等即 throw
4. **失败日志上传**：build job 末尾加 `if: failure()` 的 `actions/upload-artifact@v4`（name `build-logs`，path `build-*.log`，`if-no-files-found: ignore`）

- [ ] **步骤 2：commit + push + CI 绿验证**

commit（`ci: 冒烟双模式 + 权限收缩 + release 交叉核验 + 失败日志上传` + 详述，注明来源为 M4a 终审延后项）+ push，master run 绿（双模式两段都过）。

- [ ] **步骤 3：打 tag 发布**

```bash
cd third_party/vcxsrv
git tag zz-21.1.16.1-2
git push zzxsrv zz-21.1.16.1-2
gh run watch --repo jackfahdin/ZzXsrv --exit-status
gh release view zz-21.1.16.1-2 --repo jackfahdin/ZzXsrv --json assets --jq '.assets[].name'
```

确认资产含 `zzxsrv-64.21.1.16.1.installer.noadmin.exe`（或 nsi 改名后的实际 OutFile 名）+ `.sha256`。

- [ ] **步骤 4：记录 SHA256 与实际资产名**

```bash
gh release download zz-21.1.16.1-2 --repo jackfahdin/ZzXsrv --pattern "*.noadmin.exe.sha256" --dir /tmp/zzxsrv-zz2 --clobber
cat /tmp/zzxsrv-zz2/*.sha256
```

把 **SHA256、文件字节数、资产完整文件名** 记入报告——任务 6 数据回填用。

---

### 任务 5：主仓库 `ZzX11Viewport` 容器

**文件：**
- 创建：`src/x11/ZzX11Viewport.h`、`src/x11/ZzX11Viewport.cpp`、`src/x11/ZzX11ViewportPrivate.h`、`src/x11/ZzX11ViewportPrivate.cpp`
- 测试：创建 `tests/unit/tst_ZzX11Viewport.cpp`；修改 `tests/CMakeLists.txt` 登记
- 修改：`src/x11/CMakeLists.txt`（或 src/CMakeLists.txt，按现有 x11 模块登记方式）

**说明：** 容器职责单一：提供嵌入表面 + resize 跟随。跟随逻辑抽离为可测纯函数（不依赖 Win32）：给定容器尺寸与 X 窗口当前几何，计算 SetWindowPos 目标矩形。Win32 调用（EnumChildWindows/SetWindowPos）仅 Windows 编译，Linux 下为空实现，保证回归可跑。类名/文件名遵循 Zz 前缀 + 四文件 Pimpl 惯例。

- [ ] **步骤 1：失败测试先行（TDD）**

`tests/unit/tst_ZzX11Viewport.cpp`：

```cpp
#include <QtTest>
#include "ZzX11Viewport.h"

class tst_ZzX11Viewport : public QObject
{
    Q_OBJECT
private slots:
    void followRectMatchesContainer()
    {
        // 容器 800x600，X 子窗口任意旧几何 → 目标矩形铺满客户区
        const QRect r = ZzX11Viewport::computeFollowRect(QSize(800, 600));
        QCOMPARE(r, QRect(0, 0, 800, 600));
    }
    void followRectZeroSize()
    {
        // 容器坍缩为 0（分屏拖到边缘）→ 矩形为 0x0，不产生负尺寸
        const QRect r = ZzX11Viewport::computeFollowRect(QSize(0, 0));
        QCOMPARE(r, QRect(0, 0, 0, 0));
    }
    void windowClassNameIsBranded()
    {
        // Qt 侧识别串必须与 ZzXsrv 窗口类名品牌化一致（任务 2）
        QCOMPARE(ZzX11Viewport::x11WindowClassName(),
                 QStringLiteral("ZzXsrv/x"));
    }
};
QTEST_MAIN(tst_ZzX11Viewport)
#include "tst_ZzX11Viewport.moc"
```

运行（ linux-gcc-release 构建后 `ctest -R ZzX11Viewport`）：预期编译失败（头文件不存在）。

- [ ] **步骤 2：实现**

`src/x11/ZzX11Viewport.h`：

```cpp
#ifndef ZZX11VIEWPORT_H
#define ZZX11VIEWPORT_H

#include <QWidget>
#include <memory>

class ZzX11ViewportPrivate;

/**
 * @brief X11 嵌入容器：为 ZzXsrv 的 -parent 嵌入提供 Win32 父窗口表面。
 *
 * Windows：以自身 winId 作为 server 边界窗口的父窗口；resize 时枚举
 * 子窗口中的 X 窗口（类名 ZzXsrv/x）并 SetWindowPos 拉伸铺满客户区
 * （零 IPC 跟随；X 逻辑屏幕尺寸不变，放大后边缘可能出现黑边）。
 * 其他平台：编译期直通为空容器（嵌入模式仅 Windows 提供）。
 */
class ZzX11Viewport : public QWidget
{
    Q_OBJECT
public:
    explicit ZzX11Viewport(QWidget *parent = nullptr);
    ~ZzX11Viewport() override;

    /** @brief 供 -parent 参数使用的嵌入句柄（Windows）；其他平台为 0。 */
    quintptr embeddingHandle() const;

    /** @brief ZzXsrv 边界窗口的 Win32 类名（品牌化识别串）。 */
    static QString x11WindowClassName();

    /** @brief resize 跟随的目标矩形（抽离可测）：铺满容器客户区。 */
    static QRect computeFollowRect(const QSize &containerSize);

protected:
    void resizeEvent(QResizeEvent *event) override;

private:
    std::unique_ptr<ZzX11ViewportPrivate> d_ptr;
    Q_DECLARE_PRIVATE(ZzX11Viewport)
    Q_DISABLE_COPY_MOVE(ZzX11Viewport)
};

#endif // ZZX11VIEWPORT_H
```

cpp 要点：`embeddingHandle()` Windows 返回 `static_cast<quintptr>(winId())`；`x11WindowClassName()` 返回 `"ZzXsrv/x"`；`computeFollowRect` 返回 `QRect(0, 0, qMax(0,w), qMax(0,h))`；`resizeEvent`（仅 `#ifdef Q_OS_WIN`）调 Private 的 `repositionXChildWindows(hwnd, rect)`——Private 内用 `EnumChildWindows` 匹配类名 `ZzXsrv/x` 后 `SetWindowPos(..., SWP_NOZORDER|SWP_NOACTIVATE)`；背景填充深色（`setAutoFillBackground` + palette，server 未就绪时不刺眼）。Private 四文件按项目 Pimpl 惯例。

- [ ] **步骤 3：测试通过 + 全量回归 + commit**

`ctest -R ZzX11Viewport` 3/3 通过；全量 `ctest --preset linux-gcc-release` 全绿（43+3）；恢复 perf 记录后：

```bash
git add src/x11/ tests/ 
git commit -m "feat(x11): 新增 ZzX11Viewport 嵌入容器

- 提供 -parent 嵌入表面（winId），resize 零 IPC 跟随
  （EnumChildWindows 类名识别 + SetWindowPos 铺满）
- 跟随矩形计算抽离为可测纯函数；Win32 调用仅 Windows 编译
- 窗口类名识别串与 ZzXsrv 品牌化一致（ZzXsrv/x）"
```

（不 push。）

---

### 任务 6：主仓库嵌入装配（manager + endpoint 通路 + profile + 常量 zz2）

**文件：**
- 修改：`src/x11/ZzXServerManager.h/.cpp`（`startEmbedded` + 参数构造 + restart 记忆）
- 修改：`src/transport/ZzTransportEndpoint.h`（`x11ParentWindow` 字段）
- 修改：`src/transport/ZzSshTransport.cpp:307-342`（onX11ServerReady 嵌入分支）
- 修改：`src/session/ZzSessionProfile.h:45` 附近 + `.cpp:22/71/98`（`x11EmbedMode` 字段）
- 修改：`src/panel/ZzSessionEditDialog.cpp:153-159,197`（嵌入勾选项）
- 修改：`src/tab/ZzTabManager.cpp:65-82`（标签页装配 viewport + endpointFor 填充）
- 修改：`src/x11/ZzXServerDownloader.h:60-70`（常量 zz2）+ `.cpp:18`（kExeName → ZzXsrv.exe）
- 测试：修改 `tests/unit/tst_ZzXServerManager.cpp`（嵌入参数用例）、`tests/session/ZzSessionProfileTest.cpp`（字段 round-trip）、`tests/unit/tst_ZzSessionEditDialog.cpp`（选项断言）、`tests/unit/tst_ZzXServerDownloader.cpp:69,102,160,241,259`（桩 exe 名同步）

**说明：** 装配链路（调研已实证）：标签页创建（ZzTabManager:65-82）时若 `profile.x11Forwarding && x11EmbedMode`，把 ZzSplitContainer 与 ZzX11Viewport 放进一个垂直 QSplitter（上终端下 X11，初始比例 3:1），`endpointFor` 把 `viewport->embeddingHandle()` 写入 `ZzTransportEndpoint::x11ParentWindow`；transport 的 onX11ServerReady 见到非 0 句柄走 `startEmbedded`。任务 3 的 zz2 数据（SHA256/资产名/字节数）从任务 4 报告逐字回填。

- [ ] **步骤 1：profile 字段 + 测试（TDD）**

`ZzSessionProfile.h`（仿 :45）：

```cpp
bool x11EmbedMode = true;  ///< X11 嵌入标签页显示（false=独立窗口；缺省 true，旧 JSON 兼容）
```

`ZzSessionProfile.cpp`：匿名 namespace 加 `kX11EmbedModeKey = "x11EmbedMode"`；toJson 加 insert；fromJson 加 `obj.value(kX11EmbedModeKey).toBool(profile.x11EmbedMode)`。
`tests/session/ZzSessionProfileTest.cpp` 补 round-trip 断言（true/false 两值 + 旧 JSON 无字段时默认 true）。先跑确认失败（字段不存在），再实现，再跑通过。

- [ ] **步骤 2：endpoint 字段 + manager startEmbedded + 测试**

`ZzTransportEndpoint.h`（仿 x11Forwarding 字段风格）：

```cpp
quintptr x11ParentWindow = 0;  ///< X11 嵌入父窗口句柄（0=独立窗口模式）
```

`ZzXServerManager.h` 追加：

```cpp
/**
 * @brief 以嵌入模式启动 server：-parent <hwnd> -screen <W>x<H>。
 * @param executablePath ZzXsrv.exe 路径。
 * @param xauthorityPath cookie 文件路径。
 * @param display display 号。
 * @param parentWindow 嵌入父窗口句柄（ZzX11Viewport::embeddingHandle()）。
 * @param initialSize 容器初始像素尺寸（映射 -screen 参数）。
 * @note 仅 Windows 真用；restart() 复用同一份嵌入参数。
 */
void startEmbedded(const QString &executablePath, const QString &xauthorityPath,
                   int display, quintptr parentWindow, const QSize &initialSize);
```

cpp：`launchProcess` 抽出参数构造为独立函数（两模式共用），嵌入模式参数：

```cpp
const QStringList args = {
    QStringLiteral(":%1").arg(displayNum),
    QStringLiteral("-parent"), QString::number(parentWindow),
    QStringLiteral("-screen"), QStringLiteral("0"),
    QStringLiteral("%1x%2").arg(initialSize.width()).arg(initialSize.height()),
    QStringLiteral("-clipboard"),
    QStringLiteral("-listen"), QStringLiteral("tcp"),
    QStringLiteral("-auth"), xauthorityPath,
};
```

`m_last*` 记忆嵌入参数供 restart。`tst_ZzXServerManager.cpp` 新增 `startsEmbeddedWithExpectedArgs`：桩模式同 `startsWithExpectedArgs`（:54-81），逐参数精确比对（含 `-parent <num> -screen 0 800x600`）。先跑失败再实现。

- [ ] **步骤 3：transport 装配 + 标签页挂载**

`ZzSshTransport.cpp` onX11ServerReady（:307-342）：`m_x11Manager->start(...)` 处改分支——`m_endpoint.x11ParentWindow != 0` 时调 `startEmbedded(executablePath, xauthPath, display, m_endpoint.x11ParentWindow, m_endpoint.x11InitialSize)`（endpoint 同步加 `QSize x11InitialSize`），否则原 `start()`。
`ZzTabManager.cpp:65-82`：`profile.x11Forwarding && profile.x11EmbedMode` 时创建 viewport，与 container 装入垂直 QSplitter（`setStretchFactor(0,3); setStretchFactor(1,1);`），`addTab(splitter, profile.name)`；`endpointFor` 填 `x11ParentWindow = viewport->embeddingHandle()` 与初始尺寸。viewport 随标签页销毁（父子关系）。

- [ ] **步骤 4：对话框 + 常量 zz2**

`ZzSessionEditDialog.cpp`（仿 :153-159）：加「嵌入标签页显示（否则独立窗口）」勾选（objectName `x11EmbedCheckBox`，默认按 profile，回写 :197 处）；tooltip 注明仅 Windows 生效。`tst_ZzSessionEditDialog.cpp` 补断言。
`ZzXServerDownloader.h`：kVersion → `"21.1.16.1-zz2"`；kUrl → `https://github.com/jackfahdin/ZzXsrv/releases/download/zz-21.1.16.1-2/<任务 4 实测资产名>`；kSha256 → `<任务 4 实测值>`（数据回填，先拿值再 commit）；头注释同步。`ZzXServerDownloader.cpp:18` kExeName → `"ZzXsrv.exe"`。`tst_ZzXServerDownloader.cpp` 桩与断言的 `vcxsrv.exe` 字符串同步改 `ZzXsrv.exe`（:69,102,160,241,259）。

- [ ] **步骤 5：全量回归 + commit**

`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release` 全绿（含新增用例）；恢复 perf 记录 + 清理未跟踪 json；commit：

```bash
git commit -am "feat(x11): X11 嵌入模式装配——标签页内嵌 ZzXsrv 桌面

- ZzXServerManager::startEmbedded：-parent/-screen 参数构造与 restart 记忆
- ZzTransportEndpoint 新增父窗口句柄与初始尺寸通路，onX11ServerReady 分支装配
- profile 新增 x11EmbedMode（默认嵌入，旧 JSON 兼容）+ 会话对话框选项
- 标签页级垂直分割挂载 ZzX11Viewport（终端:X11 = 3:1）
- 下载常量升 zz2（ZzXsrv.exe 品牌化产物），桩测试同步"
```

（不 push，控制者统一处理。）

---

### 任务 7：收口（规格核销 + 人工验收清单）

**文件：**
- 修改：`docs/superpowers/specs/2026-08-24-x11-m4b-zzxsrv-embed-design.md`（§七 完成定义核销）

- [ ] **步骤 1：ZzSshCore 无改动确认**（`git -C third_party/ZzSshCore status` 干净）

- [ ] **步骤 2：规格 §7.4 完成定义逐条核销**——标注达成证据（CI run 号、zz2 release、主仓库 commit 号），人工验收条标「待用户实机验收」

- [ ] **步骤 3：主仓库全量回归复跑** + perf 记录恢复 + commit（`docs(x11): M4b 完成定义核销`，不 push）

- [ ] **步骤 4：Windows 人工验收清单（交用户）**——规格 §7.3 五条：嵌入 xclock 显示在标签页内、缩放跟随、netstat 仅回环、关标签页进程无残留、品牌化确认

---

## 自检记录

**规格覆盖度：** §3.1 裁剪 → 任务 1；§3.2 品牌化 → 任务 2；§3.3 嵌入魔改 → 任务 3（参数/WS_CHILD/manifest/初始尺寸/-parent 无效退出）；§3.4 CI 增强 → 任务 3 步骤 4（嵌入冒烟）+ 任务 4（双模式/权限/交叉核验/日志上传）；§4.1 viewport → 任务 5；§4.2 manager → 任务 6 步骤 2；§4.3 常量 zz2 → 任务 6 步骤 4；§4.4 装配（profile/对话框/标签页） → 任务 6 步骤 1/3/4；§六 错误处理 → 任务 3 步骤 1（-parent 无效退出）+ 任务 6 步骤 3（沿用 M4a 链路）；§七 验收 → 任务 7。

**占位符扫描：** 任务 6 步骤 4 的 kUrl/kSha256 为任务 4 产出后的数据回填（获取命令在任务 4 步骤 4），非设计占位；任务 3 步骤 4 已注明 `-screen` 格式与 FindWindowEx 的实测偏差预案。

**类型一致性：** `ZzX11Viewport::embeddingHandle()`（quintptr，任务 5）= `ZzTransportEndpoint::x11ParentWindow`（quintptr，任务 6）= `startEmbedded` 的 `parentWindow`（quintptr，任务 6）；窗口类名 `"ZzXsrv/x"` 在任务 2（winwindow.h）、任务 3 冒烟、任务 5（x11WindowClassName）三处一致；tag `zz-21.1.16.1-2` / kVersion `21.1.16.1-zz2` 在任务 4/6 一致。
