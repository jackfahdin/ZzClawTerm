# SFTP/传输性能调优（BDP 自适应）+ V0.2 小收尾（M6）设计

日期：2026-08-25
状态：已实现（M6a/M6b 自动化部分全部核销，2026-08-25）；人工验收（V0.2 清单）挂起待用户时间
上游：ZzSshCore SFTP 引擎（tst_ZzSftpPerf 回环门控）、V0.2 五项功能（commit 层已交付）

## 一、背景与目标

用户核心诉求：**上传下载速率不能慢，性能不达标不放行**（2026-08-25 明确）。

2026-08-25 调研结论（均有 file:line 证据，记录于会话）：

- ZzSshCore 的 SFTP 传输是异步状态机 + 上传真流水线 + 下载预取，回环吞吐 437/504 MB/s，略超系统 OpenSSH sftp（比值 1.13/1.05，`third_party/ZzSshCore/tests/perf/records/2026-08-25-zzsftp-085952.json`）
- **但门控全部是回环（127.0.0.1）口径**：默认 256KB 块 → 上传在途 ≈2MB（`ZzSftpEngine.cpp:8-13`，16×块夹 [1MB,8MB]）、下载预取 ≈4MB（libssh2 `sftp.c:1449-1453`，4×读缓冲上限 8MB）。RTT 100ms 链路理论天花板 ≈ 上传 20 MB/s、下载 40-80 MB/s——比回环低一个数量级，这正是"类似工具上传下载慢"的根源
- 块大小/流水线深度**生产不可配**：`ZzSftpSession::setTransferBlockSize`（`ZzSftpSession.h:51`）在主仓 src 零调用
- 主仓 `tests/perf/` 无 SFTP 吞吐用例（只有面板目录填充）

V0.2 五项功能（SFTP 面板/端口转发/分屏/密钥环/冷层）commit 层已交付，但：密钥环 Windows/macOS 未实机验证（仅 Linux libsecret 实测）；`docs/acceptance/` 无 V0.2 人工验收清单。

## 二、范围

### 2.1 包含

**M6a（ZzSshCore 仓）**

1. 任务 0 先决调研（产出结论记录，不写生产代码）：
   - OpenSSH 服务器在高 RTT **上传**方向通道窗口行为（是否自增长、稳定值）——决定上传天花板
   - libssh2 调大通道窗口的 API 与实际上限（下载方向 `libssh2_channel_open_ex` window_size / `LIBSSH2_CHANNEL_WINDOW_DEFAULT` / receive_window_adjust）
   - netem 注入对 OpenSSH 参照本身的影响（比值口径公平性）
2. RTT 测量：SFTP 会话初始化后、首个传输前，轻量往返计时（如一次小开销 sftp 请求），滑动平均；回环/局域网（RTT < 5ms）走现状默认，**行为零变化**
3. BDP 自适应调参（新纯逻辑组件 ZzSftpTuner）：RTT 采样 → 块大小/下载窗口/在途深度的档位或公式（具体公式由任务 0 结论定，规格不锁死）；ZzSftpEngine 应用结果——下载调 libssh2 通道窗口 + 预取上限放宽，上传放宽 staging 在途上限（对端窗口为硬顶）
4. netem CI 门控：docker 容器 `tc netem` 注入 25/50/100ms 三档 RTT，每档与系统 OpenSSH sftp 比值 **≥0.95**（力争 >1）；回环现有绝对阈值（50 MB/s）+ 5% 回归门控保留；perf 记录入库沿用现有机制

**M6b（主仓）**

5. `ZzAppSettings` 新增 `sftp/blockSize`（0=自动；手动区间 16KB-4MB，经已有的 `ZzSftpSession::setTransferBlockSize` 生效）
6. 设置页高级区："SFTP 块大小：自动/手动"（窗口/预取深度不暴露，随块大小公式走）
7. `docs/acceptance/v0.2-manual-acceptance.md`：V0.2 五项功能人工验收清单（含 SFTP 速率实测项、密钥环 Windows 实机验证项——与 X11 M5 验收清单一并由用户执行）

### 2.2 不包含

- 多通道并行分块传输（路线 B）：M6a 落地后若 netem 三档比值达标则不做；上传撞对端窗口天花板且不达标时单独立项
- staging 前缀出队 memmove、5ms 定时器泵、下载同步落盘的微优化（高 RTT 下网络是唯一瓶颈；回环已超 OpenSSH 参照，动它们只有回归风险）
- FTS5 搜索 UI（单独立项）
- 设置页会话级覆盖（v0.1 遗留，继续延后）
- 密钥环 Windows/macOS 的代码改动（只进验收清单；验证发现问题再立项修）
- ZzPureToolsPro UI（冻结中）

## 三、决策记录（用户已逐条批准）

| # | 决策点 | 结论 | 备选（未选） |
| - | ------ | ---- | ------------ |
| 1 | 里程碑边界 | 性能为主 + V0.2 小收尾；FTS 搜索 UI 单独立项 | 含 FTS UI 一起做；只做性能 |
| 2 | 性能达标口径 | netem 25/50/100ms 三档与 OpenSSH 比值 ≥0.95（CI 自动化）+ 回环 5% 回归门控 | 加绝对数值目标；真实服务器人肉实测 |
| 3 | 调优技术路线 | BDP 自适应（测 RTT 自动调参）+ 设置页手动覆盖 | 多通道并行分块；先 A 不够再 B |

