# 项目设计观察与独立维护后的待评估事项

## 当前决策

**状态：暂缓，待独立维护后重新评估。**

当前项目仍持续跟随 NyaTerm 上游。现阶段优先保持与上游的结构接近，降低后续同步、冲突处理和回归验证成本。本次只记录设计观察，不启动相关重构或功能调整。

本文不是实施计划，也不代表已经决定采用其中的方案。即使某项局部调整看起来简单，也应计入持续维护上游差异的成本。等项目明确转为独立维护后，再基于届时的代码、需求和使用反馈重新判断；上游可能已经解决问题，也可能改变了相关设计。

本文中的潜在风险不等于已确认缺陷。如果后续出现可复现且影响实际使用的问题，应单独评估必要修复，优先查看上游是否已有解决方案，避免将修复扩大为架构重构。

## 观察基线与范围

| 项目 | 记录 |
| --- | --- |
| 观察日期 | 2026-10-21 |
| 本地分支 | `master` |
| 本地提交 | `cab1c0f2e3d8c6d2be09710a5e4e60b45171fd21` |
| Workspace 版本 | `0.1.4` |
| 检查方式 | 源码、架构文档和功能文档抽查 |
| 已执行检查 | `python3 scripts/ci/check_architecture.py`，通过 |
| 未执行验证 | 完整 Cargo 构建与测试、桌面交互测试、故障注入和性能基准 |

当时 workspace 包含 16 个 crate，`crates/*/src/**/*.rs` 约 49.3 万行，其中 desktop 约 31.4 万行、占约 64%。统计包含内联测试，不包含独立 tests 目录。这些数字仅用于定位阅读重点，不能证明模块存在设计问题或运行时性能问题。

以下路径和符号对应上述基线；后续同步可能改变位置或实现。重新评估时应先核对事实，不能直接照本文修改。

## 值得保留的架构边界

- 终端状态机、控制序列解析与 GPUI 绘制分离。
- core、transport、store 和 remote-desktop 保持独立于 GPUI。
- RDP/VNC 协议解码器保持 helper 进程隔离，VNC 服务端策略仍由 helper 强制执行。
- 数据库串行执行与普通阻塞任务池分别管理生命周期和调度，保留各自语义。
- 配置、凭据、已知主机、备份和同步数据的兼容性读取及加密契约保持稳定。
- 面板不可变快照和明确刷新边界具有减少无关重绘的设计依据；不要仅为统一形式而移除。
- 每份可变状态保持一个权威 owner，避免为了拆文件而引入第二份可修改副本。

已有的 [Desktop responsibility refactor](../responsibility-refactor.md) 记录了部分职责划分和保留大组件的理由，重新评估时应一并阅读。

## 代码设计候选事项

### C1：设置保存定义与分发去重

**观察：已确认存在重复定义和分发。**

- [general_interaction.rs](../../crates/zzclawterm-desktop/src/features/settings/settings_runtime/general_interaction.rs) 中的 `SettingsSaveKind` 与 [settings/state.rs](../../crates/zzclawterm-desktop/src/features/settings/state.rs) 中的 `SettingsPersistenceDomain` 都有相同的 13 个变体，另有逐项转换。
- 立即保存的 `submit_settings_save` 与 [draft.rs](../../crates/zzclawterm-desktop/src/features/settings/settings_runtime/draft.rs) 中的草稿应用分别维护领域到 store 保存方法的映射。

未来可以评估：共用一个领域枚举、一份领域列表和一个窄的保存适配入口。立即保存和草稿提交继续保留各自的生命周期逻辑。

预期收益是减少新增设置域时需要同步修改的位置。边界是保留每域字段写入、generation 合并与过期结果过滤、草稿冲突检测、秘密掩码及工作区布局保护；不能改成整份设置覆盖，也不需要改变持久化格式。

重新验证重点：立即保存与草稿保存的一致性、保存失败、连续修改、跨窗口冲突、旧数据加载。

### C2：输入控件业务事件回到所属功能

**观察：已确认公共输入模块承担大量业务路由。**

[text_inputs.rs](../../crates/zzclawterm-desktop/src/features/text_inputs.rs) 的 `on_text_input_changed` 按字符串前缀分发到设置、AI、SSH 凭据、传输、主题等功能，并包含部分面板刷新策略。

未来可以评估：创建输入时绑定功能回调或类型化事件，让注册器集中管理控件、订阅、焦点和秘密显隐，所属功能处理字段含义、校验及刷新。先迁移一个小范围，保留旧入口逐步过渡。

预期收益是减少新增字段对全局路由的修改，以及字符串标识不匹配带来的静默失效。需要保留控件稳定 ID、生命周期和现有状态归属，避免在回调绑定中引入循环引用或新的可变镜像。

重新验证重点：IME、Changed 与 Submitted 的区别、reset/forget、密码显隐、面板关闭后的订阅清理、远端面板快照刷新。

### C3：统一单次后台任务的完成反馈

**观察：已确认回执代码重复；异常后 pending 未结束属于待复现风险。**

[stats.rs](../../crates/zzclawterm-desktop/src/features/remote/remote_runtime/stats.rs) 与 [gpu.rs](../../crates/zzclawterm-desktop/src/features/remote/remote_runtime/gpu.rs) 分别包装成功回执、入队失败、job ID 和 session ID。[blocking_jobs.rs](../../crates/zzclawterm-desktop/src/blocking_jobs.rs) 已有 `submit_task`，可以返回正常结果、取消及 panic 失败。

