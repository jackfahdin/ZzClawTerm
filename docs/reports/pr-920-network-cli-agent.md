# PR #920 网络设备 CLI Agent 执行改进

日期：2026-10-18。审查及修改基于 PR #920 的 `29c7e5adaa02678a30a9846d232f63eac23576d8`。

## 源码与原有问题

任务开始时原工作区在 Tauri `main`（`959fb1b65`），工作区干净，并非 GPUI 分支。修改放在独立 worktree `nyaterm-pr-920`、分支 `fix/pr-920-network-cli`。实现阶段未创建提交或修改远程资源；后续按维护者要求，以追加提交整合进原 PR，保留原作者提交历史。

原始调用链：

- 旧式 Agent 命令卡片经过既有风险/审批与目标校验，进入 `apply_ai_command_card_value`，再调用 `begin_ai_agent_observation` 或 `begin_ai_agent_background_execution`。
- Native Agent 的 `terminal_execute` 经过 `dispatch_capability_request` 的作用域、风险策略和审批，进入 `dispatch_native_terminal_execute`，然后复用上述两个入口。
- 外部 MCP 的 `terminal_execute` 使用独立的 `dispatch_mcp_terminal_execute`，直接调用 `SshProcessService::run_command`，因此必须另行限制。
- 前台根据 `AiExecutionProfile` 构建 Marker 包装或发送原始命令。SessionEventBridge 将有捕获任务的会话输出路由到 UI 事件处理器；`handle_session_output_after_sideband` 复用有状态解码器，再调用 `process_agent_output`。
- POSIX/PowerShell/Cmd 的 Marker 输出通过 `AgentOutputCaptureProcessor` 返回。原 SendOnly 则轮询有限 UI Tail 的长度，静止 900ms 后从旧偏移截取字符串，再启动 continuation 或回复 Native tool。

PR 原有启发式会将 `root#`、`bash-5.2#` 判成网络设备，也没有复用 SSH NetworkDevice 配置。UI Tail 上限为 128 KiB，满后长度不代表输出活动；`render_degraded` 及 Frame 合并也可能省略 UI 文本。原轮询在检查超时前先检查长度变化，持续输出可能推迟超时。后台 Agent 和外部 MCP 均存在绕回 SSH Exec 的独立路径。

## 修改范围

| 文件 | 改动 |
| --- | --- |
| `crates/nyaterm-core/src/network_cli_prompt.rs` | 显式 AI Profile 优先；Auto 先使用 NetworkDevice，再进行被动 Prompt 判断；排除常见 Shell 和输入等待提示；增加有界 ANSI/CR 末行观察及后台策略函数。 |
| `crates/nyaterm-core/src/raw_cli_capture.rs` | 新增轻量、每条命令独立的原始输出捕获；保存输出、末行及活动时间，区分 Prompt、交互、未确认静止、超时和截断。 |
| `crates/nyaterm-core/src/lib.rs` | 声明新增模块。 |
| `crates/nyaterm-desktop/src/features/ai/ai_agent_runtime.rs` | 从现有 SessionLaunchConfig/SshSessionConfig 读取 NetworkDevice；注册原始捕获；处理完成和失败；原始输出丢失时保留已捕获前缀并停止自动执行；提供统一后台限制。 |
| `crates/nyaterm-desktop/src/features/ai/state/mod.rs` | 在已有 Agent 作用域内持有捕获；通过原有 Session 输出旁路接收数据；清理取消、断线、输出丢失和重置资源；Marker 超时不再依赖 UI 长度。 |
| `crates/nyaterm-desktop/src/features/ai/state/tests.rs` | 增加连续命令、隔离、取消、断线、丢失、超时及实际 GPUI SessionEvent 回归测试。 |
| `crates/nyaterm-desktop/src/features/ai/harness_runtime.rs` | Native tool 启动失败时返回实际原因。 |
| `crates/nyaterm-desktop/src/features/capability_runtime.rs` | 外部 MCP 的后台 SSH Exec 使用同一限制。 |
| `crates/nyaterm-desktop/src/features/ai/mod.rs`、`features/runtime_jobs.rs`、`features/terminal/view_state.rs` | 删除废弃的静止常量、UI 长度/偏移字段及长度查询。 |

没有改动持久化枚举、配置格式、连接模型或 SSH Transport。已有 `SshProfile::NetworkDevice → SshSessionProfile::NetworkDevice` 转换继续使用。没有新增跨 crate 依赖或 Arc/Mutex；正常 Shell 的 Marker 构建和解析器保持原样。

## 执行与完成语义

1. 显式 Posix、Powershell、Cmd、SendOnly、Disabled 均优先于自动识别。Disabled 在前台及后台均禁止执行。
2. Auto + SSH NetworkDevice 直接选择 SendOnly；普通 SSH Auto 未命中被动提示时继续 Posix。后台对已知设备拒绝额外 Exec，即使用户显式选择 Posix，也不会借此打开设备不支持的后台通道；前台仍尊重该显式选择。
3. 原始命令在发送前注册捕获，并复用原有 SessionEventBridge 路由。有界 UI Tail 只提供开始前的 Prompt 提示，不提供命令输出、活动计数或完成偏移。
4. 开始前有已知 Prompt 时，返回同一设备的 Prompt，并静止 400ms 后结束观察。400ms 用于末行分块稳定，既有 Prompt 未返回时，即使只回显命令后停顿十秒，也不算完成。常见 `<HOST> → [HOST-view]` 和 `Router> → Router(config)#` 切换可比较。
5. Prompt 返回仅表示 CLI 回到输入状态；输出明确附带“命令成功和 exit code 未知”，exit_code 保持 None。设备错误文本不会被伪装成退出码 0。
6. 分页、确认、密码输入等待、截断、整体超时及输出丢失停止 Agent 自动执行，并将原因呈现给用户。不会自动翻页、确认或发送下一条命令。
7. 没有可靠的开始前 Prompt 时，八秒静止只触发“完成未确认”错误；即使输出形似 Prompt 也不确认为完成。完全没有输出则等待整体超时。
8. 捕获输出上限为 1 MiB，独立于 128 KiB 的 UI Tail；超过上限明确报告截断，不因长度不增长而判成功。整体截止时间不随输出续期。
9. 捕获属于已有 Agent 作用域；轮询快照不复制命令全文；每个事件仅追加/解析新数据，轮询只检查有界末行。取消、断线、结束、重置及丢失都释放捕获并恢复原有路由策略。

