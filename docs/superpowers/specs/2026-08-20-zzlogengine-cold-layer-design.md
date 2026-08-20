# ZzLogEngine 冷层设计（SQLite + ZSTD + FTS5）与温层清理策略

日期：2026-08-20
状态：已与用户逐节确认（读回语义 / 归档策略 / 压缩方案 / 库组织 / 实现方案）
上游规格：`docs/superpowers/specs/2026-08-17-zzclawterm-v0.1-design.md` §五（本文落地其中"v0.2 冷层"部分）

## 一、目标

- 滚动历史**真正无上限**：向上滚动穿越温层边界时，从冷层透明读回更老的行
- 冷层为**唯一持久真相**：温层降级为加速缓存，磁盘不双倍占用
- 全文搜索能力（FTS5），为后续搜索 UI 铺路（本次只交付引擎 API，不做 UI）
- 性能红线：滚动帧 ≤16ms；热层追加吞吐不回退超 5%

## 二、已确认的关键决策

| 决策点 | 结论 |
|---|---|
| 读回语义 | 滚动可穿越进冷层，`getLines` 三层透明归并 |
| 归档策略 | 冷层为唯一持久真相；温→冷后台持续推进；干净退出删除温层文件 |
| 压缩方案 | 引入 ZSTD（third_party/zstd 子模块，BSD 协议，锁定版本 tag） |
| 库组织 | 全局单库 `cold.db`，行携带 sessionId + 纳秒时间戳 |
| 实现方案 | 方案 A：`ZzColdStorage` 独立类，复用现有归档线程两阶段推进 |
| SQLite 访问 | 直接 sqlite3 C API（事务与 FTS5 控制精细），不走 Qt SQL |
| UI | 本次零 UI 工作；搜索只交付引擎 API + QTest |

## 三、架构与数据流

```text
终端输出
   │
   ▼
热层 RingBuffer（1万行，内存）——现有逻辑不动
   │ 热层满 → 批量驱逐（现有逻辑不动）
   ▼
温层 ZzMmapBuffer（mmap + LZ4 块，默认 100 万行窗口）——现有逻辑不动
   │ 归档线程在温层写入后异步推进 coldFrontier：
   │   读温层已落盘块 → ZSTD 压缩 → 写入全局 cold.db → 确认后温层截头回收
   ▼
冷层 ZzColdStorage（全局单库 cold.db，无限容量）
```

- 新类 `ZzColdStorage`（`src/log/ZzColdStorage.h/.cpp`，跟随项目现有双文件惯例）：批量写入、按行号范围读取、FTS5 搜索、按策略清理
- 行号约定不变：绝对单调 ID。冷层持有 `[0, coldCount)`，温层 `[warmBase, warmBase+warmCount)`，热层接续；三层区间无缝拼接。`firstLineNo()` 恒为 0，除非清理删除老数据后前移（既有语义，滚动桥已处理窗口前移）
- 不新增线程：温→冷推进复用 `ZzLogArchiveWorker` 所在 QThread
- `ZzLogEngine::Config` 扩展：
  - `coldDbPath`：为空则冷层禁用，行为退化为 v0.1
  - `coldMaxBytes`（默认 10GB）、`coldMaxAgeDays`（默认 90 天）：清理水位
- 全局单库路径：应用数据目录下 `scrollback/cold.db`

## 四、存储格式

```sql
-- 压缩块表：每块最多 1024 行，ZSTD 压缩为一个 blob
CREATE TABLE blocks (
    block_id    INTEGER PRIMARY KEY,   -- 单调递增
    first_line  INTEGER NOT NULL,      -- 块首行绝对行号
    line_count  INTEGER NOT NULL,
    session_id  TEXT NOT NULL,         -- 会话 profile id
    start_ts_ns INTEGER NOT NULL,      -- 块首行时间戳（纳秒）
    end_ts_ns   INTEGER NOT NULL,
    payload     BLOB NOT NULL          -- ZSTD(行数据序列化)
);
CREATE INDEX idx_blocks_range ON blocks(first_line);
CREATE INDEX idx_blocks_session_ts ON blocks(session_id, start_ts_ns);

-- FTS5 外部内容模式：索引文本但不复制存储
CREATE VIRTUAL TABLE lines_fts USING fts5(text, content='');

-- 元数据：coldFrontier、schema 版本、清理水位
CREATE TABLE meta (key TEXT PRIMARY KEY, value INTEGER);
```

