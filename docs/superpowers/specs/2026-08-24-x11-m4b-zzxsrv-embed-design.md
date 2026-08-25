# ZzXsrv 裁剪品牌化 + rootful 嵌入（M4b）设计规格

## 文档信息

| 项目 | 内容 |
| ---- | ---- |
| 项目名称 | ZzClawTerm |
| 文档版本 | v0.3 M4b 设计规格 |
| 日期 | 2026-08-24 |
| 范围 | ZzXsrv 裁剪 + 品牌化 + rootful 嵌入 Qt 标签页 + 主仓库装配 |
| 上游文档 | `2026-08-22-x11-forwarding-design.md`（§二里程碑、§5.4 侵入点、§5.5 构建链） |
| 前置状态 | M4a 已交付：CI 构建链（约 26 分钟全量）+ xtrans 回环绑定 + release zz-21.1.16.1-1 + 主仓库切换解门禁（`6cc5789`） |
| 用户裁决（2026-08-24 头脑风暴） | ① 品牌化连 exe 文件名一起改（ZzXsrv.exe）② 嵌入信令用方案 A：启动参数 `-parent <HWND>`，不建 IPC |

---

## 一、目标

把 M4a 的「独立原生窗口」X server 形态升级为「rootful 嵌入 Qt 标签页」（对标 MobaXterm 的 X 标签页体验），同时完成 ZzXsrv 的裁剪与品牌化，使发布物成为真正的自有组件。

## 二、范围

### 2.1 M4b 包含

