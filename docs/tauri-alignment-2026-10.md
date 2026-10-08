# GPUI / Tauri 非插件功能对齐记录

参考范围：Tauri `bad1bec45..15922a560`。实施平台：Windows，2026-10-04。

## 范围与工作区

本轮不包含插件、外部包管理器更新保护和本地 PowerShell prompt 注入。实施阶段未提交主仓库；随后按用户要求分批本地提交，未推送。已有终端编辑改动作为集成基础保留，且已由独立提交 `c9a43228b` 收录；本文件不把这些并行改动归为本轮新增功能。`temp/` 仅作只读参考。

## 逐项结果

| 批次 | 实现与兼容性 | 自动化证据 |
| --- | --- | --- |
| 串口 | `SerialFlowControl` 使用 lowercase serde；缺省 None。None/Software/Hardware 贯通连接编辑、持久模型、导入和 transport。Windows 串口 writer 不 flush；软件流控禁止 modem 二进制传输。 | 旧配置/三种值往返；Windows no-flush writer 回归。 |
| SSH hook | 完整 Bash / CWD-only / 持久脚本使用不导出的私有 hook 间接引用，清除陈旧 export 属性，保留用户 hook 和只读处理。 | Git Bash 实际执行：导出的字符串/数组、只读 PROMPT_COMMAND、陈旧 hook、子 shell 退出码；脚本语法检查。 |
| 原始输入 | `SessionManager::write_raw` 与文本写入分离；鼠标和协议字节使用 raw 路径，Telnet 仅双写 IAC，raw TCP 原样写入；录制继续使用 raw input。 | 回环 TCP/Telnet 实际传输 CR/LF、DEL、IAC、高位字节；开启文本行编辑/编码/退格策略仍不转换 raw。 |
| ZMODEM | fork 保留 0.7.2 poll/submit API，补齐 ESCCTL 协商及头/数据/CRC 转义；上传使用 `rz -e` / `rz -e -y`；保留完成后的协议字节排空。 | fork 36 项；主仓库上传启动、回显识别、取消和 loopback 文件传输测试。 |
| Windows 环境 | portable-pty 按大小写不敏感规则刷新注册表变量并展开；GPUI shell 探测使用同一完整环境，不以旧进程环境覆盖。 | fork 12 项及 doctest；transport Windows 本地 PTY、PowerShell 环境探测测试。 |
| SFTP-only | 连接 attempt 统一拥有 Unknown/Available/Unavailable。仅明确 shell/exec 拒绝标记不可用；无 shell 的复用 SSH 会话保留 SFTP 功能，终端写入和 shell 操作返回能力错误。身份查询快速跳过，保留数字身份和 SFTP 原生删除。 | 本地 russh 测试服务拒绝 shell/exec后，仍可创建会话并执行原生 SFTP 删除；后续 exec/write/raw 不再发送，新的 attempt 恢复 Unknown；普通超时不标记永久拒绝。 |
| Gitee | 可选容量接口仅由 Gitee snippet 启用；新增文件计数不包含覆盖；仅清理当前根目录可验证快照，逐次重读 head，保护预期 head、当前 head、目标和恢复候选；两次有界上传尝试、读取验证后才推进基线。 | 满容量/覆盖、其他根目录、损坏快照、head 改变、不可读 head、删除失败；既有同步冲突和恢复测试。 |
| AI 代理 | 全局 System/Direct/Custom；HTTP/SOCKS5、认证、bypass；SOCKS5h 代理侧 DNS。聊天/流式/模型发现/连接测试共用构造逻辑。密码接入既有加密、遮罩、保留和清空契约。 | Direct/bypass 回环 HTTP；SOCKS5h URL、IPv6/认证校验、错误不含凭据；旧设置缺省、密码加密/遮罩/清空、备份与同步快照往返。 |
| Agent 历史 | 原生 run 创建时冻结当前 session 的有效历史，排除重复当前任务与空占位；续跑复用冻结历史、不重新加载数据库或重复保存任务。历史上限沿用现有 `history_turns` 的消息数量语义。 | session 隔离、历史冻结/上限；OpenAI/Anthropic/Gemini 的历史在任务之前；已有工具 transcript 和 Responses 转换测试。 |
| VNC | helper fork 支持 RA2_256；Auto 可协商，显式 None/VNCAuth 保持限制。IPC v9 按 session/generation/request ID 绑定挑战和回复。归属会话内提示，首次/变化公钥分别处理；仅认证初始化成功且请求仍有效才保存。 | fork 40 项+2 doctest；共享 IPC roundtrip、拒绝/过期/错误会话/取消、保存前二次校验；两个 helper 生命周期、崩溃/超时回收测试。 |
| VNC 存储 | 独立 `vnc_known_hosts` 表和 Tauri 字段/key 契约；CAS 防止覆盖更新后的信任，保留未知字段。可选便携实体支持 backup/sync；旧快照缺省不清除已有记录。 | CAS、大小写主机匹配、备份/同步往返、旧快照和损坏记录拒绝、取消提交回滚。 |
| 命令导航 | 锚点使用 terminal 现有行 metadata；OSC 133 优先，受限 fallback 共用当前输入证据。上一/下一/连续扩选接入平台快捷键与终端焦点作用域；不新增第二套输入 tracker。 | 命令块、wrapped 输入、滚屏淘汰、alternate screen、resize 后失效、清除；快捷键默认值和文本框上下文测试。 |
| 全部清除 | 与普通 Ctrl+L 分离；Ctrl+Shift+L / macOS 对应 Meta 组合。清除滚屏和旧输出，保留输入与宿主光标；适用 SSH/Unix shell 请求重绘，Windows 本地 shell 不发送 Ctrl+L。 | 终端保存光标/模式/wrapped 输入测试；真实 CMD/ConPTY 清除后继续输入并执行成功。 |
| 监控 | 完整 RemoteStats 按 session 缓存为唯一可变数据源；切回立即显示，再按既有调度刷新；关闭/重连清理并使旧任务失效。 | A→B→A、快照隔离、网络历史、关闭/陈旧结果、连续失败阈值测试。 |
| 性能路径 | 沿用现有 GPUI 调度：在跑的高亮解析不因每帧输出取消，完成后处理最新快照；超长 wrapped group 按已有预算降级。未引入 xterm。 | 持续 shell echo 高亮保留、超长 wrapped group 降级、gutter 宽度和输出突发测试；未进行 GUI 帧率基准。 |

