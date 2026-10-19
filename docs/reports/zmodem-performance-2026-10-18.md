# ZMODEM 背压与吞吐优化（2026-10-18）

本次保持 Rust zmodem2、Cargo 固定版本和持久化配置不变。本轮未提供 SSH/lrzsz 验收端点，真实 SSH/lrzsz、内存峰值和 UI 流畅性尚未验收；这里的网络实测仅为 Windows 本机 Raw TCP 回环，不代表 SSH、串口或跨机器性能。

## 实现

- `nyaterm-transport::zmodem::runtime` 独占协议状态和会话消费者。desktop 负责检测、选择文件、任务和通知。交接在 bridge 的 dequeue/routing 锁内执行，顺序为检测器缓冲、UI pending、bridge pending、transport 后续事件；启动线程失败发生在交接之前。完成时先把未消费尾部和本批剩余事件交还 UI，再恢复 bridge。迁移窗口保留消费者并重绑唤醒。
- 选择文件期间不取输入；控制信号独立于数据。每次最多从源队列提取 128 KiB，每轮协议输出不超过 64 KiB。进度只保留最新值，文件边界保留最终进度；生命周期队列 256 项，取消可合并进最后一个信封以免等待 UI。GPUI 不阻塞发送或 join；回收器 2 个线程、64 项，满载时已取消的线程自行退出。
- SSH 写入增加 8 MiB / 1024 项 RAII 许可，覆盖命令 FIFO、连接准备、初始化暂存和正在提交 SSH channel 的数据。关闭独立、resize 合并。SSH send、receive/output 和 resize futures 独立推进；输出队列满时异步等待，释放 Tokio 执行线程。通用后台写入改为容量释放通知，25 ms 超时只用于检查取消，不是固定发送重试延迟。
- `ZmodemTransferOptions` 只存在内存中；老构造函数保持保守值。生产 SSH、PTY、Telnet、Raw TCP 使用 nonstop receive 和 256 子包窗口；串口保持保守协议参数。串口软件流控仍禁止二进制传输。
- 文件读写使用 256 KiB 缓冲，重传时执行正确 seek；相邻协议输出按 64 KiB 合并，不增加定时器延迟。所有文件批次预检，实际打开时再校验大小；超过 `u32::MAX` 拒绝。打开、读取、写入、flush、协议推进失败终止并尝试取消。满队列或断线时取消序列仅做一次非阻塞尝试，原始失败不会被覆盖为成功。
- 下载结束识别可分片 `OO` 和解码器遗留的可选 CR/LF/XON；不匹配的前缀完整返回终端。识别等待上限 250 ms。

源输出队列是 SessionManager 共享的 8 MiB 队列；运行时只增加有限批次和初始化交接数据，并非进程 RSS 上限。统计 `source_queue_peak_bytes` 是该 manager 生命周期的共享峰值，`write_queue_peak_bytes` 是该会话 mailbox 生命周期峰值；它们不是单次传输重置计数。文件缓冲、内核 TCP/SSH 缓冲和 UI 已持有的有限事件另外计量。

## 可重复基准

```powershell
# 完整矩阵：协议/文件 I/O + 实际生产运行时、双向 Raw TCP、慢消费者
python scripts/bench/zmodem_matrix.py --bytes 8388608 --repeats 3 --output zmodem-results.jsonl
# 长文件与优化构建（首次 release 编译耗时较长）
python scripts/bench/zmodem_matrix.py --release --bytes 67108864 --repeats 3 --output zmodem-release.jsonl
# 单独运行
cargo run -p nyaterm-transport --example zmodem_bench -- --window 256 --receive nonstop --buffered 1 --rtt-ms 50
cargo run -p nyaterm-transport --example zmodem_bench -- --layer runtime --direction download --delay-ms 2 --rate-kib 4096
```

