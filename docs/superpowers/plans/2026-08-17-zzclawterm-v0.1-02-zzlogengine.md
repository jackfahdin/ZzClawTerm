# ZzLogEngine 日志引擎（热层 + 温层）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 实现规格 §五 的滚动历史日志引擎热层（内存环形缓冲）与温层（mmap + LZ4 分块压缩），支撑 100 万行级滚动历史不卡顿、不丢失、内存有界。

**架构：** 四层结构——`ZzRingBuffer`（热层，固定容量 O(1) 读写，溢出批量驱逐）、`ZzMmapBuffer`（温层，QFile::map 跨平台 mmap + LZ4 每 64KB 一块压缩，超容量按块丢弃最老数据）、`ZzLineIndex`（每 1024 行一条块内偏移索引，块内小范围扫描）、`ZzLogEngine`（门面，行 ID 绝对单调递增，归档与预加载在独立 QThread 中执行，I/O 失败降级纯内存模式）。冷层（SQLite）不在本计划。

**技术栈：** C++20 / Qt 6.8+（仅 Qt6::Core + Qt6::Test，纯后端库不依赖 Widgets）/ CMake 3.25+ / LZ4 1.10（vendored git submodule，`add_subdirectory` 引入）/ QTest。

**执行前提：** 应用仓库骨架（根 `CMakeLists.txt`、`CMakePresets.json`、`CMakeUserPresets.json.example`、`third_party/` 结构、顶层 `enable_testing()`/`include(CTest)` 与 `find_package(Qt6 ... Core Test)`）已由**计划 04 任务 1** 提供，本计划在骨架完成之后执行；骨架未就绪时不要启动本计划。

---

## 前置假设（与骨架计划的接口）

本计划**不创建、也不拥有**任何根构建文件（根 `CMakeLists.txt`、`CMakePresets.json`、`.gitignore`），这些归计划 04 骨架所有。本计划的交付物仅限：

- `src/log/`：`ZzLogEngine` 静态库目标及 `src/log/CMakeLists.txt`
- `tests/log/`：全部测试目标及 `tests/log/CMakeLists.txt`
- `third_party/lz4/`：LZ4 vendored git submodule 指针；以及向骨架既有 `third_party/CMakeLists.txt` **追加**的 LZ4 `add_subdirectory` 条目（追加，不覆盖）
- `tests/perf/records/`：性能记录 JSON（测试运行时生成并提交）

`src/log` 与 `tests/log` 两个子目录接入顶层构建的 `add_subdirectory` 条目由计划 04 骨架负责预留；任务 2 步骤 1 会先验证接入点存在，若缺失则**停下来与主会话协调**（属计划 04 遗漏），不要自行修改根构建文件。

本计划中所有构建/测试命令使用无 preset 依赖的显式形式（`cmake -S . -B build/debug ...`，`build/` 已被 `.gitignore` 忽略）；若骨架已提供 CMakePresets（如 `linux-debug`），可等价替换为 `cmake --preset <名字>` 形式。

所有命令默认在仓库根目录 `/home/zz/Jackfahdin/github/ZzClawTerm` 下执行。

## 文件结构

| 文件 | 职责 |
| ---- | ---- |
| `third_party/lz4/` | LZ4 v1.10.0 vendored git submodule（温层压缩，唯一外部依赖） |
| `src/log/ZzLogLine.h` | 行数据结构：纯文本 + 不透明字符属性负载 |
| `src/log/ZzLineIndex.h/.cpp` | 分块行偏移索引（每 1024 行一条） |
| `src/log/ZzRingBuffer.h/.cpp` | 热层内存环形缓冲 |
| `src/log/ZzMmapBuffer.h/.cpp` | 温层 mmap 文件 + LZ4 分块压缩存储 |
| `src/log/ZzLogArchiveWorker.h/.cpp` | 归档工作对象（运行于独立线程） |
| `src/log/ZzLogEngine.h/.cpp` | 门面类：appendLine/getLine/getLines/totalLines/preload |
| `src/log/CMakeLists.txt` | `ZzLogEngine` 静态库目标 |
| `tests/log/ZzLineIndexTest.cpp` | 行索引 QTest |
| `tests/log/ZzRingBufferTest.cpp` | 环形缓冲 QTest |
| `tests/log/ZzMmapBufferTest.cpp` | 温层 QTest（压缩一致性/重开/裁剪） |
| `tests/log/ZzLogEngineTest.cpp` | 门面 QTest（归档往返/滚动读取等价性/降级） |
| `tests/log/ZzLogEnginePerfTest.cpp` | 性能门控 QTest（写吞吐/滚动读取延迟，写 records JSON） |
| `tests/log/CMakeLists.txt` | 测试目标注册（每测试类一个 ctest 用例，性能测试打 `perf` 标签） |
| `tests/perf/records/YYYY-MM-DD-ZzLogEngine-*.json` | 性能记录（由测试生成并提交入库） |

行 ID 约定：全引擎使用绝对单调递增 ID，可读窗口为 `[firstLineNo(), firstLineNo() + totalLines())`；温层超限丢弃或纯内存模式驱逐时 `firstLineNo()` 前移。

---

### 任务 1：引入 LZ4 vendored 子模块并接入 third_party 构建

**文件：**
- 创建：`third_party/lz4/`（git submodule，指针提交）
- 修改：`third_party/CMakeLists.txt`（计划 04 骨架已建，仅追加 LZ4 引入段）

- [ ] **步骤 1：添加 LZ4 子模块（锁定 v1.10.0）**

```bash
git submodule add --depth 1 --branch v1.10.0 https://github.com/lz4/lz4.git third_party/lz4
```

预期：生成 `third_party/lz4/` 与 `.gitmodules`；`git submodule status` 输出一行以空格（已检出）开头的 `third_party/lz4` 记录。

- [ ] **步骤 2：验证 LZ4 自带 CMake 工程可用**

```bash
ls third_party/lz4/build/cmake/CMakeLists.txt
```

预期：文件存在（LZ4 官方 CMake 工程位于 `build/cmake` 子目录，提供 `lz4_static` / `lz4_shared` 目标）。

- [ ] **步骤 3：在 third_party/CMakeLists.txt 追加 LZ4 引入段**

打开 `third_party/CMakeLists.txt`（计划 04 骨架已建）。**锚点**：文件中既有的 `add_subdirectory(...)` 条目块（如 libssh2 条目）之后，追加以下完整段落；若已存在相同段落则跳过本步：

```cmake
# LZ4（vendored）：ZzLogEngine 温层分块压缩
set(LZ4_BUILD_CLI OFF CACHE BOOL "不构建 lz4 命令行工具" FORCE)
set(LZ4_BUILD_LEGACY_LZ4C OFF CACHE BOOL "不构建 legacy 帧格式支持" FORCE)
add_subdirectory(lz4/build/cmake)
```

注意此处路径是相对于 `third_party/` 的 `lz4/build/cmake`（不是根目录视角的 `third_party/lz4/build/cmake`）。Qt6 的 Core/Test 组件引入由计划 04 骨架的根构建文件负责，本计划不触碰；若执行时发现 `Qt6::Core` 或 `Qt6::Test` 目标不存在，停下来与主会话协调。

- [ ] **步骤 4：配置并构建，验证 lz4_static 目标可用**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug
cmake --build build/debug --target lz4_static -j8
```

预期：配置成功无报错；构建输出以 `[100%] Built target lz4_static` 结束。

- [ ] **步骤 5：Commit**

```bash
git add .gitmodules third_party/lz4 third_party/CMakeLists.txt
git commit -m "build: 引入 LZ4 vendored 子模块并接入 third_party 构建"
```

---

### 任务 2：ZzLineIndex 分块行偏移索引（TDD）+ 库/测试构建骨架

**文件：**
- 创建：`src/log/ZzLogLine.h`
- 创建：`src/log/ZzLineIndex.h`
- 创建：`src/log/ZzLineIndex.cpp`
- 创建：`src/log/CMakeLists.txt`
- 创建：`tests/log/ZzLineIndexTest.cpp`
- 创建：`tests/log/CMakeLists.txt`

（`src/log` 与 `tests/log` 接入顶层构建的 `add_subdirectory` 条目归计划 04 骨架所有，本任务只做存在性验证。）

- [ ] **步骤 1：编写失败的测试**

创建 `tests/log/ZzLineIndexTest.cpp`：

```cpp
#include "ZzLineIndex.h"

#include <QtTest>

/**
 * @brief ZzLineIndex 分块行偏移索引单元测试。
 */
class ZzLineIndexTest : public QObject
{
    Q_OBJECT
private slots:
    /// @brief 默认步长为 1024（规格 §5.2）。
    void defaultStrideIs1024()
    {
        ZzLineIndex index;
        QCOMPARE(index.stride(), 1024ULL);
    }

    /// @brief 仅记录步长整数倍的行。
    void recordsOnlyStrideMultiples()
    {
        ZzLineIndex index(4);
        for (quint64 i = 0; i < 10; ++i)
            index.recordLine(i, 1000 + i, i * 10);
        QCOMPARE(index.entryCount(), 3); // 行 0、4、8
    }

    /// @brief 定位返回不大于目标行的最近条目。
    void locateReturnsNearestLowerEntry()
    {
        ZzLineIndex index(4);
        for (quint64 i = 0; i < 10; ++i)
            index.recordLine(i, 1000 + i, i * 10);

        ZzLineIndex::Entry e;
        QVERIFY(index.locate(6, &e));
        QCOMPARE(e.lineId, 4ULL);
        QCOMPARE(e.blockFirstLineId, 1004ULL);
        QCOMPARE(e.offset, 40ULL);

        QVERIFY(index.locate(0, &e));
        QCOMPARE(e.lineId, 0ULL);
    }

    /// @brief 空索引定位失败。
    void locateOnEmptyIndexFails()
    {
        ZzLineIndex index;
        ZzLineIndex::Entry e;
        QVERIFY(!index.locate(0, &e));
    }
};

QTEST_GUILESS_MAIN(ZzLineIndexTest)
#include "ZzLineIndexTest.moc"
```

创建 `tests/log/CMakeLists.txt`：

```cmake
# 日志引擎测试：每个测试类一个可执行文件并注册进 ctest。
function(zz_add_log_test name)
    add_executable(${name} ${name}.cpp)
    target_link_libraries(${name} PRIVATE ZzLogEngine Qt6::Core Qt6::Test)
    set_target_properties(${name} PROPERTIES AUTOMOC ON)
    add_test(NAME ${name} COMMAND ${name})
endfunction()

