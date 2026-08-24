# ZzClawTerm X11 Forwarding（v0.3）设计规格

## 文档信息

| 项目 | 内容 |
| ---- | ---- |
| 项目名称 | ZzClawTerm |
| 文档版本 | v0.3 X11 forwarding 设计规格 |
| 日期 | 2026-08-22 |
| 范围 | SSH X11 forwarding 三端打通 + vcxsrv 集成与魔改 |
| 前置决策 | 源码级魔改（用户拍板）；无 Windows 构建机、走 CI；先 rootful 后 multiwindow；原版直接用官方预编译二进制，魔改时才碰源码 |

---

## 一、背景与目标

ZzClawTerm 的差异化目标之一是「X11 forwarding 三端统一体验」（对标 MobaXterm）：SSH 会话里跑远端 GUI 程序，窗口在本机显示。技术现实：

- **Linux / macOS**：本机已有 X server（或 XQuartz），X11 forwarding 只需 SSH 库支持 + cookie 管理即可打通
- **Windows**：没有系统 X server，需要内建一个。选型为 vcxsrv（xorg-server 21.1.99.1 的 Windows 移植，MIT/X11 许可）

**核心策略（用户拍板）**：原版功能直接用 vcxsrv 官方预编译二进制，**不搭构建链、不碰源码**；只有到「魔改嵌入」里程碑时才建立源码构建链。这样 X11 forwarding 能提前数个里程碑在三端跑起来，最大风险项（Windows 构建链移植）被推迟到真正需要时。

## 二、里程碑

| 里程碑 | 内容 | 平台依赖 | 可验证环境 |
| ------ | ---- | -------- | ---------- |
| M1 | 官方二进制集成：按需下载官方 release（SHA256 校验）、`ZzXServerManager` 进程管理、display 号分配、xauth cookie 生成 | Windows | CI（Windows runner） |
| M2 | ZzSshCore X11 forwarding：会话请求 x11 forwarding、接收 forwarded-x11 回连 channel、桥接到本地 X server | 全平台（库） | Linux 本机（Docker: sshd + Xvfb） |
| M3 | 应用侧装配：profile X11 开关、标签页生命周期绑定；Windows 用官方二进制（独立窗口模式），Linux/macOS 走系统 X server | 全平台 | Linux 本机 + CI |
| M4a | ZzXsrv fork：建仓推源码 → CI 原链（mhmake）跑出未修改基线 → xtrans 回环绑定 patch（`Xtranssock.c` `SocketINETCreateListener`）→ CI 产物发 release → 主仓库下载源切换、解除 Windows 门禁 | Windows | CI（Windows runner）+ 人工验收 |
| M4b | 裁剪品牌化（砍 mesalib/xlaunch/plink 等）+ rootful 嵌入 Qt 标签页 | Windows | CI + Windows 实机人工验收 |
| M5 | multiwindow 分窗嵌入（每个 X 顶层窗口嵌入各自 Qt 容器） | Windows | 同 M4b；允许顺延 v0.4 |

M1 与 M2 无相互依赖，可并行；M3 依赖 M1+M2；M4a 依赖 M3 验收通过（已满足）；M4b 依赖 M4a 的 CI 构建链。

> **里程碑变更（2026-08-23 用户裁决）**：实证官方 vcxsrv 二进制 xtrans 硬编码 `INADDR_ANY`、无地址绑定 CLI（见 §5.1），用户不接受全网卡监听形态交付。M1-M3 的 Windows 端 X11 暂不上线；「xtrans 回环绑定魔改 + Windows 构建链」从原 M4 前置为 v0.3 下一里程碑（即 M4a），回环绑定就绪后 Windows 端 X11 方可启用并验收。
>
> **M4a 启动（2026-08-23）**：ZzXsrv 仓库已建（`github.com/jackfahdin/ZzXsrv`），vcxsrv 源码已推送。

### 2.1 v0.3 包含

- SSH X11 forwarding 端到端打通（三端）
- Windows 端内建 X server（M1-M3 官方二进制路径已按 2026-08-23 裁决门禁，待回环绑定魔改就绪后开放，见 §二里程碑变更注记）
- display 号 / cookie / 生命周期的自动管理（用户零配置）
- M4b 起的 rootful 嵌入 Qt 标签页

### 2.2 v0.3 不包含

- OpenGL 间接渲染（mesa/swrast，M4b 裁剪时直接砍掉）
- XDMCP、Xv、多座位等 server 高级能力
- macOS 内嵌 X server（用 XQuartz，不重复造轮子）
- vcxsrv 构建链的 CMake 化（可作为 v0.4+ 可选项，届时单独评估）

## 三、仓库与组件

### 3.1 仓库布局

