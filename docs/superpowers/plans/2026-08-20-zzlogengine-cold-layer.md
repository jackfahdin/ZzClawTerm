# ZzLogEngine 冷层（SQLite + ZSTD + FTS5）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 实现规格 `docs/superpowers/specs/2026-08-20-zzlogengine-cold-layer-design.md` 的滚动历史冷层（全局单库 SQLite + ZSTD 分块压缩 + FTS5 全文索引），让向上滚动透明穿越温层边界读回更老的行，冷层成为唯一持久真相。

**架构：** 三层结构——`ZzRingBuffer`（热层，不动）、`ZzMmapBuffer`（温层，新增保留下限 `setRetentionFloor`，由冷层归档进度驱动截头）、`ZzColdStorage`（新类，冷层：每块最多 1024 行 ZSTD level 3 压缩为一个 blob 存 `blocks` 表，FTS5 contentless 索引行文本，`meta` 表存全局 frontier/base/schema 版本）。`ZzLogArchiveWorker` 在温层批次写完后两阶段推进温→冷（复用现有归档 QThread，不新增线程）；`ZzLogEngine::getLines` 改为冷→温→热三路归并，调用方零改动。

**技术栈：** C++20 / Qt 6.8+（仅 Qt6::Core + Qt6::Test）/ CMake 3.25+ / ZSTD v1.5.7（vendored git submodule，官方 CMake 位于 `build/cmake`，目标 `libzstd_static`）/ SQLite 官方 amalgamation（vendored 进 `third_party/sqlite/`，自建静态目标 `sqlite3_static`，编译定义 `SQLITE_ENABLE_FTS5`，运行时 WAL 模式）/ QTest。

**执行前提：** v0.1 日志引擎（计划 `2026-08-17-zzclawterm-v0.1-02-zzlogengine.md`）已完成并入库：`src/log/` 下的 `ZzLogEngine` / `ZzMmapBuffer` / `ZzLogArchiveWorker` / `ZzRingBuffer` / `ZzLineIndex` 与 `tests/log/` 全部测试均存在且通过。本计划在此基础上增量演进，不重写既有逻辑。

---

## 前置说明（相对规格/任务书的有意偏离与澄清，执行前必读）

**1. SQLite 采用 vendored amalgamation，而非规格的"先探测系统库"。** 理由：三端（Linux/macOS/Windows）行为确定性——系统 SQLite 版本与编译开关各异，FTS5 并非保证可用；vendored 后 `SQLITE_ENABLE_FTS5` 由我们自己定义，能力有硬保证；SQLite 为 Public Domain，vendored 无任何许可负担。代价是仓库增大约 9MB 源文件，可接受。

**2. 块时间戳 = 归档时刻（近似）。** `ZzLogLine` 只有 `text + attributes`，无时间戳字段；改动它会破坏温层已落盘格式，不做。因此 `blocks.start_ts_ns/end_ts_ns` 取块写入冷层的时刻。热层缓冲最多 1 万行、温→冷持续推进，归档延迟通常秒级，该近似对"按时间清理/查询"足够。计划文档与代码注释均如实标注此为近似。

**3. 库内行号全局单调，引擎实例按 `m_coldOffset` 平移。** `cold.db` 是所有会话共享的全局单库，`meta.frontier` 是全局下一个待写行号；而显示层（QTermWidget / ZzScrollbackBridge）行号每会话从 0 起。因此：每个 `ZzLogEngine` 实例 `open()` 时记录 `m_coldOffset = cold->frontier()`，引擎（显示）行号 `e` 与库内全局行号 `g` 按 `g = e + m_coldOffset` 互译；本实例只能看到 `g >= m_coldOffset` 的冷层行（历史会话的行持久保留但不在本会话滚动窗口内，跨会话浏览留待后续版本）。由此：顺序会话、同 profile 并发多标签均不会冲突（各实例拿到互不重叠的全局区间）。已知限制：并发多标签时，一标签的冷层可见窗口上界取自全局 frontier，极端情况下向上滚动到最顶端可能读到另一并发标签刚归档的行——不崩溃、不损坏数据，v0.2 接受并留待 session_id 过滤读回时解决。

**4. 崩溃恢复在 `ZzLogEngine::open()` 内同步执行（调整自任务书的"FIFO 排队续传"）。** 任务书草案让 worker 以 QueuedConnection 排队续传残留温层，前提是残留温层文件被复用为活温层；但 v0.1 已确立"新会话显示层行号从 0 起，温层必须从空开始"（`ZzTerminalView::enableScrollback` 的 `QFile::remove` 注释），复用残留温层会让读回命中上一会话的行（错行）。因此恢复流程为：open() 发现残留温层 → 同步按持久化游标续传进冷层 → 删除残留文件 → 创建全新空温层。恢复仅在异常退出后触发，100 万行温层全量续传约秒级，可接受；残留温层的续传游标持久化于温层文件头（见下条），崩溃套崩溃的毫秒级窗口（冷层事务已提交、温层头未落盘）理论上可产生重复块，概率极低，v0.3 可加块内容校验去重。

**5. 温层文件头第 12 字节起新增 `coldCursor u64`（兼容旧文件）。** 温层头占 4096 字节一页，v0.1 只写前 12 字节（magic/version/skipBlocks），其余为零填充。v0.2 定义偏移 12 处为 `coldCursor`（本文件行已被冷层覆盖的游标，== 保留下限 floor），旧文件读出 0，等价于"无续传进度"，无需版本号迁移。

**6. 搜索 API（FTS5）v0.2 仅覆盖已归档进冷层的行。** 温层/热层行不入 FTS 索引（入索引意味着温层删除时需同步删 FTS，复杂化归档路径；搜索 UI 属范围边界外）。FTS5 使用默认 unicode61 分词器：ASCII 关键字按词命中，中文连续文本为整串 token、子串不命中——CJK 分词（trigram/jieba）留待搜索 UI 版本，v0.2 测试一律使用 ASCII 关键字。

**7. 任务书任务分解的两处微调。** (a) 崩溃恢复从任务 7（worker）移到任务 8（engine open），原因见第 4 条；(b) `ZzMmapBuffer` 除 `setRetentionFloor` 外新增 `clearRetentionFloor()`——冷层降级后温层必须回到 v0.1 纯 maxLines 丢弃行为，否则温层永不截头会无限增长。

**8. 线程与锁。** `ZzColdStorage` 内部 `QMutex` 保护 sqlite3 连接与全部内部状态（sqlite3 以 `SQLITE_OPEN_FULLMUTEX` 打开，双保险）；写事务短（一块一事务），读路径一次最多解压 1 块。温层读写锁模型不变。归档线程内 coldAdvance 读温层持读锁、改 floor 持写锁。

所有命令默认在仓库根目录 `/home/zz/Jackfahdin/github/ZzClawTerm` 下执行。构建用显式形式（`cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8`）；性能测试用 Release 构建：若执行环境 preset 可则用 `cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release`，否则显式 `cmake -S . -B build/release -DCMAKE_BUILD_TYPE=Release && cmake --build build/release -j8`（两种形式在任务 10 均给出）。

## 文件结构

| 文件 | 职责 |
| ---- | ---- |
| `third_party/zstd/` | ZSTD v1.5.7 vendored git submodule（冷层压缩） |
| `third_party/sqlite/{sqlite3.c,sqlite3.h,sqlite3ext.h,CMakeLists.txt}` | SQLite 官方 amalgamation（vendored 入库）+ 自建 `sqlite3_static` 目标 |
| 根 `CMakeLists.txt` | 追加 ZSTD / SQLite 两个 third_party 引入段（锚点：LZ4 段之后） |
| `src/log/ZzColdStorage.h/.cpp` | 冷层存储：块读写（ZSTD）、FTS5 搜索、自动清理、LRU 解压缓存 |
| `src/log/ZzMmapBuffer.h/.cpp` | 温层新增 `setRetentionFloor/clearRetentionFloor/coldCursor`（头部持久化游标） |
| `src/log/ZzLogArchiveWorker.h/.cpp` | 归档线程扩展：温→冷两阶段推进、降级门闩、冷层预加载槽 |
| `src/log/ZzLogEngine.h/.cpp` | 门面：三层归并、Config 扩展、崩溃恢复、干净退出删温层、`degradedToWarmOnly`、`searchLines` |
| `src/terminal/ZzTerminalView.cpp` | 装配层接线：`coldDbPath`/`sessionId` 传入引擎创建点，移除温层预删 |
| `src/log/CMakeLists.txt` | 追加 ZzColdStorage 源文件；链接 `libzstd_static`/`sqlite3_static` |
| `tests/log/ZzColdStorageTest.cpp` | 冷层 QTest（骨架/块读写/FTS5/清理） |
| `tests/log/ZzLogArchiveWorkerTest.cpp` | 归档线程温→冷推进与降级 QTest |
| `tests/log/ZzLogEngineTest.cpp` | 追加冷层集成用例（三层归并/恢复/降级/干净退出/搜索） |
| `tests/log/ZzMmapBufferTest.cpp` | 追加保留下限用例（floor 丢弃/持久化/清除恢复 v0.1） |
| `tests/log/ZzColdStoragePerfTest.cpp` | 冷层性能门控 QTest（写吞吐/随机读/连续滚动/FTS5/三层归并） |
| `tests/log/CMakeLists.txt` | 注册新测试目标 |
| `tests/perf/records/YYYY-MM-DD-ZzColdStorage-*.json` | 性能记录（由测试生成并提交入库） |

行号约定：全引擎使用绝对单调递增 ID。冷层库内空间全局单调（跨会话）；引擎空间 = 库内空间 - `m_coldOffset`，每会话从 0 起。可读窗口 `[firstLineNo(), firstLineNo() + totalLines())`；冷层清理删除老数据后 `firstLineNo()` 前移（既有语义）。

---

### 任务 1：引入 ZSTD 子模块与 SQLite amalgamation，接入根构建

**文件：**
- 创建：`third_party/zstd/`（git submodule，指针提交）
- 创建：`third_party/sqlite/sqlite3.c`、`sqlite3.h`、`sqlite3ext.h`（amalgamation 解压产物，vendored 入库）
- 创建：`third_party/sqlite/CMakeLists.txt`
- 修改：根 `CMakeLists.txt`（LZ4 段之后追加 ZSTD 与 SQLite 引入段）
- 修改：`.gitmodules`（`git submodule add` 自动追加）

- [ ] **步骤 1：添加 ZSTD 子模块（锁定 v1.5.7）**

```bash
git submodule add --depth 1 --branch v1.5.7 https://github.com/facebook/zstd.git third_party/zstd
```

预期：生成 `third_party/zstd/` 与 `.gitmodules` 新条目；`git submodule status` 输出一行以空格（已检出）开头的 `third_party/zstd` 记录。

- [ ] **步骤 2：验证 ZSTD 自带 CMake 工程可用**

```bash
ls third_party/zstd/build/cmake/CMakeLists.txt
```

预期：文件存在（ZSTD 官方 CMake 工程位于 `build/cmake` 子目录，提供 `libzstd_static` / `libzstd_shared` 目标）。

- [ ] **步骤 3：下载 SQLite 最新稳定版 amalgamation 并 vendored 入库**

```bash
mkdir -p third_party/sqlite
DL=$(curl -fsSL https://www.sqlite.org/download.html | grep -oE '[0-9]{4}/sqlite-amalgamation-[0-9]+\.zip' | head -1)
echo "最新 amalgamation：${DL}"
curl -fsSL -o /tmp/sqlite-amalgamation.zip "https://www.sqlite.org/${DL}"
sha256sum /tmp/sqlite-amalgamation.zip   # 版本号与 sha256 记录进步骤 6 的 commit message
unzip -o /tmp/sqlite-amalgamation.zip -d /tmp/sqlite-amalg
cp /tmp/sqlite-amalg/sqlite-amalgamation-*/sqlite3.c \
   /tmp/sqlite-amalg/sqlite-amalgamation-*/sqlite3.h \
   /tmp/sqlite-amalg/sqlite-amalgamation-*/sqlite3ext.h \
   third_party/sqlite/
ls -la third_party/sqlite/
```

预期：`third_party/sqlite/` 下出现 `sqlite3.c`（约 9MB）、`sqlite3.h`、`sqlite3ext.h` 三个文件；终端打印的版本号与 sha256 备用。

- [ ] **步骤 4：创建 third_party/sqlite/CMakeLists.txt（自建 sqlite3_static 目标）**

```cmake
# SQLite 官方 amalgamation（vendored，Public Domain）：ZzLogEngine 冷层持久存储。
# 必须定义 SQLITE_ENABLE_FTS5（全文索引）；线程模式保持默认 serialized。
add_library(sqlite3_static STATIC
    sqlite3.c
    sqlite3.h
    sqlite3ext.h
)
target_include_directories(sqlite3_static PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(sqlite3_static PUBLIC SQLITE_ENABLE_FTS5)
if(UNIX)
    find_package(Threads REQUIRED)
    target_link_libraries(sqlite3_static PUBLIC Threads::Threads ${CMAKE_DL_LIBS})
endif()
# amalgamation 单文件体量大，关闭第三方告警干扰
if(MSVC)
    target_compile_options(sqlite3_static PRIVATE /W0)
else()
    target_compile_options(sqlite3_static PRIVATE -w)
endif()
```

- [ ] **步骤 5：在根 CMakeLists.txt 追加 ZSTD 与 SQLite 引入段**

打开根 `CMakeLists.txt`。**锚点**：既有 LZ4 段（`add_subdirectory(third_party/lz4/build/cmake)` 一行）之后、`add_subdirectory(src)` 之前，追加以下完整段落：

```cmake
# ZSTD（冷层压缩，供 ZzLogEngine 链接 libzstd_static；官方 CMake 位于 build/cmake）
zz_require_submodule(third_party/zstd/build/cmake)
set(ZSTD_BUILD_PROGRAMS OFF CACHE BOOL "不构建 zstd 命令行工具" FORCE)
set(ZSTD_BUILD_SHARED OFF CACHE BOOL "不构建 zstd 动态库" FORCE)
set(ZSTD_BUILD_TESTS OFF CACHE BOOL "不构建 zstd 测试" FORCE)
add_subdirectory(third_party/zstd/build/cmake)

# SQLite amalgamation（vendored，Public Domain）：冷层持久存储 + FTS5
if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/sqlite/CMakeLists.txt")
    message(FATAL_ERROR
        "SQLite amalgamation 缺失：third_party/sqlite\n请先执行冷层实现计划任务 1 的下载步骤")
endif()
add_subdirectory(third_party/sqlite)
```

- [ ] **步骤 6：配置并构建，验证两个目标可用**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8 --target libzstd_static sqlite3_static
```

预期：配置成功无报错；构建输出分别以 `Built target libzstd_static`、`Built target sqlite3_static` 结束。

- [ ] **步骤 7：Commit**

commit message 中把 `<版本>` 与 `<sha256>` 替换为步骤 3 实测值（如 `3.50.4` 与完整 sha256）：

```bash
git add .gitmodules third_party/zstd third_party/sqlite CMakeLists.txt
git commit -m "build: 引入 ZSTD v1.5.7 子模块与 SQLite <版本> amalgamation" -m "- third_party/zstd：vendored git submodule，锁定 v1.5.7 tag，官方 CMake（build/cmake）提供 libzstd_static，关闭 PROGRAMS/SHARED/TESTS
- third_party/sqlite：vendored 官方 amalgamation <版本>（sha256: <sha256>），自建 sqlite3_static 静态目标并定义 SQLITE_ENABLE_FTS5
- 根 CMakeLists.txt 在 LZ4 段后追加两个引入段；SQLite 选 vendored 而非系统库：三端确定性 + 保证 FTS5 可用 + Public Domain 无许可负担"
```

---

### 任务 2：ZzColdStorage 骨架——open/建 schema/WAL/meta 读写（TDD）

**文件：**
- 创建：`src/log/ZzColdStorage.h`
- 创建：`src/log/ZzColdStorage.cpp`
- 创建：`tests/log/ZzColdStorageTest.cpp`
- 修改：`src/log/CMakeLists.txt`（追加源文件 + 链接 zstd/sqlite）
- 修改：`tests/log/CMakeLists.txt`（注册测试）

- [ ] **步骤 1：编写失败的测试**

创建 `tests/log/ZzColdStorageTest.cpp`（本任务只用骨架用例，后续任务向本文件追加用例）：

```cpp
#include "ZzColdStorage.h"

#include <QFile>
#include <QTemporaryDir>
#include <QtTest>

#include <sqlite3.h>

/**
 * @brief ZzColdStorage 冷层单元测试：骨架（schema/WAL/meta）、块读写、FTS5、清理。
 */
class ZzColdStorageTest : public QObject
{
    Q_OBJECT
    static ZzColdStorage::Config testConfig(const QString &dbPath)
    {
        ZzColdStorage::Config c;
        c.dbPath = dbPath;
        c.sessionId = QStringLiteral("test-session");
        return c;
    }
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("cold-line-%1").arg(i),
                QByteArray("C") + QByteArray::number(qint64(i))};
    }
    /// @brief 生成 [start, start+count) 的连续行序列。
    static QVector<ZzLogLine> makeLines(quint64 start, quint64 count)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(count));
        for (quint64 i = 0; i < count; ++i)
            out.append(line(start + i));
        return out;
    }

private slots:
    /// @brief 新库打开：frontier/base 均为 0，库文件落盘。
    void freshOpenCreatesDatabase()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage cold(testConfig(path));
        QString error;
        QVERIFY2(cold.open(&error), qPrintable(error));
        QVERIFY(cold.isOpen());
        QCOMPARE(cold.frontier(), 0ULL);
        QCOMPARE(cold.baseLine(), 0ULL);
        QVERIFY(QFile::exists(path));
    }

    /// @brief schema 与 WAL：用 sqlite3 C API 独立验证（测试目标直接链接 sqlite3_static）。
    void schemaAndWalCreated()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
        } // 关闭后 WAL 模式持久于库头

        sqlite3 *db = nullptr;
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);

        // WAL 模式
        sqlite3_stmt *stmt = nullptr;
        QVERIFY(sqlite3_prepare_v2(db, "PRAGMA journal_mode", -1, &stmt, nullptr) == SQLITE_OK);
        QCOMPARE(sqlite3_step(stmt), SQLITE_ROW);
        QCOMPARE(QString::fromUtf8(reinterpret_cast<const char *>(sqlite3_column_text(stmt, 0))),
                 QStringLiteral("wal"));
        sqlite3_finalize(stmt);

        // meta 初始值
        const auto metaValue = [&db](const char *key) -> qint64 {
            sqlite3_stmt *s = nullptr;
            if (sqlite3_prepare_v2(db, "SELECT value FROM meta WHERE key = ?", -1, &s, nullptr)
                != SQLITE_OK)
                return -1;
            sqlite3_bind_text(s, 1, key, -1, SQLITE_TRANSIENT);
            const qint64 v = sqlite3_step(s) == SQLITE_ROW ? sqlite3_column_int64(s, 0) : -1;
            sqlite3_finalize(s);
            return v;
        };
        QCOMPARE(metaValue("schema_version"), qint64(1));
        QCOMPARE(metaValue("frontier"), qint64(0));
        QCOMPARE(metaValue("base"), qint64(0));

        // blocks / lines_fts 已建
        const auto tableExists = [&db](const char *name) -> bool {
            sqlite3_stmt *s = nullptr;
            if (sqlite3_prepare_v2(db,
                    "SELECT count(*) FROM sqlite_master WHERE name = ?", -1, &s, nullptr)
                != SQLITE_OK)
                return false;
            sqlite3_bind_text(s, 1, name, -1, SQLITE_TRANSIENT);
            const bool ok = sqlite3_step(s) == SQLITE_ROW && sqlite3_column_int(s, 0) > 0;
            sqlite3_finalize(s);
            return ok;
        };
        QVERIFY(tableExists("blocks"));
        QVERIFY(tableExists("lines_fts"));
        sqlite3_close(db);
    }

    /// @brief 路径不可用时打开失败并给出原因（父路径是已存在文件，跨平台必失败）。
    void openFailureReturnsFalseWithError()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString blocker = dir.filePath(QStringLiteral("blocker"));
        QFile f(blocker);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();

        ZzColdStorage cold(testConfig(blocker + QStringLiteral("/cold.db")));
        QString error;
        QVERIFY(!cold.open(&error));
        QVERIFY(!cold.isOpen());
        QVERIFY(!error.isEmpty());
        QCOMPARE(cold.frontier(), 0ULL);
    }
};

QTEST_GUILESS_MAIN(ZzColdStorageTest)
#include "ZzColdStorageTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加（`ZzColdStorageTest` 需直接用 sqlite3 C API 验证 schema，故显式链接 `sqlite3_static`，不走 `zz_add_log_test`）：

```cmake
# 冷层测试：直接链接 sqlite3_static 以便用 sqlite3 C API 独立验证 schema/WAL
add_executable(ZzColdStorageTest ZzColdStorageTest.cpp)
target_link_libraries(ZzColdStorageTest PRIVATE ZzLogEngine sqlite3_static Qt6::Core Qt6::Test)
set_target_properties(ZzColdStorageTest PROPERTIES AUTOMOC ON)
add_test(NAME ZzColdStorageTest COMMAND ZzColdStorageTest)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
```

预期：构建失败，报错 `fatal error: 'ZzColdStorage.h' file not found`。

