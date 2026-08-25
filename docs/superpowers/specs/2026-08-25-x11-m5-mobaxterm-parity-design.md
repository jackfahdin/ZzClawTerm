# X11 体验对齐 MobaXterm（M5）设计

日期：2026-08-25
状态：已实现（M5a/M5b 自动化部分全部核销，2026-08-25）；人工验收（§八）挂起待用户时间
上游：M4b《2026-08-24-x11-m4b-zzxsrv-embed-design.md》（嵌入模式实现，本规格将其降级为实验选项）

## 一、背景与目标

M5 原计划为"multiwindow 分窗嵌入"（把远端 X 应用的每个顶层窗口嵌入 ZzClawTerm 分屏窗格）。2026-08-25 头脑风暴中用户裁决：**独立窗口形态已满足需求，分窗嵌入取消**，M5 重定义为——**X11 体验完全对齐 MobaXterm**。

MobaXterm 行为基准（官方文档核实，mobaxterm.mobatek.net/documentation.html）：

1. X server 随应用启动拉起，全局单例，所有会话共享
2. SSH 会话 X11 转发（ForwardX11Trusted）默认开启，用户零配置
3. 远端 X 应用以独立 Windows 窗口出现（multiwindow），剪贴板自动同步
4. 关闭会话不影响 X server 存续

2026-08-25 实机插曲：用户手动双击 ZzXsrv.exe 进入裸 rootful 模式（无窗口管理器），gitk 按远端 `~/.config/git/gitk` 记忆几何位置恢复时超出屏幕被裁切。暴露手动启动默认模式体验差，纳入本里程碑处理。

现状差距：

| # | MobaXterm | ZzClawTerm 现状 |
| - | --------- | --------------- |
| 1 | 全局单例，随应用启动 | 每会话各起一个 ZzXsrv（`ZzSshTransport.cpp:280/:314`） |
| 2 | X11 转发默认开 | 会话默认关（`ZzSessionProfile.h:45`） |
| 3 | 独立 Win32 窗口为主 | 默认嵌入模式（`ZzSessionProfile.h:46` x11EmbedMode=true） |
| 4 | 关会话不杀 server | 关标签页连带杀 server |
| 5 | 手动启动即 multiwindow | 手动双击 = 裸 rootful |

## 二、范围

### 2.1 M5a（主仓 ZzClawTerm）

- ZzXServerManager 所有权上移为应用级共享单例，随应用启动拉起
- 全局开关"启用 X server"（默认开），允许用户随时关闭/重启 server
- 会话默认值翻转：x11Forwarding 默认 true、x11EmbedMode 默认 false
- 关会话不再杀共享 server；退出应用才杀

### 2.2 M5b（ZzXsrv 仓）

- 安装包快捷方式 target 追加 `-multiwindow`（手动启动进入受管窗口模式）
- `winvalargs.c` 补 `-parent` 与 `-multiwindow` 互斥校验
- 重出 release 资产，主仓下载常量（URL/大小/SHA256）同步升级

### 2.3 不包含

- multiwindow 分窗嵌入（用户裁决取消；调研结论存档于会话记录，侵入点清单可复用）
- X 通道中断后的自动热恢复（YAGNI：崩溃只通知，重拉走 lazy 路径）
- OpenGL/GLX（维持 M4b 裁剪结论）
- ZzXsrv.exe 本体无参数缺省行为改动（只改快捷方式；exe 缺省改 multiwindow 的评估留后续版本）
- ZzPureToolsPro UI 冻结中，本里程碑不触碰

## 三、决策记录（用户已逐条批准）

| # | 决策点 | 结论 | 备选（未选） |
| - | ------ | ---- | ------------ |
| 1 | X server 生命周期 | 共享单例 + 应用启动即拉起 + 允许关闭 | 首次需要时拉起；不改架构只翻转默认值 |
| 2 | X11 转发默认值 | 会话默认开启（对齐 MobaXterm） | 保持默认关 |
| 3 | M4b 嵌入模式去留 | 保留为实验选项，默认 false | 移除嵌入代码 |
| 4 | 手动启动默认模式 | 安装包快捷方式加 `-multiwindow` | 改 exe 缺省；保持现状 |
| 5 | 关会话杀不杀 server | 不杀（随共享单例自然成立） | — |

## 四、架构设计

### 4.1 共享单例所有权

