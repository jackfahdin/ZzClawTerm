# 本地终端、RDP、VNC 稳定性修复验收记录

日期：2026-10-17。实际平台：Windows x64（MSVC），rustc/cargo 1.99.0。本记录区分自动回归与尚未完成的真实服务器/其他操作系统验收。

## 实现与边界

| 风险 | 修复行为 |
| --- | --- |
| 动态纹理内存复制/分配失败 | 紧密行借用输入；填充行一次可失败的临时分配；WGPU retained upload 可失败；DirectX 返回分配失败，Metal 检查 nil，native WGPU 捕获设备分配错误。 |
| 多远程桌面 CPU 像素占用 | 应用进程共享 1 GiB 配额，覆盖 IPC 接收、排队帧、游标、framebuffer；不可变像素与 RAII 配额一起移动/共享；helper 保留各自限制。 |
| GPU 纹理失败与后台占用 | 切换离开释放纹理，保留受配额管理的 framebuffer；激活重建。上传失败停止该会话、释放像素与纹理、提交 helper 回收；资源错误不自动重试。 |
| 终端输入积压 | Local/Telnet/Serial 共用 8 MiB / 1024 条 mailbox，包括正在写入的命令和协议控制数据。整次接收/拒绝，满载返回 WouldBlock；独立停止标记清空待写数据。SSH 写入模型保持原有实现。 |
| 传输背压 | ZMODEM、X/YMODEM、TRZSZ 后台发送循环可取消地等待容量；发送失败覆盖成功状态。Telnet 按 IAC 转义后的大小识别永久超限帧。取消报文做一次受同一容量限制的非阻塞尝试。 |
| 线程/Windows 进程操作失败 | 连接 reader/writer/supervisor 使用可失败的线程启动；各阶段失败逆序回滚。进程句柄共享、查询/等待/终止/锁失败返回错误；重复终止已退出进程成功。 |
| 标签页关闭阻塞 | close_detached 先确认队列容量再移除唯一会话所有权；固定两个回收线程、64 条等待任务；提交失败保留原会话。停止输入、取消阻塞写、终止子进程、释放 master、等待 reader/writer 均在回收任务执行。 |
| helper 无响应 | IPC 9→10；后台 supervisor 每 2 秒 Ping，匹配序号 Pong；握手/响应 10 秒超时，单次 fatal Timeout 后回收；退出宽限 750 ms 后终止。队列背压暂停计时，长调度间隔重新探测。 |

1 GiB 是上述应用像素所有权的配额，不是整个进程 RSS 上限；渲染器、协议控制对象和独立 helper 的内存另计。诊断只包含资源状态和错误类别，不记录终端内容或凭据。

配置、凭据、redb、备份和同步格式没有更改。应用与两个远程桌面 helper 必须同批更新；旧 helper 明确拒绝版本不匹配。现有打包清单 HELPER_BINS 已包含两个 helper，无需修改。

## 依赖补丁

补丁均已分别提交并推送到各 fork 的 nyaterm 分支，原因与验证记录写入各分支 NYATERM.md。主仓库 Cargo.toml/Cargo.lock 锁定：

- Zed / GPUI：`9e2fabb8dac907911852ca8b9f67ab74fdd8c6e4`，动态纹理 staging、平台 atlas 分配错误传播。
- portable-pty：`1ececcf9a10437f2fe04d0756735cd0847219a26`，Windows 进程错误、可取消 writer、初始化错误与幂等清理；后续文档提交记录 prepared ConPTY 包验证。
- gpui-kit：`a75cb2b522415d697a5856ff872b9351910b500d`，对齐同一 GPUI revision，避免类型身份分裂。

未修改 temp/vendor 或 Cargo 缓存中的第三方源码。

## 验证命令与场景

依赖 fork（Windows 实际运行）：

- `cargo test -p gpui --features test-support --lib dynamic_texture --offline`：4 项通过，紧密/填充行与校验。
- `cargo test -p gpui_wgpu --lib wgpu_atlas::tests --offline`：6 项通过，包括真实设备的无效分配失败后继续正常分配。
- portable-pty `cargo test --offline`：16 项单元测试、1 项 doctest 通过，包括 4 MiB 满管道写取消、受限句柄错误、锁失败、真实子进程终止及重复终止。
- 设置 `NYATERM_CONPTY_TEST_DLL` 为已准备的 x64 conpty.dll 后，`cargo test --offline --test conpty_backend -- --ignored`：1 项通过，bundled 选择、DLL/host 回退和关闭后 active-count 清理。

应用自动回归覆盖：配额耗尽和所有权释放，drain/reset/关闭回收，writer 的字节/条目边界、顺序、整次拒绝、可取消重试成功/取消，Telnet 协商/自动登录/逐字节/keepalive、Serial flush，Windows 本地满管道标签页异步关闭和后续会话创建，线程启动逐阶段失败回滚，两套真实 helper 的版本拒绝/Ping-Pong/普通断开/崩溃与挂起回收，以及 supervisor 握手/运行超时、错误序号、背压和休眠宽限。

新增 GPUI 回归覆盖切换后的纹理释放与 framebuffer 保留/重建，以及纹理上传失败对目标会话的隔离。

最终 workspace 回归：

| 命令 | 结果 |
| --- | --- |
| `cargo check --workspace` | 通过。 |
| `cargo test --workspace` | 45 组测试/文档测试结果，3827 项通过，0 项失败，18 项按仓库原有标记跳过。 |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo clippy --workspace --all-targets` | 通过，零警告。 |

同批调试构建命令：`cargo build --locked -p nyaterm-app -p nyaterm-rdp-helper -p nyaterm-vnc-helper`。已通过；`target/debug/nyaterm.exe`、`target/debug/nyaterm-rdp-helper.exe` 和 `target/debug/nyaterm-vnc-helper.exe` 已在同一目标目录生成。未运行安装或启动现有用户会话。

## 尚未验证的验收项

- 没有可用的真实 RDP/VNC 测试服务器，未进行长时间多会话/高分辨率服务器压力测试；helper 静止画面的测试使用无服务器的控制回路。
- 没有实际耗尽主机 RAM/显存；使用小额配额、非法 GPU extent、受限句柄和线程故障注入验证错误分支。
- 没有在 macOS / Linux 运行 Metal、Unix PTY 取消和其他平台后端；源码检查与 Windows 测试不能替代这些平台验收。
- 自动测试模拟 supervisor 长调度间隔，没有让实际 Windows 机器休眠/唤醒。
- 正式 release 安装包/签名和真实服务器验收需在发布流程进行；本轮构建结果单独记录，不把调试构建当作正式发行包。


完整验证日志保存在本地忽略目录 `target/stability-validation/`；这些日志不提交。应用源码及锁文件保留为主仓库工作区改动，依赖 fork 补丁已推送。