- [ ] **步骤 3：编写实现代码**

创建 `src/log/ZzColdStorage.h`（公开接口与后续任务逐字一致；private 段后续任务会追加成员）：

```cpp
#pragma once

#include "ZzLogLine.h"

#include <QMutex>
#include <QString>
#include <QVector>

struct sqlite3;

/**
 * @brief 冷层存储：全局单库 SQLite + ZSTD 分块压缩 + FTS5 全文索引（规格 §四）。
 *
 * - 每块最多 kMaxBlockLines 行，ZSTD level 3 压缩为一个 blob 存 blocks 表；
 *   块内行编码与温层一致（textLen u32|attrLen u32|UTF-8|属性），块头带行内偏移表；
 * - 行号：库内全局单调（跨会话共享单库），frontier() == 下一个待写入绝对行号；
 * - 块时间戳 = 归档时刻（ZzLogLine 无时间戳字段；热层缓冲最多 1 万行，归档延迟通常秒级，
 *   该近似对按时间清理/查询足够）；
 * - frontier 与块写入同一 SQLite 事务提交：崩溃不产生重复块/丢块；
 * - FTS5 contentless 索引行文本（rowid = 绝对行号），与块写入同事务；
 * - 线程安全：内部 QMutex 保护 sqlite3 连接与全部内部状态（写事务短，读只解压 1 块），
 *   全部公开方法可在任意线程调用。
 */
class ZzColdStorage
{
public:
    /**
     * @brief 冷层配置。
     */
    struct Config {
        QString dbPath;          ///< 全局单库路径
        QString sessionId;       ///< 写入行的会话归属
        qint64 maxBytes = 10LL * 1024 * 1024 * 1024; ///< 清理水位：10GB
        int maxAgeDays = 90;     ///< 清理水位：90 天
    };

    static constexpr quint64 kMaxBlockLines = 1024; ///< 单块最大行数（规格 §四）
    static constexpr int kCompressionLevel = 3;     ///< ZSTD 压缩级别（规格 §二）

    explicit ZzColdStorage(const Config &config);
    ~ZzColdStorage();

    ZzColdStorage(const ZzColdStorage &) = delete;
    ZzColdStorage &operator=(const ZzColdStorage &) = delete;

    /**
     * @brief 打开（或创建）库文件，建 schema、置 WAL、读回 meta 与块表。
     * @param errorString 失败时输出原因，可为空。
     * @return 成功返回 true；meta 与块表不一致（库损坏）时返回 false。
     */
    bool open(QString *errorString = nullptr);

    /// @brief 关闭连接（WAL 已提交数据不丢）。
    void close();

    bool isOpen() const;

    quint64 baseLine() const;    ///< 最老可读行（清理后前移）
    quint64 frontier() const;    ///< 已覆盖行数上界 == 下一个待写入绝对行号

    /**
     * @brief 追加一块（归档线程调用）。
     * @param lines 块内行，1..kMaxBlockLines 行。
     * @param firstLine 块首行绝对行号，必须 == frontier()（全局连续，崩溃无重复块）。
     * @param errorString 失败时输出原因，可为空。
     * @return 成功返回 true 且 frontier() 前移 lines.size()；失败返回 false 且状态不变。
     * @note 提交后内部自动执行一次 enforceLimits()（规格 §七：每次写入后检查水位）。
     */
    bool appendBlock(const QVector<ZzLogLine> &lines, quint64 firstLine,
                     QString *errorString = nullptr);

    /**
     * @brief 读取 [startLine, startLine + count) 区间内的行（任意线程）。
     * @return 实际读到的行；起点早于 baseLine() 或超出 frontier() 时按实际可得数量返回。
     */
    QVector<ZzLogLine> readLines(quint64 startLine, quint64 count) const;

    /// @brief 预解压 lineId 所在块及后一块进 LRU（供滚动方向预取）。
    void preload(quint64 lineId);

    /**
     * @brief FTS5 全文搜索。
     * @param pattern FTS5 MATCH 表达式（非法表达式返回空，不报错）。
     * @param maxResults 最大返回行数。
     * @return 命中行的绝对行号（升序）；仅覆盖已归档进冷层的行。
     */
    QVector<quint64> search(const QString &pattern, int maxResults = 1000) const;

    /// @brief 按 maxBytes/maxAgeDays 清理最老块（含 FTS5 同步删除与增量 VACUUM）。
    void enforceLimits();

private:
    struct BlockEntry {
        quint64 firstLine = 0;  ///< 块首行绝对行号
        quint32 lineCount = 0;  ///< 块内行数
    };

    bool execSql(const char *sql, QString *errorString) const;
    bool loadState(QString *errorString); ///< open 时读 meta + 块表并做一致性校验
    void closeLocked();                   ///< 调用方须已持有 m_mutex

    Config m_config;
    sqlite3 *m_db = nullptr;
    mutable QMutex m_mutex;       ///< 保护 sqlite3 连接与全部内部状态
    QVector<BlockEntry> m_blocks; ///< 存活块表（按 firstLine 递增）
    quint64 m_frontier = 0;
    quint64 m_baseLine = 0;
};
```

创建 `src/log/ZzColdStorage.cpp`（本任务实现骨架部分；`appendBlock`/`readLines`/`preload`/`search`/`enforceLimits` 由任务 3/4/5 追加到同一文件）：

```cpp
#include "ZzColdStorage.h"

#include <sqlite3.h>

/**
 * @file ZzColdStorage.cpp
 * @brief 冷层存储实现。行编码函数（encodeLine/skipLine/parseLine）与
 *        ZzMmapBuffer.cpp 同款（textLen u32|attrLen u32|UTF-8|属性负载），
 *        保持冷/温两层块内格式一致；任务 3 追加块读写时引入该匿名命名空间。
 */

ZzColdStorage::ZzColdStorage(const Config &config)
    : m_config(config)
{
}

ZzColdStorage::~ZzColdStorage()
{
    close();
}

bool ZzColdStorage::execSql(const char *sql, QString *errorString) const
{
    char *errmsg = nullptr;
    if (sqlite3_exec(m_db, sql, nullptr, nullptr, &errmsg) == SQLITE_OK)
        return true;
    if (errorString) {
        *errorString = QStringLiteral("SQLite 执行失败：%1（SQL：%2）")
                           .arg(QString::fromUtf8(errmsg ? errmsg : "unknown"),
                                QString::fromUtf8(sql));
    }
    sqlite3_free(errmsg);
    return false;
}

bool ZzColdStorage::open(QString *errorString)
{
    QMutexLocker locker(&m_mutex);
    if (m_db)
        return true;
    const QByteArray path = m_config.dbPath.toUtf8();
    if (sqlite3_open_v2(path.constData(), &m_db,
                        SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX,
                        nullptr)
        != SQLITE_OK) {
        if (errorString)
            *errorString = QStringLiteral("sqlite3_open_v2 失败：%1")
                               .arg(m_db ? QString::fromUtf8(sqlite3_errmsg(m_db))
                                         : QStringLiteral("内存不足"));
        if (m_db) {
            sqlite3_close(m_db);
            m_db = nullptr;
        }
        return false;
    }

    // schema 与规格 §四 原文一致（IF NOT EXISTS 保证重开幂等）
    static const char *const kSetup[] = {
        "PRAGMA journal_mode=WAL",     // WAL：崩溃不丢已提交数据（规格 §四）
        "PRAGMA synchronous=NORMAL",   // WAL 下 NORMAL 足够（崩溃不丢已提交事务）
        "CREATE TABLE IF NOT EXISTS blocks ("
        " block_id    INTEGER PRIMARY KEY,"
        " first_line  INTEGER NOT NULL,"
        " line_count  INTEGER NOT NULL,"
        " session_id  TEXT NOT NULL,"
        " start_ts_ns INTEGER NOT NULL,"
        " end_ts_ns   INTEGER NOT NULL,"
        " payload     BLOB NOT NULL)",
        "CREATE INDEX IF NOT EXISTS idx_blocks_range ON blocks(first_line)",
        "CREATE INDEX IF NOT EXISTS idx_blocks_session_ts ON blocks(session_id, start_ts_ns)",
        "CREATE VIRTUAL TABLE IF NOT EXISTS lines_fts USING fts5(text, content='')",
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value INTEGER)",
        "INSERT OR IGNORE INTO meta(key, value) VALUES('schema_version', 1)",
        "INSERT OR IGNORE INTO meta(key, value) VALUES('frontier', 0)",
        "INSERT OR IGNORE INTO meta(key, value) VALUES('base', 0)",
    };
    for (const char *sql : kSetup) {
        if (!execSql(sql, errorString)) {
            closeLocked();
            return false;
        }
    }
    if (!loadState(errorString)) {
        closeLocked();
        return false;
    }
    return true;
}

bool ZzColdStorage::loadState(QString *errorString)
{
    // 调用方须已持有 m_mutex
    const auto readMeta = [this](const char *key, quint64 *out) -> bool {
        sqlite3_stmt *stmt = nullptr;
        if (sqlite3_prepare_v2(m_db, "SELECT value FROM meta WHERE key = ?", -1, &stmt, nullptr)
            != SQLITE_OK)
            return false;
        sqlite3_bind_text(stmt, 1, key, -1, SQLITE_TRANSIENT);
        const bool found = sqlite3_step(stmt) == SQLITE_ROW;
        if (found)
            *out = quint64(sqlite3_column_int64(stmt, 0));
        sqlite3_finalize(stmt);
        return found;
    };
    if (!readMeta("frontier", &m_frontier) || !readMeta("base", &m_baseLine)) {
        if (errorString)
            *errorString = QStringLiteral("meta 表缺少 frontier/base 记录（库损坏？）");
        return false;
    }

    m_blocks.clear();
    sqlite3_stmt *stmt = nullptr;
    if (sqlite3_prepare_v2(m_db, "SELECT first_line, line_count FROM blocks ORDER BY first_line",
                           -1, &stmt, nullptr)
        != SQLITE_OK) {
        if (errorString)
            *errorString = QStringLiteral("块表读取失败：%1")
                               .arg(QString::fromUtf8(sqlite3_errmsg(m_db)));
        return false;
    }
    while (sqlite3_step(stmt) == SQLITE_ROW)
        m_blocks.append({quint64(sqlite3_column_int64(stmt, 0)),
                         quint32(sqlite3_column_int(stmt, 1))});
    sqlite3_finalize(stmt);

    // 一致性校验：frontier 与块写入同事务，meta 与块表必须吻合
    if (!m_blocks.isEmpty()) {
        const BlockEntry &last = m_blocks.last();
        if (m_blocks.first().firstLine != m_baseLine
            || last.firstLine + last.lineCount != m_frontier) {
            if (errorString)
                *errorString = QStringLiteral("meta 与块表不一致（库损坏？）："
                                              "base=%1 first=%2 frontier=%3 lastEnd=%4")
                                   .arg(m_baseLine)
                                   .arg(m_blocks.first().firstLine)
                                   .arg(m_frontier)
                                   .arg(last.firstLine + last.lineCount);
            return false;
        }
    } else if (m_baseLine > m_frontier) {
        if (errorString)
            *errorString = QStringLiteral("meta base 超过 frontier（库损坏？）");
        return false;
    }
    return true;
}

void ZzColdStorage::closeLocked()
{
    if (m_db) {
        sqlite3_close(m_db);
        m_db = nullptr;
    }
}

void ZzColdStorage::close()
{
    QMutexLocker locker(&m_mutex);
    closeLocked();
    m_blocks.clear();
    m_frontier = 0;
    m_baseLine = 0;
}

bool ZzColdStorage::isOpen() const
{
    QMutexLocker locker(&m_mutex);
    return m_db != nullptr;
}

quint64 ZzColdStorage::baseLine() const
{
    QMutexLocker locker(&m_mutex);
    return m_baseLine;
}

quint64 ZzColdStorage::frontier() const
{
    QMutexLocker locker(&m_mutex);
    return m_frontier;
}
```

修改 `src/log/CMakeLists.txt`：在源文件清单中 `ZzLogArchiveWorker.h` 之前插入两行，并扩展链接行。改后完整文件：

```cmake
# ZzLogEngine：纯 Qt Core 日志引擎库（热层环形缓冲 + 温层 mmap/LZ4 + 冷层 SQLite/ZSTD），不依赖 Widgets。
add_library(ZzLogEngine STATIC
    ZzLogLine.h
    ZzLineIndex.h
    ZzLineIndex.cpp
    ZzRingBuffer.h
    ZzRingBuffer.cpp
    ZzMmapBuffer.h
    ZzMmapBuffer.cpp
    ZzColdStorage.h
    ZzColdStorage.cpp
    ZzLogArchiveWorker.h
    ZzLogArchiveWorker.cpp
    ZzLogEngine.h
    ZzLogEngine.cpp
)
target_include_directories(ZzLogEngine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzLogEngine PUBLIC Qt6::Core PRIVATE lz4_static libzstd_static sqlite3_static)
target_compile_features(ZzLogEngine PUBLIC cxx_std_20)
set_target_properties(ZzLogEngine PROPERTIES AUTOMOC ON)
```

注意：`appendBlock`/`readLines`/`preload`/`search`/`enforceLimits` 此刻只有声明没有定义——只要无人调用它们，静态库与测试均可正常链接（本任务测试只调 open/close/isOpen/frontier/baseLine）。

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：3 个用例全部 Passed。

- [ ] **步骤 5：Commit**

```bash
git add src/log/ZzColdStorage.h src/log/ZzColdStorage.cpp src/log/CMakeLists.txt \
        tests/log/ZzColdStorageTest.cpp tests/log/CMakeLists.txt
git commit -m "feat(log): 新增 ZzColdStorage 骨架与 schema 初始化" -m "- 新类 ZzColdStorage（src/log/，双文件惯例，无 Pimpl），直接用 sqlite3 C API
- open()：建 blocks/lines_fts(FTS5 contentless)/meta 三表（schema 与规格 §四一致），置 WAL + synchronous=NORMAL，meta 初始化 schema_version/frontier/base
- loadState()：读回 meta 与块表并做一致性校验（frontier 与块写入同事务，二者必须吻合）
- 内部 QMutex 保护 sqlite3 连接；库内行号全局单调（跨会话共享单库）
- 测试用 sqlite3 C API 独立验证 WAL 模式、meta 初始值、建表结果与打开失败路径"
```

---

### 任务 3：ZzColdStorage 块读写——appendBlock/readLines + ZSTD + LRU + preload（TDD）

**文件：**
- 修改：`src/log/ZzColdStorage.h`（private 段追加成员与私有方法）
- 修改：`src/log/ZzColdStorage.cpp`（追加匿名命名空间行编码 + 块读写实现）
- 修改：`tests/log/ZzColdStorageTest.cpp`（追加用例）

- [ ] **步骤 1：编写失败的测试**

在 `tests/log/ZzColdStorageTest.cpp` 的 `private slots:` 区追加以下用例：

```cpp
    /// @brief 块读写往返：3 整块（1024×3）+ 1 尾块（37），多种窗口读回与写入逐行一致（含属性）。
    void appendBlockAndReadBack()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());

        quint64 frontier = 0;
        for (int b = 0; b < 3; ++b) {
            QVERIFY(cold.appendBlock(makeLines(frontier, 1024), frontier));
            frontier += 1024;
        }
        QVERIFY(cold.appendBlock(makeLines(frontier, 37), frontier));
        frontier += 37;
        QCOMPARE(cold.frontier(), 3109ULL);
        QCOMPARE(cold.baseLine(), 0ULL);

        // 全量顺序读
        const QVector<ZzLogLine> all = cold.readLines(0, 3109);
        QCOMPARE(all.size(), 3109);
        for (qsizetype i = 0; i < all.size(); ++i) {
            QCOMPARE(all[i].text, line(quint64(i)).text);
            QCOMPARE(all[i].attributes, line(quint64(i)).attributes);
        }
        // 跨块窗口读
        const QVector<ZzLogLine> window = cold.readLines(1000, 100); // 跨块 0/1 边界
        QCOMPARE(window.size(), 100);
        QCOMPARE(window.first().text, line(1000).text);
        QCOMPARE(window.last().text, line(1099).text);
    }

    /// @brief frontier/base 与块数据跨重开持久。
    void statePersistsAcrossReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
            QVERIFY(cold.appendBlock(makeLines(1024, 500), 1024));
        }
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QCOMPARE(cold.frontier(), 1524ULL);
        QCOMPARE(cold.baseLine(), 0ULL);
        const QVector<ZzLogLine> tail = cold.readLines(1520, 4);
        QCOMPARE(tail.size(), 4);
        QCOMPARE(tail.first().text, line(1520).text);
        QCOMPARE(tail.last().text, line(1523).text);
    }

    /// @brief 非连续写入（firstLine != frontier）拒绝且状态不变。
    void nonContiguousAppendFails()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 100), 0));

        QString error;
        QVERIFY(!cold.appendBlock(makeLines(200, 10), 200, &error)); // 期望 firstLine 100
        QVERIFY(!error.isEmpty());
        QCOMPARE(cold.frontier(), 100ULL); // 状态不变
        QVERIFY(cold.readLines(0, 200).size() == 100);

        // 非法块尺寸
        QVERIFY(!cold.appendBlock({}, 100, &error));                    // 空块
        QVERIFY(!cold.appendBlock(makeLines(100, 1025), 100, &error));  // 超 1024 行
        QCOMPARE(cold.frontier(), 100ULL);
    }

    /// @brief 越界读取按实际可得数量返回。
    void readOutOfRangeClamps()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 100), 0));
        QCOMPARE(cold.readLines(0, 1000).size(), 100);  // 超出 frontier 截断
        QVERIFY(cold.readLines(100, 1).isEmpty());      // 起点 == frontier
        QVERIFY(cold.readLines(5000, 10).isEmpty());    // 完全越界
        ZzColdStorage empty(testConfig(dir.filePath(QStringLiteral("empty.db"))));
        QVERIFY(empty.open());
        QVERIFY(empty.readLines(0, 10).isEmpty());      // 空库
    }

    /// @brief LRU 淘汰后读回仍正确：40 块 > 默认 32 块缓存，交替读最老/最新块强制重解压。
    void lruEvictionKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        quint64 frontier = 0;
        for (int b = 0; b < 40; ++b) {
            QVERIFY(cold.appendBlock(makeLines(frontier, 1024), frontier));
            frontier += 1024;
        }
        // 顺序扫一遍（缓存被最新 32 块占满，块 0..7 被淘汰）
        QCOMPARE(cold.readLines(0, frontier).size(), qsizetype(frontier));
        // 重读最老两块（缓存未命中 → SQLite + ZSTD 重解压）与最新块（缓存命中）
        for (const quint64 start : {0ULL, 1024ULL, 38ULL * 1024, 39ULL * 1024}) {
            const QVector<ZzLogLine> got = cold.readLines(start, 24);
            QCOMPARE(got.size(), 24);
            for (qsizetype j = 0; j < 24; ++j)
                QCOMPARE(got[j].text, line(start + quint64(j)).text);
        }
    }

    /// @brief 预加载不改变读取语义。
    void preloadKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
        QVERIFY(cold.appendBlock(makeLines(1024, 1024), 1024));
        cold.preload(10);        // 预解压块 0 与块 1
        cold.preload(999999999); // 越界：静默忽略
        const QVector<ZzLogLine> got = cold.readLines(2040, 8);
        QCOMPARE(got.size(), 8);
        QCOMPARE(got.first().text, line(2040).text);
    }
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：链接失败，报 `undefined reference to 'ZzColdStorage::appendBlock(...)'` 等（方法已声明未定义）。

- [ ] **步骤 3：编写实现代码**

修改 `src/log/ZzColdStorage.h`：文件顶部 include 区追加 `#include <QCache>`；private 段在 `void closeLocked();` 之后追加两行私有方法声明，在 `quint64 m_baseLine = 0;` 之后追加一个成员。改后 private 段完整内容：

```cpp
private:
    struct BlockEntry {
        quint64 firstLine = 0;  ///< 块首行绝对行号
        quint32 lineCount = 0;  ///< 块内行数
    };

    bool execSql(const char *sql, QString *errorString) const;
    bool loadState(QString *errorString); ///< open 时读 meta + 块表并做一致性校验
    void closeLocked();                   ///< 调用方须已持有 m_mutex
    QByteArray rawBlock(quint64 firstLine) const; ///< 解压块（LRU 命中或 SELECT+ZSTD）；须持锁
    qsizetype findBlockIndex(quint64 lineId) const; ///< 二分找最后 firstLine <= lineId 的块

    Config m_config;
    sqlite3 *m_db = nullptr;
    mutable QMutex m_mutex;       ///< 保护 sqlite3 连接与全部内部状态
    QVector<BlockEntry> m_blocks; ///< 存活块表（按 firstLine 递增）
    quint64 m_frontier = 0;
    quint64 m_baseLine = 0;
    mutable QCache<quint64, QByteArray> m_blockCache; ///< 解压块 LRU（键：块首行号）
```

在 `src/log/ZzColdStorage.cpp` 顶部（`#include <sqlite3.h>` 之后）追加匿名命名空间与所需 include：