## 四、架构设计

### 4.1 组件与数据流

```
ZzSshCore
 ├── ZzSftpTuner（新，纯逻辑无 QObject 依赖）
 │    输入：RTT 滑动平均（ms）
 │    输出：ZzSftpTuning { blockSize, channelWindow, maxInFlight }
 │    RTT < 5ms → 全默认值（与现状逐字节一致）
 ├── ZzSftpSession/ZzSshConnectionWorker
 │    SFTP init 后采样 RTT → tuner.compute() → 应用到 engine 与通道
 ├── ZzSftpEngine
 │    下载：libssh2 通道窗口按 tuning.channelWindow 打开/调整；读缓冲与
 │          预取深度上限随块大小公式放宽（现 8MB 硬顶需参数化）
 │    上传：staging 在途上限按 tuning.maxInFlight 放宽（现 16×block 夹
 │          [1MB,8MB]）；对端通告窗口是协议硬顶，不可超
 └── tests/perf + docker：netem 三档门控（tc qdisc add dev eth0 root netem delay Xms）

主仓
 ├── ZzAppSettings：sftp/blockSize（0=自动）
 ├── ZzSettingsPage：SFTP 块大小 combo（自动/64K/128K/256K/512K/1M/2M/4M）
 └── ZzSftpPanel → ZzSftpSessionOps → ZzSftpSession::setTransferBlockSize
     （手动值传递链；自动=不调用，走库内 BDP 自适应）
```

### 4.2 RTT 测量

- 采样点：SFTP 子系统初始化成功后、首次传输发起前；后续每个传输完成时更新滑动平均（权重最新 1/4）
- 探针：一次小开销 sftp 请求（如对远程目录 `libssh2_sftp_stat "."`）的墙钟往返
- 失败/异常（测量超时、服务端异常）→ 按回环默认处理并记日志，不影响传输功能
- 采样不阻塞首传超过 2 秒（超时按默认处理）

### 4.3 手动覆盖语义

- `sftp/blockSize = 0`（默认）：库内 BDP 自适应
- 非 0：跳过自适应的块大小维度（窗口/在途公式仍按手动块大小推导），夹取 [16KB, 4MB]
- 即改即存；进行中的传输不受影响，下一传输生效

### 4.4 错误处理

- RTT 采样失败 → 默认参数（功能不受影响，仅性能回退现状）
- netem 不可用（容器无 NET_ADMIN）→ 门控测试 fail 并提示（不放行），不静默跳过
- 调参后传输失败率上升（perf 记录比对）→ 属回归门控拦截范围

## 五、测试策略

- ZzSftpTuner 纯函数单测：各 RTT 档位的输出参数（含边界 5ms 上下、非法输入）
- ZzSshCore 既有回归：unit 15 程序 + docker integration/perf 16 项全绿
- netem 三档 perf 门控：比值 ≥0.95 为放行线；perf 记录入库（沿用 records 机制与恢复规矩）
- 主仓：设置序列化往返、设置页 combo 反映/写回、手动值传递到 setTransferBlockSize 的接线用例
- 主仓回归：45 项基线全绿 + 新增；全量 ctest 后恢复 `tests/perf/records/` 并按当天日期前缀清理

## 六、完成定义

1. ✅ M6a：netem 三档比值实测 25ms 上 0.99/下 1.64、50ms 上 1.05/下 1.58、100ms 上 1.14/下 1.57（均 ≥0.95），回环无 5% 回归；ZzSshCore 全部测试绿（unit 16 程序 + docker 集成/perf 17 项）
2. ✅ M6b：设置项 + 设置页 + ZzSftpOps 接线交付；主仓 45 基线 + 新增全绿
3. ✅ 任务 0 调研结论入库（`third_party/ZzSshCore/docs/sftp-bdp-tuning.md`，含六轮扫描原始数据与校准档位表）
4. ✅ 规格条目核销；代码审查通过（逐任务审查 + 最终宽范围审查 I-1/I-2 修复波，定向复审 2/2 ADDRESSED）
5. ⏸ 人工验收（V0.2 清单 `docs/acceptance/v0.2-manual-acceptance.md` + X11 M5 清单）由用户执行

## 七、风险

| 风险 | 概率 | 影响 | 应对 |
| ---- | ---- | ---- | ---- |
| OpenSSH 对端窗口在高 RTT 上传下自增长充分，调参无提升空间 | 中 | 低 | 任务 0 先实测；此时上传方向维持现状即达标（比值口径公平） |
| libssh2 通道窗口上限 < 预期，下载提速受限 | 中 | 中 | 任务 0 实测上限；不足时评估 libssh2 patch（库已自带 cmake 构建链） |
| netem 环境下 OpenSSH 参照同步劣化，比值口径失真 | 低 | 中 | 任务 0 第 3 项公平性验证；必要时改绝对阈值口径报用户裁决 |
| 自适应参数在真实链路抖动（RTT 突变）下振荡 | 中 | 低 | 滑动平均 + 档位滞回（升档快、降档慢） |
| docker 容器 tc 需要 NET_ADMIN 权限，CI 环境不支持 | 低 | 中 | run-integration-tests.sh 加 --cap-add；不支持时 fail 并提示（不静默放行） |