临时目录、固定种子的二进制数据，结果为逐行 JSON。矩阵分别测量保守配置、只启用文件缓冲、窗口 10/64/256/1024（含有/无缓冲），以及两种接收模式的真实后台运行时。慢消费者用真实 sleep/限速，计入 `elapsed_seconds`；协议层 RTT 用虚拟时间推进，单独输出 `simulated_wire_seconds` 和 `projected_mib_per_second`。后者包含模拟等待和本机 CPU/磁盘时间，**不是实测网络吞吐**。`sender_ack_waits` 统计 ZCRCW（包括尾包确认），不是 OS 线程等待次数。

每一项都比较完成状态、长度和完整内容，输出接收 SHA-256；运行时另有源/发送队列峰值、容量等待纳秒、写入提交耗时与泵轮数。协议层没有生产容量许可，因此其容量等待为零，`protocol_queue_peak_bytes` 表示测试调度队列峰值。测试环境、Git HEAD、工作区状态和构建 profile 写入 JSONL 第一行。性能比较需要同机、同配置、多次重复，不能把 debug 数值推广到发布构建。

## 基线和阶段对照

原始代码更改前实际执行的 8 MiB debug 基线保存在 [baseline.jsonl](zmodem-2026-10-18/baseline.jsonl)。原始接收保守值导致 8193 次等待；文件 SHA-256 为 `d521072d1cb032cc9ce2e25ba8c6fb6a03a20eb5a49c5206f21912b8fd8141a5`。

正式结果为 28 配置 × 3 次，共 84 次全部成功；8 MiB 接收内容与源数据逐字节相同，SHA-256 均与上述基线一致。数据见 [optimized.jsonl](zmodem-2026-10-18/optimized.jsonl)，分组中位数、范围及峰值见 [results-summary.json](zmodem-2026-10-18/results-summary.json)。另保留 [first-pass.jsonl](zmodem-2026-10-18/first-pass.jsonl) 作为初轮记录，未混入正式统计。

以下为 Windows debug 的实际耗时吞吐中位数；协议层不包含虚拟 RTT 等待，运行时层经过生产队列和 Raw TCP 写入接口：

| 层次/方向 | 接收模式 | 窗口 | 文件缓冲 | MiB/s 中位数 | ZCRCW 次数 |
| --- | --- | ---: | --- | ---: | ---: |
| 协议/文件 I/O | 保守 | 10 | 无 | 4.388 | 8193 |
| 协议/文件 I/O | 保守 | 10 | 256 KiB | 4.636 | 8193 |
| 协议/文件 I/O | 连续 | 10 | 256 KiB | 4.761 | 821 |
| 协议/文件 I/O | 连续 | 64 | 256 KiB | 4.814 | 129 |
| 协议/文件 I/O | 连续 | 256 | 256 KiB | 4.869 | 33 |
| 协议/文件 I/O | 连续 | 1024 | 256 KiB | 4.872 | 9 |
| 后台上传/回环 | 保守 | 256 | 256 KiB | 3.393 | 8193 |
| 后台上传/回环 | 连续 | 256 | 256 KiB | 7.575 | 33 |
| 后台上传/回环 | 连续 | 1024 | 256 KiB | 8.750 | 9 |
| 后台下载/回环 | 保守 | 256 | 256 KiB | 3.300 | 8193 |
| 后台下载/回环 | 连续 | 256 | 256 KiB | 5.023 | 33 |
| 后台下载/回环 | 连续 | 1024 | 256 KiB | 5.075 | 9 |
| 后台上传/慢对端 | 连续 | 256 | 256 KiB | 2.192 | 33 |
| 后台下载/慢对端 | 连续 | 256 | 256 KiB | 1.477 | 33 |

慢对端设置为每次读取延迟2 ms、按4096 KiB/s限制数据。其吞吐还包含协议、调用与调度耗时，不等于限速参数本身。整个矩阵源队列字节峰值860491、发送队列字节峰值68777，均低于8 MiB；这些运行没有触发发送容量等待（计数为0），满载等待与取消由自动化测试另行验证。队列峰值不能替代进程 RSS 采样。

虚拟50 ms RTT 的协议等待时间：保守409.8 s；连续窗口10/64/256/1024分别41.2/6.6/1.8/0.6 s，不能作为实际网络耗时。所有保守接收配置仍为8193次等待，说明扩大上传窗口没有越过远端流控协商。默认256按计划保留了相对于1024更小的未确认窗口；本机回环上传1024更快，256并非所有环境下的最佳值。