```cpp
#include <zstd.h>

#include <QDateTime>
#include <QtEndian>
#include <cstring>

namespace {
constexpr int kCacheBlocks = 32; ///< 解压块 LRU 容量（规格 §五：32 块 ≈ 3.2 万行）

void putU32(char *p, quint32 v) { v = qToLittleEndian(v); std::memcpy(p, &v, 4); }
quint32 getU32(const char *p) { quint32 v; std::memcpy(&v, p, 4); return qFromLittleEndian(v); }

/// @brief 行编码（与温层一致）：textLen u32 | attrLen u32 | UTF-8 文本 | 属性负载。
QByteArray encodeLine(const ZzLogLine &line)
{
    const QByteArray text = line.text.toUtf8();
    QByteArray out;
    out.reserve(8 + text.size() + line.attributes.size());
    char hdr[8];
    putU32(hdr, quint32(text.size()));
    putU32(hdr + 4, quint32(line.attributes.size()));
    out.append(hdr, 8);
    out.append(text);
    out.append(line.attributes);
    return out;
}

/// @brief 解析一行，返回下一行偏移；数据截断返回 -1。
qint64 parseLine(const QByteArray &data, qint64 off, ZzLogLine *out)
{
    if (off < 0 || off + 8 > data.size())
        return -1;
    const char *p = data.constData() + off;
    const quint32 textLen = getU32(p);
    const quint32 attrLen = getU32(p + 4);
    const qint64 next = off + 8 + textLen + attrLen;
    if (next > data.size())
        return -1;
    out->text = QString::fromUtf8(p + 8, qsizetype(textLen));
    out->attributes = QByteArray(p + 8 + textLen, qsizetype(attrLen));
    return next;
}

/// @brief 块序列化：lineCount u32 | offsets u32×lineCount（行在块内的绝对偏移）| encodeLine × N。
QByteArray serializeBlock(const QVector<ZzLogLine> &lines)
{
    QVector<QByteArray> encoded;
    encoded.reserve(lines.size());
    qsizetype payloadBytes = 0;
    for (const ZzLogLine &line : lines) {
        encoded.append(encodeLine(line));
        payloadBytes += encoded.last().size();
    }
    const qsizetype headerBytes = 4 + 4 * lines.size();
    QByteArray out;
    out.reserve(headerBytes + payloadBytes);
    char u32buf[4];
    putU32(u32buf, quint32(lines.size()));
    out.append(u32buf, 4);
    qint64 off = headerBytes;
    for (const QByteArray &e : std::as_const(encoded)) {
        putU32(u32buf, quint32(off));
        out.append(u32buf, 4);
        off += e.size();
    }
    for (const QByteArray &e : std::as_const(encoded))
        out.append(e);
    return out;
}
} // namespace
```

在 `ZzColdStorage::frontier()` 实现之后追加以下方法实现：

```cpp
bool ZzColdStorage::appendBlock(const QVector<ZzLogLine> &lines, quint64 firstLine,
                                QString *errorString)
{
    QMutexLocker locker(&m_mutex);
    if (!m_db) {
        if (errorString)
            *errorString = QStringLiteral("冷层库未打开");
        return false;
    }
    if (lines.isEmpty() || quint64(lines.size()) > kMaxBlockLines) {
        if (errorString)
            *errorString = QStringLiteral("冷层块行数必须在 1..%1，实际 %2")
                               .arg(kMaxBlockLines)
                               .arg(lines.size());
        return false;
    }
    if (firstLine != m_frontier) {
        if (errorString)
            *errorString = QStringLiteral("冷层写入行号不连续：期望 %1，实际 %2")
                               .arg(m_frontier)
                               .arg(firstLine);
        return false;
    }

    const QByteArray raw = serializeBlock(lines);
    const size_t bound = ZSTD_compressBound(size_t(raw.size()));
    QByteArray packed;
    packed.resize(qsizetype(bound));
    const size_t packedSize = ZSTD_compress(packed.data(), bound, raw.constData(),
                                            size_t(raw.size()), kCompressionLevel);
    if (ZSTD_isError(packedSize)) {
        if (errorString)
            *errorString = QStringLiteral("ZSTD 压缩失败：%1")
                               .arg(QString::fromUtf8(ZSTD_getErrorName(packedSize)));
        return false;
    }
    packed.resize(qsizetype(packedSize));

    // 块时间戳 = 归档时刻（ZzLogLine 无时间戳字段；热层缓冲最多 1 万行，归档延迟通常秒级）
    const qint64 nowNs = QDateTime::currentMSecsSinceEpoch() * 1000000LL;

    if (!execSql("BEGIN IMMEDIATE", errorString))
        return false;
    bool ok = false;
    sqlite3_stmt *stmt = nullptr;
    // 1) blocks 行
    if (sqlite3_prepare_v2(m_db,
            "INSERT INTO blocks(first_line, line_count, session_id, start_ts_ns, end_ts_ns, payload)"
            " VALUES(?, ?, ?, ?, ?, ?)",
            -1, &stmt, nullptr)
        == SQLITE_OK) {
        const QByteArray sid = m_config.sessionId.toUtf8();
        sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine));
        sqlite3_bind_int(stmt, 2, int(lines.size()));
        sqlite3_bind_text(stmt, 3, sid.constData(), int(sid.size()), SQLITE_TRANSIENT);
        sqlite3_bind_int64(stmt, 4, nowNs);
        sqlite3_bind_int64(stmt, 5, nowNs);
        sqlite3_bind_blob(stmt, 6, packed.constData(), int(packed.size()), SQLITE_TRANSIENT);
        ok = sqlite3_step(stmt) == SQLITE_DONE;
        sqlite3_finalize(stmt);
        stmt = nullptr;
    }
    // 2) FTS5 索引（rowid = 绝对行号），与块写入同事务
    if (ok) {
        ok = sqlite3_prepare_v2(m_db, "INSERT INTO lines_fts(rowid, text) VALUES(?, ?)",
                                -1, &stmt, nullptr)
             == SQLITE_OK;
        for (qsizetype i = 0; i < lines.size() && ok; ++i) {
            const QByteArray text = lines[i].text.toUtf8();
            sqlite3_reset(stmt);
            sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine + quint64(i)));
            sqlite3_bind_text(stmt, 2, text.constData(), int(text.size()), SQLITE_TRANSIENT);
            ok = sqlite3_step(stmt) == SQLITE_DONE;
        }
        if (stmt) {
            sqlite3_finalize(stmt);
            stmt = nullptr;
        }
    }
    // 3) frontier 前移（同事务：崩溃不产生重复块/丢块）
    if (ok) {
        ok = sqlite3_prepare_v2(m_db, "UPDATE meta SET value = ? WHERE key = 'frontier'",
                                -1, &stmt, nullptr)
             == SQLITE_OK;
        if (ok) {
            sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine + quint64(lines.size())));
            ok = sqlite3_step(stmt) == SQLITE_DONE;
            sqlite3_finalize(stmt);
            stmt = nullptr;
        }
    }

    if (!ok) {
        const QString detail = QString::fromUtf8(sqlite3_errmsg(m_db));
        execSql("ROLLBACK", nullptr);
        if (errorString)
            *errorString = QStringLiteral("冷层块写入失败：%1").arg(detail);
        return false;
    }
    if (!execSql("COMMIT", errorString))
        return false;

    m_blocks.append({firstLine, quint32(lines.size())});
    m_frontier = firstLine + quint64(lines.size());
    m_blockCache.insert(firstLine, new QByteArray(raw), 1); // 刚序列化的块直接入缓存

    locker.unlock();
    enforceLimits(); // 规格 §七：每次写入后检查清理水位（内部自取锁，不能在写事务内调用）
    return true;
}

QVector<ZzLogLine> ZzColdStorage::readLines(quint64 startLine, quint64 count) const
{
    QVector<ZzLogLine> out;
    QMutexLocker locker(&m_mutex);
    if (!m_db || count == 0 || m_blocks.isEmpty())
        return out;
    quint64 id = qMax(startLine, m_baseLine);
    const quint64 end = qMin(startLine + count, m_frontier);
    if (end <= id)
        return out; // 区间完全落在已清理/未写入范围
    out.reserve(qsizetype(qMin<quint64>(end - id, 100000)));

    while (id < end) {
        const qsizetype bi = findBlockIndex(id);
        if (bi < 0)
            break;
        const BlockEntry &block = m_blocks[bi];
        const QByteArray raw = rawBlock(block.firstLine);
        if (raw.size() < 4)
            break; // 块损坏：按可得数据返回
        const quint32 n = getU32(raw.constData());
        if (n != block.lineCount || raw.size() < qint64(4 + 4 * n))
            break;
        // 块内偏移表直接定位（读 24 行最多解压 1 块，规格 §四）
        const quint64 blockEnd = block.firstLine + block.lineCount;
        while (id < end && id < blockEnd) {
            const qint64 off = getU32(raw.constData() + 4 + 4 * (id - block.firstLine));
            ZzLogLine line;
            if (parseLine(raw, off, &line) < 0)
                return out;
            out.append(line);
            ++id;
        }
    }
    return out;
}

void ZzColdStorage::preload(quint64 lineId)
{
    QMutexLocker locker(&m_mutex);
    if (!m_db)
        return;
    const qsizetype bi = findBlockIndex(lineId);
    if (bi < 0)
        return;
    rawBlock(m_blocks[bi].firstLine); // 取块即入缓存
    if (bi + 1 < m_blocks.size())
        rawBlock(m_blocks[bi + 1].firstLine);
}

QByteArray ZzColdStorage::rawBlock(quint64 firstLine) const
{
    // 调用方须已持有 m_mutex
    if (const QByteArray *cached = m_blockCache.object(firstLine))
        return *cached;
    QByteArray packed;
    sqlite3_stmt *stmt = nullptr;
    if (sqlite3_prepare_v2(m_db, "SELECT payload FROM blocks WHERE first_line = ?",
                           -1, &stmt, nullptr)
        == SQLITE_OK) {
        sqlite3_bind_int64(stmt, 1, sqlite3_int64(firstLine));
        if (sqlite3_step(stmt) == SQLITE_ROW) {
            const void *blob = sqlite3_column_blob(stmt, 0);
            packed = QByteArray(reinterpret_cast<const char *>(blob),
                                sqlite3_column_bytes(stmt, 0));
        }
        sqlite3_finalize(stmt);
    }
    if (packed.isEmpty())
        return {};
    const unsigned long long rawSize =
        ZSTD_getFrameContentSize(packed.constData(), size_t(packed.size()));
    if (rawSize == ZSTD_CONTENTSIZE_ERROR || rawSize == ZSTD_CONTENTSIZE_UNKNOWN
        || rawSize > 256ULL * 1024 * 1024)
        return {}; // 数据损坏或尺寸不合理
    QByteArray raw;
    raw.resize(qsizetype(rawSize));
    const size_t n = ZSTD_decompress(raw.data(), size_t(raw.size()),
                                     packed.constData(), size_t(packed.size()));
    if (ZSTD_isError(n))
        return {};
    m_blockCache.insert(firstLine, new QByteArray(raw), 1);
    return raw;
}

qsizetype ZzColdStorage::findBlockIndex(quint64 lineId) const
{
    // 调用方须已持有 m_mutex；块表按 firstLine 递增，二分找最后 firstLine <= lineId 的块
    if (m_blocks.isEmpty() || lineId < m_blocks.first().firstLine)
        return -1;
    qsizetype lo = 0;
    qsizetype hi = m_blocks.size() - 1;
    qsizetype best = 0;
    while (lo <= hi) {
        const qsizetype mid = (lo + hi) / 2;
        if (m_blocks[mid].firstLine <= lineId) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    return best;
}
```

并在 `open()` 的 `loadState` 成功之后、`return true;` 之前追加一行缓存容量设置（锚点：`if (!loadState(errorString)) { ... }` 块之后）：

```cpp
    m_blockCache.setMaxCost(kCacheBlocks);
```

注意：`search` 与 `enforceLimits` 此刻仍只有声明——本任务测试不调用它们，链接不受影响。

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：9 个用例（任务 2 的 3 个 + 本任务 6 个）全部 Passed。

- [ ] **步骤 5：回归既有测试**

```bash
ctest --test-dir build/debug --output-on-failure -R 'Zz(LineIndex|RingBuffer|MmapBuffer|ColdStorage|LogEngine)Test'
```

预期：ZzLineIndexTest / ZzRingBufferTest / ZzMmapBufferTest / ZzLogEngineTest / ZzColdStorageTest 全部 Passed（既有四层测试零回归）。

- [ ] **步骤 6：Commit**

```bash
git add src/log/ZzColdStorage.h src/log/ZzColdStorage.cpp tests/log/ZzColdStorageTest.cpp
git commit -m "feat(log): 实现 ZzColdStorage 块读写与 ZSTD 压缩" -m "- appendBlock：块序列化（lineCount + 行内偏移表 + 温层同款行编码）→ ZSTD level 3 压缩 → blocks/FTS5/meta.frontier 同事务写入，崩溃不产生重复块
- readLines：二分定位块 + 块内偏移表直接定位单行，读 24 行最多解压 1 块；越界按可得数量返回
- 解压块 LRU 32 块（QCache，键为块首行号）；preload 预解压所在块及后一块
- 写入连续性校验：firstLine != frontier 拒绝且状态不变；非法块尺寸（空/超 1024 行）拒绝
- 块时间戳取归档时刻（ZzLogLine 无时间戳字段的近似，代码注释如实标注）"
```

---

### 任务 4：FTS5 索引与 search（TDD）

**文件：**
- 修改：`src/log/ZzColdStorage.cpp`（追加 `search` 实现）
- 修改：`tests/log/ZzColdStorageTest.cpp`（追加用例）

- [ ] **步骤 1：编写失败的测试**

在 `tests/log/ZzColdStorageTest.cpp` 的 `private slots:` 区追加以下用例：

```cpp
    /// @brief FTS5 搜索：关键字命中返回精确绝对行号（升序）。
    void searchFindsLinesByKeyword()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        // 3 块；关键字 ERROR 出现在第 7 / 2048 / 3071 行（块内/跨块边界均覆盖）
        quint64 frontier = 0;
        for (int b = 0; b < 3; ++b) {
            QVector<ZzLogLine> block = makeLines(frontier, 1024);
            if (b == 0)
                block[7].text = QStringLiteral("line 7 ERROR happened");
            if (b == 2) {
                block[0].text = QStringLiteral("line 2048 ERROR happened");
                block[1023].text = QStringLiteral("line 3071 ERROR happened");
            }
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += 1024;
        }
        const QVector<quint64> hits = cold.search(QStringLiteral("ERROR"));
        QCOMPARE(hits, (QVector<quint64>{7ULL, 2048ULL, 3071ULL}));
        // 命中行原文可读回
        QCOMPARE(cold.readLines(hits[1], 1).first().text,
                 QStringLiteral("line 2048 ERROR happened"));
    }

    /// @brief maxResults 限制生效；非法 MATCH 表达式返回空不报错。
    void searchRespectsMaxResultsAndBadPattern()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(testConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        QVector<ZzLogLine> block = makeLines(0, 1024);
        for (qsizetype i = 0; i < 100; ++i)
            block[i].text = QStringLiteral("row %1 TOKENXYZ tail").arg(i);
        QVERIFY(cold.appendBlock(block, 0));

        QCOMPARE(cold.search(QStringLiteral("TOKENXYZ"), 10).size(), 10);
        QCOMPARE(cold.search(QStringLiteral("TOKENXYZ"), 1000).size(), 100);
        QVERIFY(cold.search(QStringLiteral("\"unclosed")).isEmpty()); // 非法表达式 → 空
        QVERIFY(cold.search(QString(), 10).isEmpty());                // 空 pattern → 空
        QVERIFY(cold.search(QStringLiteral("TOKENXYZ"), 0).isEmpty()); // maxResults<=0 → 空
    }

    /// @brief 无命中返回空；FTS 索引跨重开持久。
    void searchNoMatchAndReopenPersistence()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVector<ZzLogLine> block = makeLines(0, 512);
            block[42].text = QStringLiteral("persistent NEEDLE42 here");
            QVERIFY(cold.appendBlock(block, 0));
            QVERIFY(cold.search(QStringLiteral("NOSUCHTOKEN")).isEmpty());
        }
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QCOMPARE(cold.search(QStringLiteral("NEEDLE42")),
                 (QVector<quint64>{42ULL})); // 重开后索引仍可查
    }
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：链接失败，报 `undefined reference to 'ZzColdStorage::search(...)'`。

- [ ] **步骤 3：编写实现代码**

在 `src/log/ZzColdStorage.cpp` 的 `findBlockIndex` 实现之后追加：

```cpp
QVector<quint64> ZzColdStorage::search(const QString &pattern, int maxResults) const
{
    QVector<quint64> out;
    QMutexLocker locker(&m_mutex);
    if (!m_db || pattern.isEmpty() || maxResults <= 0)
        return out;
    sqlite3_stmt *stmt = nullptr;
    // 非法 MATCH 表达式时 prepare 失败，按无命中返回（调用方负责合法 FTS5 语法）
    if (sqlite3_prepare_v2(m_db,
                           "SELECT rowid FROM lines_fts WHERE lines_fts MATCH ? LIMIT ?",
                           -1, &stmt, nullptr)
        != SQLITE_OK)
        return out;
    const QByteArray utf8 = pattern.toUtf8();
    sqlite3_bind_text(stmt, 1, utf8.constData(), int(utf8.size()), SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 2, maxResults);
    while (sqlite3_step(stmt) == SQLITE_ROW)
        out.append(quint64(sqlite3_column_int64(stmt, 0)));
    sqlite3_finalize(stmt);
    return out;
}
```

说明：FTS5 默认 unicode61 分词，ASCII 关键字按词命中；中文连续文本为整串 token、子串不命中（CJK 分词留待搜索 UI 版本，见前置说明第 6 条）。

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：12 个用例全部 Passed。

- [ ] **步骤 5：Commit**

```bash
git add src/log/ZzColdStorage.cpp tests/log/ZzColdStorageTest.cpp
git commit -m "feat(log): 实现冷层 FTS5 全文索引与搜索" -m "- lines_fts 采用 contentless 模式（content=''），索引文本但不复制存储，rowid = 绝对行号
- search()：SELECT rowid FROM lines_fts WHERE lines_fts MATCH ? LIMIT ?，返回命中行绝对行号升序
- 非法 MATCH 表达式/空 pattern/maxResults<=0 均按无命中返回；FTS 插入与块写入同事务（任务 3 已并入）
- 测试覆盖块内/跨块边界命中、maxResults 限制、重开后索引持久"
```

---

### 任务 5：enforceLimits 自动清理 + baseLine 前移（TDD）

**文件：**
- 修改：`src/log/ZzColdStorage.h`（private 段追加 `deleteOldestBlocks` 声明）
- 修改：`src/log/ZzColdStorage.cpp`（追加 `enforceLimits`/`deleteOldestBlocks` 实现）
- 修改：`tests/log/ZzColdStorageTest.cpp`（追加用例）

背景说明：FTS5 contentless 表（`content=''`）不支持普通 `DELETE FROM lines_fts WHERE rowid = ?`，删除必须走特殊命令 `INSERT INTO lines_fts(lines_fts, rowid, text) VALUES('delete', ?, ?)` 并提供被删行的原文。`enforceLimits` 按整块删除，删除前先解压待删块取回各行文本（块 payload 本就在库里，代价可控）。

- [ ] **步骤 1：编写失败的测试**

在 `tests/log/ZzColdStorageTest.cpp` 的 `private slots:` 区追加以下用例（头部 `#include <sqlite3.h>` 任务 2 已有；新增用 `#include <QRandomGenerator>`）：

