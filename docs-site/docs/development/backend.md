# 运行时、传输与存储开发

NyaTerm 没有独立的 Web 后端。原生应用的领域逻辑、协议运行时和持久化按职责拆分在多个 Rust crate 中，并通过有类型的接口与 GPUI 桌面层协作。

## Crate 边界

| Crate | 放置的行为 |
|------|------------|
| `nyaterm-core` | 纯模型、解析、策略、兼容格式和 schema-neutral 序列化 |
| `nyaterm-transport` | PTY、SSH、SFTP、Telnet、串口、隧道、远程操作和传输运行时 |
| `nyaterm-store` | redb、事务、存储 worker、加密适配和兼容性读取 |
| `nyaterm-terminal` | 终端状态机、快照、控制序列、编码和图形协议 |
| `nyaterm-remote-desktop` | RDP/VNC 会话管理、framebuffer 与输入模型、证书策略、剪贴板状态和 helper IPC 合约 |
| `nyaterm-rdp-helper` / `nyaterm-vnc-helper` | 隔离的 helper 进程，持有各自的协议解码器 |
| `nyaterm-otp` | HOTP/TOTP 兼容实现 |

低层 crate 不导入 GPUI 或桌面呈现类型。需要跨边界时定义小型 typed adapter，不把 application-wide model 下沉到 transport 或 store。

解析服务器控制字节的协议解码器只放在 helper crate 里。`nyaterm-remote-desktop` 自己不含解码器，两个 helper 必须把解码器 panic 转换成致命 IPC 错误上报。详见 [架构说明 → RDP / VNC 的进程隔离](./architecture#rdp--vnc-的进程隔离)。

## 会话与传输运行时

`nyaterm-transport` 的 `SessionManager` 管理活动会话，并为本地 PTY、SSH、Telnet、raw TCP、串口、RDP 和 VNC 提供统一生命周期入口。

会话输出通过 `SessionEvent` 返回：

- `Output` / `OutputDropped`
- `CwdChanged`
- `CommandAccepted`
- `Exited`
- `Error`

事件队列按原始事件顺序限制高吞吐输出：单个大 `Output` 会拆成有界 chunk，8/4 MiB 高低水位通过 producer wait 提供正常运行期的无损背压，不合并相邻 raw output。桌面层批量 drain 事件，而不是让后台线程直接修改 GPUI state。新增会话事件时使用有类型的 enum variant，并同时测试正常顺序、队列上限和关闭路径。

关闭单个 session 时，manager 必须先取消该 session 并唤醒 queue waiter，再关闭 transport 和 join reader；取消会清除该 session 的已排队事件并拒绝后续事件，但不能影响其他 session。manager 整体析构必须先全局关闭 queue、唤醒 producer 和 blocking consumer，再关闭并 join 全部 session。全局关闭后的空 blocking drain 立即返回；这些 shutdown 丢弃边界不改变正常过载的无损与 8/4 MiB 滞回契约。

SSH、SFTP、隧道、远程进程和 Docker 等操作也应保持异步或运行在专用 worker 上。不要让 transport 依赖窗口、对话框或桌面 feature 类型。

## 终端运行时

wire bytes 由 transport 送入 `nyaterm-terminal`。该 crate 维护网格、scrollback、模式、OSC 标记、搜索结果和图形协议状态，并生成 UI-neutral snapshot。

键盘和鼠标的协议编码中，与终端语义相关的部分留在 terminal/core 边界；像素尺寸、GPUI key event 和绘制逻辑留在 `nyaterm-terminal-gpui`。

## StoreRuntime 与持久化

`nyaterm-store::StoreRuntime` 拥有数据库 worker。桌面层通过 `StoreUiClient` 或 `StoreBlockingClient` 提交实现 `StoreRequest` 的 typed request，再以 task result 更新 UI。

数据库实现集中在 `nyaterm-store/src/storage/`。修改存储时必须保持：

- 现有 redb table 名、key 和文档 key
- 序列化字段名及未知字段处理
- 主密钥 wrapping、加密前缀和 fallback 解密
- `.nya` 备份、portable snapshot 和 Dragonfly 兼容读取
- 验证成功前不覆盖用户现有数据

纯兼容模型和策略属于 `nyaterm-core`；数据库 transaction、文件 I/O 和兼容 reader 属于 `nyaterm-store`。

## AI、同步与原生 HTTP

AI provider、翻译、云同步和更新检查通过原生 HTTP adapter 运行。请求构造、风险策略和 schema-neutral provider 设置尽量保持在 `nyaterm-core`，桌面层负责凭据遮罩、交互状态和任务协调。

云同步以 portable snapshot 为兼容合约。拉取数据必须先解密、解析和验证，再提交应用；冲突、重试和错误通过 typed result 返回 UI。

## 错误与安全

低层库返回 typed error，桌面适配层再决定用户提示。不要在错误、日志或 `Debug` 输出中包含密码、私钥、OTP、API secret 或未脱敏的终端上下文。

后台任务必须处理取消、窗口关闭和 runtime shutdown。退出时由 `AppShell` 请求 store flush/shutdown，避免 UI 已关闭后仍写入数据库。

## 测试

- pure parsing、策略和兼容格式测试放在 `nyaterm-core`。
- PTY、SSH、SFTP、Telnet、串口、隧道和事件队列测试放在 `nyaterm-transport`。
- 终端解析、快照、graphics 和 encoding 测试放在 `nyaterm-terminal`。
- 存储修改需要新数据 round trip、代表性旧数据、错误密码和损坏数据测试。
- RDP/VNC 的协议、framebuffer、输入映射、IPC、证书、剪贴板和重连测试放在 `nyaterm-remote-desktop`、`nyaterm-rdp-helper` 或 `nyaterm-vnc-helper`。两个 helper crate 的 `tests/lifecycle.rs` 覆盖握手、正常断开和崩溃 / 挂起回收，IPC 合约变更时同步更新两边。
- 跨 crate 行为优先测试小型 adapter，不依赖真实凭据或生产服务。