- **M1-M3**：不引入 vcxsrv 源码。官方 release 二进制在首次使用时按需下载到用户数据目录（`QStandardPaths::AppDataLocation/xserver/`），不打进安装包（Linux/macOS 用户无需背负 ~60MB）
- **M4a（已启动）**：fork 已建（`github.com/jackfahdin/ZzXsrv`），vcxsrv 完整历史已推送。M4a 主仓库只把下载源从官方 release 切换为 ZzXsrv CI 产物，**不引入 submodule**（主仓库构建不依赖 ZzXsrv 源码；是否以 submodule 引用 `third_party/ZzXsrv` 留到 M4b 裁剪/嵌入时评估）
- 当前 `third_party/vcxsrv` 下的克隆为 ZzXsrv 工作副本（已解除浅克隆、含上游 + zzxsrv 双 remote），**不入库**（已在 `.git/info/exclude`）

### 3.2 ZzSshCore 新增（M2）

- `ZzSshConnection` 扩展：`enableX11Forwarding(const QString &cookie)`（会话认证后、shell 打开前请求 x11 forwarding）
- 新增 `ZzSshX11Bridge`：接受 sshd 回连的 forwarded-x11 channel，桥接到本地 X server 端点（Windows: `127.0.0.1:6000+N`；Linux: `$DISPLAY` 对应 Unix socket；macOS: XQuartz socket）
- 复用现有 worker 线程泵 + channelId 分发 + 写队列背压模式，与端口转发（ZzSshTunnel）同构

### 3.3 主仓库新增（M1/M3）

```text
src/x11/
  ZzXServerManager.h/.cpp(+Private)   server 进程生命周期、display 号分配、状态上报
  ZzXAuthority.h/.cpp(+Private)       MIT-MAGIC-COOKIE-1 生成与 xauthority 文件写入
  ZzXServerDownloader.h/.cpp(+Private) 官方 release 按需下载 + SHA256 校验（Windows）
  ZzX11Viewport.h/.cpp(+Private)      M4 才启用：Qt 容器 widget（提供 winId 给嵌入）
```

- `ZzSessionProfile` 增加 `x11Forwarding` 开关（默认关），会话编辑对话框加勾选项
- 状态栏/面板复用现有瞬时提示规范（错误不弹窗）
- Linux/macOS 探测：Linux 检查 `$DISPLAY` 可用性；macOS 检查 XQuartz socket（`/tmp/.X11-unix`），缺失时提示用户安装 XQuartz

## 四、数据流

```text
profile.x11Forwarding = on
  → SSH 连接建立、认证完成
  → ZzSshCore 请求 x11 forwarding（携带 cookie A，单连接）
  → 远端 GUI 程序写 DISPLAY=localhost:10.0（sshd 默认从 10 起分配），sshd 回连 forwarded-x11 channel
  → ZzSshX11Bridge 接受 channel，桥接到本地 X server 端点
       Windows: 127.0.0.1:6000+N（ZzXsrv / 官方 vcxsrv）
       Linux:   $DISPLAY 的 Unix socket
       macOS:   XQuartz socket
  → X server 校验 cookie A → 渲染
       M1-M3: 独立原生窗口
       M4+:   rootful 嵌入标签页 HWND
```

## 五、关键设计细节

### 5.1 vcxsrv 对接面（调研结论，均为现成功能）

- **监听**：Windows 版仅 TCP 传输（`os/xstrans.c` 强制 TCPCONN），标准端口 6000+display。实证结论：官方预编译二进制的 xtrans 层硬编码 `INADDR_ANY`，无地址绑定 CLI 选项，`-listen tcp` 必然监听全网卡。**用户裁决（2026-08-23）：不接受全网卡监听形态交付**——M1-M3 的 Windows 端 X11 暂不上线（应用侧已加门禁）；xtrans 回环绑定魔改 + Windows 构建链提前为 v0.3 下一里程碑（原 M4 内容前置）。**M4a 已交付（2026-08-24；主仓库 commit 7cc787a，ZzXsrv 侧关键 commit：60f8158b3 CI 原链基线、48a279edd 回环绑定 patch、a8b15680e IPv6 对称断言、62a11463b release 流水线）**：ZzXsrv 回环绑定构建已发布（release `zz-21.1.16.1-1`），主仓库下载源已切换至该构建、Windows 门禁已解除，Windows 端默认走 downloader→manager→bridge 链路
- **认证**：`-auth <xauthority 文件>` 走标准 DIX 解析；不用 `-ac`（关闭访问控制）除非仅绑回环的调试场景
- **内部 cookie**：vcxsrv 自身用 `winGenerateAuthorization()`（`hw/xwin/winauth.c`）供剪贴板线程 / multiwindow WM 内部连接，与我们的 cookie 管理不冲突
- **事件循环**：server 线程自泵 Win32 消息（`winwakeup.c` PeekMessage），嵌入时保持「server 独立线程自泵、Qt 侧只持有 HWND 做 reparent」模型，不进 Qt 主事件循环