```cpp
    /// @brief 超龄清理：把前两块时间戳改为 100 天前，enforceLimits 后 baseLine 前移、
    ///        老行不可读、FTS 不再命中、剩余行保持可读。
    void maxAgeDaysDropsOldBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        quint64 frontier = 0;
        for (int b = 0; b < 5; ++b) {
            QVector<ZzLogLine> block = makeLines(frontier, 1024);
            if (b == 0)
                block[5].text = QStringLiteral("old OLDTOKEN5 line");
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += 1024;
        }
        QCOMPARE(cold.search(QStringLiteral("OLDTOKEN5")), (QVector<quint64>{5ULL}));

        // 测试直接改库：前两块（行 0..2047）时间戳回拨 100 天
        sqlite3 *db = nullptr;
        // 先关闭 ZzColdStorage 持有的连接，避免 WAL 下双连接写冲突
        cold.close();
        QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
        char *errmsg = nullptr;
        QCOMPARE(sqlite3_exec(db,
                              "UPDATE blocks SET start_ts_ns = 1000000000, end_ts_ns = 1000000000"
                              " WHERE first_line < 2048",
                              nullptr, nullptr, &errmsg),
                 SQLITE_OK);
        sqlite3_free(errmsg);
        sqlite3_close(db);
        QVERIFY(cold.open());

        cold.enforceLimits();
        QCOMPARE(cold.baseLine(), 2048ULL);   // 前两块被清理
        QCOMPARE(cold.frontier(), 5120ULL);   // frontier 不回退
        QVERIFY(cold.readLines(0, 10).isEmpty());            // 已清理区间
        QCOMPARE(cold.readLines(2048, 3).size(), 3);         // 幸存区间从 baseLine 起可读
        QCOMPARE(cold.readLines(2048, 3).first().text, line(2048).text);
        QVERIFY(cold.search(QStringLiteral("OLDTOKEN5")).isEmpty()); // FTS 同步删除
    }

    /// @brief 超容量清理：maxBytes 压小，写入超限后 enforceLimits 按最老块批删，
    ///        baseLine 前移且剩余行完整可读。
    void maxBytesDropsOldestBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage::Config c =
            testConfig(dir.filePath(QStringLiteral("cold.db")));
        c.maxBytes = 2 * 1024 * 1024; // 2MB
        c.maxAgeDays = 36500;         // 关闭超龄干扰
        ZzColdStorage cold(c);
        QVERIFY(cold.open());

        // 每行约 200 字节伪随机文本（按行号确定性生成，ZSTD 基本压不动），
        // 单块约 200KB（含 FTS 索引），30 块库体积约 6MB > 2MB
        const auto randLine = [](quint64 i) {
            QRandomGenerator rng(quint32(i * 2654435761ULL)); // 按行号播种：可重复验证
            QString text;
            text.reserve(200);
            for (int k = 0; k < 25; ++k)
                text.append(QString::number(rng.generate(), 16));
            return ZzLogLine{text + QString::number(qint64(i)), QByteArray()};
        };
        quint64 frontier = 0;
        for (int b = 0; b < 30; ++b) {
            QVector<ZzLogLine> block;
            block.reserve(1024);
            for (quint64 i = 0; i < 1024; ++i)
                block.append(randLine(frontier + i));
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += 1024;
        }
        cold.enforceLimits();
        const quint64 base = cold.baseLine();
        QVERIFY(base > 0ULL);                    // 最老块已被清理
        QCOMPARE(base % 1024ULL, 0ULL);          // 整块粒度删除
        QCOMPARE(cold.frontier(), 30720ULL);
        QVERIFY(base + 1024 <= 30720ULL);        // 至少幸存一块（自适应批量不得删空）
        // 幸存区间完整可读且内容正确
        const QVector<ZzLogLine> head = cold.readLines(base, 24);
        QCOMPARE(head.size(), 24);
        for (qsizetype j = 0; j < 24; ++j)
            QCOMPARE(head[j].text, randLine(base + quint64(j)).text);
    }

    /// @brief 写入后自动触发清理（appendBlock 内部调用 enforceLimits），无需手动调用。
    void enforceLimitsRunsAutomaticallyOnAppend()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage::Config c =
            testConfig(dir.filePath(QStringLiteral("cold.db")));
        c.maxBytes = 512 * 1024;
        c.maxAgeDays = 36500;
        ZzColdStorage cold(c);
        QVERIFY(cold.open());
        QRandomGenerator rng(7);
        quint64 frontier = 0;
        for (int b = 0; b < 20; ++b) {
            QVector<ZzLogLine> block;
            block.reserve(1024);
            for (quint64 i = 0; i < 1024; ++i) {
                QString text;
                for (int k = 0; k < 25; ++k)
                    text.append(QString::number(rng.generate(), 16));
                block.append({text, QByteArray()});
            }
            QVERIFY(cold.appendBlock(block, frontier)); // 仅 append，不调 enforceLimits
            frontier += 1024;
        }
        QVERIFY(cold.baseLine() > 0ULL); // 自动清理已生效
    }

    /// @brief baseLine 跨重开持久：清理后重开，老行不复活。
    void baseLinePersistsAcrossReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzColdStorage cold(testConfig(path));
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 1024), 0));
            QVERIFY(cold.appendBlock(makeLines(1024, 1024), 1024));
            cold.close();
            // 回拨第一块时间戳触发超龄清理
            sqlite3 *db = nullptr;
            QCOMPARE(sqlite3_open(path.toUtf8().constData(), &db), SQLITE_OK);
            QCOMPARE(sqlite3_exec(db,
                                  "UPDATE blocks SET start_ts_ns = 1, end_ts_ns = 1"
                                  " WHERE first_line = 0",
                                  nullptr, nullptr, nullptr),
                     SQLITE_OK);
            sqlite3_close(db);
            QVERIFY(cold.open());
            cold.enforceLimits();
            QCOMPARE(cold.baseLine(), 1024ULL);
        }
        ZzColdStorage cold(testConfig(path));
        QVERIFY(cold.open());
        QCOMPARE(cold.baseLine(), 1024ULL);
        QCOMPARE(cold.frontier(), 2048ULL);
        QVERIFY(cold.readLines(0, 1).isEmpty());
        QCOMPARE(cold.readLines(1024, 1).first().text, line(1024).text);
    }
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：链接失败，报 `undefined reference to 'ZzColdStorage::enforceLimits()'`。

- [ ] **步骤 3：编写实现代码**

修改 `src/log/ZzColdStorage.h`：private 段在 `qsizetype findBlockIndex(quint64 lineId) const;` 之后追加一行：

```cpp
    bool deleteOldestBlocks(qsizetype count, QString *errorString); ///< 删最老 count 块；须持锁
```

在 `src/log/ZzColdStorage.cpp` 的 `search` 实现之后追加：

```cpp
void ZzColdStorage::enforceLimits()
{
    QMutexLocker locker(&m_mutex);
    if (!m_db || m_blocks.isEmpty())
        return;

    QString error;
    // 1) 超龄水位：块时间戳 = 归档时刻，随行号非递减，超龄块必为最老前缀
    const qint64 cutoffNs =
        (QDateTime::currentMSecsSinceEpoch() / 1000 - qint64(m_config.maxAgeDays) * 86400)
        * 1000000000LL;
    qsizetype drop = 0;
    {
        sqlite3_stmt *stmt = nullptr;
        if (sqlite3_prepare_v2(m_db, "SELECT COUNT(*) FROM blocks WHERE end_ts_ns < ?",
                               -1, &stmt, nullptr)
            == SQLITE_OK) {
            sqlite3_bind_int64(stmt, 1, cutoffNs);
            if (sqlite3_step(stmt) == SQLITE_ROW)
                drop = qsizetype(qMax<qint64>(0, sqlite3_column_int64(stmt, 0)));
            sqlite3_finalize(stmt);
        }
    }
    if (drop > 0 && !deleteOldestBlocks(drop, &error))
        return; // 删除失败：保持现状，下次写入时再试

    // 2) 容量水位：page_count × page_size 超限则按最老块批删（按块均体积估算批量，
    //    单批上限 64 块；自适应批量保证不过度删除导致库被清空）
    const auto dbBytes = [this]() -> qint64 {
        qint64 pages = 0;
        qint64 pageSize = 0;
        sqlite3_stmt *stmt = nullptr;
        if (sqlite3_prepare_v2(m_db, "PRAGMA page_count", -1, &stmt, nullptr) == SQLITE_OK) {
            if (sqlite3_step(stmt) == SQLITE_ROW)
                pages = sqlite3_column_int64(stmt, 0);
            sqlite3_finalize(stmt);
            stmt = nullptr;
        }
        if (sqlite3_prepare_v2(m_db, "PRAGMA page_size", -1, &stmt, nullptr) == SQLITE_OK) {
            if (sqlite3_step(stmt) == SQLITE_ROW)
                pageSize = sqlite3_column_int64(stmt, 0);
            sqlite3_finalize(stmt);
        }
        return pages * pageSize;
    };
    while (!m_blocks.isEmpty()) {
        const qint64 size = dbBytes();
        if (size <= m_config.maxBytes)
            break;
        const qint64 perBlock = qMax<qint64>(1, size / m_blocks.size());
        qsizetype n = qsizetype(qMin<qint64>(64, (size - m_config.maxBytes) / perBlock + 1));
        n = qMin(n, m_blocks.size());
        if (!deleteOldestBlocks(n, &error))
            break; // 删除失败：保持现状，下次写入时再试
    }
}

bool ZzColdStorage::deleteOldestBlocks(qsizetype count, QString *errorString)
{
    // 调用方须已持有 m_mutex
    count = qMin(count, m_blocks.size());
    if (count <= 0)
        return true;
    if (!execSql("BEGIN IMMEDIATE", errorString))
        return false;
    bool ok = true;

    // 1) FTS5 contentless 删除需提供被删行原文：先解压待删块取回文本
    sqlite3_stmt *del = nullptr;
    ok = sqlite3_prepare_v2(
             m_db, "INSERT INTO lines_fts(lines_fts, rowid, text) VALUES('delete', ?, ?)",
             -1, &del, nullptr)
         == SQLITE_OK;
    for (qsizetype b = 0; b < count && ok; ++b) {
        const BlockEntry block = m_blocks[b]; // 拷贝：rawBlock 可能写缓存，不持有引用
        const QByteArray raw = rawBlock(block.firstLine);
        if (raw.size() < 4) {
            ok = false;
            break;
        }
        const quint32 n = getU32(raw.constData());
        if (n != block.lineCount || raw.size() < qint64(4 + 4 * n)) {
            ok = false;
            break;
        }
        for (quint32 i = 0; i < n && ok; ++i) {
            const qint64 off = getU32(raw.constData() + 4 + 4 * i);
            ZzLogLine line;
            if (parseLine(raw, off, &line) < 0) {
                ok = false;
                break;
            }
            const QByteArray text = line.text.toUtf8();
            sqlite3_reset(del);
            sqlite3_bind_int64(del, 1, sqlite3_int64(block.firstLine + i));
            sqlite3_bind_text(del, 2, text.constData(), int(text.size()), SQLITE_TRANSIENT);
            ok = sqlite3_step(del) == SQLITE_DONE;
        }
    }
    if (del)
        sqlite3_finalize(del);

    // 2) 块表删除
    if (ok) {
        sqlite3_stmt *stmt = nullptr;
        ok = sqlite3_prepare_v2(m_db, "DELETE FROM blocks WHERE first_line = ?",
                                -1, &stmt, nullptr)
             == SQLITE_OK;
        for (qsizetype b = 0; b < count && ok; ++b) {
            sqlite3_reset(stmt);
            sqlite3_bind_int64(stmt, 1, sqlite3_int64(m_blocks[b].firstLine));
            ok = sqlite3_step(stmt) == SQLITE_DONE;
        }
        if (stmt)
            sqlite3_finalize(stmt);
    }

    // 3) base 前移（与删除同事务）；全部删光时 base = frontier
    const quint64 newBase =
        count < m_blocks.size() ? m_blocks[count].firstLine : m_frontier;
    if (ok) {
        sqlite3_stmt *stmt = nullptr;
        ok = sqlite3_prepare_v2(m_db, "UPDATE meta SET value = ? WHERE key = 'base'",
                                -1, &stmt, nullptr)
             == SQLITE_OK;
        if (ok) {
            sqlite3_bind_int64(stmt, 1, sqlite3_int64(newBase));
            ok = sqlite3_step(stmt) == SQLITE_DONE;
            sqlite3_finalize(stmt);
        }
    }

    if (!ok) {
        execSql("ROLLBACK", nullptr);
        if (errorString)
            *errorString = QStringLiteral("冷层清理失败：%1")
                               .arg(QString::fromUtf8(sqlite3_errmsg(m_db)));
        return false;
    }
    if (!execSql("COMMIT", errorString))
        return false;
    execSql("PRAGMA incremental_vacuum", nullptr); // 回收空闲页（规格 §七）

    m_blocks.remove(0, count);
    m_baseLine = newBase;
    m_blockCache.clear(); // 删除是稀有事件，整体失效最简单
    return true;
}
```

注意：`appendBlock` 末尾已有 `locker.unlock(); enforceLimits();`（任务 3 写入），本任务实现补齐后自动触发路径即生效。

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzColdStorageTest
```

预期：16 个用例全部 Passed。

- [ ] **步骤 5：Commit**

```bash
git add src/log/ZzColdStorage.h src/log/ZzColdStorage.cpp tests/log/ZzColdStorageTest.cpp
git commit -m "feat(log): 实现冷层自动清理与 baseLine 前移" -m "- enforceLimits：超龄（end_ts_ns 早于 maxAgeDays 前，块时间戳非递减故超龄块必为最老前缀）+ 超容量（page_count×page_size 超 maxBytes，按最老块每批 64 块批删）
- deleteOldestBlocks：FTS5 contentless 删除走 'delete' 特殊命令并提供原文（先解压待删块取回），块表删除与 meta.base 前移同事务，提交后增量 VACUUM
- appendBlock 提交后自动触发一次清理（规格 §七）；baseLine/frontier 跨重开持久
- 测试覆盖超龄清理、超容量清理、自动触发、清理后重开老行不复活、FTS 同步删除"
```

---

### 任务 6：ZzMmapBuffer::setRetentionFloor 保留下限（TDD）

**文件：**
- 修改：`src/log/ZzMmapBuffer.h`（新增公开方法与私有成员）
- 修改：`src/log/ZzMmapBuffer.cpp`（头部读写 coldCursor、dropOldestBlocks 分支、新方法实现）
- 修改：`tests/log/ZzMmapBufferTest.cpp`（追加用例）

行为契约：设置 floor 后，`dropOldestBlocks` 只丢弃**整体行号区间 ≤ floor**（即 `lineStart + lineCount <= floor`）的最老块，跨界块保留；未设置 floor 时保持 v0.1 纯 maxLines 丢弃行为不变。floor 同时持久化到温层文件头偏移 12 处（`coldCursor u64`），作为异常退出后冷层续传的游标（前置说明第 4/5 条）。

- [ ] **步骤 1：编写失败的测试**

在 `tests/log/ZzMmapBufferTest.cpp` 的 `private slots:` 区追加以下用例（该文件已有 `line()` 辅助与 QTemporaryDir 用法，沿用既有风格）：

```cpp
    /// @brief 保留下限：只丢弃整体行号 ≤ floor 的最老块，跨界块保留；游标可读回。
    void retentionFloorDropsOnlyCoveredBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")), 1000000); // 大 maxLines 排除 v0.1 丢弃
        QVERIFY(buf.open());
        // 每行约 1KB，单块（64KB）约 60 行；写 300 行形成多块
        const QString payload(1000, QLatin1Char('x'));
        QVector<ZzLogLine> batch;
        batch.reserve(300);
        for (int i = 0; i < 300; ++i)
            batch.append({payload + QString::number(i), QByteArray()});
        QVERIFY(buf.appendLines(batch));
        QCOMPARE(buf.firstLineId(), 0ULL);
        QCOMPARE(buf.lineCount(), 300ULL);
        QCOMPARE(buf.coldCursor(), 0ULL); // 未设置 floor 时游标为 0

        buf.setRetentionFloor(120); // 行 0..119 已被冷层覆盖
        const quint64 first = buf.firstLineId();
        QVERIFY(first > 0ULL);    // 至少丢了一块
        QVERIFY(first <= 120ULL); // 跨界块必须保留：首存活块覆盖 floor
        QCOMPARE(buf.lineCount(), 300ULL - first);
        QCOMPARE(buf.coldCursor(), 120ULL);
        // 幸存行内容正确
        QCOMPARE(buf.readLines(first, 1).first().text,
                 payload + QString::number(qint64(first)));
        QCOMPARE(buf.readLines(299, 1).first().text, payload + QStringLiteral("299"));
    }

    /// @brief floor 游标与丢弃进度跨重开持久（崩溃恢复依据）。
    void retentionFloorPersistsAcrossReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        const QString payload(1000, QLatin1Char('x'));
        quint64 firstBeforeClose = 0;
        {
            ZzMmapBuffer buf(path, 1000000);
            QVERIFY(buf.open());
            QVector<ZzLogLine> batch;
            for (int i = 0; i < 300; ++i)
                batch.append({payload + QString::number(i), QByteArray()});
            QVERIFY(buf.appendLines(batch));
            buf.setRetentionFloor(120);
            firstBeforeClose = buf.firstLineId();
            buf.flush();
            buf.close();
        }
        ZzMmapBuffer buf(path, 1000000);
        QVERIFY(buf.open());
        QCOMPARE(buf.coldCursor(), 120ULL);           // 游标持久
        QCOMPARE(buf.firstLineId(), firstBeforeClose); // 已丢块不复活（skipBlocks 持久）
        QVERIFY(buf.readLines(0, 1).first().text
                == payload + QString::number(qint64(firstBeforeClose))); // 越界夹取到幸存首行
    }

    /// @brief 清除 floor 后恢复 v0.1 纯 maxLines 丢弃（冷层降级路径）。
    void clearRetentionFloorRestoresV01Dropping()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString payload(1000, QLatin1Char('x'));
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")), 150); // maxLines 150
        QVERIFY(buf.open());
        QVector<ZzLogLine> batch;
        for (int i = 0; i < 300; ++i)
            batch.append({payload + QString::number(i), QByteArray()});
        QVERIFY(buf.appendLines(batch)); // 未设 floor：v0.1 丢弃，lineCount ≤ 150
        QVERIFY(buf.lineCount() <= 150ULL);
        const quint64 firstAfterV01 = buf.firstLineId();

        buf.setRetentionFloor(firstAfterV01 + 30); // floor 模式：再丢一些整块
        const quint64 firstWithFloor = buf.firstLineId();
        QVERIFY(firstWithFloor >= firstAfterV01);
        buf.clearRetentionFloor(); // 降级：回到 v0.1

        QVector<ZzLogLine> more;
        for (int i = 300; i < 420; ++i)
            more.append({payload + QString::number(i), QByteArray()});
        QVERIFY(buf.appendLines(more)); // 触发 v0.1 maxLines 丢弃
        QVERIFY(buf.lineCount() <= 150ULL);
        QVERIFY(buf.firstLineId() > firstWithFloor); // 按 maxLines 继续前移
    }
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build build/debug -j8
```

预期：编译失败，报 `error: 'class ZzMmapBuffer' has no member named 'setRetentionFloor'` 等。

- [ ] **步骤 3：编写实现代码**

修改 `src/log/ZzMmapBuffer.h`：在 `void flush();` 声明之后追加公开方法声明；在私有成员 `qint64 m_droppedBytes = 0;` 之后追加三个成员。追加的公开声明：

```cpp
    /**
     * @brief 设置保留下限（冷层模式）：dropOldestBlocks 只丢弃整体行号区间
     *        （lineStart + lineCount <= firstDisposableLineId）的最老块，跨界块保留。
     * @param firstDisposableLineId 第一个可被丢弃的行 ID == 冷层已覆盖行数上界（本文件局部行号空间）。
     * @note 立即生效（调用 dropOldestBlocks），并把该游标持久化到文件头偏移 12 处
     *       （coldCursor，异常退出后冷层续传依据）；未设置 floor 时保持 v0.1 纯 maxLines 丢弃。
     */
    void setRetentionFloor(quint64 firstDisposableLineId);

    /// @brief 清除保留下限，恢复 v0.1 纯 maxLines 丢弃（冷层降级时由归档线程调用）。
    void clearRetentionFloor();

    /// @brief 冷层续传游标（文件头持久化；未启用冷层的文件读出 0）。
    quint64 coldCursor() const { return m_coldCursor; }
```

追加的私有成员：

```cpp
    quint64 m_retentionFloor = 0;      ///< 保留下限（m_hasRetentionFloor 为 true 时有效）
    bool m_hasRetentionFloor = false;  ///< 是否处于冷层保留下限模式
    quint64 m_coldCursor = 0;          ///< 文件头持久化的冷层续传游标（== 最近一次 floor）
```

修改 `src/log/ZzMmapBuffer.cpp`：

1) `writeHeader()` 增加游标持久化，改后完整函数：

```cpp
void ZzMmapBuffer::writeHeader()
{
    char *base = reinterpret_cast<char *>(m_map);
    putU32(base, kFileMagic);
    putU32(base + 4, kFileVersion);
    putU32(base + 8, m_skipBlocks);
    putU64(base + 12, m_coldCursor); // v0.2：冷层续传游标（v0.1 文件此处为零填充，读出 0）
}
```

2) `open()` 重开既有文件分支读取游标（锚点：`m_skipBlocks = getU32(base + 8);` 一行之后）追加：

```cpp
        m_coldCursor = getU64(base + 12); // v0.1 文件读出 0 == 无续传进度
```

3) `dropOldestBlocks()` 改为按模式分支，改后完整函数：

```cpp
void ZzMmapBuffer::dropOldestBlocks()
{
    if (m_hasRetentionFloor) {
        // 冷层模式：只丢弃冷层已完整覆盖的最老块（跨界块保留），容量水位由
        // 冷层持续推进保证；至少保留一块（与 v0.1 一致的空防护）
        while (m_blocks.size() > 1
               && m_blocks.first().lineStart + m_blocks.first().lineCount
                      <= m_retentionFloor) {
            const BlockInfo oldest = m_blocks.first();
            m_lineCount -= oldest.lineCount;
            m_droppedBytes += kBlockHeaderSize + oldest.compSize;
            ++m_skipBlocks;
            m_blocks.removeFirst();
        }
    } else {
        // v0.1 行为：超 maxLines 按整块粒度丢弃最老数据，至少保留一块
        while (m_lineCount > m_maxLines && m_blocks.size() > 1) {
            const BlockInfo oldest = m_blocks.first();
            m_lineCount -= oldest.lineCount;
            m_droppedBytes += kBlockHeaderSize + oldest.compSize;
            ++m_skipBlocks;
            m_blocks.removeFirst();
        }
    }
    if (m_skipBlocks > 0)
        writeHeader(); // 持久化丢弃进度，重开后最老行不复活
    // 浪费空间超过已用一半且文件超过一个扩容粒度时，物理压缩文件
    if (m_droppedBytes > m_appendOffset / 2 && m_appendOffset > kGrowGranularity)
        compact();
}
```

4) 文件末尾追加两个新方法实现：