zz_add_log_test(ZzLineIndexTest)
```

创建 `src/log/CMakeLists.txt`：

```cmake
# ZzLogEngine：纯 Qt Core 日志引擎库（热层环形缓冲 + 温层 mmap/LZ4），不依赖 Widgets。
add_library(ZzLogEngine STATIC
    ZzLogLine.h
    ZzLineIndex.h
    ZzLineIndex.cpp
)
target_include_directories(ZzLogEngine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzLogEngine PUBLIC Qt6::Core PRIVATE lz4_static)
target_compile_features(ZzLogEngine PUBLIC cxx_std_20)
set_target_properties(ZzLogEngine PROPERTIES AUTOMOC ON)
```

验证骨架已为 `src/log` 与 `tests/log` 预留构建接入点（接入点归计划 04 骨架所有；若骨架按模块目录自动 glob 接入，则可能无显式条目，以能配置成功为准）：

```bash
grep -rn "add_subdirectory(src/log)" CMakeLists.txt src/ 2>/dev/null
grep -rn "add_subdirectory(tests/log)" CMakeLists.txt tests/ 2>/dev/null
```

预期：两条命令合计至少各有一行命中。若无命中且下一步配置报错找不到这两个目录，**停下来与主会话协调**（属计划 04 遗漏），不要自行修改根构建文件。

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
```

预期：CMake 配置或构建失败——`src/log/CMakeLists.txt` 引用的 `ZzLineIndex.cpp` 尚不存在（报错 `Cannot find source file: ZzLineIndex.cpp`），或被测头文件缺失（`fatal error: 'ZzLineIndex.h' file not found`）。两者皆是预期的红灯。

- [ ] **步骤 3：编写最少实现代码**

创建 `src/log/ZzLogLine.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QString>

/**
 * @brief 滚动历史中的一行：纯文本 + 不透明的字符属性负载。
 *
 * attributes 承载颜色 / 粗体等完整字符属性，其序列化格式由终端层
 * （ZzTermWidget 侧）定义；日志引擎只做不透明存储与回读，不解析内容。
 */
struct ZzLogLine {
    QString text;          ///< 行纯文本（不含换行符）
    QByteArray attributes; ///< 字符属性负载（可为空）
};
```

创建 `src/log/ZzLineIndex.h`：

```cpp
#pragma once

#include <QVector>
#include <QtGlobal>

/**
 * @brief 分块行偏移索引：每 stride 行记录一条（块首行 ID + 块内未压缩偏移）。
 *
 * 定位时取不大于目标行的最近条目，随后由调用方在块内做小范围扫描；
 * 借鉴 klogg 行索引思路：stride=1024 时 1,000 万行索引仅约 80KB。
 */
class ZzLineIndex
{
public:
    /**
     * @brief 一条索引条目。
     */
    struct Entry {
        quint64 lineId = 0;           ///< 行 ID（绝对、单调递增）
        quint64 blockFirstLineId = 0; ///< 所在块的首行 ID
        quint64 offset = 0;           ///< 行首在块未压缩数据中的字节偏移
    };

    explicit ZzLineIndex(quint64 stride = 1024);

    quint64 stride() const { return m_stride; }
    qsizetype entryCount() const { return m_entries.size(); }

    /**
     * @brief 记录一行位置；仅当 lineId 为 stride 的整数倍时真正写入。
     * @param lineId 行 ID。
     * @param blockFirstLineId 该行所在块的首行 ID。
     * @param offset 行首在块未压缩数据中的字节偏移。
     */
    void recordLine(quint64 lineId, quint64 blockFirstLineId, quint64 offset);

    /**
     * @brief 定位不大于 lineId 的最近条目。
     * @param lineId 目标行 ID。
     * @param out 输出条目，不可为空。
     * @return 索引为空或 out 为空返回 false。
     */
    bool locate(quint64 lineId, Entry *out) const;

    void clear();

private:
    quint64 m_stride;
    QVector<Entry> m_entries;
};
```

创建 `src/log/ZzLineIndex.cpp`：

```cpp
#include "ZzLineIndex.h"

ZzLineIndex::ZzLineIndex(quint64 stride)
    : m_stride(stride)
{
}

void ZzLineIndex::recordLine(quint64 lineId, quint64 blockFirstLineId, quint64 offset)
{
    if (m_stride == 0 || lineId % m_stride != 0)
        return;
    m_entries.append({lineId, blockFirstLineId, offset});
}

bool ZzLineIndex::locate(quint64 lineId, Entry *out) const
{
    if (!out || m_entries.isEmpty())
        return false;
    // 条目按 lineId 严格递增，二分查找最后一个 lineId <= 目标 的条目
    qsizetype lo = 0;
    qsizetype hi = m_entries.size() - 1;
    qsizetype best = 0;
    while (lo <= hi) {
        const qsizetype mid = (lo + hi) / 2;
        if (m_entries[mid].lineId <= lineId) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    *out = m_entries[best];
    return true;
}

void ZzLineIndex::clear()
{
    m_entries.clear();
}
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzLineIndexTest$'
```

预期：构建成功；ctest 输出 `1/1 Test #1: ZzLineIndexTest ... Passed` 与 `100% tests passed, 0 tests failed out of 1`。

- [ ] **步骤 5：Commit**

```bash
git add src/log tests/log
git commit -m "feat: 新增 ZzLineIndex 分块行偏移索引"
```

---

### 任务 3：ZzRingBuffer 热层环形缓冲（TDD）

**文件：**
- 创建：`src/log/ZzRingBuffer.h`
- 创建：`src/log/ZzRingBuffer.cpp`
- 创建：`tests/log/ZzRingBufferTest.cpp`
- 修改：`src/log/CMakeLists.txt`（追加源文件）
- 修改：`tests/log/CMakeLists.txt`（注册测试）

- [ ] **步骤 1：编写失败的测试**

创建 `tests/log/ZzRingBufferTest.cpp`：

```cpp
#include "ZzRingBuffer.h"

#include <QtTest>

/**
 * @brief ZzRingBuffer 热层环形缓冲单元测试。
 */
class ZzRingBufferTest : public QObject
{
    Q_OBJECT
    /// @brief 构造带属性负载的测试行。
    static ZzLogLine makeLine(const QString &text)
    {
        return {text, QByteArray("attr:") + text.toUtf8()};
    }

private slots:
    /// @brief 顺序追加后可按逻辑下标原序读回，属性完整保留。
    void appendAndReadInOrder()
    {
        ZzRingBuffer ring(8);
        for (int i = 0; i < 5; ++i)
            QVERIFY(!ring.append(makeLine(QStringLiteral("line%1").arg(i))));
        QCOMPARE(ring.count(), 5);
        for (int i = 0; i < 5; ++i)
            QCOMPARE(ring.at(i).text, QStringLiteral("line%1").arg(i));
        QCOMPARE(ring.at(3).attributes, QByteArray("attr:line3"));
    }

    /// @brief 写满后追加驱逐最老行，并通过出参返回被驱逐行。
    void overflowEvictsOldest()
    {
        ZzRingBuffer ring(4);
        for (int i = 0; i < 4; ++i)
            QVERIFY(!ring.append(makeLine(QStringLiteral("line%1").arg(i))));
        QVERIFY(ring.isFull());

        ZzLogLine evicted;
        QVERIFY(ring.append(makeLine(QStringLiteral("line4")), &evicted));
        QCOMPARE(evicted.text, QStringLiteral("line0"));
        QCOMPARE(ring.count(), 4);
        QCOMPARE(ring.at(0).text, QStringLiteral("line1"));
        QCOMPARE(ring.at(3).text, QStringLiteral("line4"));
    }

    /// @brief takeOldest 按序批量取走最老行（归档路径）。
    void takeOldestReturnsBatchInOrder()
    {
        ZzRingBuffer ring(8);
        for (int i = 0; i < 5; ++i)
            ring.append(makeLine(QStringLiteral("line%1").arg(i)));
        QVector<ZzLogLine> batch = ring.takeOldest(3);
        QCOMPARE(batch.size(), 3);
        QCOMPARE(batch.first().text, QStringLiteral("line0"));
        QCOMPARE(batch.last().text, QStringLiteral("line2"));
        QCOMPARE(ring.count(), 2);
        QCOMPARE(ring.at(0).text, QStringLiteral("line3"));
    }

    /// @brief takeOldest 超过现存行数时按现存行数截取。
    void takeOldestClampsToCount()
    {
        ZzRingBuffer ring(8);
        ring.append(makeLine(QStringLiteral("only")));
        QCOMPARE(ring.takeOldest(10).size(), 1);
        QCOMPARE(ring.count(), 0);
    }

    /// @brief 溢出驱逐不破坏多字节文本与二进制属性。
    void attributesPreservedOnEviction()
    {
        ZzRingBuffer ring(2);
        ring.append({QStringLiteral("中文行"), QByteArray("\x01\x02", 2)});
        ring.append(makeLine("b"));
        ZzLogLine evicted;
        ring.append(makeLine("c"), &evicted);
        QCOMPARE(evicted.text, QStringLiteral("中文行"));
        QCOMPARE(evicted.attributes, QByteArray("\x01\x02", 2));
    }
};

QTEST_GUILESS_MAIN(ZzRingBufferTest)
#include "ZzRingBufferTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加一行：

```cmake
zz_add_log_test(ZzRingBufferTest)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
```

预期：构建失败，报错 `fatal error: 'ZzRingBuffer.h' file not found`。

- [ ] **步骤 3：编写最少实现代码**

创建 `src/log/ZzRingBuffer.h`：

```cpp
#pragma once

#include "ZzLogLine.h"

#include <QVector>

/**
 * @brief 热层内存环形缓冲区（规格 §5.2 热层）。
 *
 * 固定容量、固定内存占用，append / at / takeOldest 均为 O(1) 或 O(n) 批量，
 * 完整保留字符属性；写满后继续追加将驱逐最老行（由上层归档到温层）。
 */
class ZzRingBuffer
{
public:
    explicit ZzRingBuffer(qsizetype capacity = 10000);

    qsizetype capacity() const { return m_capacity; }
    qsizetype count() const { return m_count; }
    bool isFull() const { return m_count == m_capacity; }

    /**
     * @brief 追加一行；已满时驱逐最老行。
     * @param line 待追加行。
     * @param evicted 若非空且发生驱逐，写入被驱逐的最老行。
     * @return 发生驱逐返回 true。
     */
    bool append(const ZzLogLine &line, ZzLogLine *evicted = nullptr);

    /**
     * @brief 按序取走最老的至多 n 行（批量归档用）。
     * @param n 期望取走的行数。
     * @return 实际取走的行（可能少于 n）。
     */
    QVector<ZzLogLine> takeOldest(qsizetype n);

    /**
     * @brief 读取逻辑下标 index 处的行（0 为当前最老行）。
     * @note 调用方须保证 0 <= index < count()，否则触发断言。
     */
    const ZzLogLine &at(qsizetype index) const;

    void clear();

private:
    qsizetype physical(qsizetype index) const { return (m_head + index) % m_capacity; }