静态控制流显示：部分 `submit_detached` 任务依靠业务闭包主动发送完成事件；如果闭包在发送前 panic，调度器捕获异常后，其 outcome 不会自动转换成该功能的完成事件。[job_state.rs](../../crates/zzclawterm-desktop/src/features/remote/job_state.rs) 中的 pending 则依赖匹配的完成事件清除。因此存在面板持续显示运行中的可能性，尚未故障注入复现，不能视为已确认的用户故障。

未来可以评估：将需要界面反馈的单次任务迁到已有 `submit_task`，统一等待结果，再转换为所属功能的类型化事件。保留有界队列、过期结果过滤和长寿命专用 worker。

重新验证重点：成功、入队拒绝、panic、取消、乱序结果、切换会话与关闭窗口。若风险已被上游解决，则删除或更新本候选项。

### C4：集中设置编辑会话的职责

**观察：已确认事务相关状态分散；未据此认定存在重复可变 owner 缺陷。**

草稿基线位于 [shell/state.rs](../../crates/zzclawterm-desktop/src/features/shell/state.rs) 的 `ShellSettingsNavigationState`，修改域与持久化状态位于 `SettingsFeatureState`，跨功能快照类型位于 [app_state/types.rs](../../crates/zzclawterm-desktop/src/features/app_state/types.rs)。

未来可以评估：由 settings 内的 `SettingsEditSession` 集中管理基线、修改域和提交状态；shell 管理当前标签、窗口和布局。AI、同步、翻译的运行状态继续由原功能持有，应用层保留跨功能采集和应用的适配职责。

预期收益是让取消、应用、关闭确认和冲突处理更容易理解。此项涉及多个功能和秘密草稿，合并影响面较大，应在独立维护后谨慎评估。

重新验证重点：取消恢复、关闭确认、多窗口 revision 冲突、云同步及托盘操作与设置草稿的交互。

### C5：共享 RDP/VNC 的帧队列机制

**观察：已确认应用侧队列存在相似实现，是否值得提取仍需评估。**

[session.rs](../../crates/zzclawterm-remote-desktop/src/session.rs) 与 [vnc.rs](../../crates/zzclawterm-remote-desktop/src/vnc.rs) 分别维护帧容量与字节限制、epoch 过滤、等待、排空和关闭逻辑。

未来可以评估：在 remote-desktop 内提取小范围的共用队列机制，协议控制事件继续分别适配。收益是修复队列边界时减少两份实现偏离的可能。

需要保留帧 Reset 顺序、背压与 heartbeat 的关系、游标合并、VNC trust gate 和协议差异。解码器、两个 helper 进程及其安全策略保持现有隔离。

重新验证重点：队列容量边界、过期帧、Reset 顺序、关闭时唤醒阻塞发送者、两种协议的 helper 生命周期。

## 功能设计候选事项

以下为基于代码和文档提出的交互方向，尚未经过用户研究或实际桌面操作验证。

| 编号 | 候选方向 | 观察与未来评估方式 |
| --- | --- | --- |
| F1 | 扩展统一命令面板 | 当前 [quick_switch_items](../../crates/zzclawterm-desktop/src/features/shell/quick_switch_runtime.rs) 检索会话、连接及连接中的条目。可评估加入设置、面板、分屏和重连动作，复用 [SHORTCUT_REGISTRY](../../crates/zzclawterm-desktop/src/shortcuts/mod.rs)，显示快捷键及不可用原因。验证是否减少寻找菜单的步骤，并保持终端快捷键和焦点行为。 |
| F2 | 提供精简工作区预设 | 可评估“终端 / 运维 / 远程桌面”布局预设，允许用户调整。只对新用户或主动选择预设时应用，保留已有布局。用首次连接、传输和排障任务验证是否降低学习成本。 |
| F3 | 将主机监控组织为一组工具 | 资源、进程、Docker、GPU、NPU 当前有多个独立入口。可评估统一“主机工具”入口与内部切换，同时允许常用工具独立固定；GPU/NPU 当前已有默认关闭行为。验证是否减少入口负担，并保留多面板同时查看的能力。 |
| F4 | 设置项搜索与生效说明 | 可评估搜索直接定位字段，并明确“立即生效 / 下次连接生效 / 需重启”等语义。逐项核对实际行为，避免仅添加文案而与实现不符。 |

功能背景见 [工作区与布局](../../docs-site/docs/guide/layout-and-workspace.md)。这些方向不要求立即增加新框架或改变配置结构。

## 独立维护后的重新评估流程

触发条件是项目明确转为独立维护，并决定重新开展设计评估。本文没有预定实施日期。

1. 记录新的代码基线和最终同步的上游版本，重新阅读相关模块与既有设计说明。
2. 逐项标记“仍存在 / 上游已解决 / 需求已变化 / 不值得修改”，移除失效建议。
3. 对 C3 等风险先做最小复现；对 F1–F4 先走查实际任务流程，再判断是否需要设计变化。
4. 根据实际维护成本、用户收益、改动范围和验证成本重新排序。原始候选顺序仅供参考：C1 → C3 → F1 → C2 → C4；C5 和其他交互方向单独评估。
5. 为被选中的单项形成新的设计和实施计划，明确兼容边界及验收条件，分开提交和验证。

若届时仍计划选择性吸收上游提交，也应继续评估冲突成本。独立维护本身不构成全面重构的理由。

## 本次记录的边界

本次仅新增这份延期评估文档。源码、依赖、持久化格式、功能入口和默认布局均未调整；没有创建实施任务或定时跟进。本文保留观察依据，供后续重新分析时使用。
