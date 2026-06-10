# ZzClawTerm 日志引擎（无限存储 + 零卡顿）

这是项目的核心创新，解决现有终端「日志多了卡死或被删除」的痛点。

## 问题根因

现有终端：所有行存内存（内存爆炸）、全量渲染（O(n) 卡顿）、单线程（VT解析/渲染/IO 互相阻塞）。

## 三层存储

```
实时流(PTY/SSH)
   ▼
热数据层 Ring Buffer        最近 1万行 | 内存 ~10MB 固定 | O(1) | 完整 VT 属性
   ▼ 溢出归档
温数据层 mmap + LZ4         最近 100万行 | 磁盘 ~100MB | 分块压缩 + 行偏移索引
   ▼ 定期归档
冷数据层 SQLite+ZSTD+FTS5   无限 | 原始 10-20% | 全文索引 | 按会话/时间分区
```

### 热层 ZzRingBuffer
- 固定大小数组，`append()` O(1)，满时返回被覆盖行交给归档。
- 内存：1万行 × 1KB ≈ 10MB 固定。

### 温层 ZzMmapBuffer
- mmap 内存映射文件，64KB 分块，LZ4 批量压缩（减少系统调用）。
- 读：按需解压可见区域；写：批量。
- 借鉴 klogg：mmap 按需加载、行偏移索引、并行分块搜索。

### 冷层 ZzColdStorage
- SQLite 表 `terminal_logs`（session_id, line_number, timestamp, content BLOB(ZSTD), attributes）+ 索引。
- FTS5 虚拟表 `logs_fts`（unicode61 分词）做全文搜索。
- 写入缓冲批量提交（BATCH_SIZE=1000），WAL 模式防损坏。
- 支持按时间/会话查询、导出（txt/json/csv/html）、自动清理。

## 行偏移索引（ZzLineIndex，借鉴 klogg）

- 分块索引：每 1024 行记录一个 `{file_offset, line_number}`。
- 1000万行 → ~80KB 索引（对比 klogg 逐行 ~80MB，省 1000 倍）。
- 块内精确定位用小范围扫描或 LRU 缓存。
- 支持实时追加（`appendLine(offset)` 增量更新）。

## 虚拟滚动（零卡顿关键）

无论多少行只渲染可见 24-50 行。
- 热层：直接内存访问，零延迟。
- 温层：按需解压可见块 + 预加载相邻块。
- 冷层：异步查询 + 加载指示器。
- 性能对比：全量渲染百万行 500ms+/10GB → 虚拟滚动 24 行 <2ms/10MB。

## 三层并行搜索（ZzSearchEngine）

`searchAsync` 同时查三层，增量回调：热层最先返回让用户立即看到 → 温层多线程分块（类 klogg）→ 冷层 FTS5 毫秒级。支持正则、大小写敏感、结果高亮。

## 与 klogg 的关系

klogg 是只读静态大文件查看器；本项目是「实时终端 + 无限存储 + 查看器」三合一。从 klogg 吸收：行偏移索引、mmap 按需加载、并行分块搜索、增量文件监控。klogg 的能力是本项目温层的子集。可顺带提供「打开日志文件」的类 klogg 查看器模式。

## 配置（ZzLogConfig）

`hot_lines=10000`、`warm_lines=1000000`、`enable_compression`、`enable_cold_storage`、`archive_interval_ms=5000`、`max_disk_usage_mb=10000`、`max_session_age_days=90`、`io_thread_count`、`archive_thread_count`、`enable_prefetch`。

## 性能目标

| 指标 | 传统 | 本方案 |
|------|------|--------|
| 内存 | 10GB+ | ~50-100MB |
| 滚动延迟 | 500ms | <2ms |
| 搜索 1000万行 | 不支持 | ≤100ms |
| 写入吞吐 | — | ≥1M 行/秒 |