### 5.2 官方二进制下载（M1）

- 来源：M1-M3 为 vcxsrv SourceForge/GitHub release 的 7z/zip 压缩包，URL 与 SHA256 硬编码在 `ZzXServerDownloader`（版本常量集中管理）；**M4a 起切换为 ZzXsrv 仓库 GitHub Releases 的 CI 产物**（含回环绑定 patch），常量同步更新
- 下载到用户数据目录，解压出最小运行集（vcxsrv.exe + xkbcomp + 必要数据文件）；校验失败或下载失败 → 状态栏提示 + 重试入口
- 更新策略：启动时检测本地版本，低于内置常量版本则后台提示更新（不强制）

### 5.3 display 号与 cookie 管理

- display 号从 0 递增探测（检查 6000+N 端口是否被占），每 X11 会话独占一个 server 实例（M1-M3 简化模型；M4 后评估多会话共享单 server）
- cookie：16 字节随机数（Qt 加密随机源），写 xauthority 文件（0600 权限）传给 server `-auth`，同值经 SSH x11 forwarding 请求发给远端；会话结束即废弃
- server 进程随标签页关闭而终止（`QProcess` 子进程 + 父进程退出兜底清理）

### 5.4 魔改侵入点（M4a 回环绑定 / M4b 嵌入）

**M4a 回环绑定（本里程碑核心 patch）**：

- `X11/xtrans/Xtranssock.c` `SocketINETCreateListener`（vcxsrv 把 xtrans 作为独立 vendored 库，不在 `xorg-server/os/` 下；patch 后绑定点 :1391 IPv4 / :1401 IPv6）硬编码 `INADDR_ANY` → 默认改绑 `127.0.0.1`（IPv6 分支同步绑 `::1`），保留环境变量旁路 `ZZXSRV_LISTEN_ANY=1` 恢复全网卡（仅限隔离调试；主程序原 `ZZCLAWTERM_X11_ALLOW_ANY_BIND` 旁路已于 M4a 解除门禁时删除）
- 同步检查 `Xtranslcl.c`（本地 IPC 传输）与 IPv6 分支（`::1`）行为一致性

**M4b 嵌入侵入点（届时细化）**：

- rootful 边界窗口：`xorg-server/hw/xwin/wincreatewnd.c:291` `CreateWindowExA` → 改 hWndParent 为 Qt 容器 `winId()`、样式改 WS_CHILD、坐标换算去 `SM_XVIRTUALSCREEN` 全局化
- multiwindow 顶层窗口：`winmultiwindowwindow.c:568`（M5 用）；同文件 :727 已有 `SetParent` 重挂载先例
- 控制面：vcxsrv 无 IPC 协议，给 `winmsgwindow.c:120` 的隐藏消息窗口加自定义 WM_APP 消息（起停/状态查询）