    QVector<ZzLogLine> m_lines;
    qsizetype m_capacity;
    qsizetype m_head = 0; ///< 最老行的物理下标
    qsizetype m_count = 0;
};
```

创建 `src/log/ZzRingBuffer.cpp`：

```cpp
#include "ZzRingBuffer.h"

ZzRingBuffer::ZzRingBuffer(qsizetype capacity)
    : m_lines(capacity > 0 ? capacity : 1)
    , m_capacity(capacity > 0 ? capacity : 1)
{
}

bool ZzRingBuffer::append(const ZzLogLine &line, ZzLogLine *evicted)
{
    if (isFull()) {
        if (evicted)
            *evicted = m_lines[m_head];
        m_lines[m_head] = line;
        m_head = (m_head + 1) % m_capacity;
        return true;
    }
    m_lines[physical(m_count)] = line;
    ++m_count;
    return false;
}

QVector<ZzLogLine> ZzRingBuffer::takeOldest(qsizetype n)
{
    n = qMin(n, m_count);
    QVector<ZzLogLine> out;
    out.reserve(n);
    for (qsizetype i = 0; i < n; ++i)
        out.append(m_lines[physical(i)]);
    m_head = (m_head + n) % m_capacity;
    m_count -= n;
    return out;
}

const ZzLogLine &ZzRingBuffer::at(qsizetype index) const
{
    Q_ASSERT(index >= 0 && index < m_count);
    return m_lines[physical(index)];
}

void ZzRingBuffer::clear()
{
    m_head = 0;
    m_count = 0;
}
```

在 `src/log/CMakeLists.txt` 的 `add_library(ZzLogEngine STATIC ...)` 源文件列表中 `ZzLineIndex.cpp` 之后追加两行：

```cmake
    ZzRingBuffer.h
    ZzRingBuffer.cpp
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzRingBufferTest$'
```

预期：`100% tests passed, 0 tests failed out of 1`，`ZzRingBufferTest` Passed。

- [ ] **步骤 5：Commit**

```bash
git add src/log tests/log
git commit -m "feat: 新增 ZzRingBuffer 热层环形缓冲"
```

---

### 任务 4：ZzMmapBuffer 温层 mmap + LZ4 追加/读取/重开（TDD）

**文件：**
- 创建：`src/log/ZzMmapBuffer.h`
- 创建：`src/log/ZzMmapBuffer.cpp`
- 创建：`tests/log/ZzMmapBufferTest.cpp`
- 修改：`src/log/CMakeLists.txt`（追加源文件）
- 修改：`tests/log/CMakeLists.txt`（注册测试）

本任务实现温层核心：追加（序列化 → 按 64KB 分块 → LZ4 压缩 → 写入映射文件）、按需解压读取（含 `ZzLineIndex` 块内定位 + 8 块解压缓存）、崩溃安全的重开扫描。容量裁剪留待任务 5（本任务 `maxLines` 仅存储不生效）。

存储格式（全部小端）：

```text
文件头（4096 字节）：magic u32 "ZZLM"(0x5A5A4C4D) | version u32(1) | skipBlocks u32 | 保留
块记录（重复）：magic u32 "ZZBK"(0x5A5A424B) | lineStart u64 | lineCount u32
              | uncompSize u32 | compSize u32 | compSize 字节 LZ4 负载
行编码（块未压缩数据内）：textLen u32 | attrLen u32 | UTF-8 文本 | 属性负载
```

- [ ] **步骤 1：编写失败的测试**

创建 `tests/log/ZzMmapBufferTest.cpp`：

```cpp
#include "ZzMmapBuffer.h"

#include <QTemporaryDir>
#include <QtTest>

/**
 * @brief ZzMmapBuffer 温层存储单元测试。
 */
class ZzMmapBufferTest : public QObject
{
    Q_OBJECT
    /// @brief 构造带中文与属性负载的测试行（平均每行约 40 字节）。
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("第 %1 行 the quick brown fox").arg(i),
                QByteArray("A") + QByteArray::number(qint64(i))};
    }

private slots:
    /// @brief 压缩解压一致性：5000 行（约 200KB，跨多个 64KB 块）随机抽查逐字节相等。
    void appendReadRoundtrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());

        QVector<ZzLogLine> written;
        for (quint64 i = 0; i < 5000; ++i)
            written.append(line(i));
        QVERIFY(buf.appendLines(written));
        QCOMPARE(buf.lineCount(), 5000ULL);
        QCOMPARE(buf.firstLineId(), 0ULL);

        for (quint64 i : {0ULL, 1ULL, 1024ULL, 2500ULL, 4096ULL, 4999ULL}) {
            QVector<ZzLogLine> got = buf.readLines(i, 1);
            QCOMPARE(got.size(), 1);
            QCOMPARE(got.first().text, line(i).text);
            QCOMPARE(got.first().attributes, line(i).attributes);
        }
    }

    /// @brief 分批追加后顺序全量读回与写入序列完全一致（滚动读取等价性的温层基础）。
    void sequentialReadMatchesWrite()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());
        for (quint64 batch = 0; batch < 10; ++batch) {
            QVector<ZzLogLine> lines;
            for (quint64 i = 0; i < 1000; ++i)
                lines.append(line(batch * 1000 + i));
            QVERIFY(buf.appendLines(lines));
        }
        QVector<ZzLogLine> all = buf.readLines(0, 10000);
        QCOMPARE(all.size(), 10000);
        for (int i = 0; i < 10000; ++i)
            QCOMPARE(all[i].text, line(quint64(i)).text);
    }

    /// @brief 关闭重开后数据完整，且可继续向后追加。
    void reopenPreservesData()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        {
            ZzMmapBuffer buf(path);
            QVERIFY(buf.open());
            QVERIFY(buf.appendLines({line(0), line(1), line(2)}));
            buf.flush();
            buf.close();
        }
        ZzMmapBuffer buf(path);
        QVERIFY(buf.open());
        QCOMPARE(buf.lineCount(), 3ULL);
        QCOMPARE(buf.readLines(0, 3).size(), 3);
        QVERIFY(buf.appendLines({line(3)}));
        QCOMPARE(buf.lineCount(), 4ULL);
        QCOMPARE(buf.readLines(3, 1).first().text, line(3).text);
    }

    /// @brief 越界读取按实际可得行数返回。
    void readOutOfRangeReturnsLess()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());
        QVERIFY(buf.appendLines({line(0), line(1)}));
        QCOMPARE(buf.readLines(0, 100).size(), 2);
        QVERIFY(buf.readLines(5, 1).isEmpty());
    }
};

QTEST_GUILESS_MAIN(ZzMmapBufferTest)
#include "ZzMmapBufferTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加一行：

```cmake
zz_add_log_test(ZzMmapBufferTest)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
```

预期：构建失败，报错 `fatal error: 'ZzMmapBuffer.h' file not found`。

- [ ] **步骤 3：编写实现代码**

创建 `src/log/ZzMmapBuffer.h`：

```cpp
#pragma once

#include "ZzLineIndex.h"
#include "ZzLogLine.h"

#include <QCache>
#include <QFile>
#include <QVector>

/**
 * @brief 温层存储：mmap 内存映射文件 + LZ4 分块压缩（规格 §5.2 温层）。
 *
 * - 每 64KB 未压缩数据压缩为一块，按需解压（单块微秒级），附带 8 块解压缓存；
 * - 通过 ZzLineIndex（每 1024 行一条）定位块内偏移，块内小范围扫描；
 * - 行 ID 绝对单调递增且永不复用，裁剪/压缩文件后保持可读；
 * - 文件预分配并按 4MB 粒度增长；尾部半写块在重开扫描时被安全忽略（崩溃安全）；
 * - 超出 maxLines 时按整块粒度丢弃最老数据（v0.2 改为归档冷层）。
 *
 * @note 线程安全由上层（ZzLogEngine 的 QReadWriteLock）保证，本类自身不加锁。
 */
class ZzMmapBuffer
{
public:
    explicit ZzMmapBuffer(const QString &filePath, quint64 maxLines = 1000000);
    ~ZzMmapBuffer();

    ZzMmapBuffer(const ZzMmapBuffer &) = delete;
    ZzMmapBuffer &operator=(const ZzMmapBuffer &) = delete;

    /**
     * @brief 打开（或创建）映射文件并重建块表。
     * @return 文件无法打开、扩容或映射失败、魔数不匹配时返回 false。
     */
    bool open();

    /// @brief 解除映射并关闭文件（unmap 时脏页由 OS 落盘）。
    void close();

    bool isOpen() const { return m_file.isOpen(); }

    /// @brief 当前最老存活行 ID（无数据时等于下一个待分配 ID）。
    quint64 firstLineId() const;

    quint64 lineCount() const { return m_lineCount; }
    quint64 maxLines() const { return m_maxLines; }

    /**
     * @brief 追加一批行（序列化 → 64KB 分块 → LZ4 压缩 → 写映射区）。
     * @param lines 待追加行。
     * @param errorString 失败时输出原因，可为空。
     * @return 压缩或扩容失败返回 false（已写入的部分保持一致可读）。
     */
    bool appendLines(const QVector<ZzLogLine> &lines, QString *errorString = nullptr);

    /**
     * @brief 读取 [startId, startId + count) 区间内的行。
     * @return 实际读到的行；起点早于 firstLineId() 或超出末尾时按实际可得数量返回。
     */
    QVector<ZzLogLine> readLines(quint64 startId, quint64 count) const;

    /// @brief 预加载：将 lineId 所在块及其后一块解压进缓存（供滚动方向预取）。
    void preload(quint64 lineId) const;

    /// @brief 刷新文件流缓冲；mmap 脏页由 OS 回写，close/unmap 时保证落盘。
    void flush();

    static constexpr quint64 kChunkSize = 64 * 1024; ///< 单块未压缩数据上限（字节）

private:
    struct BlockInfo {
        quint64 lineStart = 0;  ///< 块首行 ID
        quint32 lineCount = 0;  ///< 块内行数
        qint64 fileOffset = 0;  ///< 块头在文件中的偏移
        quint32 uncompSize = 0; ///< 未压缩字节数
        quint32 compSize = 0;   ///< 压缩后字节数
    };

    bool remap();
    bool scanFile();
    void writeHeader();
    bool ensureCapacity(qint64 extraBytes);
    bool writeBlock(const QByteArray &chunk, quint64 lineStart, quint32 lineCount,
                    QString *errorString);
    QByteArray decompressBlock(const BlockInfo &block) const;
    qsizetype findBlockIndex(quint64 lineId) const;

    QFile m_file;
    uchar *m_map = nullptr;     ///< 全文件映射基址
    qint64 m_mappedSize = 0;    ///< 映射长度（== 文件长度）
    qint64 m_appendOffset = 0;  ///< 下一块写入偏移
    quint64 m_maxLines;
    quint64 m_lineCount = 0;    ///< 存活行数
    quint64 m_nextLineId = 0;   ///< 下一个待分配行 ID
    quint32 m_skipBlocks = 0;   ///< 文件头部已逻辑丢弃的块数（持久化于文件头）
    qint64 m_droppedBytes = 0;  ///< 已逻辑丢弃的字节数（触发物理压缩用）
    QVector<BlockInfo> m_blocks;              ///< 存活块表（按 lineStart 递增）
    ZzLineIndex m_lineIndex;                  ///< 块内行偏移索引
    mutable QCache<quint64, QByteArray> m_blockCache; ///< 解压缓存，键为块首行 ID
};
```