## 依赖补丁

以下 revision 已提交、推送至对应 fork 的 `zzclawterm` 分支，并更新 Cargo.toml / Cargo.lock。各 checkout 的 `ZZCLAWTERM.md` 记录基础 revision、独立补丁及验证。

| fork | revision | 验证 |
| --- | --- | --- |
| nyakang/zmodem2 | `372662d9fef892ba36d628107bf8152ab46ba4ca` | 36 单测、fmt、clippy；真实 lrzsz 测试未运行。 |
| nyakang/portable-pty | `06515356358548ee2064f8a09c751472cbaff4ca` | 12 单测及 doctest；保留 ConPTY 补丁。独立 clippy 存在未改动上游 `uninit_vec` lint，允许该单项后通过。 |
| nyakang/vnc-rs | `689a65166d6440946529b2696314386a70d71d86` | RA2_256 与信任加固为独立提交；40 单测、2 doctest、fmt、严格 clippy。 |

## 最终验证

以下命令在实施完成时的源码上完成，退出码均为 0：

| 命令 | 结果 |
| --- | --- |
| `cargo check --workspace` | 通过。 |
| `cargo test --workspace` | 38 个测试套件，共 3256 passed、0 failed、13 ignored；包含两个 helper 的生命周期测试。 |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo clippy --workspace --all-targets` | 通过。 |
| `git diff --check` | 通过；Git 的 LF/CRLF 提示不属于空白错误。 |
| `cargo test -p zzclawterm-desktop --test clear_all_windows -- --ignored` | 单独运行真实 Windows CMD/ConPTY 测试，1 passed。该 opt-in 测试在上述 workspace 测试中为 ignored。 |

另单独运行当前构建的 `zzclawterm-terminal-gpui` 测试程序中的 `keyword_highlight_benchmark --ignored --nocapture`，1 passed。每种场景 200 行、10 次迭代，现有新路径匹配耗时分别为 3777 / 6891 / 9164 / 25264 / 7555 微秒，对照旧路径为 6695 / 43855 / 77297 / 143048 / 53072 微秒（无匹配、稀疏、密集、Unicode、密集短 token）。这是匹配微基准，不代表真实 GUI 帧率或持续输出的端到端性能验收。

原始日志为仓库根目录 `alignment-*.log`，按既有 `.gitignore` 不纳入 Git。依赖 fork 的验证和 revision 见上一节。

分批提交审查时，恢复了监控改动中误删的 GPU/NPU 面板 `clear_data` 清理语句，保持原有行为。随后运行 `cargo test -p zzclawterm-desktop --lib features::remote::state::tests`，23 passed、0 failed；重新运行 `cargo fmt --all -- --check` 通过。上述 workspace 全量结果为此前实施验证，未将这次定向复测表述为再次全量测试。

## 主仓库分批提交

| commit | 范围 |
| --- | --- |
| `feb7b3472` | 依赖 fork revision、SOCKS 和 helper 依赖。 |
| `049b73d88` | 串口流控、raw 输入、SSH/SFTP-only、Windows 环境和 ZMODEM 集成。 |
| `5e1366830` | Gitee 容量恢复和 UI。 |
| `6db6e7126` | AI 全局代理、凭据持久化和原生 Agent 历史。 |
| `ecb828a59` | VNC RA2_256、IPC 信任确认和 known-hosts 兼容存储。 |
| `eef8c5d55` | 命令导航、全部清除和 CMD 回归测试。 |
| `7f3cbe0a6` | 按 session 缓存资源快照。 |

本记录以独立文档提交收尾。共享文件按功能差异块暂存，未改写已有终端编辑提交；验证针对集成工作区，未分别检出每个中间提交重跑全量检查。

## 验收限制

- 没有真实串口设备：未做 None/Software/Hardware 的硬件流控和长时间收发验收。
- 未连接真实 RA2_256/Raspberry Pi 服务端：密码、密钥变化及拒绝场景以 fork 脚本化对端和 IPC/状态测试验证；跨窗口视觉交互未手工验收。
- 未在 Unix/macOS 上运行 SSH/PTY 或系统快捷键验收；Git Bash 脚本执行不能等同于 Unix SSH 实测。
- 未进行真实 Unix `rz/sz` 互通，也未配置一次性外部 SFTP 服务。仓库 opt-in E2E 仍受环境条件约束。
- 未用真实 Gitee 账号进行满容量删除/并发恢复，未用外部 HTTP/SOCKS5 认证代理连接各 AI 服务商。
- Windows 真实 CMD/ConPTY 清除测试通过；打包 ConPTY DLL 后端测试需要 `ZZCLAWTERM_CONPTY_TEST_DLL`，本轮未运行。
- 自动化通过代表已覆盖代码路径，不代表以上硬件、外部服务、GUI 和跨平台验收全部完成。
