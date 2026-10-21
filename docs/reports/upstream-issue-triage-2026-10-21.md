# 上游开放 Issue 适用性与修复建议

本报告按“继续跟随上游、尽量减少独立改动”的维护策略，核对上游开放问题与当前 ZzClawTerm GPUI 实现。仅做分析，不实施修复、不提交代码，也不向上游发送消息。

## 核查范围与版本基线

| 项目 | 本次核查基线 |
| --- | --- |
| 日期 | 2026-10-21，Asia/Shanghai |
| 本地 | `master`，`cab1c0f2e3d8c6d2be09710a5e4e60b45171fd21`，workspace `0.1.4` |
| 已同步的上游基线 | `5b39574cec78980f96eb4e33c1c5259ded6813d9`，位于 preview.7 之后 |
| 上游 Tauri 线 | `main`，`6703ce6d9c6ce855708786657fd5abf4c26900c6`；React 19 / Tauri 2，应用版本 `1.3.0` |
| 上游 GPUI 线 | `migration/gpui`，`2a27ce3750167c73a2d4044c376e23341a247be9`；`2.0.0-preview.8` |
| 开放条目 | GitHub API 返回 55 项，其中 48 条 issue、7 条 PR；下表覆盖全部 48 条 issue |
| 验证方式 | issue 正文与评论、当前源码、相关 PR 目标分支、依赖变更；另做无副作用 Bash 最小验证 |
| 未执行 | Cargo 构建/测试、GUI 实测、Windows/macOS/Citrix/网络设备/RDP 服务端验证 |