创建 `src/log/ZzMmapBuffer.cpp`：

```cpp
#include "ZzMmapBuffer.h"

#include <lz4.h>

#include <QtEndian>
#include <cstring>

namespace {
constexpr quint32 kFileMagic = 0x5A5A4C4D;   ///< 文件魔数 "ZZLM"
constexpr quint32 kFileVersion = 1;          ///< 存储格式版本
constexpr quint32 kBlockMagic = 0x5A5A424B;  ///< 块魔数 "ZZBK"
constexpr qint64 kHeaderSize = 4096;         ///< 文件头占一页
constexpr qint64 kGrowGranularity = 4 * 1024 * 1024; ///< 映射扩容粒度 4MB
constexpr qsizetype kBlockHeaderSize = 24;   ///< 块头字节数
constexpr int kCacheBlocks = 8;              ///< 解压缓存块数

void putU32(char *p, quint32 v) { v = qToLittleEndian(v); std::memcpy(p, &v, 4); }
void putU64(char *p, quint64 v) { v = qToLittleEndian(v); std::memcpy(p, &v, 8); }
quint32 getU32(const char *p) { quint32 v; std::memcpy(&v, p, 4); return qFromLittleEndian(v); }
quint64 getU64(const char *p) { quint64 v; std::memcpy(&v, p, 8); return qFromLittleEndian(v); }

/// @brief 行编码：textLen u32 | attrLen u32 | UTF-8 文本 | 属性负载。
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

/// @brief 跳过一行，返回下一行偏移；数据截断返回 -1。
qint64 skipLine(const QByteArray &data, qint64 off)
{
    if (off < 0 || off + 8 > data.size())
        return -1;
    const char *p = data.constData() + off;
    const quint32 textLen = getU32(p);
    const quint32 attrLen = getU32(p + 4);
    const qint64 next = off + 8 + textLen + attrLen;
    return next > data.size() ? -1 : next;
}

/// @brief 解析一行，返回下一行偏移；数据截断返回 -1。
qint64 parseLine(const QByteArray &data, qint64 off, ZzLogLine *out)
{
    const qint64 next = skipLine(data, off);
    if (next < 0)
        return -1;
    const char *p = data.constData() + off;
    const quint32 textLen = getU32(p);
    const quint32 attrLen = getU32(p + 4);
    out->text = QString::fromUtf8(p + 8, qsizetype(textLen));
    out->attributes = QByteArray(p + 8 + textLen, qsizetype(attrLen));
    return next;
}
} // namespace

ZzMmapBuffer::ZzMmapBuffer(const QString &filePath, quint64 maxLines)
    : m_file(filePath)
    , m_maxLines(qMax<quint64>(maxLines, 1))
{
}

ZzMmapBuffer::~ZzMmapBuffer()
{
    close();
}

bool ZzMmapBuffer::open()
{
    if (m_file.isOpen())
        return true;
    if (!m_file.open(QIODevice::ReadWrite))
        return false;

    const bool isNew = m_file.size() < kHeaderSize;
    if (isNew && !m_file.resize(kHeaderSize + kGrowGranularity)) {
        m_file.close();
        return false;
    }
    if (!remap()) {
        m_file.close();
        return false;
    }

    if (isNew) {
        m_skipBlocks = 0;
        writeHeader();
        m_appendOffset = kHeaderSize;
    } else {
        const char *base = reinterpret_cast<const char *>(m_map);
        if (getU32(base) != kFileMagic) { // 非本格式文件
            close();
            return false;
        }
        m_skipBlocks = getU32(base + 8);
        scanFile();
    }
    m_blockCache.setMaxCost(kCacheBlocks);
    return true;
}

void ZzMmapBuffer::close()
{
    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    m_mappedSize = 0;
    if (m_file.isOpen())
        m_file.close();
    m_blockCache.clear();
}

quint64 ZzMmapBuffer::firstLineId() const
{
    return m_blocks.isEmpty() ? m_nextLineId : m_blocks.first().lineStart;
}

bool ZzMmapBuffer::appendLines(const QVector<ZzLogLine> &lines, QString *errorString)
{
    if (!m_map) {
        if (errorString)
            *errorString = QStringLiteral("温层文件未打开");
        return false;
    }
    QByteArray chunk;
    chunk.reserve(qsizetype(kChunkSize) + 256);
    quint64 chunkFirstId = m_nextLineId;
    quint32 chunkLines = 0;

    for (const ZzLogLine &line : lines) {
        const QByteArray encoded = encodeLine(line);
        // 单块不超 64KB（单行超过 64KB 时独占一块）
        if (chunkLines > 0 && chunk.size() + encoded.size() > qint64(kChunkSize)) {
            if (!writeBlock(chunk, chunkFirstId, chunkLines, errorString))
                return false;
            chunk.clear();
            chunkFirstId = m_nextLineId;
            chunkLines = 0;
        }
        m_lineIndex.recordLine(m_nextLineId, chunkFirstId, quint64(chunk.size()));
        chunk.append(encoded);
        ++chunkLines;
        ++m_nextLineId;
        ++m_lineCount;
    }
    if (chunkLines > 0 && !writeBlock(chunk, chunkFirstId, chunkLines, errorString))
        return false;
    return true;
}

QVector<ZzLogLine> ZzMmapBuffer::readLines(quint64 startId, quint64 count) const
{
    QVector<ZzLogLine> out;
    if (!m_map || count == 0 || m_blocks.isEmpty())
        return out;

    quint64 id = qMax(startId, firstLineId());
    const quint64 end = qMin(startId + count, m_nextLineId);
    if (end <= id)
        return out; // 区间完全落在已丢弃/未写入范围
    out.reserve(qsizetype(qMin<quint64>(end - id, 100000)));

    while (id < end) {
        const qsizetype bi = findBlockIndex(id);
        if (bi < 0)
            break;
        const BlockInfo &block = m_blocks[bi];
        const QByteArray data = decompressBlock(block);
        if (data.isEmpty())
            break; // 块损坏：按可得数据返回

        // 块内定位：优先行索引（每 1024 行一条），否则从块首小范围扫描
        quint64 scanId = block.lineStart;
        qint64 off = 0;
        ZzLineIndex::Entry e;
        if (m_lineIndex.locate(id, &e) && e.blockFirstLineId == block.lineStart) {
            scanId = e.lineId;
            off = qint64(e.offset);
        }
        while (scanId < id) {
            const qint64 next = skipLine(data, off);
            if (next < 0)
                return out;
            off = next;
            ++scanId;
        }

        const quint64 blockEnd = block.lineStart + block.lineCount;
        while (id < end && id < blockEnd) {
            ZzLogLine line;
            const qint64 next = parseLine(data, off, &line);
            if (next < 0)
                return out;
            out.append(line);
            off = next;
            ++id;
        }
    }
    return out;
}

void ZzMmapBuffer::preload(quint64 lineId) const
{
    if (!m_map)
        return;
    const qsizetype bi = findBlockIndex(lineId);
    if (bi < 0)
        return;
    decompressBlock(m_blocks[bi]); // 取块即入缓存
    if (bi + 1 < m_blocks.size())
        decompressBlock(m_blocks[bi + 1]);
}

void ZzMmapBuffer::flush()
{
    if (m_file.isOpen())
        m_file.flush();
}

bool ZzMmapBuffer::remap()
{
    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    m_mappedSize = m_file.size();
    if (m_mappedSize <= 0)
        return false;
    m_map = m_file.map(0, m_mappedSize);
    return m_map != nullptr;
}

bool ZzMmapBuffer::scanFile()
{
    m_blocks.clear();
    m_lineIndex.clear();
    m_lineCount = 0;
    m_nextLineId = 0;
    m_droppedBytes = 0;

    const char *base = reinterpret_cast<const char *>(m_map);
    qint64 offset = kHeaderSize;
    QVector<BlockInfo> physical;
    while (offset + kBlockHeaderSize <= m_mappedSize) {
        if (getU32(base + offset) != kBlockMagic)
            break; // 到达未写区域（预分配零页）或尾部半写块
        BlockInfo b;
        b.fileOffset = offset;
        b.lineStart = getU64(base + offset + 4);
        b.lineCount = getU32(base + offset + 12);
        b.uncompSize = getU32(base + offset + 16);
        b.compSize = getU32(base + offset + 20);
        const qint64 next = offset + kBlockHeaderSize + b.compSize;
        if (next > m_mappedSize)
            break; // 半写块
        physical.append(b);
        offset = next;
    }
    m_appendOffset = offset;

    // 跳过文件头记录的已逻辑丢弃块
    const qsizetype skip = qMin<qsizetype>(m_skipBlocks, physical.size());
    for (qsizetype i = 0; i < skip; ++i)
        m_droppedBytes += kBlockHeaderSize + physical[i].compSize;
    m_blocks = physical.mid(skip);
    for (const BlockInfo &b : std::as_const(m_blocks))
        m_lineCount += b.lineCount;
    if (!physical.isEmpty())
        m_nextLineId = physical.last().lineStart + physical.last().lineCount;
    return true;
}

void ZzMmapBuffer::writeHeader()
{
    char *base = reinterpret_cast<char *>(m_map);
    putU32(base, kFileMagic);
    putU32(base + 4, kFileVersion);
    putU32(base + 8, m_skipBlocks);
}

bool ZzMmapBuffer::ensureCapacity(qint64 extraBytes)
{
    if (m_appendOffset + extraBytes <= m_mappedSize)
        return true;
    const qint64 need = m_appendOffset + extraBytes - m_mappedSize;
    const qint64 grow =
        ((qMax(need, kGrowGranularity) + kGrowGranularity - 1) / kGrowGranularity) * kGrowGranularity;
    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    if (!m_file.resize(m_mappedSize + grow))
        return false; // 磁盘满等：由上层降级为纯内存模式
    return remap();
}

bool ZzMmapBuffer::writeBlock(const QByteArray &chunk, quint64 lineStart, quint32 lineCount,
                              QString *errorString)
{
    const int bound = LZ4_compressBound(int(chunk.size()));
    QByteArray compressed;
    compressed.resize(bound);
    const int compSize = LZ4_compress_default(chunk.constData(), compressed.data(),
                                              int(chunk.size()), bound);
    if (compSize <= 0) {
        if (errorString)
            *errorString = QStringLiteral("LZ4 压缩失败");
        return false;
    }
    compressed.resize(compSize);

    if (!ensureCapacity(kBlockHeaderSize + compSize)) {
        if (errorString)
            *errorString = QStringLiteral("温层映射文件扩容失败（磁盘空间不足？）");
        return false;
    }

    char *p = reinterpret_cast<char *>(m_map) + m_appendOffset;
    putU32(p, kBlockMagic);
    putU64(p + 4, lineStart);
    putU32(p + 12, lineCount);
    putU32(p + 16, quint32(chunk.size()));
    putU32(p + 20, quint32(compSize));
    std::memcpy(p + kBlockHeaderSize, compressed.constData(), size_t(compSize));

    m_blocks.append({lineStart, lineCount, m_appendOffset, quint32(chunk.size()), quint32(compSize)});
    m_appendOffset += kBlockHeaderSize + compSize;
    return true;
}

QByteArray ZzMmapBuffer::decompressBlock(const BlockInfo &block) const
{
    if (const QByteArray *cached = m_blockCache.object(block.lineStart))
        return *cached;
    QByteArray out;
    out.resize(qsizetype(block.uncompSize));
    const int n = LZ4_decompress_safe(
        reinterpret_cast<const char *>(m_map) + block.fileOffset + kBlockHeaderSize,
        out.data(), int(block.compSize), int(block.uncompSize));
    if (n != int(block.uncompSize))
        return {}; // 数据损坏
    m_blockCache.insert(block.lineStart, new QByteArray(out), 1);
    return out;
}

qsizetype ZzMmapBuffer::findBlockIndex(quint64 lineId) const
{
    if (m_blocks.isEmpty() || lineId < m_blocks.first().lineStart)
        return -1;
    // 块表按 lineStart 递增，二分找最后一个 lineStart <= lineId 的块
    qsizetype lo = 0;
    qsizetype hi = m_blocks.size() - 1;
    qsizetype best = 0;
    while (lo <= hi) {
        const qsizetype mid = (lo + hi) / 2;
        if (m_blocks[mid].lineStart <= lineId) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    return best;
}
```