```cpp
void ZzMmapBuffer::setRetentionFloor(quint64 firstDisposableLineId)
{
    m_hasRetentionFloor = true;
    m_retentionFloor = firstDisposableLineId;
    if (m_coldCursor != firstDisposableLineId) {
        m_coldCursor = firstDisposableLineId;
        writeHeader(); // 持久化续传游标（崩溃恢复依据；一页 mmap 写，代价可忽略）
    }
    dropOldestBlocks(); // 立即按新 floor 截头
}

void ZzMmapBuffer::clearRetentionFloor()
{
    m_hasRetentionFloor = false;
    m_retentionFloor = 0;
    dropOldestBlocks(); // 回到 v0.1：若已超 maxLines 立即按容量丢弃
}
```

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R ZzMmapBufferTest
```

预期：既有 8 个用例（含 `capacityDropsOldestBlocks`/`dropSurvivesReopen`，验证未设 floor 时 v0.1 行为不变）+ 新增 3 个用例全部 Passed。

- [ ] **步骤 5：Commit**

```bash
git add src/log/ZzMmapBuffer.h src/log/ZzMmapBuffer.cpp tests/log/ZzMmapBufferTest.cpp
git commit -m "feat(log): 温层新增保留下限 setRetentionFloor" -m "- setRetentionFloor：冷层模式下 dropOldestBlocks 只丢弃整体行号 ≤ floor 的最老整块，跨界块保留；未设 floor 时 v0.1 纯 maxLines 丢弃行为不变
- clearRetentionFloor：冷层降级时恢复 v0.1 容量丢弃（否则温层永不截头会无限增长）
- floor 持久化到文件头偏移 12 处（coldCursor u64，复用 v0.1 头部零填充保留区，旧文件读出 0 无需迁移），作为异常退出后冷层续传游标
- 测试覆盖：floor 整块粒度丢弃与跨界保留、游标与丢弃进度跨重开持久、清除 floor 后 v0.1 丢弃恢复"
```

---

### 任务 7：ZzLogArchiveWorker 温→冷推进与冷层降级（TDD）

**文件：**
- 修改：`src/log/ZzLogArchiveWorker.h`（整体替换）
- 修改：`src/log/ZzLogArchiveWorker.cpp`（整体替换）
- 创建：`tests/log/ZzLogArchiveWorkerTest.cpp`
- 修改：`tests/log/CMakeLists.txt`（注册测试）

设计要点：构造新参数（`cold`/`coldBase`/`coldFrontier`）带默认值 `nullptr`，保证既有调用点（`ZzLogEngine::open`）在本任务不改动也能编译，任务 8 再接线；`coldAdvance` 是私有方法（由 `archiveLines`/`flush` 内部直接调用，不经事件队列，已在归档线程上无需再排队）；崩溃恢复续传在 `ZzLogEngine::open()` 同步执行（前置说明第 4 条），属任务 8。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/log/ZzLogArchiveWorkerTest.cpp`（直接构造 worker 于栈上并同步调用槽——槽函数本身是普通成员函数语义，无需起线程；信号同步发射，QSignalSpy 直接连接即可捕获）：

```cpp
#include "ZzLogArchiveWorker.h"

#include "ZzColdStorage.h"
#include "ZzMmapBuffer.h"

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

#include <atomic>

/**
 * @brief ZzLogArchiveWorker 温→冷推进单元测试：整块推进、尾批冲刷、冷层失败降级。
 *
 * 行号说明：worker 的 m_coldCursor 与温层一致使用温层局部行号空间；
 * 写入冷层时 firstLine 取 cold->frontier()（库内全局空间）。本测试单写入者，
 * 两个空间数值相同（偏移 0）。
 */
class ZzLogArchiveWorkerTest : public QObject
{
    Q_OBJECT
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("worker-line-%1").arg(i), QByteArray()};
    }
    static QVector<ZzLogLine> makeLines(quint64 start, quint64 count)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(count));
        for (quint64 i = 0; i < count; ++i)
            out.append(line(start + i));
        return out;
    }

private slots:
    /// @brief 温层批次写完后 coldAdvance 只推进整块（1024 行）：3348 行 → 冷层 3072 行，
    ///        温层保留下限生效且跨界块保留。
    void archiveAdvancesColdInFullBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 1000000);
        QVERIFY(warm.open());
        ZzColdStorage::Config cc;
        cc.dbPath = dir.filePath(QStringLiteral("cold.db"));
        cc.sessionId = QStringLiteral("test-session");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0}, coldBase{0}, coldFrontier{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount,
                                  &cold, &coldBase, &coldFrontier);

        worker.archiveLines(makeLines(0, 2048));
        worker.archiveLines(makeLines(2048, 1300)); // 温层共 3348 行

        QCOMPARE(cold.frontier(), 3072ULL);          // 只推进了 3 个整块
        QCOMPARE(coldFrontier.load(), 3072ULL);      // 发布位同步
        QCOMPARE(coldBase.load(), 0ULL);
        // floor=3072 截头：整体行号 ≤3072 的温层整块被丢弃，跨界块保留
        QVERIFY(warm.firstLineId() <= 3072ULL);
        QVERIFY(warm.firstLineId() + warm.lineCount() > 3072ULL);
        // 冷层读回与写入一致
        const QVector<ZzLogLine> got = cold.readLines(1020, 8); // 跨块 0/1 边界
        QCOMPARE(got.size(), 8);
        QCOMPARE(got.first().text, line(1020).text);
        QCOMPARE(got.last().text, line(1027).text);
    }

    /// @brief flush() 冲刷不足一块的尾批进冷层（干净退出同步点）。
    void flushArchivesPartialTail()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 1000000);
        QVERIFY(warm.open());
        ZzColdStorage::Config cc;
        cc.dbPath = dir.filePath(QStringLiteral("cold.db"));
        cc.sessionId = QStringLiteral("test-session");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0}, coldBase{0}, coldFrontier{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount,
                                  &cold, &coldBase, &coldFrontier);

        worker.archiveLines(makeLines(0, 3348));
        QCOMPARE(cold.frontier(), 3072ULL); // 尾批 276 行未进冷层
        worker.flush();
        QCOMPARE(cold.frontier(), 3348ULL); // flush 后尾批已进冷层
        QCOMPARE(coldFrontier.load(), 3348ULL);
        QCOMPARE(cold.readLines(3347, 1).first().text, line(3347).text);
        QCOMPARE(warm.coldCursor(), 3348ULL); // 游标持久化到温层头
    }

    /// @brief 冷层写入失败：重试 3 次后发射 coldFailed（门闩只发一次），
    ///        温层清除 floor 回到 v0.1 maxLines 丢弃。
    void coldFailureEmitsColdFailedAndRevertsWarmDropping()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        // 短行单块约 1700 行；maxLines 2000 → v0.1 恢复后超限整块丢弃可观察
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 2000);
        QVERIFY(warm.open());
        ZzColdStorage::Config cc;
        cc.dbPath = dir.filePath(QStringLiteral("cold.db"));
        cc.sessionId = QStringLiteral("test-session");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0}, coldBase{0}, coldFrontier{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount,
                                  &cold, &coldBase, &coldFrontier);
        QSignalSpy coldFailedSpy(&worker, &ZzLogArchiveWorker::coldFailed);
        QSignalSpy archiveOkSpy(&worker, &ZzLogArchiveWorker::archiveCompleted);

        cold.close(); // 制造冷层写失败（appendBlock 立即失败）
        worker.archiveLines(makeLines(0, 1500));
        QCOMPARE(archiveOkSpy.count(), 1);    // 温层写入成功
        QCOMPARE(coldFailedSpy.count(), 1);   // 重试 3 次后降级
        QVERIFY(!coldFailedSpy.first().first().toString().isEmpty());

        worker.archiveLines(makeLines(1500, 1500)); // 门闩：不再重复发射
        QCOMPARE(coldFailedSpy.count(), 1);

        // floor 已清除：继续写超 maxLines 后 v0.1 容量丢弃生效
        worker.archiveLines(makeLines(3000, 1500));
        QVERIFY(warm.lineCount() <= 2000ULL + 1700ULL); // 至多一个块的余量
        QVERIFY(warm.firstLineId() > 0ULL);
    }

    /// @brief 无冷层（cold == nullptr）：纯 v0.1 行为，archiveLines 不触碰冷层路径。
    void nullColdKeepsV01Behavior()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer warm(dir.filePath(QStringLiteral("warm.log")), 1000000);
        QVERIFY(warm.open());
        QReadWriteLock lock;
        std::atomic<quint64> warmBase{0}, warmCount{0};
        ZzLogArchiveWorker worker(&warm, &lock, &warmBase, &warmCount); // 不传冷层
        QSignalSpy coldFailedSpy(&worker, &ZzLogArchiveWorker::coldFailed);
        worker.archiveLines(makeLines(0, 3000));
        worker.flush();
        QCOMPARE(warmCount.load(), 3000ULL);
        QCOMPARE(coldFailedSpy.count(), 0);
        QCOMPARE(warm.coldCursor(), 0ULL); // floor 从未设置
    }
};

QTEST_GUILESS_MAIN(ZzLogArchiveWorkerTest)
#include "ZzLogArchiveWorkerTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加一行：

```cmake
zz_add_log_test(ZzLogArchiveWorkerTest)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
```

预期：编译失败，报 `error: no matching constructor for ZzLogArchiveWorker`（新参数不存在）与 `'coldCursor' is not a member` 之外的 `coldFailed` 未定义等。

- [ ] **步骤 3：编写实现代码**

整体替换 `src/log/ZzLogArchiveWorker.h`：

```cpp
#pragma once

#include "ZzLogLine.h"

#include <QObject>
#include <QReadWriteLock>
#include <QVector>

#include <atomic>

class ZzColdStorage;
class ZzMmapBuffer;

/**
 * @brief 温层归档工作对象：运行在 ZzLogEngine 拥有的独立 QThread 中。
 *
 * 负责把热层驱逐出的批量行写入温层（绝不阻塞 I/O 线程与 UI 线程，规格 §5.3），
 * 并执行滚动方向上的块预加载；通过传入的原子计数器向读路径发布温层区间。
 *
 * v0.2 扩展（规格 §六）：温层批次写完后追加 coldAdvance——从温层读
 * [m_coldCursor, warmEnd) 凑满 1024 行一批写入冷层，提交后前移温层保留下限；
 * 冷层写入失败重试 3 次后发射 coldFailed 并清除温层保留下限（回到 v0.1 容量丢弃）。
 */
class ZzLogArchiveWorker : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 构造工作对象。
     * @param buffer 温层存储（由 ZzLogEngine 持有，生命周期长于本对象）。
     * @param lock 温层读写锁（归档持写锁、读路径持读锁）。
     * @param warmBase 温层首行 ID 发布位。
     * @param warmCount 温层行数发布位。
     * @param cold 冷层存储（可空；空 = v0.1 无冷层行为）。
     * @param coldBase 冷层最老可读行发布位（库内全局空间原始值，由读路径自行减偏移；cold 为空时忽略）。
     * @param coldFrontier 本会话已落冷层行数上界发布位（引擎空间 == m_coldCursor；cold 为空时忽略）。
     */
    ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                       std::atomic<quint64> *warmBase, std::atomic<quint64> *warmCount,
                       ZzColdStorage *cold = nullptr,
                       std::atomic<quint64> *coldBase = nullptr,
                       std::atomic<quint64> *coldFrontier = nullptr,
                       QObject *parent = nullptr);

public slots:
    /// @brief 归档一批行到温层；失败发射 archiveFailed；成功后追加 coldAdvance。
    void archiveLines(const QVector<ZzLogLine> &lines);

    /// @brief 预加载 lineId 所在温层块及后一块到解压缓存。
    void preloadAround(quint64 lineId);

    /// @brief 预解压 globalLineId（库内全局空间）所在冷层块及后一块进 LRU。
    void preloadCold(quint64 globalLineId);

    /// @brief 冲刷温层文件缓冲，并把不足一块的尾批也推进冷层（测试与关闭前的确定性同步点）。
    void flush();

signals:
    void archiveCompleted();                    ///< 一批行完成归档
    void archiveFailed(const QString &message); ///< 温层 I/O 失败
    void coldFailed(const QString &message);    ///< 冷层写入重试 3 次仍失败（降级依据）

private:
    /**
     * @brief 温→冷推进：把 [m_coldCursor, warmEnd) 按块写入冷层并前移温层保留下限。
     * @param includePartial true 时不足一块的尾批也写入（flush/干净退出用）。
     * @note 仅在本对象所在线程由 archiveLines/flush 直接调用，不经事件队列。
     */
    void coldAdvance(bool includePartial);

    ZzMmapBuffer *m_buffer;
    QReadWriteLock *m_lock;
    std::atomic<quint64> *m_warmBase;
    std::atomic<quint64> *m_warmCount;
    ZzColdStorage *m_cold;            ///< 可空
    std::atomic<quint64> *m_coldBase; ///< 随 m_cold 一并提供
    std::atomic<quint64> *m_coldFrontier;
    quint64 m_coldCursor = 0;         ///< 本会话已落入冷层的行数（温层局部行号空间）
    bool m_coldFailed = false;        ///< 冷层降级门闩（只发射一次 coldFailed）
};
```

整体替换 `src/log/ZzLogArchiveWorker.cpp`：

```cpp
#include "ZzLogArchiveWorker.h"

#include "ZzColdStorage.h"
#include "ZzMmapBuffer.h"

ZzLogArchiveWorker::ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                                       std::atomic<quint64> *warmBase,
                                       std::atomic<quint64> *warmCount,
                                       ZzColdStorage *cold,
                                       std::atomic<quint64> *coldBase,
                                       std::atomic<quint64> *coldFrontier, QObject *parent)
    : QObject(parent)
    , m_buffer(buffer)
    , m_lock(lock)
    , m_warmBase(warmBase)
    , m_warmCount(warmCount)
    , m_cold(cold)
    , m_coldBase(coldBase)
    , m_coldFrontier(coldFrontier)
{
}

void ZzLogArchiveWorker::archiveLines(const QVector<ZzLogLine> &lines)
{
    QString error;
    {
        QWriteLocker locker(m_lock);
        if (!m_buffer->appendLines(lines, &error)) {
            emit archiveFailed(error);
            return;
        }
        m_warmBase->store(m_buffer->firstLineId());
        m_warmCount->store(m_buffer->lineCount());
    }
    emit archiveCompleted();
    coldAdvance(false); // 温层批次写完后推进冷层（规格 §六；本线程直接调用）
}

void ZzLogArchiveWorker::coldAdvance(bool includePartial)
{
    if (!m_cold || m_coldFailed || !m_cold->isOpen())
        return;
    for (;;) {
        const quint64 warmEnd = m_warmBase->load() + m_warmCount->load();
        const quint64 available = warmEnd - m_coldCursor;
        if (available < ZzColdStorage::kMaxBlockLines
            && !(includePartial && available > 0))
            break;
        const quint64 batch = qMin(available, ZzColdStorage::kMaxBlockLines);
        QVector<ZzLogLine> lines;
        {
            QReadLocker locker(m_lock);
            lines = m_buffer->readLines(m_coldCursor, batch);
        }
        if (quint64(lines.size()) != batch)
            break; // 温层读回不完整（块损坏）：留待下一批，避免写入错位
        QString error;
        bool ok = false;
        for (int attempt = 0; attempt < 3 && !ok; ++attempt)
            ok = m_cold->appendBlock(lines, m_cold->frontier(), &error); // 库内全局空间连续追加
        if (!ok) {
            m_coldFailed = true;
            {
                QWriteLocker locker(m_lock);
                m_buffer->clearRetentionFloor(); // 回到 v0.1 容量丢弃（规格 §六失败隔离）
            }
            emit coldFailed(QStringLiteral("冷层写入失败（已重试 3 次）：%1").arg(error));
            return;
        }
        m_coldCursor += batch;
        m_coldFrontier->store(m_coldCursor); // 引擎空间：本会话覆盖上界（读路径按此行号路由）
        m_coldBase->store(m_cold->baseLine()); // 库内全局空间原始值（读路径减 m_coldOffset 夹取）
        {
            QWriteLocker locker(m_lock);
            m_buffer->setRetentionFloor(m_coldCursor); // 温层截头 + 游标持久化
        }
    }
}

void ZzLogArchiveWorker::preloadAround(quint64 lineId)
{
    // 持写锁而非读锁：preload 会写解压缓存（QCache），须与调用线程的读路径互斥
    QWriteLocker locker(m_lock);
    m_buffer->preload(lineId);
}

void ZzLogArchiveWorker::preloadCold(quint64 globalLineId)
{
    if (m_cold && !m_coldFailed)
        m_cold->preload(globalLineId); // 冷层自带互斥锁，不占用温层读写锁
}

void ZzLogArchiveWorker::flush()
{
    {
        QWriteLocker locker(m_lock);
        m_buffer->flush();
    }
    coldAdvance(true); // 干净退出同步点：不足一块的尾批也写入冷层
}
```

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
ctest --test-dir build/debug --output-on-failure -R 'Zz(LogArchiveWorker|ColdStorage|LogEngine)Test'
```

预期：ZzLogArchiveWorkerTest 4 个用例、ZzColdStorageTest 16 个用例、ZzLogEngineTest 既有用例（worker 构造新参数带默认值，ZzLogEngine 未改动仍可编译运行，纯 v0.1 行为）全部 Passed。

- [ ] **步骤 5：Commit**

```bash
git add src/log/ZzLogArchiveWorker.h src/log/ZzLogArchiveWorker.cpp \
        tests/log/ZzLogArchiveWorkerTest.cpp tests/log/CMakeLists.txt
git commit -m "feat(log): 归档线程新增温→冷两阶段推进" -m "- archiveLines 写完温层后同线程追加 coldAdvance(false)：读 [m_coldCursor, warmEnd) 凑满 1024 行一批写冷层（firstLine 取 cold->frontier()，库内全局空间连续），提交后发布 coldBase/coldFrontier 并 setRetentionFloor 截头
- flush() 追加 coldAdvance(true)：不足一块的尾批也写入冷层（干净退出同步点）
- 冷层写入失败重试 3 次后置降级门闩：发射 coldFailed（只发一次）+ clearRetentionFloor 回到 v0.1 容量丢弃
- 新增 preloadCold 槽（冷层预解压，不占用温层读写锁）；构造新参数带默认值 nullptr，既有调用点零改动
- 测试直接构造 worker 同步调槽：整块推进/尾批冲刷/失败降级门闩/无冷层纯 v0.1"
```

---

### 任务 8：ZzLogEngine 三层归并 + Config 扩展 + 崩溃恢复 + 干净退出删温层（TDD）

**文件：**
- 修改：`src/log/ZzLogEngine.h`（整体替换）
- 修改：`src/log/ZzLogEngine.cpp`（整体替换）
- 修改：`tests/log/ZzLogEngineTest.cpp`（追加冷层用例）

设计要点：
- `m_coldOffset` = open() 时（崩溃恢复续传完成之后）冷层全局 frontier，引擎行号 `e` ↔ 库内行号 `g = e + m_coldOffset`（前置说明第 3 条）。
- 两个发布位的空间约定不同，成员注释逐字写明：`m_coldBase` 为**库内全局空间**（清理前移的原始值），`m_coldFrontier` 为**引擎空间**（== worker 的 `m_coldCursor`，本会话已落冷层行数上界）。
- 崩溃恢复在 open() 同步执行（前置说明第 4 条）；干净退出时 flush 后若冷层启用且未降级则删除温层文件；冷层降级后**不得**删除温层文件（规格 §六）。

- [ ] **步骤 1：编写失败的测试**

在 `tests/log/ZzLogEngineTest.cpp` 顶部 include 区追加 `#include "ZzColdStorage.h"` 与 `#include "ZzMmapBuffer.h"`（崩溃恢复/降级用例直接构造温层与冷层），在类静态辅助区追加：

```cpp
    static ZzLogEngine::Config testColdConfig(const QString &warmPath, const QString &dbPath)
    {
        ZzLogEngine::Config c = testConfig(warmPath); // 热 100 / 批 16 / 温 10 万
        c.coldDbPath = dbPath;
        c.sessionId = QStringLiteral("test-profile");
        return c;
    }
```

在 `private slots:` 区追加以下用例：