```
ZzMainWindow（应用级）
 └── ZzXServerManager（共享单例，随主窗口构造创建）
      ├── ZzXsrv.exe :N -multiwindow -clipboard -listen tcp -auth <app.xauth>
      ├── app 级 cookie（server 启动时 ZzXAuthority 生成一次，所有会话共用）
      └── display / localEndpoint（只读查询）

ZzSshTransport（每会话）
 └── 非嵌入会话：只读引用共享单例（display/cookie/localEndpoint），
     不再 new/拥有 ZzXServerManager，不调 start/stop
```

- cookie 共享安全性：xauth 文件 0600 仅本用户可读 + 仅监听 127.0.0.1，威胁模型与现状同级
- cookie 在每次 launchProcess 前生成并覆写 xauth 文件（含 lazy 重拉）；重拉前已断开的旧转发无需保持兼容
- Unix 路径语义不变：`start()` 的 Unix 分支复用系统 X server（不拉进程），共享单例同样适用

### 4.2 双路径共存与互斥

| 条件 | 路径 |
| ---- | ---- |
| 全局开关关 | 不拉起共享 server；会话不发起 X11 转发（静默跳过） |
| 开关开 + 会话 x11EmbedMode=false（默认） | 共享 multiwindow 单例 |
| 开关开 + 会话 x11EmbedMode=true（实验） | 该会话走 M4b 原路径：自带独立 server（`-parent` 嵌入），不占用共享实例 |

- 共享 server 与嵌入 server 的 display 号分配不冲突：`allocateDisplay()` 按端口探测，天然错开
- 嵌入会话关闭仍杀其自带 server（M4b 行为不变）；共享实例不受任何会话生命周期影响

### 4.3 生命周期状态机

- **应用启动**：全局开关开 → allocateDisplay + 生成 cookie/写 xauth + launchProcess（multiwindow 参数组）；开关关 → 不拉起
- **会话连接**：x11Forwarding=true 且开关开 → transport 从共享单例取 display/cookie/localEndpoint → `requestX11Forwarding`；共享 server 未运行（崩溃未恢复）→ 尝试重拉一次，失败则通知并跳过转发
- **会话关闭**：不触碰共享 server
- **全局开关切换**：关→开：立即拉起；开→关：stop() 并阻止后续转发
- **应用退出**：随组合根（ZzAppShell）析构，QProcess 析构直接终止 ZzXsrv 子进程（实现按此落地；不走 stop() 的 terminate→3s→kill 异步收尾——应用已在退出路径上，异步收尾来不及完成）
- **server 崩溃**：发通知（复用 crashed 信号链）；不做自动热恢复；下个 X11 会话连接或开关重拨时 lazy 重拉

## 五、主仓改动清单（M5a）

| 文件 | 改动 |
| ---- | ---- |
| `src/session/ZzSessionProfile.h:45-46` | x11Forwarding 默认 true、x11EmbedMode 默认 false；注释去"实验性"措辞（转发转正，嵌入标实验） |
| `src/session/ZzSessionProfile.cpp` | 序列化键不变；旧 JSON 无键时取新默认值（行为变化见 §九） |
| `src/settings/ZzAppSettings.h/.cpp` | 新增 `x11ServerEnabled`（默认 true） |
| `src/settings/ZzSettingsPage.h/.cpp` | 新增"启用 X server（启动时自动运行）"复选框，切换即生效（拉起/停止） |
| `src/x11/ZzX11Service.h/.cpp`（新增） | 应用级共享门面：承载单例语义、cookie 生成、懒重拉与开关联动（原拟改动 ZzXServerManager 注释/restart() 语义，实现时由本类替代，manager 保持原样） |
| `src/transport/ZzSshTransport.h/.cpp` | 非嵌入路径移除 m_x11Manager 拥有权，改为注入的共享实例引用（仅查询 display/cookie/localEndpoint 及触发 ensureRunning 重拉，不负责 start/stop 生命周期）；嵌入路径保留现状 |
| `src/tab/ZzTabManager.h/.cpp` | 构造/装配处注入共享实例并透传给 transport；endpoint 映射逻辑适配 |
| 主窗口装配处（ZzAppShell） | 创建并持有共享单例（ZzX11Service）；应用退出随析构终止子进程 |
| `src/panel/ZzSessionEditDialog.cpp` | "嵌入标签页"选项标注（实验）；默认值随 profile 翻转 |