在 `src/log/CMakeLists.txt` 的源文件列表中 `ZzRingBuffer.cpp` 之后追加两行：

```cmake
    ZzMmapBuffer.h
    ZzMmapBuffer.cpp
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzMmapBufferTest$'
```

预期：`ZzMmapBufferTest` Passed，`100% tests passed`。

- [ ] **步骤 5：回归既有测试并 Commit**

```bash
ctest --test-dir build/debug --output-on-failure
git add src/log tests/log
git commit -m "feat: 新增 ZzMmapBuffer 温层 mmap+LZ4 追加与读取"
```

预期：全部测试（ZzLineIndexTest、ZzRingBufferTest、ZzMmapBufferTest）通过。

---

### 任务 5：ZzMmapBuffer 容量裁剪、物理压缩与预读验证（TDD）

**文件：**
- 修改：`src/log/ZzMmapBuffer.h`（追加私有方法声明）
- 修改：`src/log/ZzMmapBuffer.cpp`（实现裁剪与压缩）
- 修改：`tests/log/ZzMmapBufferTest.cpp`（追加测试用例）

- [ ] **步骤 1：编写失败的测试**

在 `tests/log/ZzMmapBufferTest.cpp` 的 `private slots:` 区域末尾（`readOutOfRangeReturnsLess` 之后）追加两个用例：

```cpp
    /// @brief 超出 maxLines 时按整块粒度丢弃最老数据，剩余数据完整连续。
    void capacityDropsOldestBlocks()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")), /*maxLines=*/2000);
        QVERIFY(buf.open());
        QVector<ZzLogLine> lines;
        for (quint64 i = 0; i < 5000; ++i)
            lines.append(line(i));
        QVERIFY(buf.appendLines(lines));

        QVERIFY(buf.lineCount() <= 2000ULL);
        QVERIFY(buf.lineCount() > 0ULL);
        QVERIFY(buf.firstLineId() > 0ULL);
        // 剩余数据完整且与写入序列逐字节一致
        QVector<ZzLogLine> rest = buf.readLines(buf.firstLineId(), buf.lineCount());
        QCOMPARE(quint64(rest.size()), buf.lineCount());
        for (int i = 0; i < rest.size(); ++i)
            QCOMPARE(rest[i].text, line(buf.firstLineId() + quint64(i)).text);
        // 已丢弃的行读不到（区间被裁剪后为空）
        QVERIFY(buf.readLines(0, 1).isEmpty());
    }

    /// @brief 裁剪状态持久化：重开后最老行不复活，且可继续追加。
    void dropSurvivesReopen()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        quint64 firstAfterDrop = 0;
        {
            ZzMmapBuffer buf(path, /*maxLines=*/2000);
            QVERIFY(buf.open());
            QVector<ZzLogLine> lines;
            for (quint64 i = 0; i < 5000; ++i)
                lines.append(line(i));
            QVERIFY(buf.appendLines(lines));
            buf.flush();
            firstAfterDrop = buf.firstLineId();
            buf.close();
        }
        ZzMmapBuffer buf(path, 2000);
        QVERIFY(buf.open());
        QCOMPARE(buf.firstLineId(), firstAfterDrop);
        QVERIFY(buf.appendLines({line(5000)}));
        QCOMPARE(buf.readLines(5000, 1).first().text, line(5000).text);
    }

    /// @brief 预读后读取结果不变（预读只影响缓存，不影响语义）。
    void preloadKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzMmapBuffer buf(dir.filePath(QStringLiteral("warm.log")));
        QVERIFY(buf.open());
        QVector<ZzLogLine> lines;
        for (quint64 i = 0; i < 3000; ++i)
            lines.append(line(i));
        QVERIFY(buf.appendLines(lines));
        buf.preload(1500);
        buf.preload(0); // 边界：首块
        QVector<ZzLogLine> got = buf.readLines(1490, 30);
        QCOMPARE(got.size(), 30);
        for (int i = 0; i < 30; ++i)
            QCOMPARE(got[i].text, line(1490 + quint64(i)).text);
    }
```

注意：`capacityDropsOldestBlocks` 断言 `lineCount() <= 2000`，当前实现不做裁剪，实测约 5000 行，必然失败。

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzMmapBufferTest$'
```

预期：`capacityDropsOldestBlocks` FAIL（`lineCount()` 实测 5000 而非 ≤2000）；`dropSurvivesReopen` 与 `preloadKeepsReadsCorrect` 此时即可通过——前者的回归价值在实现裁剪后防止"重开后最老行复活"，后者锁定预读不改变语义的性质。其余既有用例 PASS。

- [ ] **步骤 3：编写实现代码**

在 `src/log/ZzMmapBuffer.h` 的 `private:` 声明区 `findBlockIndex` 声明之后追加两行：

```cpp
    void dropOldestBlocks();
    void compact();
```

在 `src/log/ZzMmapBuffer.cpp` 中做两处修改。

修改一：在 `appendLines` 函数末尾（第二个 `writeBlock` 调用之后、`return true;` 之前）插入裁剪调用，使函数结尾变为：

```cpp
    if (chunkLines > 0 && !writeBlock(chunk, chunkFirstId, chunkLines, errorString))
        return false;
    dropOldestBlocks();
    return true;
}
```

修改二：在 `findBlockIndex` 实现之后追加两个函数实现：

```cpp
void ZzMmapBuffer::dropOldestBlocks()
{
    // 按整块粒度丢弃最老数据，至少保留一块（v0.2 改为归档冷层）
    while (m_lineCount > m_maxLines && m_blocks.size() > 1) {
        const BlockInfo &oldest = m_blocks.first();
        m_lineCount -= oldest.lineCount;
        m_droppedBytes += kBlockHeaderSize + oldest.compSize;
        ++m_skipBlocks;
        m_blocks.removeFirst();
    }
    if (m_skipBlocks > 0)
        writeHeader(); // 持久化丢弃进度，重开后最老行不复活
    // 浪费空间超过已用一半且文件超过一个扩容粒度时，物理压缩文件
    if (m_droppedBytes > m_appendOffset / 2 && m_appendOffset > kGrowGranularity)
        compact();
}

void ZzMmapBuffer::compact()
{
    // 把存活块原样（保持压缩态）搬运到新文件头部之后，块内数据与行 ID 不变
    qint64 total = 0;
    for (const BlockInfo &b : std::as_const(m_blocks))
        total += kBlockHeaderSize + b.compSize;

    QByteArray alive;
    alive.reserve(qsizetype(total));
    for (BlockInfo &b : m_blocks) {
        const qint64 oldOffset = b.fileOffset;
        alive.append(reinterpret_cast<const char *>(m_map) + oldOffset,
                     qsizetype(kBlockHeaderSize + b.compSize));
        b.fileOffset = kHeaderSize + alive.size() - (kBlockHeaderSize + b.compSize);
    }

    if (m_map) {
        m_file.unmap(m_map);
        m_map = nullptr;
    }
    if (!m_file.resize(kHeaderSize + alive.size() + kGrowGranularity) || !remap()) {
        // 压缩失败（磁盘满等）：m_map 为空后读写路径均按空数据安全返回，
        // 由上层（ZzLogEngine）走 I/O 失败降级路径。
        return;
    }
    std::memcpy(m_map + kHeaderSize, alive.constData(), size_t(alive.size()));
    m_appendOffset = kHeaderSize + alive.size();
    m_skipBlocks = 0;
    m_droppedBytes = 0;
    writeHeader();
}
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzMmapBufferTest$'
```

预期：`ZzMmapBufferTest` 全部用例（含 `capacityDropsOldestBlocks`、`dropSurvivesReopen`、`preloadKeepsReadsCorrect`）PASS。

- [ ] **步骤 5：回归并 Commit**

```bash
ctest --test-dir build/debug --output-on-failure
git add src/log tests/log
git commit -m "feat: ZzMmapBuffer 支持容量裁剪、物理压缩与块预读"
```

预期：全部测试通过。

---

### 任务 6：ZzLogArchiveWorker + ZzLogEngine 门面（TDD）

**文件：**
- 创建：`src/log/ZzLogArchiveWorker.h`
- 创建：`src/log/ZzLogArchiveWorker.cpp`
- 创建：`src/log/ZzLogEngine.h`
- 创建：`src/log/ZzLogEngine.cpp`
- 创建：`tests/log/ZzLogEngineTest.cpp`
- 修改：`src/log/CMakeLists.txt`（追加源文件）
- 修改：`tests/log/CMakeLists.txt`（注册测试）

- [ ] **步骤 1：编写失败的测试**

创建 `tests/log/ZzLogEngineTest.cpp`：

```cpp
#include "ZzLogEngine.h"

#include <QFile>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

/**
 * @brief ZzLogEngine 门面单元测试：归档往返、滚动读取等价性、预加载、降级。
 *
 * 测试配置：热层 100 行、归档批 16 行、温层上限 10 万行。
 * 追加 N 行时热层按批驱逐：第 100+16k 次追加（0 起计）驱逐 16 行。
 */