后台 Agent 和外部 MCP 当前明确拒绝设备/raw CLI 执行。用户可使用内置 Agent 并关闭 Background Execution，通过现有交互终端执行；没有静默改用 Shell 包装，也没有绕过原有高风险审批。

## 与 Netcatty 的比较

原工作区没有指定的 `temp/Netcatty`。为完成参考分析，在新 worktree 的忽略目录中只读克隆了 `binaricat/Netcatty`（`f9230e76c36a030967b1091ab181834e93e33325`），核对了任务列出的五个文件。

共同点是显式网络设备配置优先、原始 PTY 命令、不依赖 UI 文本的命令捕获、输出活动/整体超时/取消生命周期，以及避免不兼容的额外 SSH Exec。这里复用了 Rust SessionEventBridge 和 GPUI Agent 作用域，没有复制 Electron/TypeScript 的监听器架构。

Netcatty Raw PTY 的三秒静止、最初两个块六秒静止、无响应成功回退和达到输出上限后停止重置静止计时，并不能证明命令完成。本实现使用 Prompt 比较；未知完成、分页、截断和无输出均不会通过静止时间伪装成成功。

SSH Transport 没有现成传递到 Session Metadata 的服务器软件标识字段。本次没有为 Banner 重构 Transport，也没有新增远端探测命令。识别目标是执行环境，无法从 `HOST#` 精确判定品牌；继续扩展厂商正则不能消除与自定义 Shell Prompt 的歧义，因此没有新增厂商规则引擎或 Prompt Regex UI。

## 验证

Windows 本机实际执行：

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo check -p nyaterm-core -p nyaterm-desktop --locked` | 通过，无编译警告。 |
| `cargo test -p nyaterm-core --locked` | 429 个单元测试及 23 个集成测试通过；1 个原有手动复杂度基准被忽略。所有既有 Marker 捕获测试通过。 |
| `cargo test -p nyaterm-desktop --lib features::ai:: --locked` | 109 通过，1 个原有合成性能测试被忽略。 |
| `cargo test -p nyaterm-desktop --lib features::shell::event_pump:: --locked` | 32 通过。 |
| `cargo test -p nyaterm-desktop --lib models::session_event_bridge:: --locked` | 15 通过。 |
| `cargo test -p nyaterm-desktop --lib features::capability_runtime:: --locked` | 3 通过，包括审批重检及禁用执行。 |

新增覆盖包括 NetworkDevice/Auto 与显式 Profile 优先级、后台禁止策略、常见 H3C/Huawei/Cisco/Ruijie/Aruba Prompt、Linux root/bash 反例、ANSI/CR 重绘、回显后长停顿、分页/确认、无输出、持续输出、UI 容量之外的数据、捕获截断、命令间隔离、作用域切换、取消、断线和输出丢失。

GPUI 集成测试真实注入 `SessionEvent::Output`，在已有 UI Tail 填满并启用 render_degraded 时仍得到完整的当前命令结果；不依赖真实 SSH 设备。没有运行整个 Workspace 的最重构建。

## 已知边界与合并判断

- 仅凭 `HOST#` 无法完全区分网络设备和自定义 Shell；应优先配置 NetworkDevice，或显式指定 Shell Profile。
- Raw PTY 没有 Marker 事务边界；异步 syslog、用户手动输入、发送前已排队的输出，以及内容恰好等于 Prompt 的命令结果仍可能混入或影响观察。当前串行 Agent 目标锁不能约束人工输入或设备自行输出。
- 复杂 ANSI 光标移动、自定义/改名 Prompt、部分嵌套视图切换和缺少初始 Prompt 会保守失败；末行观察不是完整终端仿真。分页和确认由用户接管。
- 取消或观察超时释放本地资源并停止自动 continuation，不保证远端命令已经停止；不会盲目注入 Ctrl+C 或确认输入。用户恢复自动执行前应检查终端状态。
- 整体输出仍通过既有 UI 事件旁路处理；每事件处理和内存有界，但没有将整个 AI 捕获重构到后台线程。高输出超过预算会明确停止观察。UI Frame 文本省略不影响捕获，Transport 的 OutputDropped 则明确视为数据丢失。
- 本次没有重构 SSH Banner 传递、自动设置设备分页策略或承诺远端命令语义成功；普通 SSH 自身已有的连接/探测策略没有扩大到这次改动。
- 上述结果仅为 Windows 本地自动化验证，不能据此判定远程 PR 已达到合并条件。仍需新提交的远程 CI、代码审查和实际网络设备验收，特别是原始报告的 macOS 场景及 Prompt/编码/分页/慢速输出。没有真实设备或跨平台手工验证结果，不能宣称任意设备均可靠完成。Issue #910 保持 Open，PR #920 在这些验证完成前不合并。
