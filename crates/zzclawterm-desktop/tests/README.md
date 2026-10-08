# Shell Free Type 验证

编辑模型位于 `zzclawterm-core::terminal::editing`，真实 cell 映射和 OSC 输入锚点位于
`zzclawterm-terminal`，按会话保存的状态与 GPUI 协调位于 desktop 的
`features/terminal/editing_state.rs` 和 `editing_runtime.rs`。

编辑只在输入内容与实际光标完整匹配后启用。无 OSC 的回退要求光标所在的单行或
软换行区域中输入唯一匹配。移动、删除和覆盖写入成功后进入 Pending，完整回显
才确认；部分重绘、重复文本、复杂字素和过期选区均不能授权合成编辑。

## 自动验证

```powershell
cargo test -p zzclawterm-core -p zzclawterm-terminal -p zzclawterm-terminal-gpui
cargo test -p zzclawterm-desktop --lib features::terminal::editing
cargo test -p zzclawterm-desktop --lib replacement_paste_frames_only_inserted_encoded_text
cargo check -p zzclawterm-app
```

desktop 状态测试覆盖建议关闭、低延迟和外层命令抑制下的无 OSC 回退、MouseUp
保留选区、失败写入、部分回显、旧快照禁用、超时、最新点击、OSC 边界和密码/鼠标报告/广播
门禁。IME 测试使用 GPUI 输入处理器及真实本地 TCP 连接，检查预编辑不修改输入、
物理文字键交由提交入口、覆盖只发送一次。粘贴载荷测试检查编码后的正文被包装，
移动与删除序列在 bracketed-paste 外。

## 本轮结果（2026-10-04）

为保留主工作区正在并行修改的 AI、Serial、transfers 和 VNC 代码，本次功能补丁
另以 `e77524b39` 为基线应用到隔离工作树验证，未加入那些并行功能的补丁。

* `cargo check --workspace`、`cargo test --workspace`、
  `cargo fmt --all -- --check` 和 `cargo clippy --workspace --all-targets` 均通过。
* desktop 全量测试为 1707 项通过、7 项忽略；冻结后的编辑状态测试 10 项通过。
* terminal 178 项通过；terminal-gpui 145 项通过、1 项忽略。
* Windows PowerShell ConPTY 测试显式运行并通过。

这些结果覆盖本次功能及基线代码。主工作区验证期间出现过并行 AI proxy 测试失败、
VNC 新事件匹配分支未覆盖及其他功能文件的格式差异；上述结果不代表那些并行改动
已完成验证。

## Windows shell 验证

```powershell
cargo test -p zzclawterm-desktop --test shell_free_type_windows -- --ignored
```

该测试启动隔离的 Windows PowerShell + PSReadLine，经真实 ConPTY 传输验证光标
移动、删除、文字覆盖、粘贴覆盖、软换行以及跨软换行向左移动。测试不执行输入的
命令，不保存历史，不打印终端正文，并在退出时关闭会话。

本轮已运行并通过该 ConPTY 测试。原生窗口的实际鼠标点击/拖选、操作系统 IME、
SSH bash/zsh/fish 及 Docker shell 尚未实测；GPUI/TCP 测试不能替代这些人工场景。

人工验证时，分别在有/无 OSC 的 shell 输入尾空格、中文宽字符和软换行命令，
依次检查点击定位、拖选及双/三击后的删除、左右键折叠、文字/IME/粘贴覆盖。
关闭命令建议并启用低延迟后重复；在 Docker/SSH 嵌套 shell 确认外层 command-running
不阻止唯一回显匹配。在重复文本、历史滚动、密码、alternate screen、鼠标报告和
广播模式检查操作回退为普通终端行为。