class ZzLogEngineTest : public QObject
{
    Q_OBJECT
    static ZzLogEngine::Config testConfig(const QString &warmPath)
    {
        ZzLogEngine::Config c;
        c.hotCapacity = 100;
        c.archiveBatchSize = 16;
        c.warmMaxLines = 100000;
        c.warmFilePath = warmPath;
        return c;
    }
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("engine-line-%1").arg(i),
                QByteArray("E") + QByteArray::number(qint64(i))};
    }

private slots:
    /// @brief 归档往返：溢出热层的行经温层完整读回（含属性），最新行仍走热层。
    void archiveRoundtrip()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        for (quint64 i = 0; i < 250; ++i)
            engine.appendLine(line(i));
        engine.flush(); // 等待全部已排队批次归档完成
        QCOMPARE(engine.totalLines(), 250ULL);
        QCOMPARE(engine.firstLineNo(), 0ULL);

        ZzLogLine got;
        QVERIFY(engine.getLine(0, &got)); // 最老行：温层
        QCOMPARE(got.text, line(0).text);
        QCOMPARE(got.attributes, line(0).attributes);
        QVERIFY(engine.getLine(249, &got)); // 最新行：热层
        QCOMPARE(got.text, line(249).text);
    }

    /// @brief 滚动读取等价性：跨热/温层的滑动窗口与写入序列逐行一致。
    void scrollReadEquivalence()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        constexpr quint64 N = 500; // 400 行入温层，100 行留热层
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush();
        for (quint64 start = 0; start + 60 <= N; start += 17) {
            QVector<ZzLogLine> window = engine.getLines(start, 60);
            QCOMPARE(window.size(), 60);
            for (int j = 0; j < 60; ++j)
                QCOMPARE(window[j].text, line(start + quint64(j)).text);
        }
    }

    /// @brief 预加载不改变读取语义。
    void preloadKeepsReadsCorrect()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine engine(testConfig(dir.filePath(QStringLiteral("warm.log"))));
        QVERIFY(engine.open());
        for (quint64 i = 0; i < 300; ++i)
            engine.appendLine(line(i));
        engine.flush();
        engine.preload(10);
        QTest::qWait(50); // 让预加载在归档线程执行完
        QVector<ZzLogLine> got = engine.getLines(10, 20);
        QCOMPARE(got.size(), 20);
        QCOMPARE(got.first().text, line(10).text);
    }

    /// @brief 纯内存模式（无温层文件）：容量受限于热层，最老行被驱逐。
    void memoryOnlyModeCapsAtHotCapacity()
    {
        ZzLogEngine engine(testConfig(QString()));
        QVERIFY(engine.open());
        QVERIFY(engine.isMemoryOnly());
        for (quint64 i = 0; i < 250; ++i)
            engine.appendLine(line(i));
        // 10 批 × 16 行被驱逐：剩余 90 行，首行 ID 160
        QCOMPARE(engine.totalLines(), 90ULL);
        QCOMPARE(engine.firstLineNo(), 160ULL);
        ZzLogLine got;
        QVERIFY(engine.getLine(160, &got));
        QCOMPARE(got.text, line(160).text);
    }

    /// @brief 温层文件无法打开时降级纯内存并发射信号（规格 §八）。
    void warmOpenFailureDegrades()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        // 用一个已存在的文件当“父目录”，其下路径必然打不开（跨平台）
        const QString blocker = dir.filePath(QStringLiteral("blocker"));
        QFile f(blocker);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();

        ZzLogEngine::Config c = testConfig(blocker + QStringLiteral("/warm.log"));
        ZzLogEngine engine(c);
        QSignalSpy spy(&engine, &ZzLogEngine::degradedToMemoryOnly);
        QVERIFY(engine.open()); // 降级视为可用
        QVERIFY(engine.isMemoryOnly());
        QCOMPARE(spy.count(), 1);
        for (quint64 i = 0; i < 200; ++i)
            engine.appendLine(line(i));
        // 7 批 × 16 行被驱逐：剩余 88 行
        QCOMPARE(engine.totalLines(), 88ULL);
    }

    /// @brief 引擎析构后温层数据保留，重开引擎可继续读（热层随进程销毁属设计行为）。
    void engineReopenRestoresWarmLayer()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        const QString path = dir.filePath(QStringLiteral("warm.log"));
        {
            ZzLogEngine engine(testConfig(path));
            QVERIFY(engine.open());
            for (quint64 i = 0; i < 200; ++i)
                engine.appendLine(line(i));
            engine.flush(); // 温层 112 行
        }
        ZzLogEngine engine(testConfig(path));
        QVERIFY(engine.open());
        QVERIFY(engine.totalLines() >= 100ULL);
        ZzLogLine got;
        QVERIFY(engine.getLine(engine.firstLineNo(), &got));
        QCOMPARE(got.text, line(engine.firstLineNo()).text);
    }
};

QTEST_GUILESS_MAIN(ZzLogEngineTest)
#include "ZzLogEngineTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加一行：

```cmake
zz_add_log_test(ZzLogEngineTest)
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake -S . -B build/debug -DCMAKE_BUILD_TYPE=Debug && cmake --build build/debug -j8
```

预期：构建失败，报错 `fatal error: 'ZzLogEngine.h' file not found`。

- [ ] **步骤 3：编写实现代码**

创建 `src/log/ZzLogArchiveWorker.h`：

```cpp
#pragma once

#include "ZzLogLine.h"

#include <QObject>
#include <QReadWriteLock>
#include <QVector>

#include <atomic>

class ZzMmapBuffer;

/**
 * @brief 温层归档工作对象：运行在 ZzLogEngine 拥有的独立 QThread 中。
 *
 * 负责把热层驱逐出的批量行写入温层（绝不阻塞 I/O 线程与 UI 线程，规格 §5.3），
 * 并执行滚动方向上的块预加载；通过传入的原子计数器向读路径发布温层区间。
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
     */
    ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                       std::atomic<quint64> *warmBase, std::atomic<quint64> *warmCount,
                       QObject *parent = nullptr);

public slots:
    /// @brief 归档一批行到温层；失败发射 archiveFailed。
    void archiveLines(const QVector<ZzLogLine> &lines);

    /// @brief 预加载 lineId 所在块及后一块到解压缓存。
    void preloadAround(quint64 lineId);

    /// @brief 冲刷温层文件缓冲（测试与关闭前的确定性同步点）。
    void flush();

signals:
    void archiveCompleted();                    ///< 一批行完成归档
    void archiveFailed(const QString &message); ///< 温层 I/O 失败

private:
    ZzMmapBuffer *m_buffer;
    QReadWriteLock *m_lock;
    std::atomic<quint64> *m_warmBase;
    std::atomic<quint64> *m_warmCount;
};
```

创建 `src/log/ZzLogArchiveWorker.cpp`：

```cpp
#include "ZzLogArchiveWorker.h"

#include "ZzMmapBuffer.h"

ZzLogArchiveWorker::ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                                       std::atomic<quint64> *warmBase,
                                       std::atomic<quint64> *warmCount, QObject *parent)
    : QObject(parent)
    , m_buffer(buffer)
    , m_lock(lock)
    , m_warmBase(warmBase)
    , m_warmCount(warmCount)
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
}

void ZzLogArchiveWorker::preloadAround(quint64 lineId)
{
    QReadLocker locker(m_lock);
    m_buffer->preload(lineId);
}

void ZzLogArchiveWorker::flush()
{
    QWriteLocker locker(m_lock);
    m_buffer->flush();
}
```

创建 `src/log/ZzLogEngine.h`：

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

class ZzLogArchiveWorker;
class ZzMmapBuffer;

/**
 * @brief 日志引擎门面：热层环形缓冲 + 温层 mmap/LZ4 的统一读写入口（规格 §五）。
 *
 * 行号约定：绝对单调递增 ID，可读窗口为 [firstLineNo(), firstLineNo()+totalLines())；
 * 温层超限丢弃或纯内存模式驱逐时 firstLineNo() 前移。
 *
 * 线程模型：appendLine 可在任意线程调用（通常为终端 I/O 线程）；归档与预加载
 * 在内部独立 QThread 中执行，绝不阻塞调用方；getLine/getLines 可在任意线程调用
 * （热层互斥锁 + 温层读写锁保护）。归档是异步的，flush() 返回后所有已排队批次
 * 保证完成归档（测试与关闭前的确定性同步点）。
 *
 * 温层 I/O 失败（磁盘满等）时降级为纯内存模式并发射 degradedToMemoryOnly，
 * 不影响终端交互（规格 §八）。
 *
 * @code
 * ZzLogEngine::Config config;
 * config.warmFilePath = QStringLiteral("/path/to/session-warm.log");
 * ZzLogEngine engine(config);
 * if (engine.open()) {
 *     engine.appendLine({QStringLiteral("hello"), QByteArray()});
 *     QVector<ZzLogLine> window = engine.getLines(0, 60);
 * }
 * @endcode
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
        QString warmFilePath;              ///< 温层 mmap 文件路径；为空则纯内存模式
    };

    explicit ZzLogEngine(const Config &config, QObject *parent = nullptr);
    ~ZzLogEngine() override;

    /**
     * @brief 打开引擎（含温层文件）；温层打开失败时降级纯内存并发射信号。
     * @return 恒返回 true（降级视为可用）；通过 isMemoryOnly() 查询实际模式。
     * @note 建议在 open() 之前连接 degradedToMemoryOnly 信号。
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
     * @return 实际读到的行，按行 ID 递增排列。
     */
    QVector<ZzLogLine> getLines(quint64 startLine, quint64 count) const;

    quint64 totalLines() const;   ///< 当前可读总行数（热层 + 温层）
    quint64 firstLineNo() const;  ///< 当前最老可读行 ID
    bool isMemoryOnly() const { return m_memoryOnly; }

    /// @brief 预加载 lineNo 附近温层块到解压缓存（异步，不阻塞调用方）。
    void preload(quint64 lineNo);

    /**
     * @brief 阻塞至全部已排队归档批次完成并冲刷温层缓冲。
     * @note 不得从归档线程内部调用（BlockingQueuedConnection 会死锁）。
     */
    void flush();

signals:
    void archiveFinished();                            ///< 一批行完成温层归档
    void degradedToMemoryOnly(const QString &reason);  ///< 温层不可用，降级纯内存

private:
    Config m_config;
    ZzRingBuffer m_hot;             ///< 热层（m_hotMutex 保护）
    mutable QMutex m_hotMutex;
    std::unique_ptr<ZzMmapBuffer> m_warm; ///< 温层（纯内存模式为空）
    QReadWriteLock m_warmLock;      ///< 温层读写锁（读路径读锁、归档写锁）
    QThread m_workerThread;         ///< 归档线程
    ZzLogArchiveWorker *m_worker = nullptr; ///< 运行于归档线程
    std::atomic<quint64> m_warmBase{0};  ///< 温层首行 ID（归档线程发布）
    std::atomic<quint64> m_warmCount{0}; ///< 温层行数（归档线程发布）
    quint64 m_hotBase = 0;          ///< 热层首行 ID（m_hotMutex 保护）
    bool m_memoryOnly = false;
};
```

创建 `src/log/ZzLogEngine.cpp`：

```cpp
#include "ZzLogEngine.h"