- 块内序列化格式与温层一致（text + 属性 blob），块头带行内偏移表，支持块内定位单行；读 24 行最多解压 1 块
- session_id 与时间戳为后续"按会话/时间查询"铺路
- FTS5 与块表同事务写入；搜索返回行号列表，再回块表取原文
- SQLite 使用 WAL 模式，崩溃不丢已提交数据

## 五、读取归并

```text
getLines(start, count) 请求 [start, start+count)：
  ├─ 与冷层区间 [0, coldCount) 重叠 → ZzColdStorage::getLines（解压块 + LRU 缓存）
  ├─ 与温层区间重叠                  → 现有 mmap 路径（不动）
  └─ 与热层区间重叠                  → 现有环形缓冲（不动）
按行号升序拼接返回；调用方与滚动桥零改动
```

- `preload()` 扩展：滚动位置落入冷层区间时，归档线程预解压相邻块
- `ZzColdStorage` 内置解压块 LRU（默认 32 块 ≈ 3.2 万行），连续滚动基本全命中

## 六、归档推进与温层清理

```text
归档线程循环（复用现有 QThread）：
1. 温层批次写完 → 触发 coldAdvance 任务
2. 从温层读 [coldFrontier, warmBase+warmCount) 中已完整成块的部分（1024 行一批）
3. ZSTD 压缩 + SQLite 事务写入（块表 + FTS5 同事务）→ 提交后 coldFrontier 前移
4. 温层截头：coldFrontier 已覆盖的温层前缀复用既有"最老块丢弃"机制回收空间
5. 干净退出：flush 保证全部已排队行落入冷层后删除温层文件
   异常退出：温层文件残留，下次启动比对 coldFrontier 续传未归档部分后删除
```

- 失败隔离：冷层写失败（磁盘满/库损坏）→ 重试 3 次后降级"无冷层模式"（行为 = v0.1 温层超限丢弃），发射 `degradedToWarmOnly` 信号；与温层降级模式对称，不影响终端交互
- 崩溃恢复：启动时 `coldFrontier`（meta 表）与温层实际区间比对，未归档部分续传

## 七、自动清理

- 每次冷层写入后检查 `coldMaxBytes` / `coldMaxAgeDays`
- 超限则按最老块批量删除 + FTS5 同步删除 + 增量 VACUUM
- 删除导致 `firstLineNo()` 前移

## 八、测试与性能门控

性能记录统一落 `tests/perf/records/YYYY-MM-DD-<功能名>.json`（沿用既有统一 schema），不达标不验收。

| 测试 | 阈值 | 说明 |
|---|---|---|
| 冷层写入吞吐 | ≥ 50 万行/s | ZSTD level 3，1024 行/块 |
| 冷层随机读 24 行（缓存未命中） | ≤ 5ms | 含解压 |
| 冷层连续滚动（缓存命中） | ≤ 1ms/页 | LRU 命中路径 |
| FTS5 全文搜索 1000 万行 | ≤ 500ms | 单关键词 |
| 三层归并滚动（跨冷/温边界） | ≤ 16ms/帧 | 滚动不卡顿红线 |
| 追加路径回归 | 热层追加 ≥ 200 万行/s | 现有 232 万行/s 不得回退超 5% |

QTest 单元测试覆盖：块读写往返、FTS5 搜索、崩溃恢复续传、清理策略、降级路径。

## 九、依赖与构建

- `third_party/zstd`：Git 子模块，锁定 release tag，CMake 静态编译接入
- SQLite：直接使用 sqlite3 C API；构建时探测系统库 FTS5 编译开关，不可用则 third_party 引入官方 amalgamation（Public Domain）
- 所有新代码遵守项目既有约束：C++20、Zz 前缀、文件名与类名一致、Doxygen 简体中文注释、commit 首行中文简述 + 空行 + 中文详细说明（Conventional Commits 前缀）

## 十、范围边界（明确不做）

- 搜索 UI、按会话/时间查询 UI（等 ZzPureToolsPro）
- 冷层导出功能（v0.3 候选）
- 跨设备同步、云端备份