## 六、ZzXsrv 仓改动（M5b）

| 位置 | 改动 |
| ---- | ---- |
| `installer/vcxsrv.nsi`（快捷方式段） | 开始菜单/桌面快捷方式 target 追加 `-multiwindow` |
| `xorg-server/hw/xwin/winvalargs.c:78-165` | 补 `-parent` ↔ `-multiwindow` 互斥校验（调研发现同给时 M4b 分支会把隐藏 screen 窗口 SetParent 进 Qt 容器，未定义行为） |
| release 流程 | 新 tag + 四资产；主仓 `ZzXServerDownloader` 常量（URL/字节数/SHA256）同步升级，沿用 zz2 的交叉核验流程 |

## 七、错误处理

- 共享 server 启动失败（FailedToStart）：通知栏提示"X server 启动失败"，X11 会话静默跳过转发，不阻断 SSH
- 运行中崩溃：通知提示；已有会话的 X 通道中断如实上报（现有 x11ForwardingFailed 信号链）；不做自动热恢复
- 全局开关关闭时连接 X11 会话：静默不转发（不打断用户），状态栏/日志留痕
- 嵌入路径错误处理维持 M4b 现状不变

## 八、测试策略

- 新增单测：共享单例语义（注入引用不拥有、stop 仅应用级触发、开关切换拉起/停止）、profile 默认值翻转序列化往返、旧 JSON 缺键取新默认
- 主仓回归：44 项基线全绿 + 新增用例；perf 记录规矩不变（跑完恢复 `tests/perf/records/`，按当天日期前缀清理新文件）
- ZzXsrv 仓：CI 双模式冒烟保留；nsi 改动经 release 资产冒烟覆盖
- 人工验收（用户执行，当前挂起）：清单改为以 multiwindow 独立窗口为主路径——①SSH 连接默认即带 X11 转发，xclock 弹独立 Win32 窗口带标题栏可拖动 ②多会话共享同一 server（仅一个 ZzXsrv.exe/托盘图标） ③关会话 server 不退出 ④全局开关关闭后 server 停止、新会话不转发 ⑤手动从开始菜单快捷方式启动进入 multiwindow

## 九、兼容与行为变化

- **旧会话 JSON 行为变化**：`x11Forwarding`/`x11EmbedMode` 采用"缺键取代码默认"模式（`ZzSessionProfile.cpp:100-101`），升级后旧会话将自动变为"转发开 + 独立窗口"。这是对齐 MobaXterm 的目标行为，在 release notes 显式声明；不需要的用户经全局开关一键关闭
- 已显式存过这两个键的会话：尊重存档值，不被默认值翻转影响
- 凭据/端口转发等其他 profile 字段不受影响

## 十、风险

| 风险 | 概率 | 影响 | 应对 |
| ---- | ---- | ---- | ---- |
| 旧会话 X11 默认开引发意外转发流量 | 中 | 低 | release notes 声明 + 全局开关一键关 |
| 共享 cookie 被同机恶意进程利用 | 低 | 中 | 0600 + 127.0.0.1，与现状同级；文档声明威胁模型 |
| 双路径（共享/嵌入）并存复杂度 | 中 | 中 | §4.2 互斥规则 + 单测覆盖；嵌入标实验降低维护承诺 |
| 共享单例改造触碰 transport 装配链引入回归 | 中 | 中 | 44 项回归基线 + 新增单测；小步提交 |
| nsi 快捷方式参数影响静默安装/升级流程 | 低 | 低 | CI release 冒烟覆盖 |

## 十一、完成定义

1. ✅ M5a/M5b 全部自动化测试绿（主仓 45 项全绿；ZzXsrv CI 冒烟双模式，zz3 release 构建 32815874367 成功）
2. ✅ ZzXsrv 新 release 资产发布（zz-21.1.16.1-3 四资产），主仓下载常量升级并交叉核验 sha256（bd2e7b3e…fc4f 实测与随附 .sha256 一致）
3. ✅ 规格条目核销，代码审查通过（逐任务审查 + 最终宽范围审查 1 Critical/1 Important 修复波，定向复审 2/2 ADDRESSED）
4. ⏸ 人工验收清单（§八）更新完毕——执行挂起待用户时间（清单同时覆盖 multiwindow 主路径与嵌入实验路径；总开关对全部会话生效，含嵌入）