#include "ZzLogArchiveWorker.h"
#include "ZzMmapBuffer.h"

#include <QMetaObject>

ZzLogEngine::ZzLogEngine(const Config &config, QObject *parent)
    : QObject(parent)
    , m_config(config)
    , m_hot(qMax<qsizetype>(config.hotCapacity, 1))
{
}

ZzLogEngine::~ZzLogEngine()
{
    flush(); // 让已排队批次落盘
    if (m_workerThread.isRunning()) {
        m_workerThread.quit();
        m_workerThread.wait();
    }
    delete m_worker; // 队列已排空、线程已停止，直接删除安全
    m_worker = nullptr;
}

bool ZzLogEngine::open()
{
    qRegisterMetaType<QVector<ZzLogLine>>("QVector<ZzLogLine>");
    if (m_config.warmFilePath.isEmpty()) {
        m_memoryOnly = true;
        return true;
    }
    m_warm = std::make_unique<ZzMmapBuffer>(m_config.warmFilePath, m_config.warmMaxLines);
    if (!m_warm->open()) {
        const QString path = m_config.warmFilePath;
        m_warm.reset();
        m_memoryOnly = true;
        emit degradedToMemoryOnly(
            QStringLiteral("温层文件打开失败，降级为纯内存模式：%1").arg(path));
        return true; // 降级不影响终端交互（规格 §八）
    }
    // 恢复既有温层数据（引擎重开场景）
    m_warmBase.store(m_warm->firstLineId());
    m_warmCount.store(m_warm->lineCount());
    {
        QMutexLocker locker(&m_hotMutex);
        m_hotBase = m_warmBase.load() + m_warmCount.load();
    }
    m_worker = new ZzLogArchiveWorker(m_warm.get(), &m_warmLock, &m_warmBase, &m_warmCount);
    m_worker->moveToThread(&m_workerThread);
    connect(m_worker, &ZzLogArchiveWorker::archiveCompleted,
            this, &ZzLogEngine::archiveFinished);
    connect(m_worker, &ZzLogArchiveWorker::archiveFailed, this,
            [this](const QString &message) {
                m_memoryOnly = true; // 后续批次直接丢弃，不再尝试写盘
                emit degradedToMemoryOnly(message);
            });
    m_workerThread.start();
    return true;
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

    // 1) 温层区间
    if (m_warm) {
        const quint64 warmEnd = m_warmBase.load() + m_warmCount.load();
        if (id < warmEnd) {
            QReadLocker locker(&m_warmLock);
            out = m_warm->readLines(id, remaining);
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
    return m_warmCount.load() + quint64(m_hot.count());
}

quint64 ZzLogEngine::firstLineNo() const
{
    if (m_warm)
        return m_warmBase.load();
    QMutexLocker locker(&m_hotMutex);
    return m_hotBase;
}

void ZzLogEngine::preload(quint64 lineNo)
{
    if (!m_worker)
        return;
    QMetaObject::invokeMethod(m_worker, "preloadAround", Qt::QueuedConnection,
                              Q_ARG(quint64, lineNo));
}

void ZzLogEngine::flush()
{
    if (!m_worker || !m_workerThread.isRunning())
        return;
    // 队列先进先出：先前排队的 archiveLines 先于 flush 执行，返回即归档完成
    QMetaObject::invokeMethod(m_worker, "flush", Qt::BlockingQueuedConnection);
}
```

在 `src/log/CMakeLists.txt` 的源文件列表中 `ZzMmapBuffer.cpp` 之后追加四行：

```cmake
    ZzLogArchiveWorker.h
    ZzLogArchiveWorker.cpp
    ZzLogEngine.h
    ZzLogEngine.cpp
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzLogEngineTest$'
```

预期：`ZzLogEngineTest` 全部 6 个用例 PASS。

- [ ] **步骤 5：回归并 Commit**

```bash
ctest --test-dir build/debug --output-on-failure
git add src/log tests/log
git commit -m "feat: 新增 ZzLogEngine 门面与后台归档线程"
```

预期：全部测试通过。

---

### 任务 7：性能门控测试与 records 落盘（规格 §9.1）

**文件：**
- 创建：`tests/log/ZzLogEnginePerfTest.cpp`
- 修改：`tests/log/CMakeLists.txt`（注册带 `perf` 标签的测试）

- [ ] **步骤 1：创建性能测试**

创建 `tests/log/ZzLogEnginePerfTest.cpp`：

```cpp
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

/**
 * @brief ZzLogEngine 性能门控测试（规格 §9.1）。
 *
 * 阈值失败即测试失败；结果持久化到 tests/perf/records/YYYY-MM-DD-<功能名>.json，
 * 内容含阈值、实测值、环境信息与 git commit hash。仅 Release 构建数字有效，
 * Debug 构建整体跳过。
 */
class ZzLogEnginePerfTest : public QObject
{
    Q_OBJECT
    static ZzLogLine line(quint64 i)
    {
        return {QStringLiteral("性能测试行 %1 0123456789abcdef").arg(i), QByteArray(8, 'x')};
    }

    /// @brief 采集环境信息：CPU/OS/Qt 版本/编译器/构建类型/git commit hash。
    static QJsonObject environmentInfo()
    {
        QJsonObject env;
        env[QStringLiteral("cpu")] = QSysInfo::currentCpuArchitecture();
        env[QStringLiteral("os")] = QSysInfo::prettyProductName();
        env[QStringLiteral("kernel")] = QSysInfo::kernelVersion();
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
        QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
#endif
    }

    /// @brief 写入吞吐门控：20 万行（含热层驱逐 + 温层归档落盘）≥ 50,000 行/秒。
    void writeThroughput()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config; // 默认：热 10,000 / 温 1,000,000 / 批 1024
        config.warmFilePath = dir.filePath(QStringLiteral("perf-warm.log"));
        ZzLogEngine engine(config);
        QVERIFY(engine.open());

        constexpr quint64 N = 200000;
        QElapsedTimer timer;
        timer.start();
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush(); // 含全部归档与冲刷
        const qint64 ms = timer.elapsed();

        const double linesPerSec = double(N) / (double(ms) / 1000.0);
        constexpr double threshold = 50000.0;
        const bool passed = linesPerSec >= threshold;
        writeRecord(QStringLiteral("ZzLogEngine-write-throughput"), threshold,
                    QStringLiteral("lines/s"), linesPerSec, passed,
                    {{QStringLiteral("lineCount"), qint64(N)},
                     {QStringLiteral("elapsedMs"), ms}});
        QVERIFY2(passed, qPrintable(QStringLiteral("写入吞吐 %1 行/秒，低于阈值 %2")
                                        .arg(linesPerSec)
                                        .arg(threshold)));
    }

    /// @brief 滚动读取延迟门控：20 万行中随机窗口读取 60 行，最差值 ≤ 16ms。
    void scrollReadLatency()
    {
        QTemporaryDir dir;
        QVERIFY(dir.isValid());
        ZzLogEngine::Config config;
        config.warmFilePath = dir.filePath(QStringLiteral("perf-warm.log"));
        ZzLogEngine engine(config);
        QVERIFY(engine.open());

        constexpr quint64 N = 200000;
        for (quint64 i = 0; i < N; ++i)
            engine.appendLine(line(i));
        engine.flush();
        QCOMPARE(engine.totalLines(), N);

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
            worstNs = qMax(worstNs, ns);
            totalNs += ns;
        }

        const double worstMs = double(worstNs) / 1e6;
        const double avgMs = double(totalNs) / samples / 1e6;
        constexpr double threshold = 16.0; // 滚动帧时间上限（规格 §5.1 / §9.1）
        const bool passed = worstMs <= threshold;
        writeRecord(QStringLiteral("ZzLogEngine-scroll-read-latency"), threshold,
                    QStringLiteral("ms"), worstMs, passed,
                    {{QStringLiteral("samples"), samples},
                     {QStringLiteral("avgMs"), avgMs},
                     {QStringLiteral("windowRows"), 60},
                     {QStringLiteral("totalLines"), qint64(N)}});
        QVERIFY2(passed, qPrintable(QStringLiteral("滚动读取最差 %1ms，超过阈值 %2ms")
                                        .arg(worstMs)
                                        .arg(threshold)));
    }
};