保守配置是新实现的配置对照，不能伪称历史架构的实测结果。原架构只保留了协议层单次基线，未保存 SSH 背压、后台交接各阶段的独立历史测速，不能量化“移出 GPUI”本身的加速比例。以上数值均不推广为跨机器或发布构建收益。

## 自动验证

覆盖：空文件、多文件、非整子包、全部字节值、分片头、四种窗口、CRC 损坏重传/回退 seek、大小边界、批次预检、再次打开校验、读取中缩短、下载 flush 失败；选择文件不取数据、满写队列取消、暂停 UI 仍完成、其他会话输出隔离、最终帧+提示符、分片 OO/不匹配输出、SSH in-flight/初始化许可、项数限制/控制绕过/容量唤醒/关闭、异步源队列满载与取消。大小边界不实际传输数 GiB。

Windows 11（10.0.26300），debug 构建，最终运行结果保存在 [validation.json](zmodem-2026-10-18/validation.json)：

| 命令 | 结果 |
| --- | --- |
| `cargo check -j 4 --workspace` | 通过 |
| `cargo test -j 4 --workspace` | 3926 通过，18 忽略，0 失败 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy -j 4 --workspace --all-targets` | 通过，无警告 |

其中 transport 单元测试 506 通过、2 忽略。收尾过程中曾出现终端捕获标记过滤测试失败；为此增加捕获结果与显示结果的断言。重新构建后单项通过、额外连续5次通过，最终完整 workspace 测试也通过；未修改捕获策略或解析器。另执行 `python -m py_compile scripts/bench/zmodem_matrix.py` 和 `git diff --check`。

实际 SSH 远端窗口耗尽及真实 GPUI 交互仍按以下清单验收。自动测试中的 SSH 满载是 mailbox/控制信号模拟，运行时基准使用 Raw TCP，不能替代真实 SSH 验收。

## 真实 SSH / lrzsz 待验收（需要可访问的测试环境）

1. 记录 NyaTerm 构建、Windows 版本、SSH 服务端、`sz --version` / `rz --version`、RTT、带宽和目标磁盘。旧版 NyaTerm/WindTerm 如需对比，应使用相同主机和文件。远端 lrzsz 仅作为验收对端，不是新增的本地运行时依赖。
2. 用可公开的确定性数据建立空文件、1 B、1023/1024/1025 B、8 MiB、64 MiB、多文件批次；两端 SHA-256 和长度逐项比较。Windows `Get-FileHash -Algorithm SHA256`，远端 `sha256sum`。使用含所有字节取值的数据，避免只测文本。
3. 远端 `sz -b 文件...` 验证下载，远端 `rz -b` + 本地选择文件验证上传；另测产品自动上传命令。分别在低 RTT、约50/150 ms RTT以及限速/慢磁盘环境重复3次，记录端到端时间。为大于4 GiB的稀疏文件验证上传在首文件前明确拒绝。
4. 选择文件窗口保持30秒，再接受/取消；测试远端断开、关闭标签、连续两次传输、完成后立即输出提示符、其他会话持续输出，以及选择期间迁移标签。应无提示符丢失、重复和二进制泄漏。
5. 高速接收与慢磁盘、大量发送与远端不读/窗口耗尽期间取消、关闭。记录取消响应时间；继续切换标签、键入、调整窗口并观察 UI。验证读写双向不形成等待环。
6. 用任务管理器/Process Explorer 或 `Get-Process nyaterm` 定时采样 WorkingSet64/PrivateMemorySize64，区分传输前基线、稳定峰值、结束后回落；记录容量统计。8 MiB 指每个队列的字节许可，并非整个应用只能增加8 MiB。
7. 录制策略分别开启/关闭 binary payload，验证终端录制和 AI 捕获策略；串口另测保守模式、取消及软件流控禁用提示。

真实 SSH 速度、RSS、远端窗口、串口、UI 流畅性：**待验收，未冒充已运行**。