- ZzXsrv 裁剪：mesalib + glx 间接渲染、xlaunch、apps/*、tools/plink、Xv/XDMCP/record 扩展
- ZzXsrv 品牌化：exe 改名 ZzXsrv.exe、版本信息/托盘/关于框、NOTICE、README
- rootful 嵌入：`-parent <HWND>` 启动参数 + WS_CHILD 边界窗口 + Qt 容器 resize 跟随
- 主仓库：`ZzX11Viewport` 容器、`ZzXServerManager` 嵌入模式、下载常量升 zz2
- CI：嵌入冒烟 + 终审 M4b 候选项清理（冒烟双模式、build job 权限收缩、release sha256 交叉核验、失败日志上传）

### 2.2 M4b 不包含

- multiwindow 分窗嵌入（M5）
- 多会话共享单 server 实例（YAGNI 延后，维持每会话一实例）
- RANDR 动态屏幕尺寸（resize 后 X 逻辑尺寸不变，窗口只做显示层拉伸；留 M5 评估）
- OpenGL/GLX（随 mesa 裁剪移除，v0.3 不包含，与上游规格 §2.2 一致）
- 分发形态改 zip（见 §五 决策记录：保留 NSIS）
- 手动双击 ZzXsrv.exe 的默认模式：裸 rootful 无窗口管理器（X 应用无标题栏、不可拖动，按记忆几何位置摆放可跑出屏幕外，实机 2026-08-25 用户验收时实测踩到）。产品化时应默认 multiwindow（安装包快捷方式加 `-multiwindow`，或评估 exe 缺省行为），候选方案已记录，实现留待后续版本决策

## 三、ZzXsrv 侧设计

### 3.1 裁剪

| 裁掉 | 内容 | 影响 |
| ---- | ---- | ---- |
| mesalib + glx | swrast_dri.dll、swrastwgl_dri.dll、dxtn.dll 不再构建 | 安装包减重约一半；X 应用 OpenGL 不可用（已声明不包含） |
| xlaunch | 图形启动向导 | 我们用命令行参数驱动，不需要 |
| apps/* | xcalc/xclock/xwininfo/xhost/xrdb/xauth 等 | cookie 管理由主仓库 ZzXAuthority 自持，不依赖 xauth |
| tools/plink | SSH 客户端 | 我们有 ZzSshCore |
| Xv/XDMCP/record 扩展 | server 高级能力 | 上游规格 §2.2 已声明不包含 |

**保留**：xkbcomp（server 运行时必调）、xkbdata/locale/fonts/bitmaps 数据、全部运行时 DLL。

裁剪方式：从 mhmake 构建图移除对应子目录（xorg-server/makefile 的 SUBDIRS/load_makefile 与打包脚本段落），而非删源码目录——保持与上游 rebase 的最小冲突面。裁剪后先跑一次 CI 全量基线，记录新时长（预期显著低于 26 分钟）。

### 3.2 品牌化

- `vcxsrv.exe` → `ZzXsrv.exe`：xorg-server/makefile 的 WINAPP 目标名、XWin.rc 资源（产品名/公司名/文件描述）、托盘提示文字、关于框
- nsi 安装脚本：文件名、产品名、安装目录名（VcXsrv → ZzXsrv）、删 xlaunch/plink/apps 段落
- 新增 `NOTICE`：上游 MIT/X11 声明保留说明 + fork 差异点（回环绑定、-parent 嵌入、裁剪清单）
- README 改写为 fork 自述：与上游的差异、构建方式（指向 CI）、发布物说明
- 发布 tag：`zz-21.1.16.1-2`；xorg 版本号不动（21.1.16.1），zz 序号区分自有构建代际

### 3.3 嵌入魔改（方案 A）

- **新增启动参数** `-parent <HWND>`（十进制句柄）：`hw/xwin/winprocarg.c` 参数解析 + 全局变量，无效句柄（窗口不存在/非顶层）时记日志退出——不静默退化为独立窗口
- **边界窗口创建点** `hw/xwin/wincreatewnd.c`（`CreateWindowExA`，M4a 规格 §5.4 已定位）：`-parent` 有效时 hWndParent 改为该 HWND、样式 WS_CHILD（去 WS_OVERLAPPEDWINDOW 的标题栏/边框）、坐标换算去 `SM_XVIRTUALSCREEN` 全局化（相对父窗口客户区原点）
- **初始尺寸**：Qt 启动时经既有 `-screen <W>x<H>` 参数传入容器像素尺寸，无新机制
- **resize 跟随**：Qt 容器 resizeEvent → `EnumChildWindows` 找 X 子窗口（窗口类名识别）→ `SetWindowPos` 拉伸填满客户区；零 IPC。X 逻辑屏幕尺寸不变，放大后边缘可接受黑边
- **DPI**：server manifest 的 DPI awareness 声明与主程序清单对齐（Per Monitor V2），避免嵌入后缩放错位
- server 线程自泵 Win32 消息的模型不变（M4a 规格 §5.1），Qt 侧只持有 HWND 做 reparent，不进 Qt 主事件循环

### 3.4 CI 增强

- **嵌入冒烟**（新）：PowerShell/WinForms 创建隐藏父窗口取 HWND → 启动 `ZzXsrv.exe -parent <hwnd> :99 -screen 800x600 -auth <xauth>` → 断言：进程存活 + 父窗口下存在 WS_CHILD 子窗口（EnumChildWindows）+ 回环监听断言（沿用 M4a）
- **冒烟双模式**：默认模式与 `ZZXSRV_LISTEN_ANY=1` 旁路模式各跑一次监听断言（终审延后项：旁路分支此前 CI 从未自动执行）
- build job 权限收缩：置顶 `permissions: contents: read`（终审延后项）
- release job 交叉核验：收集步骤重算 exe 哈希对比 .sha256 内容（终审延后项）
- 构建失败时上传 `build-*.log` 为 artifact（终审延后项）
- 产物核验同步更新：断言 ZzXsrv.exe 存在、swrast 等已裁 DLL 不存在（防裁剪回退）

## 四、主仓库侧设计

### 4.1 新增 `src/x11/ZzX11Viewport.h/.cpp(+Private)`

QWidget 容器，职责单一：提供嵌入表面。公开接口：

- `WId embeddingWinId() const`——给 ZzXServerManager 的 `-parent` 参数
- resizeEvent 内实现 §3.3 的 X 子窗口跟随（Windows 专属，其他平台编译期直通为空实现）
- 嵌入内容绘制由 X server 子窗口承担，本 widget 自身只画背景色（server 未就绪时）

### 4.2 `ZzXServerManager` 扩展

- 新增嵌入模式：`startEmbedded(WId parentHwnd, const QSize &initialSize)`——启动参数 `-parent <hwnd> -screen <W>x<H> -auth <xauthority>`，不再传 `-multiwindow`
- 独立窗口模式（M4a 形态）保留为降级设置项（profile 级，默认嵌入）
- display 号分配、cookie、生命周期（标签页关闭杀进程）沿用 M4a 逻辑不动

### 4.3 `ZzXServerDownloader` 常量升级

分发形态保留 NSIS noadmin（决策记录见 §五）：nsi 脚本品牌化 + 删段落后 CI 继续产出 `zzxsrv-64.21.1.16.1.installer.noadmin.exe` 形态资产。主仓库只换三个常量（kVersion → `21.1.16.1-zz2`、kUrl、kSha256）+ 可执行文件名常量（vcxsrv.exe → ZzXsrv.exe，含 `serverExecutablePath()` 与安装核验）。机制零改动。

### 4.4 会话装配

- profile 层不动（X11 开关沿用）；标签页内 X11 区域挂 ZzX11Viewport（嵌入模式时）
- 独立窗口模式与嵌入模式的切换：会话编辑对话框加选项（默认嵌入），不改 profile 序列化格式的字段语义（新增字段带默认值，向后兼容）

## 五、决策记录

| 决策 | 结论 | 理由 |
| ---- | ---- | ---- |
| 嵌入信令 | 启动参数 `-parent <HWND>`（方案 A） | 用户裁决；单向无 IPC，YAGNI；生命周期 QProcess 已覆盖；WM_APP 控制面留 M5 |
| 品牌化深度 | exe 文件名一起改 | 用户裁决；品牌一致性，改动约 10 处可控 |
| 分发形态 | 保留 NSIS noadmin，不随裁剪砍掉 | 下载器零改动 vs 引入解压依赖（主仓库无 zlib/minizip）；nsi 品牌化成本低于下载器改造 |
| resize 策略 | 窗口拉伸 + X 逻辑尺寸不变 | 零 IPC；RANDR 动态尺寸留 M5 评估 |
| `-parent` 无效 | server 退出走 crashed 链路 | 行为可预期，不静默退化 |
| 裁剪方式 | 从构建图移除而非删源码 | 保持与上游 rebase 的最小冲突面 |

## 六、错误处理

| 场景 | 行为 |
| ---- | ---- |
| 嵌入参数无效（HWND 已销毁） | server 退出 → crashed → 状态栏瞬时提示 + 会话照常（M4a 链路） |
| 标签页关闭早于 server 退出 | QProcess 终止 + 父进程退出兜底（M4a 逻辑沿用） |
| 下载/校验失败 | 沿用 M4a：提示 + 重试入口，不阻断会话 |
| 裁剪后某 DLL 缺失导致 server 起不来 | CI 产物核验兜底（断言已裁 DLL 不存在、必需 DLL 存在）；运行时按 crashed 链路处理 |
| CI Windows 构建失败 | 不阻塞主仓库其他平台 CI（独立 workflow，沿用 M4a） |

## 七、测试与验收

### 7.1 ZzXsrv CI（自动化）

- 裁剪后全量基线绿 + 新时长记录
- 回环监听冒烟双模式（默认/旁路）
- 嵌入冒烟：WS_CHILD 子窗口存在 + 进程存活
- 产物核验：ZzXsrv.exe 存在、已裁 DLL 不存在

### 7.2 主仓库（自动化）

- 全量回归 43 项全绿（门禁）
- ZzX11Viewport 可测逻辑（resize 跟随的参数计算）单测——Windows 专属代码 Linux 不编译，逻辑抽离到可测函数

### 7.3 Windows 人工验收（用户执行）

1. 勾选 X11 转发（嵌入模式）→ SSH 连接 → 远端 `xclock` 显示在标签页内
2. 缩放主窗口/分屏 → X 区域跟随（黑边可接受，布局不错乱）
3. `netstat -ano | findstr :600` 仅 127.0.0.1
4. 关闭标签页 → ZzXsrv.exe 进程无残留
5. 任务管理器/安装目录确认品牌化为 ZzXsrv

—— 以上 5 条全部：待用户实机验收（2026-08-24 标注，自动化部分见 §7.1/§7.2，均已达成）

### 7.4 M4b 完成定义

- ZzXsrv CI 全绿（含嵌入冒烟与双模式监听断言），裁剪后基线时长入库 — 已达成（2026-08-24）：master run 32757767677 全绿（双模式回环监听断言 + 嵌入冒烟三段全过）；裁剪后全量基线 19m19s 入库（任务 1 run 32703578403，裁剪前 24m58s）
- release `zz-21.1.16.1-2` 发布（ZzXsrv.exe 品牌化产物） — 已达成（2026-08-24）：四资产齐全（admin/noadmin zip + SHA256），noadmin SHA256 `2c182ee294716c654fe8795d3bc634c8a5151cbef6ae8c85f0486090ba65b14b` / 39,209,845 字节（本地下载实测一致）
- 主仓库常量升 zz2 + 嵌入模式装配，回归 44/44 — 已达成（2026-08-24）：887bfa9（ZzX11Viewport）+ 621a041（常量 zz2 + 嵌入模式装配）+ 0aaf256（重连回填修复）；全量回归 44/44（原规格 43/43 基线 + 1 新测试可执行）
- Windows 人工验收 5 条通过 — 待用户实机验收（清单见 §7.3）

## 八、风险

| 风险 | 等级 | 应对 |
| ---- | ---- | ---- |
| 裁剪误伤运行依赖（server 起不来/键盘坏） | 中 | CI 产物核验 + 嵌入冒烟；裁剪从构建图移除可逐项目录回退 |
| reparent 后焦点/输入法/高 DPI 异常 | 中 | 上游规格已留实测缓冲；server 自泵消息模型规避事件循环冲突；DPI manifest 对齐 |
| exe 改名遗漏引用点 | 低 | CI 产物核验断言 ZzXsrv.exe；主仓库常量集中；grep 兜底 |
| resize 黑边体验差 | 低 | 设计已声明可接受；M5 评估 RANDR |
| 无 Windows 实机，嵌入效果只能 CI + 人工验收 | 中 | 沿用 M4a 验收模型（CI 冒烟 + 用户人工验收） |
