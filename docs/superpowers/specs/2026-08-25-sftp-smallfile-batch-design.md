# SFTP 小文件批量传输优化 设计规格

- 日期：2026-08-25
- 状态：已实现（人工验收挂起）
- 范围：ZzSshCore 库层目录递归传输 + 有界并发调度 + 主仓面板最小接线
- 前置事实：engine 已支持多传输并发（`m_transfers` + `pumpTransfers()`），缺的是目录递归与有界调度；面板当前仅平铺多选文件、无目录递归、下载仅单文件。

## 1. 目标与验收标准

小文件场景（数千文件）大幅超越 WindTerm（回环 3.9MB/s 级）与 OpenSSH sftp。

**性能门控（比值门控，用户选定）：**

- 新增 perf 场景 `zzsftp-smallfiles`：2000 小文件 × 20KB 目录树（含 ≥16 个子目录，对齐 WindTerm benchmark 形态）。
- 回环：vs OpenSSH `sftp -r` 吞吐比值 ≥ 1.5。
- WAN 50ms（netem）：比值 ≥ 2。
- 防回归：现有大文件回环 + WAN 三档（25/50/100ms）门控照旧全绿。

**功能验收：** 目录树递归上传/下载内容一致（docker 集成 sha256 全量校验）；批取消语义正确；单文件失败不阻断批次。

## 2. 架构与组件

全部落在 ZzSshCore，跟随现有 .h/.cpp 两文件惯例（本库不用 Pimpl 四文件结构）。

- `ZzSftpDirWalker`（纯逻辑组件）：本地树用 `QDirIterator` 遍历，产出任务列表 = 目录任务（按深度排序，父先子后）+ 文件任务 + 批总字节数。不依赖网络，直接单测。
- `ZzSftpBatchScheduler`（worker 线程侧，`ZzSftpEngine` 内部组件）：持有任务队列，维持 N 个在飞传输（默认 8，可配 1–32），每个完成事件补投下一个。
- `ZzSftpEngine` 新增入口：
  - `startDirUpload(requestId, localDir, remoteDir, errorString)`
  - `startDirDownload(requestId, remoteDir, localDir, errorString)`
  - 内部创建 scheduler，`pumpTransfers()` 一并推进。**现有单文件路径零改动。**
  - 批取消复用 `cancelTransfer(requestId)`：清在飞 + 清待投队列。
- 链路透传：`ZzSftpSession` / 主仓 `ZzSftpOps` 各加 `uploadDir` / `downloadDir`；`ZzMockSftpOps` 同步扩展。
- 面板最小接线：「上传文件夹」「下载文件夹」两个入口（`QFileDialog::getExistingDirectory` / 右键目录项下载）。不动 ZzPureToolsPro。

## 3. 数据流

上传（下载对称）：

1. 面板调 `ops->uploadDir(localDir, remoteDir)` → queued 到 worker 线程。
2. walker 遍历本地树（纯本地，微秒级）→ 任务列表 + 批总字节。
3. 阶段一（建目录）：按深度序同步 mkdir；已存在的目录 stat 确认是目录后跳过。
4. 阶段二（传文件）：scheduler 维持 8 在飞 `startUpload`（blockSize=0，走 BDP 自动档），完成一个补投一个。
5. 批级进度复用现有 `ProgressFn`：done=批已确认字节、total=批总字节，沿用 ≥1MB / ≥100ms 节流。
6. 队列空且在飞清零 → `FinishFn` 回调。

下载差异点：远端树用 `listDir` 递归（4400 文件/16 目录形态下遍历仅 16 RTT，V1 不做遍历流水线）；本地建目录用 `QDir().mkpath`，零网络成本。

## 4. 错误处理

- 单文件失败：记入批级错误列表，**继续**其余文件；批结束时错误列表非空 → error 回调带汇总（首错码 + 失败计数）。
- 取消：`cancelTransfer(批 requestId)` 清在飞 + 清队列；已落盘文件不回滚（与 rsync 语义一致）。
- 遍历失败（权限等）：立即 error 回调，批不启动。
- 断线：走现有会话级错误路径，批随之 error。
- 内存不变式：在飞 ≤ N（默认 8）；小文件 staging 实际占用 ≈ 文件本身大小；混合大文件时上限 N×8MB（默认 64MB），可用并发数配置下调。

## 5. 测试

- 单测（ZzSshCore，QtTest）：
  - walker：嵌套/空目录/深度序/总字节统计正确性。
  - scheduler 不变式：在飞 ≤ N、完成补投、取消清空、错误汇总语义。
- docker 集成：2000 文件 × 20KB 目录树递归上传 + 下载，sha256 全量校验一致。
- perf 门控：`zzsftp-smallfiles` 回环比值 ≥1.5、WAN 50ms 比值 ≥2（记录入库，沿用滚动基线机制）。
- 面板：mock 扩展后现有面板测试适配 + 文件夹入口最小用例。

## 6. 非目标（YAGNI）

- 多 SFTP 通道并行（方案 B，收益存疑复杂度高）。
- 远端目录遍历流水线（深树遍历优化，V1 不需要）。
- 断点续传、失败自动重试、批级暂停/恢复。
- "失败即中止"开关（取消已覆盖该语义）。

## 7. 约束与惯例

- commit：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详述；push 需用户确认。
- 类名 Zz 前缀、文件名 = 类名、Doxygen 简体中文注释。
- ZzSshCore 回归：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit`（16 程序）+ `tests/integration/docker/run-integration-tests.sh build/linux-release`（17 项）；perf records 为入库滚动基线（留 5 份随 commit）。
- 主仓回归：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release`（基线 45）；跑完全量 ctest 后 `git checkout -- tests/perf/records/` 并按当天日期前缀删未跟踪新记录（禁用月份通配）。
- 库改动先推库、再 bump 主仓 gitlink（push 前逐项问用户）。

## 8. 实现核销

- 任务 1-5（ZzSshCore）：walker/scheduler/engine 集成/docker IT/perf 门控，全部完成（库侧 92ceea7；README 实测数据补充 8955cff）。
- 任务 6（主仓接线）：面板入口 + gitlink bump，完成（主仓 e198b2e）。
- 实测：回环比值 上/下 = 2.02/2.22（160.1/162.1 MB/s vs OpenSSH 79.1/73.0）；WAN 50ms 上/下 = 2.90/3.84（0.368/0.364 MB/s vs 0.127/0.095）。门控 ≥1.5 / ≥2.0 均过（records 见 ZzSshCore `tests/perf/records/2026-08-26-zzsftp-smallfiles-082558-3132477.json`）。
- 人工验收挂起：Windows 实机「上传文件夹/下载文件夹」入口与真实服务器小文件体验，已并入 V0.2 验收清单（`docs/acceptance/v0.2-manual-acceptance.md`）。