```cpp
    /// @brief 冷层禁用（coldDbPath 为空）：isColdEnabled 为 false，行为与 v0.1 完全一致。
    void coldDisabledKeepsV01Behavior()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        QVERIFY(!engine.isColdEnabled());
        QVERIFY(!engine.isMemoryOnly());
        for (quint64 i = 0; i < 200; ++i)
            engine.appendLine(line(i));
        engine.flush();
        QCOMPARE(engine.totalLines(), 200ULL);
        QVERIFY(engine.searchLines(QStringLiteral("engine-line")).isEmpty()); // 无冷层 → 空
    }

    /// @brief 三层归并：2500 行（2400 冷层 + 100 热层）滑动窗口与写入序列逐行一致，
    ///        跨冷/温/热边界无错位。
    void coldRoundtripThreeLayerMerge()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testColdConfig(dir.filePath(QStringLiteral("warm.log")),
                                          dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(engine.open());
        QVERIFY(engine.isColdEnabled());
        constexpr quint64 N = 2500; // 驱逐 150 批 × 16 = 2400 行入温层→flush 后全入冷层；100 行留热层
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush();
        QCOMPARE(engine.totalLines(), N);
        QCOMPARE(engine.firstLineNo(), 0ULL);

        for (quint64 start = 0; start + 60 <= N; start += 17) {
            const QVector<ZzLogLine> window = engine.getLines(start, 60);
            QCOMPARE(window.size(), 60);
            for (int j = 0; j < 60; ++j)
                QCOMPARE(window[j].text, line(start + quint64(j)).text);
        }
        // 冷层数据已由冷层承载（库内验证在 cleanExitDeletesWarmFile 用例做）
    }

    /// @brief 干净退出：flush 后析构删除温层文件，全部已归档行持久于冷层库。
    void cleanExitDeletesWarmFileAndPersistsCold()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString warmPath = dir.filePath(QStringLiteral("warm.log"));
        const QString dbPath = dir.filePath(QStringLiteral("cold.db"));
        {
            ZzLogEngine engine(testColdConfig(warmPath, dbPath));
            QVERIFY(engine.open());
            for (quint64 i = 0; i < 2500; ++i)
                engine.appendLine(line(i));
            engine.flush();
        } // 析构：冷层健康 → 删除温层文件
        QVERIFY(!QFile::exists(warmPath));
        QVERIFY(QFile::exists(dbPath));

        // 直接打开冷层库验证（首个会话 offset 为 0，库内行号 == 引擎行号）
        ZzColdStorage::Config cc;
        cc.dbPath = dbPath;
        cc.sessionId = QStringLiteral("test-profile");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QCOMPARE(cold.frontier(), 2400ULL); // flush 把温层 2400 行全部推进冷层
        const QVector<ZzLogLine> head = cold.readLines(0, 3);
        QCOMPARE(head.size(), 3);
        QCOMPARE(head[0].text, line(0).text);
        QCOMPARE(head[2].text, line(2).text);
    }

    /// @brief 崩溃恢复：残留温层（游标 500，共 1500 行）+ 冷层已归档 500 行；
    ///        引擎 open 同步续传 [500,1500) 进冷层后删除残留、全新开始；
    ///        续传的行属上一会话，对本会话不可见但持久可查。
    void crashRecoveryResumesResidualWarm()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString warmPath = dir.filePath(QStringLiteral("warm.log"));
        const QString dbPath = dir.filePath(QStringLiteral("cold.db"));
        // 构造"崩溃现场"：温层 1500 行、游标 500；冷层已有 [0,500)
        {
            ZzMmapBuffer warm(warmPath, 100000);
            QVERIFY(warm.open());
            for (quint64 base = 0; base < 1500; base += 500)
                QVERIFY(warm.appendLines(makeLines(base, 500)));
            warm.setRetentionFloor(500); // 持久化续传游标（短行单块覆盖 1500 行，不丢块）
            QCOMPARE(warm.firstLineId(), 0ULL);
            warm.flush();
            warm.close();
            ZzColdStorage::Config cc;
            cc.dbPath = dbPath;
            cc.sessionId = QStringLiteral("test-profile");
            ZzColdStorage cold(cc);
            QVERIFY(cold.open());
            QVERIFY(cold.appendBlock(makeLines(0, 500), 0));
        }
        // 引擎 open：同步续传 [500,1500) → 删除残留 → 全新温层
        ZzLogEngine engine(testColdConfig(warmPath, dbPath));
        QSignalSpy warmOnlySpy(&engine, &ZzLogEngine::degradedToWarmOnly);
        QVERIFY(engine.open());
        QCOMPARE(warmOnlySpy.count(), 0);
        QVERIFY(engine.isColdEnabled());
        QCOMPARE(engine.totalLines(), 0ULL); // 新会话从空开始（残留行已入冷层，不属于本会话）
        QCOMPARE(engine.firstLineNo(), 0ULL);

        // 冷层库内验证：1500 行全部在库且内容正确
        ZzColdStorage::Config cc;
        cc.dbPath = dbPath;
        cc.sessionId = QStringLiteral("test-profile");
        ZzColdStorage cold(cc);
        QVERIFY(cold.open());
        QCOMPARE(cold.frontier(), 1500ULL);
        QCOMPARE(cold.readLines(499, 3).first().text, line(499).text);  // 续传边界
        QCOMPARE(cold.readLines(1499, 1).first().text, line(1499).text);

        // 新会话写入的行接续全局空间；searchLines 返回引擎空间行号
        for (quint64 i = 0; i < 200; ++i) {
            ZzLogLine l = line(i);
            if (i == 10)
                l.text = QStringLiteral("post-recovery FATALMARK line");
            engine.appendLine(l);
        }
        engine.flush(); // 驱逐 7 批 × 16 = 112 行入冷层（全局 [1500,1612)）
        QCOMPARE(engine.searchLines(QStringLiteral("FATALMARK")),
                 (QVector<quint64>{10ULL})); // 引擎空间：本会话第 10 行
    }

    /// @brief 冷层库打开失败：发射 degradedToWarmOnly 降级为 v0.1 温层模式；
    ///        残留温层无续传去向被删除全新开始；降级后析构不得删除温层文件。
    void coldOpenFailureDegradesToWarmOnly()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString warmPath = dir.filePath(QStringLiteral("warm.log"));
        // 预置残留温层（100 行）
        {
            ZzMmapBuffer warm(warmPath, 100000);
            QVERIFY(warm.open());
            QVERIFY(warm.appendLines(makeLines(0, 100)));
            warm.flush();
            warm.close();
        }
        // 用一个已存在的文件当“父目录”，其下 cold.db 必然打不开（跨平台）
        const QString blocker = dir.filePath(QStringLiteral("blocker"));
        QFile f(blocker);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();

        ZzLogEngine::Config c = testConfig(warmPath);
        c.coldDbPath = blocker + QStringLiteral("/cold.db");
        c.sessionId = QStringLiteral("test-profile");
        {
            ZzLogEngine engine(c);
            QSignalSpy warmOnlySpy(&engine, &ZzLogEngine::degradedToWarmOnly);
            QSignalSpy memoryOnlySpy(&engine, &ZzLogEngine::degradedToMemoryOnly);
            QVERIFY(engine.open());
            QCOMPARE(warmOnlySpy.count(), 1);
            QCOMPARE(memoryOnlySpy.count(), 0);
            QVERIFY(!engine.isColdEnabled());
            QVERIFY(!engine.isMemoryOnly());
            QCOMPARE(engine.totalLines(), 0ULL); // 残留温层已删除，全新开始
            for (quint64 i = 0; i < 200; ++i)
                engine.appendLine(line(i));
            engine.flush();
            QCOMPARE(engine.totalLines(), 200ULL); // v0.1 温层路径正常
        }
        QVERIFY(QFile::exists(warmPath)); // 冷层降级后不得删除温层文件（规格 §六）
    }
```

（注：`makeLines` 辅助在任务 7 的 worker 测试文件中定义过同名静态函数；本文件需同样存在——`ZzLogEngineTest.cpp` 现有 `line()` 辅助，本任务在静态辅助区追加如下 `makeLines`：）

```cpp
    static QVector<ZzLogLine> makeLines(quint64 start, quint64 count)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(count));
        for (quint64 i = 0; i < count; ++i)
            out.append(line(start + i));
        return out;
    }
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build build/debug -j8
```

预期：编译失败，报 `error: 'struct ZzLogEngine::Config' has no member named 'coldDbPath'`、`'class ZzLogEngine' has no member named 'searchLines'` 等。

- [ ] **步骤 3：编写实现代码**

整体替换 `src/log/ZzLogEngine.h`：

```cpp
#pragma once

#include "ZzLogLine.h"
#include "ZzRingBuffer.h"

#include <QMutex>
#include <QObject>
#include <QReadWriteLock>
#include <QThread>

#include <atomic>
#include <memory>

class ZzColdStorage;
class ZzLogArchiveWorker;
class ZzMmapBuffer;

/**
 * @brief 日志引擎门面：热层环形缓冲 + 温层 mmap/LZ4 + 冷层 SQLite/ZSTD 的统一读写入口。
 *
 * 行号约定：绝对单调递增 ID，可读窗口为 [firstLineNo(), firstLineNo()+totalLines())；
 * 温层超限丢弃、纯内存模式驱逐或冷层清理时 firstLineNo() 前移。
 * 冷层启用时库内行号全局单调（跨会话共享 cold.db），引擎行号 = 库内行号 - m_coldOffset，
 * 每会话从 0 起；本会话只能看到 g >= m_coldOffset 的冷层行（历史会话行持久保留但不可见）。
 *
 * 线程模型：appendLine 可在任意线程调用（通常为终端 I/O 线程）；归档与预加载
 * 在内部独立 QThread 中执行，绝不阻塞调用方；getLine/getLines 可在任意线程调用
 * （热层互斥锁 + 温层读写锁 + 冷层内部互斥锁保护）。归档是异步的，flush() 返回后
 * 所有已排队批次保证完成归档（含温→冷推进，不足一块的尾批也落入冷层）。
 *
 * 降级：温层 I/O 失败 → degradedToMemoryOnly（纯内存）；冷层打开/写入失败 →
 * degradedToWarmOnly（无冷层，温层回到 v0.1 容量丢弃，温层文件保留）。
 * 降级不影响终端交互（规格 §八）。
 *
 * 干净退出：析构时 flush 后若冷层启用且未降级，删除温层文件（冷层为唯一持久真相）；
 * 异常退出温层文件残留，下次 open() 按文件头游标续传进冷层后删除。
 */
class ZzLogEngine : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 引擎配置。
     */
    struct Config {
        qsizetype hotCapacity = 10000;     ///< 热层环形缓冲行数（规格默认 1 万）
        quint64 warmMaxLines = 1000000;    ///< 温层最大行数（规格默认 100 万）
        qsizetype archiveBatchSize = 1024; ///< 单次归档批量行数
        QString warmFilePath;              ///< 温层 mmap 文件路径；为空则纯内存模式（冷层配置同时被忽略）
        QString coldDbPath;                ///< 冷层全局单库路径；为空 = 冷层禁用（行为 = v0.1）
        QString sessionId;                 ///< 会话 profile id（冷层行归属标注）
        qint64 coldMaxBytes = 10LL * 1024 * 1024 * 1024; ///< 冷层清理水位：10GB
        int coldMaxAgeDays = 90;                         ///< 冷层清理水位：90 天
    };

    explicit ZzLogEngine(const Config &config, QObject *parent = nullptr);
    ~ZzLogEngine() override;

    /**
     * @brief 打开引擎（含温层文件与冷层库）；失败时按层降级并发射对应信号。
     * @return 恒返回 true（降级视为可用）；通过 isMemoryOnly()/isColdEnabled() 查询实际模式。
     * @note 建议在 open() 之前连接 degradedToMemoryOnly/degradedToWarmOnly 信号。
     * @note 冷层启用时 open() 会同步完成残留温层的崩溃恢复续传（仅异常退出后触发，
     *       通常秒级；见 recoverResidualWarm）。
     */
    bool open();

    /// @brief 追加一行（线程安全；热层满时最老批次异步归档到温层）。
    void appendLine(const ZzLogLine &line);

    /**
     * @brief 读取单行。
     * @param lineNo 绝对行 ID。
     * @param out 输出行，不可为空。
     * @return 行在当前可读窗口内返回 true。
     */
    bool getLine(quint64 lineNo, ZzLogLine *out) const;

    /**
     * @brief 读取 [startLine, startLine+count) 窗口（滚动读取主路径）。
     *        冷→温→热三路透明归并，调用方零改动。
     * @return 实际读到的行，按行 ID 递增排列。
     */
    QVector<ZzLogLine> getLines(quint64 startLine, quint64 count) const;

    quint64 totalLines() const;   ///< 当前可读总行数（冷层 + 温层 + 热层）
    quint64 firstLineNo() const;  ///< 当前最老可读行 ID
    bool isMemoryOnly() const { return m_memoryOnly.load(); }
    /// @brief 冷层启用且未降级。
    bool isColdEnabled() const { return m_cold && !m_coldDegraded.load(); }

    /// @brief 预加载 lineNo 附近块到解压缓存（异步，不阻塞调用方；按行号路由冷层或温层）。
    void preload(quint64 lineNo);

    /**
     * @brief 冷层全文搜索（FTS5）。
     * @param pattern FTS5 MATCH 表达式。
     * @param maxResults 最大返回行数。
     * @return 命中行的引擎空间行号（升序）；仅覆盖已归档进冷层的行
     *         （温层/热层行不入索引，属 v0.2 范围边界）。
     */
    QVector<quint64> searchLines(const QString &pattern, int maxResults = 1000) const;

    /**
     * @brief 阻塞至全部已排队归档批次完成并冲刷温层/冷层缓冲（尾批落入冷层）。
     * @note 不得从归档线程内部调用（BlockingQueuedConnection 会死锁）。
     */
    void flush();

signals:
    void archiveFinished();                            ///< 一批行完成温层归档
    void degradedToMemoryOnly(const QString &reason);  ///< 温层不可用，降级纯内存
    void degradedToWarmOnly(const QString &reason);    ///< 冷层不可用，降级 v0.1 温层模式

private:
    /// @brief 崩溃恢复：残留温层按文件头 coldCursor 游标续传进冷层后删除；
    ///        续传失败时降级温层模式（发射 degradedToWarmOnly）并删除残留全新开始。
    void recoverResidualWarm();

    Config m_config;
    ZzRingBuffer m_hot;             ///< 热层（m_hotMutex 保护）
    mutable QMutex m_hotMutex;
    std::unique_ptr<ZzMmapBuffer> m_warm; ///< 温层（纯内存模式为空）
    mutable QReadWriteLock m_warmLock; ///< 温层读写锁（读路径读锁、归档写锁）
    QThread m_workerThread;         ///< 归档线程
    ZzLogArchiveWorker *m_worker = nullptr; ///< 运行于归档线程
    std::atomic<quint64> m_warmBase{0};  ///< 温层首行 ID（归档线程发布）
    std::atomic<quint64> m_warmCount{0}; ///< 温层行数（归档线程发布）
    quint64 m_hotBase = 0;          ///< 热层首行 ID（m_hotMutex 保护）
    std::atomic<bool> m_memoryOnly{false}; ///< 降级标志（archiveFailed 回调跨线程写）
    std::unique_ptr<ZzColdStorage> m_cold; ///< 冷层（coldDbPath 为空或打开/恢复失败时为空）
    quint64 m_coldOffset = 0;       ///< 本会话引擎行号 → 库内全局行号的平移量（open 时确定）
    std::atomic<quint64> m_coldBase{0};     ///< 冷层最老可读行（库内全局空间；读路径减 m_coldOffset 夹取）
    std::atomic<quint64> m_coldFrontier{0}; ///< 本会话已落冷层行数上界（引擎空间 == worker 游标）
    std::atomic<bool> m_coldDegraded{false}; ///< 冷层降级门闩（coldFailed 回调跨线程写）
};
```

整体替换 `src/log/ZzLogEngine.cpp`：

```cpp
#include "ZzLogEngine.h"

#include "ZzColdStorage.h"
#include "ZzLogArchiveWorker.h"
#include "ZzMmapBuffer.h"

#include <QFile>
#include <QMetaObject>

ZzLogEngine::ZzLogEngine(const Config &config, QObject *parent)
    : QObject(parent)
    , m_config(config)
    , m_hot(qMax<qsizetype>(config.hotCapacity, 1))
{
}

ZzLogEngine::~ZzLogEngine()
{
    flush(); // 让已排队批次落盘（含温→冷推进与尾批）
    if (m_workerThread.isRunning()) {
        m_workerThread.quit();
        m_workerThread.wait();
    }
    delete m_worker; // 队列已排空、线程已停止，直接删除安全
    m_worker = nullptr;
    // 干净退出：冷层启用且未降级时温层数据已全量落冷层，删除温层文件
    // （冷层为唯一持久真相，规格 §六）；降级后保留温层文件
    const bool removeWarm = isColdEnabled();
    if (m_warm) {
        m_warm->close();
        m_warm.reset();
    }
    if (removeWarm)
        QFile::remove(m_config.warmFilePath);
    if (m_cold)
        m_cold->close();
}

bool ZzLogEngine::open()
{
    qRegisterMetaType<QVector<ZzLogLine>>("QVector<ZzLogLine>");
    if (m_config.warmFilePath.isEmpty()) {
        m_memoryOnly = true;
        return true; // 纯内存模式：无归档线程可推进，冷层配置忽略
    }
    if (!m_config.coldDbPath.isEmpty()) {
        ZzColdStorage::Config coldConfig;
        coldConfig.dbPath = m_config.coldDbPath;
        coldConfig.sessionId = m_config.sessionId;
        coldConfig.maxBytes = m_config.coldMaxBytes;
        coldConfig.maxAgeDays = m_config.coldMaxAgeDays;
        m_cold = std::make_unique<ZzColdStorage>(coldConfig);
        QString error;
        if (!m_cold->open(&error)) {
            m_cold.reset();
            m_coldDegraded = true;
            // 冷层不可用：残留温层无续传去向，按 v0.1 崩溃语义删除后全新开始
            QFile::remove(m_config.warmFilePath);
            emit degradedToWarmOnly(
                QStringLiteral("冷层库打开失败，降级为温层模式：%1").arg(error));
        } else {
            recoverResidualWarm(); // 崩溃恢复（失败时内部降级并发射信号）
            if (isColdEnabled()) {
                // 本会话行号基线接续全局单调空间（含刚续传进的残留行；
                // 残留行属上一会话，对本会话不可见）
                m_coldOffset = m_cold->frontier();
                m_coldBase.store(m_cold->baseLine());
                m_coldFrontier.store(0); // 引擎空间：本会话尚未有任何行落冷层
            }
        }
    }
    m_warm = std::make_unique<ZzMmapBuffer>(m_config.warmFilePath, m_config.warmMaxLines);
    if (!m_warm->open()) {
        const QString path = m_config.warmFilePath;
        m_warm.reset();
        m_memoryOnly = true;
        m_cold.reset(); // 无温层即无归档线程，冷层对本会话无意义
        m_coldDegraded = false;
        emit degradedToMemoryOnly(
            QStringLiteral("温层文件打开失败，降级为纯内存模式：%1").arg(path));
        return true; // 降级不影响终端交互（规格 §八）
    }
    // 恢复既有温层数据（引擎重开场景；冷层模式下残留已在上方处理，此处读回为空）
    m_warmBase.store(m_warm->firstLineId());
    m_warmCount.store(m_warm->lineCount());
    {
        QMutexLocker locker(&m_hotMutex);
        m_hotBase = m_warmBase.load() + m_warmCount.load();
    }
    m_worker = new ZzLogArchiveWorker(m_warm.get(), &m_warmLock, &m_warmBase, &m_warmCount,
                                      m_cold.get(), &m_coldBase, &m_coldFrontier);
    m_worker->moveToThread(&m_workerThread);
    connect(m_worker, &ZzLogArchiveWorker::archiveCompleted,
            this, &ZzLogEngine::archiveFinished);
    connect(m_worker, &ZzLogArchiveWorker::archiveFailed, this,
            [this](const QString &message) {
                // 门闩：exchange 返回旧值，已降级则直接返回，避免队列中后续
                // 失败批次逐批重复发射 degradedToMemoryOnly
                if (m_memoryOnly.exchange(true))
                    return;
                emit degradedToMemoryOnly(message);
            });
    connect(m_worker, &ZzLogArchiveWorker::coldFailed, this,
            [this](const QString &message) {
                // 门闩同上：冷层降级为 v0.1 温层模式（与温层降级对称），温层文件保留
                if (m_coldDegraded.exchange(true))
                    return;
                emit degradedToWarmOnly(message);
            });
    m_workerThread.start();
    return true;
}

void ZzLogEngine::recoverResidualWarm()
{
    // 异常退出残留的温层文件：按文件头持久化的 coldCursor 续传进冷层后删除（规格 §六）。
    // 残留温层不能复用为活温层——新会话显示层行号从 0 起，复用会让读回命中
    // 上一会话的行（错行，v0.1 enableScrollback 删文件逻辑同源）。
    if (!QFile::exists(m_config.warmFilePath))
        return;
    ZzMmapBuffer residual(m_config.warmFilePath, m_config.warmMaxLines);
    if (!residual.open()) {
        // 打不开：无法续传，按 v0.1 崩溃语义删除残留（冷层仍保有已归档部分）
        QFile::remove(m_config.warmFilePath);
        return;
    }
    // 游标之前的行已被冷层覆盖（floor 整块丢弃不变式保证）；首存活块可能跨界
    quint64 cursor = qMax(residual.coldCursor(), residual.firstLineId());
    const quint64 warmEnd = residual.firstLineId() + residual.lineCount();
    bool ok = true;
    while (cursor < warmEnd && ok) {
        const quint64 batch = qMin(warmEnd - cursor, ZzColdStorage::kMaxBlockLines);
        const QVector<ZzLogLine> lines = residual.readLines(cursor, batch);
        if (quint64(lines.size()) != batch)
            break; // 块损坏：终止续传
        QString error;
        ok = m_cold->appendBlock(lines, m_cold->frontier(), &error);
        if (ok) {
            cursor += batch;
            residual.setRetentionFloor(cursor); // 续传进度随批持久化（崩溃窗口最小化）
        }
    }
    residual.close();
    if (cursor < warmEnd) {
        // 续传失败：降级温层模式；残留按 v0.1 崩溃语义删除全新开始（避免错行）
        m_coldDegraded = true;
        m_cold.reset();
        QFile::remove(m_config.warmFilePath);
        emit degradedToWarmOnly(QStringLiteral("残留温层续传失败，降级为温层模式"));
        return;
    }
    QFile::remove(m_config.warmFilePath); // 续传完成，删除残留
}

void ZzLogEngine::appendLine(const ZzLogLine &line)
{
    QVector<ZzLogLine> batch;
    {
        QMutexLocker locker(&m_hotMutex);
        if (m_hot.isFull()) {
            batch = m_hot.takeOldest(qMin(m_config.archiveBatchSize, m_hot.count()));
            m_hotBase += quint64(batch.size());
        }
        m_hot.append(line);
    }
    if (batch.isEmpty() || m_memoryOnly || !m_worker)
        return; // 纯内存模式：最老批次直接丢弃
    QMetaObject::invokeMethod(m_worker, "archiveLines", Qt::QueuedConnection,
                              Q_ARG(QVector<ZzLogLine>, batch));
}

bool ZzLogEngine::getLine(quint64 lineNo, ZzLogLine *out) const
{
    if (!out)
        return false;
    const QVector<ZzLogLine> lines = getLines(lineNo, 1);
    if (lines.isEmpty())
        return false;
    *out = lines.first();
    return true;
}

QVector<ZzLogLine> ZzLogEngine::getLines(quint64 startLine, quint64 count) const
{
    QVector<ZzLogLine> out;
    if (count == 0)
        return out;
    out.reserve(qsizetype(qMin<quint64>(count, 10000)));

    quint64 id = startLine;
    quint64 remaining = count;

    // 0) 冷层区间：引擎空间 [?, m_coldFrontier)，读时平移到库内全局空间
    if (isColdEnabled()) {
        const quint64 coldBaseG = m_coldBase.load();        // 库内全局空间
        const quint64 coldLocalEnd = m_coldFrontier.load(); // 引擎空间（本会话覆盖上界）
        if (id < coldLocalEnd && remaining > 0) {
            quint64 gid = id + m_coldOffset;
            if (gid < coldBaseG) { // 起点已被清理：前移到冷层最老可读行
                const quint64 skip = qMin(coldBaseG - gid, coldLocalEnd - id);
                id += skip;
                gid += skip;
                remaining -= qMin(remaining, skip);
            }
            const quint64 want = qMin(remaining, coldLocalEnd - id);
            if (want > 0) {
                out = m_cold->readLines(gid, want);
                const quint64 got = quint64(out.size());
                id += got;
                remaining -= got;
            }
        }
    }
    // 1) 温层区间
    if (m_warm) {
        const quint64 warmEnd = m_warmBase.load() + m_warmCount.load();
        if (id < warmEnd) {
            QReadLocker locker(&m_warmLock);
            out += m_warm->readLines(id, remaining);
            const quint64 got = quint64(out.size());
            id += got;
            remaining -= got;
        }
    }
    // 2) 热层区间（in-flight 归档批次形成的小空洞直接跳过，属瞬时状态）
    if (remaining > 0) {
        QMutexLocker locker(&m_hotMutex);
        const quint64 hotBase = m_hotBase;
        const quint64 hotEnd = hotBase + quint64(m_hot.count());
        if (id < hotBase)
            id = hotBase;
        while (remaining > 0 && id < hotEnd) {
            out.append(m_hot.at(qsizetype(id - hotBase)));
            ++id;
            --remaining;
        }
    }
    return out;
}

quint64 ZzLogEngine::totalLines() const
{
    QMutexLocker locker(&m_hotMutex);
    const quint64 hotEnd = m_hotBase + quint64(m_hot.count());
    // 行空间连续 [firstLineNo(), hotEnd)：冷层启用时窗口起点为冷层基线（引擎空间），
    // 否则为温层首行/热层首行（与 v0.1 的 m_warmCount + hot.count() 数值等价）
    quint64 first;
    if (isColdEnabled()) {
        const qint64 base = qint64(m_coldBase.load()) - qint64(m_coldOffset);
        first = base > 0 ? qMin(quint64(base), m_coldFrontier.load()) : 0;
    } else if (m_warm) {
        first = m_warmBase.load();
    } else {
        first = m_hotBase;
    }
    return hotEnd - first;
}

quint64 ZzLogEngine::firstLineNo() const
{
    if (isColdEnabled()) {
        // 引擎空间 = 库内空间 - m_coldOffset；库内 base 低于本会话基线时窗口从 0 起
        const qint64 base = qint64(m_coldBase.load()) - qint64(m_coldOffset);
        return base > 0 ? qMin(quint64(base), m_coldFrontier.load()) : 0;
    }
    if (m_warm)
        return m_warmBase.load();
    QMutexLocker locker(&m_hotMutex);
    return m_hotBase;
}

QVector<quint64> ZzLogEngine::searchLines(const QString &pattern, int maxResults) const
{
    if (!isColdEnabled())
        return {};
    const QVector<quint64> hits = m_cold->search(pattern, maxResults);
    QVector<quint64> out;
    out.reserve(hits.size());
    const quint64 first = firstLineNo();
    for (const quint64 g : hits) {
        if (g < m_coldOffset)
            continue; // 历史会话行：持久保留但不属于当前会话窗口
        const quint64 local = g - m_coldOffset;
        if (local >= first && local < m_coldFrontier.load())
            out.append(local);
    }
    return out;
}

void ZzLogEngine::preload(quint64 lineNo)
{
    if (!m_worker)
        return;
    // 按行号路由：落入本会话冷层区间的走冷层预解压，其余走温层（现有路径）
    if (isColdEnabled() && lineNo < m_coldFrontier.load()) {
        QMetaObject::invokeMethod(m_worker, "preloadCold", Qt::QueuedConnection,
                                  Q_ARG(quint64, lineNo + m_coldOffset));
        return;
    }
    QMetaObject::invokeMethod(m_worker, "preloadAround", Qt::QueuedConnection,
                              Q_ARG(quint64, lineNo));
}

void ZzLogEngine::flush()
{
    if (!m_worker || !m_workerThread.isRunning())
        return;
    // 队列先进先出：先前排队的 archiveLines 先于 flush 执行，返回即归档完成
    // （worker 的 flush 槽会把不足一块的尾批也推进冷层）
    QMetaObject::invokeMethod(m_worker, "flush", Qt::BlockingQueuedConnection);
}
```

