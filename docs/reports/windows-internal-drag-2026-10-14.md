# Windows 内部拖拽修复与验证

## 行为与根因

Windows 跨窗口路由曾把鼠标下任意 HWND 的 `GWLP_USERDATA` 当成
`Weak<WindowsWindowInner>`。VS Code 等外部窗口可能在该槽位存放非零数据，
读取为 Rust 对象会导致非法访问。桌面窗口数据槽为空，因此表现不同。

改为 UI 线程内的 HWND 弱引用登记表，创建成功登记，`WM_NCDESTROY` 开始时
注销；查询升级为 `Rc` 后才进入回调。跨窗口目标还必须属于同一 GPUI 平台
实例。外部窗口回退到源窗口处理移动、松开，保留现有坐标及 DPI 转换。

| 对象 | 外部松开 | Esc / 意外捕获丢失 |
| --- | --- | --- |
| 面板、快捷命令、连接等内部组件 | 不提交移动，保持布局或顺序 | 取消拖拽及高亮 |
| 会话标签 | 分离一次；有效跨窗口放置优先 | 取消协调器，不分离 |
| 声明原生导出的文件 | 原生文件拖放 | 原生流程管理取消 |

离开窗口不取消，允许拖回源窗口或另一个 NyaTerm 窗口。保留已有拖拽视觉。

## 取消生命周期

GPUI 新增 `PointerCancelEvent` 及 `PlatformInput::PointerCancelled`。取消先清除
拖拽、预览、指针捕获、待点击状态，再通知应用，不生成鼠标松开或放置事件。
Windows 对正常松开、原生文件交接和显式取消区分捕获释放原因；意外取消通过
带窗口校验值和平台共享手势代次的消息延后投递。重复消息只消费一次，旧消息
不能取消同窗口或另一窗口的新手势。

NyaTerm 在内部拖拽活跃时消费 Esc；其余情况保留原有按键处理。统一清除标签
协调器、快捷命令、安全面板、连接、终端与传输目标高亮，结束选择、滚动条及
尺寸调整。尺寸保留最后有效值，终端选区保留有效范围且取消时不自动复制；
已发送给终端的鼠标按下补发协议释放，避免远端保持按下状态。

## Fork 集成

- Zed 基线：`9d03436aa8185c972e957f36e95044ac524b2cf4`。
- 安全窗口查询：`c104958f820e0dc424b6c7ae5fae8dcd4c721306`。
- 取消生命周期及 CI：`c776ed2a4bd732e977a1249807616e6c18207afe`。
- 取消优先于已排队松开：`09a27ca822b2a6906f90e55515e104cc04176f19`（最终 GPUI 固定修订）。
- GPUI Kit 依赖对齐：`93727f1311df9b9d3924263ac6c4988fd7474c33`，基于已发布
  `d613d7bf07baf02bc32864117640e78aa0e2f077`，保留通知层修复。
- 两个 fork 的 `nyaterm` 分支已推送，`NYATERM.md` 已更新，Zed README 人工
  审阅标记保留。NyaTerm 的固定修订和 Cargo.lock 同步更新。
- Cargo.lock 仅改变上述 fork 来源修订；不涉及配置、凭据、持久化格式迁移。
  未修改 `temp/vendor`。

## 自动验证（Windows）

| 检查 | 结果 |
| --- | --- |
| GPUI `cargo test -p gpui drag --lib` | 9 通过 |
| Windows `cargo test -p gpui_windows --features test-support --lib` | 43 通过；1 个子进程夹具由父测试调用 |
| GPUI Windows / platform check、GPUI scoped fmt | 通过 |
| GPUI Kit check、Bun GPUI pin coherence | 通过 |
| NyaTerm app check | 通过 |
| NyaTerm pointer_cancel | 2 通过 |
| NyaTerm tab_drag | 13 通过 |
| NyaTerm workspace check | 通过 |
| NyaTerm quick-command / activity-entry 回归 | 各 1 项通过 |
| NyaTerm `cargo test --workspace`（最终固定修订） | 3704 通过，18 按既有配置忽略 |
| NyaTerm `cargo fmt --all -- --check` | 通过 |
| NyaTerm `cargo clippy --workspace --all-targets` | 通过 |

最终修订的本地 workspace check、test、fmt、clippy 均通过。18 个忽略项沿用
仓库既有测试配置。NyaTerm 应用改动保留在工作区，尚未创建主仓库提交。

最后检查 fork CI 时，最终 Zed 修订的 Linux 及 macOS 作业已通过，Windows
作业仍在运行；最终 Kit 修订的 CI 仍在运行。对应运行：
[Zed 38589632807](https://github.com/nyakang/zed/actions/runs/38589632807)、
[GPUI Kit 38589976838](https://github.com/nyakang/gpui-kit/actions/runs/38589976838)。
此前生命周期修订的 Zed 三平台 CI 和 Kit CI 已通过；最终 Windows 原生套件
43 项在本机通过，不把未完成的远端作业记为通过。

原生测试覆盖同进程外部 HWND 的无效非零数据槽、子进程 HWND、过期弱引用、
注销及复用的登记键、跨线程查询；捕获状态测试覆盖一次取消、正常释放、
原生文件交接、同窗口及跨窗口新手势代次。GPUI 事件测试验证取消后不能产生
点击、放置或恢复已取消的待拖拽状态。应用测试覆盖标签分离一次、跨窗口有效
放置优先、关闭源工作区及取消不分离，以及混合 DPI 坐标的纯逻辑。

## 实际桌面验收

尝试使用 Windows computer-use 的 `sky.list_windows()`，工具返回
`Computer Use native pipe is unavailable`（系统找不到指定文件，os error 2）。
因此未执行真实鼠标交互；自动原生窗口及状态测试不代替这些验收。

| 场景 | 待人工执行的断言 | 状态 |
| --- | --- | --- |
| 下方为 VS Code、浏览器、桌面 | 内部组件拖出并松开无卡死、崩溃或顺序改变 | 未验证 |
| 拖出后拖回 | 保持拖拽，回到有效目标可放置一次 | 未验证 |
| 跨 NyaTerm 窗口 | 标签成功迁移，不额外新建窗口 | 未验证 |
| 混合 DPI、负坐标显示器 | 拖拽路由及分离窗口位置正确 | 未验证 |
| Esc、其他窗口抢捕获、关闭源窗口 | 高亮及拖拽清除，不分离；下一手势正常 | 未验证 |
| 本地文件及远程文件导出 | 仍能原生导出；取消不遗留半完成交互 | 未验证 |

Linux/macOS 编译和原生 promise 回归通过 fork CI 跟踪，未进行对应桌面交互。