裁剪清单（M4b 第一步）：mesalib + glx 间接渲染（-162MB 源码）、xlaunch、apps/*、tools/plink、NSIS 安装器、Xv/XDMCP/record 扩展；品牌化（改名 ZzXsrv、去托盘或改为我们控制）。

### 5.5 构建链（M4a 建立）

- GitHub Actions windows-2022 runner：VS2022 + Cygwin（bison/flex/gawk/gperf/nasm/python27/python38）+ Strawberry Perl + Python 3.9(lxml/mako)
- 先原链（mhmake）跑出未修改基线，再增量魔改；依赖层（freetype/openssl/pthreads 编译产物）做 CI 缓存（全量构建数小时，缓存后增量约十分钟级）
- xorg 代码为 C（非 C++），构建为独立 C 目标，不混入我们的 C++20 编译单元
- DPI manifest 冲突：`XWin.exe.manifest` 的 DPI 声明与主程序清单需对齐（M4b 实测处理）

## 六、错误处理

| 场景 | 行为 |
| ---- | ---- |
| 远端拒绝 x11 forwarding 请求 | 会话照常建立，状态栏瞬时提示「X11 转发被服务端拒绝」 |
| display 号冲突 | 自动递增探测，用户无感 |
| Windows 官方二进制下载失败 / 校验失败 | 状态栏提示 + 重试入口；会话本身不受影响 |
| X server 进程崩溃 | 检测退出码，状态栏提示 + 一键重启；转发 channel 桥接断开并回收 |
| cookie 校验失败（server 侧拒绝） | 记应用日志（不含 cookie 值），面板提示 |
| Linux 无 $DISPLAY / macOS 无 XQuartz | 勾选 X11 时提示不可用原因，不阻断 SSH 连接 |
| CI Windows 构建失败 | 不阻塞主仓库其他平台 CI（vcxsrv 构建独立 workflow） |

## 七、测试策略与性能门控

### 7.1 ZzSshCore（Linux 本机可全测）

- Docker 集成环境扩展：sshd 容器加 Xvfb + x11-apps；端到端回环——经 SSH 转发跑 `xdpyinfo` 查询与 `xeyes` 类绘图命令，断言返回内容正确
- 负路径：远端拒绝 x11 请求、cookie 错误、转发中途断连
- 性能门控（入 tests/perf/records 体系）：X11 channel 吞吐基准（批量 X 协议请求回环 MB/s）、转发建立耗时；沿用 5% 回归容忍

### 7.2 主仓库

- 单元测试：display 号分配冲突、cookie 生成/文件权限、profile 序列化、下载器校验失败路径（mock 网络层）
- `ZzXServerManager` 用桩进程（fake server）测生命周期
- M4a 起：CI Windows runner 冒烟——构建产物启动 + `netstat` 断言仅监听 127.0.0.1:6000+N；M4b 增加嵌入 server + 截图比对基线

### 7.3 已知验收风险

开发机无 Windows，M1/M4/M5 的 Windows 端实机验收只能靠 CI 产物 + 用户人工验证。验收标准中明确：Windows 端功能以「CI 冒烟通过 + 人工验收通过」为完成定义。

## 八、许可证

- vcxsrv 本体（xorg-server + hw/xwin）为 MIT/X11：闭源分发无义务，保留版权声明即可
- `tools/mhmake` 为 GPLv3，但仅是构建期工具、不链接进产品，不构成分发障碍；不分发 mhmake 本身
- 顶层 GPLv3 `COPYING` 与各文件 MIT 头并存属历史遗留混乱，以各源文件头为准；ZzXsrv fork 保留全部原始声明并在 NOTICE 中说明
- OpenSSL（Apache-2.0）、pthreads4w（Apache-2.0）、freetype（FTL）、mesa（MIT，M4 裁剪移除）均无分发障碍

## 九、风险

| 风险 | 等级 | 应对 |
| ---- | ---- | ---- |
| Windows 构建链（mhmake/Cygwin/Perl）在 CI 搭建复杂 | 高 | M4a 核心工作；先原链基线再魔改；官方 release 二进制兜底 |
| 无 Windows 实机，嵌入效果只能 CI + 人工验收 | 中 | CI 截图冒烟 + 明确的验收门禁；用户后续补 Windows 机 |
| 官方 release 下载源可用性/版本漂移 | 中 | SHA256 校验 + 版本常量集中 + 下载失败重试；必要时镜像到自有仓库 release |
| reparent 后焦点/输入法/高 DPI 行为异常 | 中 | M4 预留实测缓冲；server 自泵消息模型已在设计中规避事件循环冲突 |
| 多 X11 会话资源占用（每会话一个 server 进程） | 低 | M1-M3 可接受；M4 评估共享 server |

## 十、验收标准

- M2 完成定义：Docker 端到端用例通过（经 SSH 转发 xdpyinfo 返回正确 display 信息），X11 性能门控入库
- M3 完成定义：Linux 本机勾选 X11 的 SSH 会话可跑远端 xeyes 并显示在系统 X server；macOS 同链路（XQuartz）代码就绪；Windows 端随 M4a 交付（2026-08-24，原 2026-08-23 裁决：官方二进制监听全网卡，不按现状交付）
- M4a 完成定义（2026-08-24 核销：前三条已达成，人工验收待用户实机执行）：
  - ZzXsrv CI 原链基线构建通过 — 已达成（2026-08-24）：run 32662240347（commit 60f8158b3，含 xkbdata 内容核验，约 26 分钟）
  - 回环绑定 patch 后产物冒烟仅监听 127.0.0.1:6000+N — 已达成（2026-08-24）：反例 run 32673661364（未 patch 基线监听 0.0.0.0:6099 被断言如期抓住）+ 正例 run 32676838484（patch 48a279edd 后仅监听 127.0.0.1:6099 + [::1]:6099，IPv6 对称断言 a8b15680e）
  - 主仓库下载源切换至 ZzXsrv release（`zz-21.1.16.1-1`）、Windows 门禁解除 — 已达成（2026-08-24）：主仓库 commit 7cc787a（kVersion 21.1.16.1-zz1，SHA256 4c6e568b…a0fa6，回归 43/43）；该 release 由 ZzXsrv 流水线 commit 62a11463b 产出
  - Windows 端 X11 会话经 SSH 转发跑远端 GUI 程序人工验收通过 — 待用户实机验收（验收清单见任务 5 步骤 3）
- M4b 完成定义：裁剪后 CI 构建出自有 ZzXsrv.exe，rootful 嵌入 Qt 标签页，截图冒烟通过 + 人工验收通过
- 全量回归：主仓库既有 43 项测试与 ZzSshCore 既有 14+14 项测试保持全绿，性能记录无超 5% 回归