- [ ] **步骤 4：构建并运行测试确认通过**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
ctest --test-dir build/debug --output-on-failure -R 'ZzLogEngineTest'
```

预期：ZzLogEngineTest 全部用例 Passed——既有 7 个用例（v0.1 回归，`engineReopenRestoresWarmLayer` 走 coldDbPath 为空路径不受影响）+ 新增 5 个冷层用例。

- [ ] **步骤 5：日志库全量回归**

```bash
ctest --test-dir build/debug --output-on-failure -R 'Zz(LineIndex|RingBuffer|MmapBuffer|ColdStorage|LogArchiveWorker|LogEngine)Test'
```

预期：6 个测试可执行文件全部 Passed。

- [ ] **步骤 6：Commit**

```bash
git add src/log/ZzLogEngine.h src/log/ZzLogEngine.cpp tests/log/ZzLogEngineTest.cpp
git commit -m "feat(log): 引擎三层归并与冷层集成" -m "- Config 新增 coldDbPath（空=禁用，行为=v0.1）/sessionId/coldMaxBytes/coldMaxAgeDays；新信号 degradedToWarmOnly（冷层打开或写入失败重试 3 次后降级，与温层降级对称）
- getLines 冷→温→热三路归并：库内行号全局单调（跨会话共享 cold.db），引擎行号 = 库内行号 - m_coldOffset（open 时接续全局 frontier，每会话从 0 起）；调用方零改动
- 崩溃恢复：open() 同步把残留温层按文件头 coldCursor 游标续传进冷层后删除（续传失败降级并删除残留全新开始，避免复用残留导致读回上一会话的错行）
- 干净退出：flush（含尾批落冷层）后析构，冷层健康则删除温层文件；降级后保留
- preload 按行号路由冷层（preloadCold）或温层（preloadAround）；新增 searchLines（FTS5，仅覆盖冷层行，返回引擎空间行号）
- totalLines 改为 hotEnd - firstLineNo 统一口径（与 v0.1 数值等价，冷层窗口自然纳入）"
```

---

### 任务 9：应用装配层接线（coldDbPath/sessionId 传入引擎创建点）

**文件：**
- 修改：`src/terminal/ZzTerminalView.cpp`（`enableScrollback`）

引擎创建点：`ZzTabManager::openSession`/`reconnectSession` → `ZzTerminalView::enableScrollback(sessionId)`（sessionId 为会话 profile id，`profile.id.toString(QUuid::WithoutBraces)`）。滚动桥 `ZzScrollbackBridge` 零改动（三层归并对调用方透明）。

- [ ] **步骤 1：修改 enableScrollback 接线**

打开 `src/terminal/ZzTerminalView.cpp`，定位 `enableScrollback` 函数（约 169 行起）。**锚点 1**：既有注释块 `// 新会话显示层历史基线恒从 0 起……QFile::remove(warmPath);`——删除该注释块与 `QFile::remove(warmPath);` 一行，替换为：

```cpp
    // 温层残留文件不再由本层预删：冷层模式下引擎 open() 会按文件头游标完成
    // 崩溃恢复续传后删除残留并创建全新空温层；冷层不可用时引擎降级为温层模式
    // 并自行删除残留全新开始（与 v0.1 行为一致，新会话显示层行号仍从 0 起）
```

**锚点 2**：`ZzLogEngine::Config config; config.warmFilePath = warmPath;` 之后追加两行：

```cpp
    config.coldDbPath = dirPath + QStringLiteral("/cold.db"); // 全局单库：所有会话共享
    config.sessionId = sessionId;                             // 会话 profile id（冷层行归属）
```

**锚点 3**：既有 `connect(m_scrollbackBridge, &ZzScrollbackBridge::degraded, ...)` 块之后、`engine->open();` 之前追加冷层降级提示接线：

```cpp
    // 冷层降级 → 状态栏提示（与温层降级同一路径，不打断终端）
    connect(engine, &ZzLogEngine::degradedToWarmOnly, this,
            [this](const QString &reason) {
                emit errorOccurred(QStringLiteral("滚动历史已降级为温层模式：%1")
                                       .arg(reason));
            });
```

保留 `engine->open();` 必须最后调用的既有注释与顺序（open 可能同步发射降级信号，先于接线会丢失提示）。

- [ ] **步骤 2：构建并跑装配相关测试（零回归验证）**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
ctest --test-dir build/debug --output-on-failure -R 'tst_Zz(TerminalView|ScrollbackBridge|TabManager)'
```

预期：3 个 UI 级测试全部 Passed（离屏运行）。`tst_ZzScrollbackBridge` 等测试自建引擎时不传 `coldDbPath`（默认空 = 冷层禁用），行为与 v0.1 一致，故无需修改测试代码。

- [ ] **步骤 3：Commit**

```bash
git add src/terminal/ZzTerminalView.cpp
git commit -m "feat(terminal): 滚动历史接入冷层全局单库" -m "- enableScrollback 接线 coldDbPath（应用配置目录 scrollback/cold.db，所有会话共享）与 sessionId（会话 profile id）
- 移除温层文件预删：冷层模式下由引擎 open() 完成崩溃恢复续传后删除；冷层降级时引擎自行删除残留全新开始（与 v0.1 语义一致）
- degradedToWarmOnly 接入 errorOccurred 状态栏提示链（与温层降级同路径）
- 滚动桥与读回路径零改动：三层归并对调用方透明"
```

---

### 任务 10：性能门控测试 ZzColdStoragePerfTest + 全量回归

**文件：**
- 创建：`tests/log/ZzColdStoragePerfTest.cpp`
- 修改：`tests/log/CMakeLists.txt`（注册性能测试，打 `perf` 标签）
- 创建（测试运行时生成并提交）：`tests/perf/records/YYYY-MM-DD-ZzColdStorage-*.json`

门控阈值（规格 §八，不达标不验收）：冷层写入 ≥ 50 万行/s；冷层随机读 24 行（缓存未命中）≤ 5ms；冷层连续滚动（缓存命中）≤ 1ms/页；FTS5 搜索 1000 万行 ≤ 500ms；三层归并滚动读 ≤ 16ms/帧；既有 writeThroughput/scrollReadLatency 不回退（相对 `tests/perf/records/2026-08-17-ZzLogEngine-*.json` 回退不得超 5%）。

- [ ] **步骤 1：编写性能门控测试**

创建 `tests/log/ZzColdStoragePerfTest.cpp`（writeRecord/environmentInfo/totalMemoryMB 模式照抄 `ZzLogEnginePerfTest.cpp`，保持 records 统一 schema）：

```cpp
#include "ZzColdStorage.h"
#include "ZzLogEngine.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QRandomGenerator>
#include <QSysInfo>
#include <QTemporaryDir>
#include <QtTest>

#if defined(Q_OS_MACOS)
#  include <sys/sysctl.h>
#elif defined(Q_OS_WIN)
#  include <windows.h>
#endif

/**
 * @brief ZzColdStorage 冷层性能门控测试（规格 §八）。
 *
 * 阈值失败即测试失败；结果持久化到 tests/perf/records/YYYY-MM-DD-ZzColdStorage-*.json，
 * 内容含阈值、实测值、环境信息与 git commit hash。仅 Release 构建数字有效，
 * Debug 构建整体跳过。
 */
class ZzColdStoragePerfTest : public QObject
{
    Q_OBJECT
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("冷层性能测试行 %1 0123456789abcdef").arg(i), QByteArray(8, 'x')};
    }
    static QVector<ZzLogLine> makeBlock(quint64 firstLine)
    {
        QVector<ZzLogLine> out;
        out.reserve(qsizetype(ZzColdStorage::kMaxBlockLines));
        for (quint64 i = 0; i < ZzColdStorage::kMaxBlockLines; ++i)
            out.append(line(firstLine + i));
        return out;
    }
    static ZzColdStorage::Config coldConfig(const QString &dbPath)
    {
        ZzColdStorage::Config c;
        c.dbPath = dbPath;
        c.sessionId = QStringLiteral("perf-session");
        return c;
    }
    /// @brief 向冷层写入 totalLines 行（1024 行/块），返回耗时毫秒。
    static qint64 fillCold(ZzColdStorage &cold, quint64 totalLines)
    {
        QElapsedTimer timer;
        timer.start();
        quint64 frontier = 0;
        while (frontier < totalLines) {
            if (!cold.appendBlock(makeBlock(frontier), frontier))
                return -1;
            frontier += ZzColdStorage::kMaxBlockLines;
        }
        return timer.elapsed();
    }

    /// @brief 返回物理内存总量（MB），无法获取时返回 -1。
    static qint64 totalMemoryMB()
    {
#if defined(Q_OS_LINUX)
        QFile f(QStringLiteral("/proc/meminfo"));
        if (f.open(QIODevice::ReadOnly)) {
            const QByteArray content = f.readAll();
            const qsizetype pos = content.indexOf("MemTotal:");
            if (pos >= 0) {
                const QByteArray line = content.mid(pos, content.indexOf('\n', pos) - pos);
                return QString::fromLatin1(line).split(' ', Qt::SkipEmptyParts).value(1).toLongLong()
                       / 1024;
            }
        }
        return -1;
#elif defined(Q_OS_MACOS)
        int mib[2] = {CTL_HW, HW_MEMSIZE};
        int64_t mem = 0;
        size_t len = sizeof(mem);
        if (sysctl(mib, 2, &mem, &len, nullptr, 0) == 0)
            return mem / 1048576;
        return -1;
#elif defined(Q_OS_WIN)
        MEMORYSTATUSEX status{sizeof(status)};
        if (GlobalMemoryStatusEx(&status))
            return static_cast<qint64>(status.ullTotalPhys / 1048576);
        return -1;
#else
        return -1; // 其他平台无内存采集实现，记录为 -1
#endif
    }

    /// @brief 采集环境信息：CPU/OS/内存/Qt 版本/编译器/构建类型/git commit hash。
    static QJsonObject environmentInfo()
    {
        QJsonObject env;
        env[QStringLiteral("cpu")] = QSysInfo::currentCpuArchitecture();
        env[QStringLiteral("os")] = QSysInfo::prettyProductName();
        env[QStringLiteral("kernel")] = QSysInfo::kernelVersion();
        env[QStringLiteral("memory_mb")] = double(totalMemoryMB());
        env[QStringLiteral("qtVersion")] = QStringLiteral(QT_VERSION_STR);
        env[QStringLiteral("buildType")] = QStringLiteral("Release");
#if defined(Q_CC_MSVC)
        env[QStringLiteral("compiler")] = QStringLiteral("MSVC %1").arg(_MSC_VER);
#elif defined(Q_CC_CLANG)
        env[QStringLiteral("compiler")] = QStringLiteral("Clang %1").arg(__clang_major__);
#elif defined(Q_CC_GNU)
        env[QStringLiteral("compiler")] = QStringLiteral("GCC %1.%2.%3")
                                              .arg(__GNUC__)
                                              .arg(__GNUC_MINOR__)
                                              .arg(__GNUC_PATCHLEVEL__);
#else
        env[QStringLiteral("compiler")] = QStringLiteral("unknown");
#endif
        QProcess git;
        git.start(QStringLiteral("git"), {QStringLiteral("rev-parse"), QStringLiteral("HEAD")});
        if (git.waitForFinished(5000) && git.exitCode() == 0)
            env[QStringLiteral("gitCommit")] =
                QString::fromUtf8(git.readAllStandardOutput()).trimmed();
        return env;
    }

    /// @brief 写性能记录到 tests/perf/records/YYYY-MM-DD-<name>.json。
    static void writeRecord(const QString &name, double threshold, const QString &unit,
                            double measured, bool passed, const QJsonObject &details)
    {
        QJsonObject root;
        root[QStringLiteral("testName")] = name;
        root[QStringLiteral("threshold")] = threshold;
        root[QStringLiteral("unit")] = unit;
        root[QStringLiteral("measured")] = measured;
        root[QStringLiteral("passed")] = passed;
        root[QStringLiteral("environment")] = environmentInfo();
        root[QStringLiteral("details")] = details;
        root[QStringLiteral("timestamp")] = QDateTime::currentDateTimeUtc().toString(Qt::ISODate);

        const QString dirPath = QStringLiteral(PERF_RECORDS_DIR);
        QDir().mkpath(dirPath);
        const QString filePath = QDir(dirPath).filePath(
            QDateTime::currentDateTimeUtc().toString(QStringLiteral("yyyy-MM-dd"))
            + QStringLiteral("-") + name + QStringLiteral(".json"));
        QFile f(filePath);
        QVERIFY2(f.open(QIODevice::WriteOnly), qPrintable(filePath));
        f.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    }