版本来源：[main 清单](https://github.com/nyakang/nyaterm/blob/6703ce6d9c6ce855708786657fd5abf4c26900c6/package.json)、[GPUI 清单](https://github.com/nyakang/nyaterm/blob/2a27ce3750167c73a2d4044c376e23341a247be9/Cargo.toml)、[preview.8 发布](https://github.com/nyakang/nyaterm/releases/tag/v2.0.0-preview.8)。preview.8 于北京时间 2026-10-21 19:30 发布。

我们跟随 GPUI 线，本地 `0.1.4` 与上游 `1.3.0` 不是同一版本序列。Web/Docker 模式属于另一条运行路径。issue 尚未关闭、具有 GPUI 标签，或代码中存在同名依赖，都不能单独证明当前项目仍有该缺陷。

相较已同步基线，preview.8 [新增 41 个提交、涉及 233 个文件](https://github.com/nyakang/nyaterm/compare/5b39574cec78980f96eb4e33c1c5259ded6813d9...2a27ce3750167c73a2d4044c376e23341a247be9)，包含双栏文件管理、面板结构、IME、编辑器标签及依赖升级。下一轮同步应单独安排，不宜把整批升级当成某一 issue 的修复证明。

## 处理建议

近期修复候选集中在六项：**866、925、950、946、886、889**。其中只有 866 的 heredoc 包装问题做了最小运行复现；其余有具体源码依据，但仍需目标环境验证。这里的优先级是维护顺序，不是对所有用户影响程度的断言。

- **优先准备最小修复：** 866 的 POSIX heredoc 包装，边界清楚，适合小范围补丁。
- **先复现、再跟进上游：** 925 的认证身份筛选、950 的 WSL 工作目录、946 的 emoji 宽度。
- **按实际设备需求安排：** 886 的旧 Dropbear 探测策略、889 的 GNOME RDP 能力协商。两者均涉及协议或配置兼容，不适合凭症状直接改开关。
- **保留兼容性回归：** 939 的 ble.sh、938 的 Citrix、897 的跳板设备、865 的 macOS RDP，以及下表中的旧版交互场景。
- **新功能继续等上游：** 外部 Agent、skills、脚本引擎、宏、新协议和跨平台通知等。

## 全量分流表

“已有相关实现”只表示当前路径已不同或能力部分存在，不表示原报告在所有环境都已解决。“先复现”表示证据不足以直接安排代码修复。

| Issue | 报告对象 / 需求 | 当前项目判断 | 建议 |
| --- | --- | --- | --- |
| [951](https://github.com/nyakang/nyaterm/issues/951) | Qoder CLI Agent 接入 | 当前 backend 没有 Qoder；属于新增集成 | 延期，等上游 |
| [950](https://github.com/nyakang/nyaterm/issues/950) | GPUI / Win11，WSL 启动目录错误 | 仅设置宿主进程 cwd，没有 WSL home / Linux 工作目录专门处理 | 近期复现后修复 |
| [949](https://github.com/nyakang/nyaterm/issues/949) | skills、未选目标时 DSML 输出 | 已有活动终端默认目标与无目标拒绝；DSML 需按模型复现；skills 尚缺 | 拆分问题，协议异常复现，skills 延期 |
| [946](https://github.com/nyakang/nyaterm/issues/946) | GPUI / Win10，emoji 覆盖相邻字符 | 按单字符算宽、FE0F 附着链路存在风险；需要字体与实际终端验证 | 近期复现后修复 |
| [939](https://github.com/nyakang/nyaterm/issues/939) | ble.sh 初始化冲突，版本未明 | 报告注入脚本不同于当前实现；PROMPT_COMMAND / DEBUG hook 仍有兼容性风险 | 定向复现，不凭缺少标签排除 |
| [938](https://github.com/nyakang/nyaterm/issues/938) | GPUI / Citrix 无 GPU，高 CPU | 属于适用场景；禁用 DirectComposition 已被报告者反馈无效，根因未确定 | 跟踪上游，先做目标环境性能采样 |
| [928](https://github.com/nyakang/nyaterm/issues/928) | GPUI / Ubuntu，希望 deb 在线更新 | Linux 原生安装更新仅允许符合条件的 AppImage；deb 不在该路径 | 打包能力需求，延期或改进说明 |
| [925](https://github.com/nyakang/nyaterm/issues/925) | SSH Agent 身份过多导致认证失败 | 登录仍依次尝试全部身份；转发白名单不控制登录认证 | 近期验证 MaxAuthTries 场景并跟进修复 |
| [916](https://github.com/nyakang/nyaterm/issues/916) | MCP 审批系统通知 | 有应用内审批，缺操作系统通知入口 | 新功能延期 |
| [909](https://github.com/nyakang/nyaterm/issues/909) | Tauri 1.3.0，重启丢失待自动推送 | 当前 Startup 检查发现 LocalChanged 可推送；仍受启动检查、provider 等条件限制 | 用原步骤回归，不直接移植 main PR |
| [897](https://github.com/nyakang/nyaterm/issues/897) | Tauri 1.3.0，经跳板连设备后断开 | 维护者认为跳板与 shell 已成功；多 channel 兼容性尚属推测 | 与 886 共用设备复现矩阵 |
| [894](https://github.com/nyakang/nyaterm/issues/894) | 图标布局与面板打开状态同步/备份 | 活动栏配置已有序列化；活动面板、宽度等明确按设备保留 | 部分已有；剩余属于同步范围设计 |
| [891](https://github.com/nyakang/nyaterm/issues/891) | Bell / 任务栏提醒 | terminal 产出 bell，但 desktop 没有对应系统提醒消费链路 | 确有功能缺口，可等上游小补丁 |
| [889](https://github.com/nyakang/nyaterm/issues/889) | Tauri 1.3.0，GNOME Remote Desktop 50.2 要求 EGFX | 当前 helper 未启用 Graphics Pipeline 能力声明；依赖存在 EGFX 不等于已可用 | 近期按目标服务端验证并跟进修复 |
| [887](https://github.com/nyakang/nyaterm/issues/887) | 快捷命令转义与暂停 | 当前纯文本路径无此功能；上游已有 GPUI PR 931 | 等 PR，重点审查旧命令兼容性 |
| [886](https://github.com/nyakang/nyaterm/issues/886) | Tauri 1.2.12，旧 Dropbear 被后台探测断开 | 当前缺单连接资源/进程/Docker 细粒度开关；network_device 还会禁用 SFTP | 近期设备复现，跟进精细策略 |
| [877](https://github.com/nyakang/nyaterm/issues/877) | Tauri 1.3.0，WebKitGTK 2.54.1 刷新异常 | 原生 GPUI 不走该 WebView 渲染链路 | 不移植旧渲染修复 |
| [873](https://github.com/nyakang/nyaterm/issues/873) | Tauri 1.3.0，IPv6 SSH 连接 | 当前支持 host/port 分离直连；报告无文本错误证据 | 先核实裸 IPv6 输入，再分测代理/跳板 |
| [866](https://github.com/nyakang/nyaterm/issues/866) | Tauri 1.3.0，AI heredoc / SQL REPL 超时 | heredoc 在当前模板仍失败；SQL REPL 是独立协议问题 | heredoc 优先最小修复，REPL 单独复现 |
| [865](https://github.com/nyakang/nyaterm/issues/865) | macOS RDP；GPUI 标签与正文 1.3.0 冲突 | 日志只支持 TLS upgrade 被 reset，不能确定当前 GPUI 根因 | 核实实际构建后实机复现 |
| [864](https://github.com/nyakang/nyaterm/issues/864) | Web / Docker 1.3.0，SSH cwd 等 | 部署形态及前端路径不同于本项目 | 不直接纳入原生 GPUI 修复 |
| [859](https://github.com/nyakang/nyaterm/issues/859) | Tauri 1.3，CachyOS / KDE Wayland 窗口闪烁 | 缺日志，不能外推当前 GPUI | 保留 Linux 冒烟场景 |
| [849](https://github.com/nyakang/nyaterm/issues/849) | Linux 标题栏按钮/装饰不一致 | 评论确认实际是 GPUI 2.0；当前有按钮代码，缺失原因需窗口管理器验证 | 先测 labwc；装饰风格选项延期 |
| [709](https://github.com/nyakang/nyaterm/issues/709) | Tauri 1.2.10，新建 SSH 偶发无响应 | 当前异步会话链路已不同，原报告证据不足 | 保留连接/取消/重连回归 |
| [707](https://github.com/nyakang/nyaterm/issues/707) | 固定终端列数、水平滚动 | 当前列数来自视口；新增能力牵涉 resize / reflow / 选择 | 功能延期 |
| [696](https://github.com/nyakang/nyaterm/issues/696) | Tauri 1.2.10，选区错位 | GPUI 有独立 grid 选择与宽字符测试；仍需原场景验证 | 作为字体/DPI/宽字符回归样例 |
| [686](https://github.com/nyakang/nyaterm/issues/686) | Tauri 1.2.10，Qoder CLI 无法输入 | GPUI 输入栈不同，缺当前版本 CLI / shell 组合证据 | 实测后再决定 |
| [643](https://github.com/nyakang/nyaterm/issues/643) | 自定义 Agent 提示词 | 当前内建协议提示仍固定 | 功能延期，等上游扩展入口 |
| [634](https://github.com/nyakang/nyaterm/issues/634) | Tauri 1.2.9，pi 粘贴/Esc/Ctrl-C 异常 | 当前已有按键和 bracketed paste 逻辑，但不证明该 TUI 正常 | 纳入 TUI 兼容回归 |
| [612](https://github.com/nyakang/nyaterm/issues/612) | 自动化脚本引擎 | 大型新功能；NyaScript PR 854 面向 main | 延期，不移植旧前端方案 |
| [601](https://github.com/nyakang/nyaterm/issues/601) | Telnet 代理/跳板 | 当前 Telnet 直接 TCP，没有路由配置链路 | 新功能延期 |
| [595](https://github.com/nyakang/nyaterm/issues/595) | Mosh | 没有原生 Mosh 会话；PR 853 面向 main | 新协议延期 |
| [590](https://github.com/nyakang/nyaterm/issues/590) | Tauri 1.2.8，Win7 虚拟机拖动闪烁 | 旧平台/旧渲染路径，不能与 GPUI Citrix 合并归因 | 不纳入本轮修复 |
| [560](https://github.com/nyakang/nyaterm/issues/560) | 原生 FIDO2 / SK 密钥创建 | 外部 Agent 签名入口存在，原生硬件访问和生成仍缺 | 先验证现有 Agent 组合，新能力延期 |
| [544](https://github.com/nyakang/nyaterm/issues/544) | OpenCode 本地/远端集成 | 当前无对应 backend；React 方案不适用 GPUI | 新集成延期 |
| [536](https://github.com/nyakang/nyaterm/issues/536) | 批量编辑连接配置 | 未找到对应批量编辑流程；属于效率功能 | 延期，等上游明确设计 |
| [519](https://github.com/nyakang/nyaterm/issues/519) | Tauri 1.2.5，华为防火墙 AI 执行超时 | 已包含 PR 920 相关 NetworkDevice / SendOnly 改进 | 做设备验收，不重复实现 |
| [509](https://github.com/nyakang/nyaterm/issues/509) | Tauri 1.2.5，交互堡垒机 Agent 等待 cwd | 当前使用缓存 Option cwd，不再走原报告的远程 cwd 等待路径 | 当前堡垒机场景复现后再修 |
| [503](https://github.com/nyakang/nyaterm/issues/503) | Tauri 1.2.5，tmux 同步输入泄漏协议响应 | 当前 PTY 协议响应单 session 直写，不走同步输入广播 | 保留 tmux 回归 |
| [499](https://github.com/nyakang/nyaterm/issues/499) | SPICE / KVM | 无 SPICE 会话或协议实现 | 新协议延期 |
| [445](https://github.com/nyakang/nyaterm/issues/445) | Tauri 1.2.3，xrdp 首次断开/重连剪贴板 | 当前已有独立 ClipboardBridge 与 resize 降级，但无原场景实测 | 按原 xrdp 版本回归 |
| [441](https://github.com/nyakang/nyaterm/issues/441) | Tauri 1.2.2，远端 head 指向缺失快照 | 当前已有损坏快照诊断、兼容副本与明确恢复冲突路径 | 数据完整性回归，不重复实现 |
| [438](https://github.com/nyakang/nyaterm/issues/438) | 宏 | 新交互/自动化能力 | 功能延期 |
| [374](https://github.com/nyakang/nyaterm/issues/374) | Tauri 1.1.18，命令建议空格丢失 | 原前端高亮问题；GPUI 分段显示也应目视验证边界空格 | 跟踪 main PR 947，保留场景，不直接套补丁 |
| [372](https://github.com/nyakang/nyaterm/issues/372) | Tauri 1.1.18，建议焦点/历史/提示符异常 | GPUI 有独立建议状态链路；原交互序列仍有回归价值 | 跟踪 main PR 945，不直接套补丁 |
| [357](https://github.com/nyakang/nyaterm/issues/357) | Tauri 1.1.18，关键词高亮不一致 | GPUI 已有独立 snapshot/cache 与测试，未验证原 ping 场景 | 低成本显示回归 |
| [316](https://github.com/nyakang/nyaterm/issues/316) | AI 文件/图片、视觉、思考强度 | 思考强度已有；当前聊天请求附件为空，完整视觉上传链路仍缺 | 部分已有，其余延期 |
| [201](https://github.com/nyakang/nyaterm/issues/201) | trzsz 传输 | 已有上传/下载 worker、协议过滤、transport 引擎和 UI | 不重复建设，按具体兼容场景回归 |

## 六项近期候选的证据与验收边界

### 866：AI heredoc 包装破坏 Shell 语法

证据：[agent_capture.rs](../../crates/zzclawterm-core/src/agent_capture.rs) 的 `build_posix_capture_command`（基线第 24–27 行），通过 [terminal_execution.rs](../../crates/zzclawterm-desktop/src/features/capability_runtime/terminal_execution.rs) 的真实命令执行链使用。

当前包装形态为：

```text
printf START; { <command>; }; ... printf END ...
```

当 command 是 heredoc 时，结束符必须独占一行。现模板会把结束符变成 `EOF; }; ...`。即使 command 自带末尾换行，也会生成下一行以 `; };` 开头的非法语法。

只读提取源码中的实际 Rust 格式字符串，以 `bash --noprofile --norc` 执行无副作用样例，结果如下：

| 输入 | 结果 |
| --- | --- |
| 普通 `printf 'ok\n'` | 退出码 0，START/END 标记均存在 |
| `cat <<'EOF'` + 正文 + `EOF`，无末尾换行 | 退出码 2，heredoc 结束符未匹配、unexpected EOF |
| 同上，保留末尾换行 | 退出码 2，unexpected token `;` |

这是当前模板的可确认缺陷。交互 PTY 中等待输入、超时及 UI 表现尚未运行验证。修复候选应仅调整命令包装的换行/分隔边界，并验证普通命令、多行命令、heredoc、末尾注释、非零退出码与 marker 完整性。

原 issue 中 SQLPlus/MySQL/psql 的 REPL 问题另行处理：当前网络 CLI 的 SendOnly 改进不等于通用 REPL 协议支持，不能用 heredoc 小修承诺解决整条 issue。

### 925：SSH Agent 登录没有身份筛选

证据：[ssh_auth.rs](../../crates/zzclawterm-transport/src/ssh_auth.rs) 第 244 行获取全部 identities，第 260 行开始逐一认证；[connection.rs](../../crates/zzclawterm-core/src/models/connection.rs) 明确 forwarding policy 独立于登录认证。

当 Agent 有很多密钥、正确密钥排序靠后且服务端 `MaxAuthTries` 较低时，存在提前断开的实际风险。现有“转发白名单”不能解决这个问题。

下一步应复现多身份与低 MaxAuthTries 组合，优先跟踪上游独立认证身份选择方案。如果新增持久化字段，旧配置默认仍应保持原行为，并补兼容性测试。临时可用明确的私钥配置或减少 Agent 身份；不要借用转发策略改变登录语义。

### 950：WSL 工作目录缺少专门处理

证据：[session start.rs](../../crates/zzclawterm-desktop/src/features/session/session_runtime/start.rs) 第 436 行传递工作目录；[transport lib.rs](../../crates/zzclawterm-transport/src/lib.rs) 第 1121 行仅调用宿主 `command.cwd`；[local_shell/mod.rs](../../crates/zzclawterm-transport/src/local_shell/mod.rs) 构造 shell 参数，没有 WSL Linux cwd 转换。

Windows 宿主进程 cwd 与 WSL 内部 Linux cwd 语义不同。需要在 Windows 分别测试默认目录、Linux 绝对路径、Windows 路径、用户显式传入 `--cd` 的情况。修复应限制在 shell 启动适配，避免覆盖用户已有参数或影响普通 PowerShell/cmd。

### 946：emoji VS16 与终端单元格宽度

证据：[cells.rs](../../crates/zzclawterm-terminal/src/cells.rs) 第 24 行起按 Unicode scalar 计算宽度，FE0F 作为零宽字符附着/跳过；现有 variation selector 测试使用 `a + FE0F`，不能覆盖 `ℹ️` 的 emoji 呈现宽度。固定 cell 绘制位于 [element/mod.rs](../../crates/zzclawterm-terminal-gpui/src/element/mod.rs)。锁定的 alacritty 输入逻辑也使用逐字符宽度。

需要用 issue 的精确字符样例，在 Windows 字体回退环境检查 grid 列数、光标移动、选择复制和实际字形宽度。只有确认是解析、布局还是字体 fallback 问题后，才能选择正确修复层；不能只扩大绘制范围掩盖 grid 错位。

已核对上游 alacritty 从 `4155cc1` 到 preview.8 使用的 `3b60b2e` 的补丁，涉及 Kitty mode stack 和 grid generation，没有修改 FE0F/字符输入宽度，不能把这次依赖升级算作该问题已修复。

### 886：旧 Dropbear 与后台探测

证据：[session_config.rs](../../crates/zzclawterm-transport/src/session_config.rs) 第 355–364 行：`network_device` 同时禁用 SFTP 和资源统计；[auto_refresh.rs](../../crates/zzclawterm-desktop/src/features/remote/remote_runtime/auto_refresh.rs) 使用全局面板设置；[remote_process.rs](../../crates/zzclawterm-transport/src/remote_process.rs) 第 470 行附近打开额外 exec channel。

用户要求的是“只对某个旧设备关闭资源/进程/Docker 探测，同时保留 SFTP”。当前宽范围 network_device 模式不等同这一能力，但后台探测是否仍导致断连，需要在旧 Dropbear 上验证。

建议与 897 共用：直连/跳板 × 目录跟随开/关 × standard/network_device 的矩阵。若确认问题，跟进单连接策略，并在 transport 打开 channel 前执行门禁；只隐藏面板无法保证停止后台请求。不要先重做 SSH 连接复用架构。

### 889：GNOME RDP 的 Graphics Pipeline 协商

证据：[RDP helper main.rs](../../crates/zzclawterm-rdp-helper/src/main.rs) 第 589–627 行的 ConfigBuilder 没有调用 `with_graphics_pipeline`。当前 IronRDP `56dd868` 的 builder 默认关闭该能力。

依赖已有 EGFX 模块和部分 compositor，并不代表本应用已向服务端声明支持，更不代表 GNOME 所需 codec 全部可用。应在 GNOME Remote Desktop 50.2 验证握手、首帧、连续重绘和 resize，再判断最小改动范围。

已对比 preview.8 使用的 IronRDP `1bc71d95`：`config.rs` 与当前版本相同，Graphics Pipeline 仍默认关闭；EGFX 坏帧容错和 Reset 缓存保留属于协商后的可靠性改进。该依赖升级本身不能证明本地协商问题解决。修复仍应保留协议解码在 helper 内，不引入第二套 RDP 引擎。

## 容易误判的已实现或部分实现项

- **909 云同步重启：** [coordinator.rs](../../crates/zzclawterm-desktop/src/app_shell/controller/auto_sync/coordinator.rs) 已调度 Startup；[auto_sync.rs](../../crates/zzclawterm-desktop/src/features/sync/auto_sync.rs) 在 LocalChanged 且开启自动推送时执行 push。但关闭启动检查时会跳过 Startup，GitHub Gist 也明确跳过若干触发器，因此需要按 WebDAV 原步骤和实际开关组合回归，不能泛称所有 provider 已修复。
- **894 布局同步：** [portable_snapshot/mod.rs](../../crates/zzclawterm-core/src/portable_snapshot/mod.rs) 的 `project_sync_settings` 保留活动栏等可同步配置，明确移除 open_tabs、面板宽度、active_left/right_panel 等设备本地字段；导入同步时再保留本地值。相关序列化测试已存在。完整备份与云同步投影也应分开看；剩余诉求需要决定哪些状态适合跨设备共享。
- **441 快照缺失：** [cloud_sync/protocol.rs](../../crates/zzclawterm-core/src/cloud_sync/protocol.rs) 已将不可读 head 转成明确的恢复/冲突状态，并检查兼容副本。应保留缺失、损坏及恢复回归；不要采用评论中清空远端片段再上传的破坏性建议。
- **519 华为设备：** 当前已有 NetworkDevice / SendOnly 相关实现，可参考 [PR 920 历史验证报告](pr-920-network-cli-agent.md)。仍需验收真实 prompt、分页、编码及慢输出，历史报告中的合并状态应按当时日期理解。
- **503 tmux：** [view_io/input.rs](../../crates/zzclawterm-desktop/src/features/terminal/terminal_runtime/view_io/input.rs) 的 `write_terminal_pty_responses` 单 session 写入，不走同步输入广播。原 xterm.js 报告的直接因果链已不同，但还需 tmux 实测。
- **201 trzsz：** [trzsz_runtime.rs](../../crates/zzclawterm-desktop/src/features/session/trzsz_runtime.rs) 已有上传、下载、worker 与 UI 链路；不是只有 parser。仍可按 tmux、目录传输和对端版本做兼容性验收。

版本判断还需特别注意：849 的评论明确纠正为 GPUI；865 则存在 GPUI 标签与正文 1.3.0 的冲突。两者都不能只看标签分流。

## 已有开放 PR 与等待策略

以下是核查时的 7 条开放 PR；目标分支决定其能否直接用于当前项目。

| PR | 目标分支 | 处理建议 |
| --- | --- | --- |
| [948](https://github.com/nyakang/nyaterm/pull/948) 重启后恢复云同步自动推送 | `main` | 对应 909；GPUI 已有另一套调度实现，借鉴场景，不直接移植 |
| [947](https://github.com/nyakang/nyaterm/pull/947) 建议高亮保留空格 | `main` | 对应 374；借鉴回归样例 |
| [945](https://github.com/nyakang/nyaterm/pull/945) 历史导航与提示符匹配 | `main` | 对应 372；不直接套用旧前端补丁 |
| [931](https://github.com/nyakang/nyaterm/pull/931) 快捷命令转义与暂停 | `migration/gpui` | 对应 887；优先等上游审查/合并，再评估同步 |
| [854](https://github.com/nyakang/nyaterm/pull/854) NyaScript 自动化 | `main` | 大型新功能，延期 |
| [853](https://github.com/nyakang/nyaterm/pull/853) Windows Mosh | `main` | 新协议能力，延期 |
| [718](https://github.com/nyakang/nyaterm/pull/718) Braille 等字形填满单元格 | `main` | 与 emoji VS16 宽度并非同一问题，不能当作 946 的现成修复 |

931 尤其需要注意：“存储格式不变”不等于“旧命令行为不变”。若全局开始解释 `\n`、`\t`、`\p` 等，已有 printf、正则或 Windows 路径可能改变含义。同步前应核查启用方式/迁移策略，以及暂停期间固定目标会话、取消和断连处理。本轮没有完整审查其实现，也没有另写竞争方案。

## 后续执行边界

1. 按本报告补齐目标环境证据，确认当前构建仍复现后再选择小补丁。866 可先从纯逻辑回归测试开始。
2. 有上游补丁时先审查对应 GPUI 提交；不同架构的 PR 只借鉴复现与测试，不机械搬运。
3. 应用层修复与依赖 fork 修复分开；如涉及 alacritty、GPUI 或 IronRDP，走 fork 提交与固定 revision 更新流程，不修改 `temp/vendor` 或 Cargo checkout。
4. 新增连接配置/同步字段时保留旧数据默认行为与兼容测试；终端、SSH、RDP 的纯逻辑仍留在对应 crate，协议解码保持 helper 隔离。
5. 大型功能与架构调整继续遵循 [延期设计评审](design-review-deferred-2026-10-21.md)。本报告仅列出维护候选，不表示已决定实施这些需求。

本次只新增分析文档；未修改应用源码、依赖或配置，未提交。除 heredoc 的 Bash 最小验证外，其他结论来自静态核查，不能替代目标平台和真实服务端验收。