QTEST_GUILESS_MAIN(ZzLogEnginePerfTest)
#include "ZzLogEnginePerfTest.moc"
```

在 `tests/log/CMakeLists.txt` 末尾追加：

```cmake
# 性能门控测试（规格 §9.1）：结果写入 tests/perf/records/，Release 构建数字才有效。
add_executable(ZzLogEnginePerfTest ZzLogEnginePerfTest.cpp)
target_link_libraries(ZzLogEnginePerfTest PRIVATE ZzLogEngine Qt6::Core Qt6::Test)
set_target_properties(ZzLogEnginePerfTest PROPERTIES AUTOMOC ON)
target_compile_definitions(ZzLogEnginePerfTest PRIVATE
    PERF_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records")
add_test(NAME ZzLogEnginePerfTest COMMAND ZzLogEnginePerfTest)
set_tests_properties(ZzLogEnginePerfTest PROPERTIES LABELS "perf")
```

- [ ] **步骤 2：Debug 构建验证测试可编译且整体跳过**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure -R '^ZzLogEnginePerfTest$'
```

预期：测试可执行文件编译通过；QTest 输出 `QSKIP : ... 性能门控仅在 Release 构建下有效`，ctest 判 Passed（SKIPs 不算失败）。

- [ ] **步骤 3：Release 构建跑门控（有效数字）**

```bash
cmake -S . -B build/release -DCMAKE_BUILD_TYPE=Release
cmake --build build/release -j8
ctest --test-dir build/release --output-on-failure -L perf
```

预期：`ZzLogEnginePerfTest` Passed，`100% tests passed, 0 tests failed out of 1`。若阈值不达标：不允许跳过或放水，回到任务 4/6 优化实现（常见瓶颈：逐行互斥锁、块缓存未命中、归档批次过小），再重跑本步。

- [ ] **步骤 4：验证性能记录已生成且字段完整**

```bash
ls tests/perf/records/$(date +%F)-ZzLogEngine-*.json
cat tests/perf/records/$(date +%F)-ZzLogEngine-write-throughput.json
```

预期：两个文件存在（`ZzLogEngine-write-throughput` 与 `ZzLogEngine-scroll-read-latency`）；JSON 含 `testName`、`threshold`、`unit`、`measured`、`passed: true`、`environment`（含 `cpu`/`os`/`qtVersion`/`compiler`/`buildType`/`gitCommit`）、`timestamp`。

- [ ] **步骤 5：Commit（测试代码与首轮性能记录一起入库）**

```bash
git add tests/log tests/perf/records
git commit -m "test: 新增日志引擎性能门控测试并记录首轮结果"
```

---

### 任务 8：全量回归与收尾验证

**文件：** 无新增；本任务只做验证。

- [ ] **步骤 1：Debug 全量测试**

```bash
cmake --build build/debug -j8 && ctest --test-dir build/debug --output-on-failure
```

预期：5 个测试（ZzLineIndexTest、ZzRingBufferTest、ZzMmapBufferTest、ZzLogEngineTest、ZzLogEnginePerfTest）全部 Passed（性能测试在 Debug 下为 QSKIP，ctest 计 Passed）。

- [ ] **步骤 2：Release 全量测试**

```bash
cmake --build build/release -j8 && ctest --test-dir build/release --output-on-failure
```

预期：5 个测试全部 Passed，性能门控在 Release 下真实执行并通过。

- [ ] **步骤 3：纯 Qt Core 依赖审计（规格 §十：核心库不依赖 Widgets）**

```bash
ldd build/debug/tests/log/ZzLogEngineTest | grep -iE 'Qt6(Widgets|Gui)' || echo "OK: 无 Widgets/Gui 依赖"
```

预期：输出 `OK: 无 Widgets/Gui 依赖`（链接的 Qt 库仅为 Qt6Core / Qt6Test；Windows 上对应动作为 `dumpbin /dependents`，macOS 为 `otool -L`）。

- [ ] **步骤 4：确认工作区干净、记录已入库**

```bash
git status --short
git ls-files tests/perf/records/
git log --oneline -8
```

预期：`git status` 无未跟踪/未提交的本计划产物；`git ls-files` 列出至少两个 records JSON；最近 8 条提交依次为任务 1-7 的 Conventional Commits 中文提交（build:/feat:×4/test:×2）。

---

## 自检结论（编写者已执行；2026-08-17 修订后复检通过）

**0. 修订复检（路径迁移 src/logengine→src/log、tests/logengine→tests/log，目标改名 zzlogengine→ZzLogEngine，根构建文件步骤移除）：**
- 全文 `src/logengine` / `tests/logengine` / `zzlogengine` / `zz_add_logengine_test` 已无残留；库目标统一为 `ZzLogEngine`，测试统一经 `zz_add_log_test` 注册
- 性能记录文件名随目标名统一为 `YYYY-MM-DD-ZzLogEngine-write-throughput.json` / `YYYY-MM-DD-ZzLogEngine-scroll-read-latency.json`（测试代码与验证命令中的名字一致）
- 根构建文件（根 `CMakeLists.txt` / `CMakePresets.json` / `.gitignore`）在全文仅作为"计划 04 骨架提供"的引用出现，无任何创建/修改步骤；本计划唯一修改的骨架文件是 `third_party/CMakeLists.txt`（追加 LZ4 段，锚点：既有 `add_subdirectory` 条目块之后）
- `src/log` 与 `tests/log` 的顶层接入条目归计划 04，任务 2 步骤 1 以 grep 验证其存在，缺失则停止协调

**1. 规格覆盖度（规格 §五 / §9.1 / §十）：**
- 热层环形缓冲：默认 1 万行可配置、O(1) 读写、保留字符属性、溢出归档 → 任务 3 + 任务 6
- 温层 mmap + LZ4 分块（64KB/块）、按需解压、默认 100 万行可配置、超限丢弃最老块 → 任务 4 + 任务 5
- 分块行偏移索引（每 1024 行一条、块内小范围扫描）→ 任务 2 + 任务 4 读路径
- 门面 appendLine/getLine/getLines/totalLines/预加载 → 任务 6
- 归档后台线程、不阻塞 I/O 与 UI 线程 → 任务 6（ZzLogArchiveWorker + QThread）
- I/O 失败降级纯内存模式（§八）→ 任务 6（`degradedToMemoryOnly` + `warmOpenFailureDegrades` 用例）
- 测试矩阵：环形缓冲溢出/归档往返、压缩解压一致性、行索引定位、滚动读取等价性 → 任务 2/3/4/5/6
- 性能门控：写入吞吐 + 滚动读取延迟（16ms），records JSON 含阈值/实测/环境/git hash，进 ctest（`perf` 标签），Release 有效 → 任务 7
- LZ4 vendored add_subdirectory、C++20、Zz 前缀、文件名与类名一致、Doxygen 中文注释、纯 Qt Core → 任务 1/2 + 任务 8 审计
- 冷层（SQLite + FTS5）→ v0.2，明确不在本计划

**2. 占位符扫描：** 已扫描全文，无 "TODO/待定/后续实现/类似任务N" 类占位；所有代码步骤均含完整代码块。

**3. 类型一致性（已交叉核对）：**
- `ZzLineIndex::Entry{lineId, blockFirstLineId, offset}` 与 `recordLine/locate` 签名在任务 2 定义，任务 4 `appendLines`/`readLines` 中一致使用
- `ZzMmapBuffer::{appendLines, readLines, preload, flush, firstLineId, lineCount, open, close}` 在任务 4 定义，任务 5/6 的 Worker 与 Engine 中一致调用
- `ZzLogEngine::Config{hotCapacity, warmMaxLines, archiveBatchSize, warmFilePath}` 在任务 6 定义，任务 6/7 测试中一致使用
- 信号链：`ZzLogArchiveWorker::archiveCompleted/archiveFailed` → `ZzLogEngine::archiveFinished/degradedToMemoryOnly`，连接点仅在任务 6 `open()` 中
- 测试中的驱逐批次数学（250 行 → 驱逐 160 / 剩 90；200 行 → 驱逐 112 / 剩 88）已按热层 100 + 批 16 推演核对

---

## 附录：公开 API 清单

以下为本计划交付的 `ZzLogEngine` 静态库（`src/log/`，仅依赖 Qt6::Core + lz4_static）对外公开的全部类型与接口，供 UI / 终端集成计划对接。

### `ZzLogLine`（src/log/ZzLogLine.h）

```cpp
struct ZzLogLine {
    QString text;          ///< 行纯文本（不含换行符）
    QByteArray attributes; ///< 不透明字符属性负载（格式由终端层定义）
};
```

### `ZzLogEngine`（src/log/ZzLogEngine.h）——门面，集成方唯一必读

```cpp
struct ZzLogEngine::Config {
    qsizetype hotCapacity = 10000;     ///< 热层行数
    quint64 warmMaxLines = 1000000;    ///< 温层最大行数
    qsizetype archiveBatchSize = 1024; ///< 单次归档批量行数
    QString warmFilePath;              ///< 温层 mmap 文件路径；为空 = 纯内存模式
};

ZzLogEngine(const Config &config, QObject *parent = nullptr);
~ZzLogEngine() override;

bool open();                                          // 恒 true；温层失败降级纯内存
void appendLine(const ZzLogLine &line);               // 线程安全，任意线程可调
bool getLine(quint64 lineNo, ZzLogLine *out) const;
QVector<ZzLogLine> getLines(quint64 startLine, quint64 count) const; // 滚动读取主路径
quint64 totalLines() const;                           // 可读总行数（热+温）
quint64 firstLineNo() const;                          // 最老可读行 ID
bool isMemoryOnly() const;
void preload(quint64 lineNo);                         // 异步预加载附近温层块
void flush();                                         // 阻塞至已排队归档完成（勿在归档线程调）

// 信号
void archiveFinished();                               // 一批行完成温层归档
void degradedToMemoryOnly(const QString &reason);     // 温层 I/O 失败降级
```

行号约定：绝对单调递增 ID，可读窗口 `[firstLineNo(), firstLineNo() + totalLines())`；温层超限丢弃或纯内存驱逐时 `firstLineNo()` 前移。

### `ZzRingBuffer`（src/log/ZzRingBuffer.h）——热层（内部组件，可直接单测/复用）

```cpp
explicit ZzRingBuffer(qsizetype capacity = 10000);
qsizetype capacity() const;
qsizetype count() const;
bool isFull() const;
bool append(const ZzLogLine &line, ZzLogLine *evicted = nullptr); // 满则驱逐最老行
QVector<ZzLogLine> takeOldest(qsizetype n);                       // 批量取走最老行
const ZzLogLine &at(qsizetype index) const;                       // 0 = 当前最老行
void clear();
```

### `ZzMmapBuffer`（src/log/ZzMmapBuffer.h）——温层（内部组件，自身不加锁）

```cpp
explicit ZzMmapBuffer(const QString &filePath, quint64 maxLines = 1000000);
bool open();
void close();
bool isOpen() const;
quint64 firstLineId() const;
quint64 lineCount() const;
quint64 maxLines() const;
bool appendLines(const QVector<ZzLogLine> &lines, QString *errorString = nullptr);
QVector<ZzLogLine> readLines(quint64 startId, quint64 count) const;
void preload(quint64 lineId) const;   // 解压所在块及后一块入缓存
void flush();
static constexpr quint64 kChunkSize = 64 * 1024;
```

### `ZzLineIndex`（src/log/ZzLineIndex.h）——行偏移索引（内部组件）

```cpp
struct ZzLineIndex::Entry { quint64 lineId; quint64 blockFirstLineId; quint64 offset; };

explicit ZzLineIndex(quint64 stride = 1024);
quint64 stride() const;
qsizetype entryCount() const;
void recordLine(quint64 lineId, quint64 blockFirstLineId, quint64 offset); // 仅 stride 整数倍落盘
bool locate(quint64 lineId, Entry *out) const;                             // 最近不大于目标行条目
void clear();
```

### `ZzLogArchiveWorker`（src/log/ZzLogArchiveWorker.h）——归档线程工作对象（内部）

```cpp
ZzLogArchiveWorker(ZzMmapBuffer *buffer, QReadWriteLock *lock,
                   std::atomic<quint64> *warmBase, std::atomic<quint64> *warmCount,
                   QObject *parent = nullptr);
// 槽：archiveLines(QVector<ZzLogLine>) / preloadAround(quint64) / flush()
// 信号：archiveCompleted() / archiveFailed(QString)
```

### 构建产物

- 静态库目标：`ZzLogEngine`（`src/log/CMakeLists.txt`；`PUBLIC Qt6::Core`，`PRIVATE lz4_static`，C++20）
- 测试目标：`ZzLineIndexTest` / `ZzRingBufferTest` / `ZzMmapBufferTest` / `ZzLogEngineTest` / `ZzLogEnginePerfTest`（ctest；性能测试带 `perf` 标签，仅 Release 有效）
- 性能记录：`tests/perf/records/YYYY-MM-DD-ZzLogEngine-{write-throughput,scroll-read-latency}.json`