private slots:
    void initTestCase()
    {
#ifdef QT_NO_DEBUG
        // Release：执行门控
#else
        QSKIP("性能门控仅在 Release 构建下有效（规格 §八）");
#endif
    }

    /// @brief 冷层写入吞吐门控：50 万行（1024 行/块，含 ZSTD + SQLite 事务 + FTS5 索引）
    ///        ≥ 500,000 行/秒。
    void coldWriteThroughput()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 500000;
        const qint64 ms = fillCold(cold, N);
        QVERIFY(ms > 0);
        QCOMPARE(cold.frontier(), N);

        const double linesPerSec = double(N) / (double(ms) / 1000.0);
        constexpr double threshold = 500000.0;
        const bool passed = linesPerSec >= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-write-throughput"), threshold,
                    QStringLiteral("lines/s"), linesPerSec, passed,
                    {{QStringLiteral("lineCount"), qint64(N)},
                     {QStringLiteral("blockLines"), qint64(ZzColdStorage::kMaxBlockLines)},
                     {QStringLiteral("elapsedMs"), ms}});
        QVERIFY2(passed, qPrintable(QStringLiteral("冷层写入吞吐 %1 行/秒，低于阈值 %2")
                                        .arg(linesPerSec)
                                        .arg(threshold)));
    }

    /// @brief 冷层随机读 24 行（缓存未命中）门控：3,379,200 行（3300 块），
    ///        100 个采样点间隔 33 块（严格递增，保证 LRU 未命中），最差值 ≤ 5ms。
    void coldRandomRead24Lines()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 3300 * ZzColdStorage::kMaxBlockLines; // 3,379,200 行
        QVERIFY(fillCold(cold, N) > 0);

        constexpr int samples = 100;
        qint64 worstNs = 0;
        for (int s = 0; s < samples; ++s) {
            const quint64 start = quint64(s) * 33 * ZzColdStorage::kMaxBlockLines + 7;
            QVERIFY(start + 24 <= N);
            QElapsedTimer t;
            t.start();
            const QVector<ZzLogLine> window = cold.readLines(start, 24);
            const qint64 ns = t.nsecsElapsed();
            QCOMPARE(window.size(), 24);
            QCOMPARE(window.first().text, line(start).text);
            worstNs = qMax(worstNs, ns);
        }

        const double worstMs = double(worstNs) / 1e6;
        constexpr double threshold = 5.0;
        const bool passed = worstMs <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-random-read-24"), threshold,
                    QStringLiteral("ms"), worstMs, passed,
                    {{QStringLiteral("samples"), samples},
                     {QStringLiteral("windowRows"), 24},
                     {QStringLiteral("totalLines"), qint64(N)},
                     {QStringLiteral("cacheBlocks"), 32},
                     {QStringLiteral("sampleStrideBlocks"), 33}});
        QVERIFY2(passed, qPrintable(QStringLiteral("冷层随机读 24 行最差 %1ms，超过阈值 %2ms")
                                        .arg(worstMs)
                                        .arg(threshold)));
    }

    /// @brief 冷层连续滚动（缓存命中）门控：1,024,000 行从头按 24 行/页连读 1000 页，
    ///        平均值 ≤ 1ms/页。
    void coldSequentialScroll()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 1000 * ZzColdStorage::kMaxBlockLines; // 1,024,000 行
        QVERIFY(fillCold(cold, N) > 0);

        constexpr int pages = 1000;
        qint64 totalNs = 0;
        qint64 worstNs = 0;
        for (int p = 0; p < pages; ++p) {
            const quint64 start = quint64(p) * 24;
            QElapsedTimer t;
            t.start();
            const QVector<ZzLogLine> window = cold.readLines(start, 24);
            const qint64 ns = t.nsecsElapsed();
            QCOMPARE(window.size(), 24);
            totalNs += ns;
            worstNs = qMax(worstNs, ns);
        }

        const double avgMs = double(totalNs) / pages / 1e6;
        constexpr double threshold = 1.0;
        const bool passed = avgMs <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-sequential-scroll"), threshold,
                    QStringLiteral("ms/page"), avgMs, passed,
                    {{QStringLiteral("pages"), pages},
                     {QStringLiteral("worstMs"), double(worstNs) / 1e6},
                     {QStringLiteral("pageRows"), 24},
                     {QStringLiteral("totalLines"), qint64(N)}});
        QVERIFY2(passed, qPrintable(QStringLiteral("冷层连续滚动平均 %1ms/页，超过阈值 %2ms")
                                        .arg(avgMs)
                                        .arg(threshold)));
    }

    /// @brief FTS5 全文搜索门控：1000 万行库内单关键词搜索 ≤ 500ms。
    /// @note 建库写入阶段约数十秒（≥50 万行/s 时 10M 行 ≈ 20s+），属预期耗时。
    void coldFtsSearch10M()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzColdStorage cold(coldConfig(dir.filePath(QStringLiteral("cold.db"))));
        QVERIFY(cold.open());
        constexpr quint64 N = 10000 * ZzColdStorage::kMaxBlockLines; // 10,240,000 行
        quint64 frontier = 0;
        while (frontier < N) {
            QVector<ZzLogLine> block = makeBlock(frontier);
            // 每 100003 行埋一个关键词 NEEDLE（约 102 个命中）
            const quint64 nextNeedle = (frontier / 100003 + 1) * 100003;
            if (nextNeedle < frontier + ZzColdStorage::kMaxBlockLines)
                block[qsizetype(nextNeedle - frontier)].text =
                    QStringLiteral("埋点 NEEDLE %1").arg(nextNeedle);
            QVERIFY(cold.appendBlock(block, frontier));
            frontier += ZzColdStorage::kMaxBlockLines;
        }

        QElapsedTimer t;
        t.start();
        const QVector<quint64> hits = cold.search(QStringLiteral("NEEDLE"));
        const qint64 ms = t.elapsed();
        QVERIFY(!hits.isEmpty());

        constexpr double threshold = 500.0;
        const bool passed = double(ms) <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-fts-search-10m"), threshold,
                    QStringLiteral("ms"), double(ms), passed,
                    {{QStringLiteral("totalLines"), qint64(N)},
                     {QStringLiteral("hits"), hits.size()}});
        QVERIFY2(passed, qPrintable(QStringLiteral("FTS5 搜索 1000 万行耗时 %1ms，超过阈值 %2ms")
                                        .arg(ms)
                                        .arg(threshold)));
    }

    /// @brief 三层归并滚动门控：30 万行（冷+温+热）随机窗口读 60 行，最差值 ≤ 16ms。
    ///        随机采样统计覆盖冷/温、温/热边界穿越。
    void mergedScrollRead()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config; // 默认：热 10,000 / 温 1,000,000 / 批 1024
        config.warmFilePath = dir.filePath(QStringLiteral("warm.log"));
        config.coldDbPath = dir.filePath(QStringLiteral("cold.db"));
        config.sessionId = QStringLiteral("perf-session");
        ZzLogEngine engine(config);
        QVERIFY(engine.open());
        QVERIFY(engine.isColdEnabled());

        constexpr quint64 N = 300000;
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush(); // 温层全部推进冷层，热层留存尾部
        QCOMPARE(engine.totalLines(), N);
        QVERIFY(engine.firstLineNo() == 0);

        constexpr int samples = 200;
        qint64 worstNs = 0;
        qint64 totalNs = 0;
        for (int s = 0; s < samples; ++s) {
            const quint64 start = quint64(QRandomGenerator::global()->bounded(qint64(N - 60)));
            QElapsedTimer t;
            t.start();
            const QVector<ZzLogLine> window = engine.getLines(start, 60);
            const qint64 ns = t.nsecsElapsed();
            QCOMPARE(window.size(), 60);
            QCOMPARE(window.first().text, line(start).text);
            worstNs = qMax(worstNs, ns);
            totalNs += ns;
        }

        const double worstMs = double(worstNs) / 1e6;
        const double avgMs = double(totalNs) / samples / 1e6;
        constexpr double threshold = 16.0; // 滚动帧时间上限（规格 §一红线）
        const bool passed = worstMs <= threshold;
        writeRecord(QStringLiteral("ZzColdStorage-merged-scroll-read"), threshold,
                    QStringLiteral("ms"), worstMs, passed,
                    {{QStringLiteral("samples"), samples},
                     {QStringLiteral("avgMs"), avgMs},
                     {QStringLiteral("windowRows"), 60},
                     {QStringLiteral("totalLines"), qint64(N)}});
        QVERIFY2(passed, qPrintable(QStringLiteral("三层归并滚动最差 %1ms，超过阈值 %2ms")
                                        .arg(worstMs)
                                        .arg(threshold)));
    }
};

QTEST_GUILESS_MAIN(ZzColdStoragePerfTest)
#include "ZzColdStoragePerfTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加：

```cmake
# 冷层性能门控测试（规格 §八）：结果写入 tests/perf/records/，Release 构建数字才有效。
add_executable(ZzColdStoragePerfTest ZzColdStoragePerfTest.cpp)
target_link_libraries(ZzColdStoragePerfTest PRIVATE ZzLogEngine Qt6::Core Qt6::Test)
set_target_properties(ZzColdStoragePerfTest PROPERTIES AUTOMOC ON)
target_compile_definitions(ZzColdStoragePerfTest PRIVATE
    PERF_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records")
add_test(NAME ZzColdStoragePerfTest COMMAND ZzColdStoragePerfTest)
set_tests_properties(ZzColdStoragePerfTest PROPERTIES LABELS "perf")
```

- [ ] **步骤 2：Debug 构建验证（性能用例整体 QSKIP，编译必须通过）**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
ctest --test-dir build/debug --output-on-failure -R ZzColdStoragePerfTest
```

预期：测试 Passed（Debug 下 QSKIP 计入 Passed）。

- [ ] **步骤 3：Release 构建并执行冷层性能门控**

preset 可用时：

```bash
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release -j8
ctest --test-dir build/linux-gcc-release --output-on-failure -R ZzColdStoragePerfTest
```

preset 不可用时（显式形式）：

```bash
cmake -S . -B build/release -DCMAKE_BUILD_TYPE=Release && cmake --build build/release -j8
ctest --test-dir build/release --output-on-failure -R ZzColdStoragePerfTest
```

预期：5 个门控用例全部 Passed；`tests/perf/records/` 新增 5 个 `YYYY-MM-DD-ZzColdStorage-*.json`（`coldFtsSearch10M` 用例建库阶段耗时数十秒属预期）。

- [ ] **步骤 4：既有性能门控回归（不回退验证）**

```bash
ctest --test-dir build/linux-gcc-release --output-on-failure -R ZzLogEnginePerfTest
# 显式形式环境改用：ctest --test-dir build/release --output-on-failure -R ZzLogEnginePerfTest
```

预期：ZzLogEnginePerfTest 2 个用例 Passed。随后对比新旧 records：

```bash
cat tests/perf/records/*-ZzLogEngine-write-throughput.json | grep -E '"(timestamp|measured)"'
cat tests/perf/records/*-ZzLogEngine-scroll-read-latency.json | grep -E '"(timestamp|measured)"'
```

人工核对：新记录 `measured` 相对 2026-08-17 基线回退不超 5%（写吞吐基线 ≈232 万行/s → 不得低于约 220 万行/s；滚动读延迟基线见既有记录 → 不得高于基线 ×1.05）。若回退超标，停下来排查追加路径是否被冷层改动波及（正常情况下 appendLine 路径未变，唯一新增开销是归档线程内的 coldAdvance，不占 I/O 线程）。

- [ ] **步骤 5：Debug 全量回归**

```bash
ctest --test-dir build/debug --output-on-failure
```

预期：全部测试 Passed（含 log/session/UI 各级；两个性能测试在 Debug 下 QSKIP 计 Passed）。

- [ ] **步骤 6：纯 Qt Core 依赖审计（冷层不得引入 Widgets/Gui 依赖）**

```bash
ldd build/debug/tests/log/ZzColdStorageTest | grep -iE 'Qt6(Widgets|Gui)' || echo "OK: 无 Widgets/Gui 依赖"
```

预期：输出 `OK: 无 Widgets/Gui 依赖`（Windows 对应 `dumpbin /dependents`，macOS 对应 `otool -L`）。

- [ ] **步骤 7：提交性能记录并收尾**

```bash
git add tests/log/ZzColdStoragePerfTest.cpp tests/log/CMakeLists.txt tests/perf/records/
git commit -m "test(log): 新增冷层性能门控测试与全量回归" -m "- ZzColdStoragePerfTest 五个门控：写入 ≥50 万行/s、随机读 24 行（清缓存语义：33 块步长严格递增保证 LRU 未命中）≤5ms、连续滚动 ≤1ms/页、FTS5 搜索 1000 万行 ≤500ms、三层归并滚动 ≤16ms/帧
- records 沿用统一 schema（testName/threshold/unit/measured/passed/details/timestamp/environment），落 tests/perf/records/YYYY-MM-DD-ZzColdStorage-*.json
- 既有 ZzLogEnginePerfTest 回归通过且相对 2026-08-17 基线回退不超 5%
- Debug 全量回归 Passed；纯 Qt Core 依赖审计通过"
```

---

## 自检结论（编写者已执行）

**1. 规格覆盖度（`2026-08-20-zzlogengine-cold-layer-design.md` §一~§十）：**
- §一 滚动穿越冷层读回 → 任务 8（getLines 三路归并）+ 任务 10（mergedScrollRead 16ms 红线）
- §一 FTS5 引擎 API（不做 UI）→ 任务 4（ZzColdStorage::search）+ 任务 8（ZzLogEngine::searchLines）；§十 范围边界（无搜索 UI/导出/同步）→ 全计划无 UI 任务
- §二 决策表：三层归并 → 任务 8；冷层唯一持久真相/干净退出删温层 → 任务 8 析构 + recoverResidualWarm；ZSTD 子模块锁 tag → 任务 1；全局单库 + sessionId → 任务 2 schema + 任务 9 接线；方案 A 独立类复用归档线程 → 任务 2/7；sqlite3 C API → 任务 2-5
- §三 数据流（温→冷后台推进、温层截头）→ 任务 6（setRetentionFloor）+ 任务 7（coldAdvance）
- §四 schema 原文（blocks/lines_fts content=''/meta、WAL、块内偏移表、行编码同温层）→ 任务 2/3
- §五 读取归并 + preload 路由 + 32 块 LRU → 任务 3（LRU/preload）+ 任务 7（preloadCold）+ 任务 8（路由）
- §六 归档推进/失败重试 3 次降级/崩溃恢复续传 → 任务 7（重试与降级门闩）+ 任务 8（open 同步续传）；§六 "冷层降级后不得删除温层文件" → 任务 8 析构条件 + coldOpenFailureDegradesToWarmOnly 用例
- §七 自动清理（maxBytes/maxAgeDays、FTS 同步删除、增量 VACUUM、firstLineNo 前移）→ 任务 5
- §八 性能门控六行全部 → 任务 10（含既有门控回归与 5% 回退红线）；QTest 覆盖（块读写往返/FTS5/崩溃恢复/清理/降级）→ 任务 3/4/5/7/8
- §九 依赖与构建（zstd 子模块、SQLite amalgamation、C++20/Zz 前缀/文件名一致/Doxygen 中文/commit 规范）→ 任务 1 + 全任务 commit 步骤 + 任务 10 步骤 6 依赖审计

**2. 与任务书决策的逐条对照：** 决策 1（滚动可穿越）→ 任务 8 ✓；决策 2（唯一持久真相/续传/删温层）→ 任务 8 ✓（恢复实现为 open 同步，偏离已在前置说明第 4 条写明理由）；决策 3（ZSTD v1.5.7/build/cmake/libzstd_static/关三选项/级别 3）→ 任务 1 + 任务 3 ✓；决策 4（vendored amalgamation + FTS5 + WAL + 偏离理由写明）→ 任务 1 + 前置说明第 1 条 ✓；决策 5（全局单库/sessionId/块时间戳=归档时刻近似写明）→ 任务 2/3 + 前置说明第 2 条 ✓；决策 6（ZzColdStorage 双文件无 Pimpl、sqlite3 C API）→ 任务 2 ✓。

**3. 占位符扫描：** 已扫描全文，无 "TODO/待定/后续实现/类似任务 N/为上述代码编写测试" 类占位；所有测试步骤均含完整 QTest 代码，实现步骤均含完整代码块或精确锚点 + 完整替换文本。任务 1 步骤 3/7 的 `<版本>`/`<sha256>` 为执行期环境数据（下载时实测），非设计占位。

**4. 类型/签名一致性（已交叉核对）：**
- `ZzColdStorage` 公开接口（Config{dbPath, sessionId, maxBytes, maxAgeDays}、open/close/isOpen/baseLine/frontier/appendBlock/readLines/preload/search/enforceLimits、kMaxBlockLines、kCompressionLevel）在任务 2 头文件逐字定义；任务 3/4/5 实现、任务 7（kMaxBlockLines）、任务 8（recoverResidualWarm 用 kMaxBlockLines）、任务 10（makeBlock/coldConfig）引用一致
- `ZzMmapBuffer::setRetentionFloor/clearRetentionFloor/coldCursor` 在任务 6 定义；任务 7（coldAdvance 的 floor 推进与降级清除）、任务 8（recoverResidualWarm 读 coldCursor、续传进度 setRetentionFloor 持久化）一致调用
- `ZzLogArchiveWorker` 构造（buffer, lock, warmBase, warmCount, cold=nullptr, coldBase=nullptr, coldFrontier=nullptr, parent）在任务 7 定义；任务 8 open() 接线逐字一致；信号链 `coldFailed → degradedToWarmOnly` 连接点仅在任务 8 open()；`preloadCold` 槽任务 7 定义、任务 8 preload 路由调用一致
- `ZzLogEngine::Config` 新增四字段在任务 8 定义，任务 8/9/10 使用一致；`isColdEnabled()` 内联于头文件，cpp 中复用同一判定
- 两个发布位空间约定（m_coldBase 库内全局 / m_coldFrontier 引擎空间）在任务 7 头注释、任务 8 头注释与 getLines/firstLineNo/totalLines 实现中一致
- 测试数学已推演：2500 行（热 100/批 16）→ 驱逐 150 批 = 2400 行；任务 8 cleanExit 用例 cold.frontier()==2400 与此一致；任务 7 的 3348 = 2048+1300、整块推进 3072 = 3×1024 一致

**5. 已知残留风险（如实记录，不属本计划范围）：** 并发同 profile 多标签的冷层窗口交叉可见（前置说明第 3 条）；崩溃套崩溃毫秒级窗口的重复块（前置说明第 4 条）；FTS5 CJK 子串不命中（前置说明第 6 条）；open() 同步恢复在百万行残留时约秒级阻塞 UI（前置说明第 4 条）。

---

## 附录：公开 API 清单

以下为本计划完成后 `ZzLogEngine` 静态库（`src/log/`，仅依赖 Qt6::Core + lz4_static + libzstd_static + sqlite3_static）对外公开及内部跨任务复用的全部类型与接口。

### `ZzColdStorage`（src/log/ZzColdStorage.h）——冷层（内部组件，任意线程可调）

```cpp
struct ZzColdStorage::Config {
    QString dbPath;                                ///< 全局单库路径
    QString sessionId;                             ///< 写入行的会话归属
    qint64 maxBytes = 10LL * 1024 * 1024 * 1024;   ///< 清理水位：10GB
    int maxAgeDays = 90;                           ///< 清理水位：90 天
};

static constexpr quint64 ZzColdStorage::kMaxBlockLines = 1024; // 单块最大行数
static constexpr int ZzColdStorage::kCompressionLevel = 3;     // ZSTD 压缩级别

explicit ZzColdStorage(const Config &config);
~ZzColdStorage();
bool open(QString *errorString = nullptr);   // 建 schema + WAL + 读 meta/块表（含一致性校验）
void close();
bool isOpen() const;
quint64 baseLine() const;                    // 最老可读行（库内全局空间，清理后前移）
quint64 frontier() const;                    // 已覆盖上界 == 下一个待写入绝对行号（库内全局空间）
bool appendBlock(const QVector<ZzLogLine> &lines, quint64 firstLine,
                 QString *errorString = nullptr); // firstLine 必须 == frontier()；≤1024 行/块
QVector<ZzLogLine> readLines(quint64 startLine, quint64 count) const;
void preload(quint64 lineId);                // 预解压所在块及后一块进 32 块 LRU
QVector<quint64> search(const QString &pattern, int maxResults = 1000) const; // FTS5，库内行号
void enforceLimits();                        // maxBytes/maxAgeDays 清理最老整块
```

### `ZzMmapBuffer`（src/log/ZzMmapBuffer.h）——温层新增方法（v0.1 接口不变）

```cpp
void setRetentionFloor(quint64 firstDisposableLineId); // 冷层模式：只丢整体 ≤ floor 的最老整块
void clearRetentionFloor();                            // 冷层降级：恢复 v0.1 纯 maxLines 丢弃
quint64 coldCursor() const;                            // 文件头偏移 12 持久化的续传游标
```

### `ZzLogArchiveWorker`（src/log/ZzLogArchiveWorker.h）——归档线程工作对象（内部）

```cpp
ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                   std::atomic<quint64> *warmBase, std::atomic<quint64> *warmCount,
                   ZzColdStorage *cold = nullptr,
                   std::atomic<quint64> *coldBase = nullptr,
                   std::atomic<quint64> *coldFrontier = nullptr,
                   QObject *parent = nullptr);
// 槽：archiveLines(QVector<ZzLogLine>) / preloadAround(quint64) / preloadCold(quint64) / flush()
// 信号：archiveCompleted() / archiveFailed(QString) / coldFailed(QString)
// 私有：coldAdvance(bool includePartial)——温→冷按 1024 行整块推进，flush 时含尾批
```

### `ZzLogEngine`（src/log/ZzLogEngine.h）——门面，集成方唯一必读（新增部分）

```cpp
struct ZzLogEngine::Config {
    // v0.1 字段：hotCapacity / warmMaxLines / archiveBatchSize / warmFilePath（不变）
    QString coldDbPath;      // 冷层全局单库路径；为空 = 冷层禁用（行为 = v0.1）
    QString sessionId;       // 会话 profile id（冷层行归属标注）
    qint64 coldMaxBytes = 10LL * 1024 * 1024 * 1024;
    int coldMaxAgeDays = 90;
};

bool isColdEnabled() const;                                     // 冷层启用且未降级
QVector<quint64> searchLines(const QString &pattern,
                             int maxResults = 1000) const;      // FTS5，引擎空间行号
// 新信号：void degradedToWarmOnly(const QString &reason);      // 冷层降级（温层文件保留）
// 既有接口 appendLine/getLine/getLines/totalLines/firstLineNo/isMemoryOnly/preload/flush
// 与信号 archiveFinished/degradedToMemoryOnly 语义不变；getLines 内部冷→温→热三路归并
```

行号约定：引擎空间绝对单调 ID，可读窗口 `[firstLineNo(), firstLineNo() + totalLines())`；冷层启用时引擎行号 = 库内全局行号 - m_coldOffset（open 时确定，每会话从 0 起）；冷层清理后 firstLineNo() 前移。

### 构建产物与性能记录

- 第三方目标：`libzstd_static`（third_party/zstd，v1.5.7）、`sqlite3_static`（third_party/sqlite amalgamation，`SQLITE_ENABLE_FTS5`）
- 库目标：`ZzLogEngine`（`PUBLIC Qt6::Core`，`PRIVATE lz4_static libzstd_static sqlite3_static`，C++20）
- 新测试目标：`ZzColdStorageTest` / `ZzLogArchiveWorkerTest` / `ZzColdStoragePerfTest`（perf 标签，仅 Release 有效）
- 性能记录：`tests/perf/records/YYYY-MM-DD-ZzColdStorage-{write-throughput,random-read-24,sequential-scroll,fts-search-10m,merged-scroll-read}.json`
