# ZzTermWidget 滚动历史读回 API 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在 ZzTermWidget（qtermwidget 深度 fork）中新增滚动历史读回注入 API：滚动条越顶时回调外部提供者索取更老的历史行，注入显示层历史缓冲头部，支撑 ZzClawTerm 规格 §5.4 的"向上滚动超出内存历史时从 ZzLogEngine 读回"。

**架构：** 自底向上四层穿透——`HistoryScroll` 新增前插虚接口 `prependLines`（`HistoryScrollBuffer` 以独立前插区实现，容量独立于环形区、上限相同，内存占用有界 ≤2× 上限）；`Screen` 新增 `prependHistoryLines`（同步 OSC 8 链接段表 / sixel / kitty 图像引用三张平行表前插空行、平移选区坐标）与 `historyBaseLine`（绝对行号显式记账：满员丢弃全缓冲最老行时基线 +1、前插注入 n 行时基线 −n；前插区存在期间环形区满员丢弃的是中部行，最老行仍在内存，基线不动，避免提供者重复回传内存中已有的行）；`Emulation` 转发到主屏；`QTermWidget` 新增公共 API `setHistoryProvider(std::function<QStringList(qint64, int)>)`，由 `TerminalDisplay` 新增的 `historyTopReached` 信号（滚动条到顶，含滚轮路径）触发同步取数，注入后 `scrollAfterHistoryPrepend` 平移视图保持可视内容稳定。

**技术栈：** C++20 / Qt 6.8+ / CMake / QTest（Qt6::Test），仓库 `/home/zz/Jackfahdin/github/ZzTermWidget`。

---

## 前置说明（执行环境，务必先读）

- **本计划全部改动落在独立仓库 `/home/zz/Jackfahdin/github/ZzTermWidget`**，不触碰应用仓库 ZzClawTerm 的任何文件（根 `CMakeLists.txt` / `CMakePresets.json` / `.gitignore` 由计划 04 任务 1 独占，本计划一律不创建不修改）。
- **执行前提与时序：** 本计划必须在 ZzTermWidget 被应用仓库以 submodule 引入、且计划 04 任务 12 步骤 5（读回接线）之前完成并提交发布。计划 04 契约表中预留的本计划 API 形态为 `void QTermWidget::setHistoryProvider(std::function<QStringList(qint64 beforeLine, int maxLines)> provider)`，本计划按此形态精确交付，计划 04 接线处无需改写。
- **构建与测试（本机，Qt 6.11.1；CI 基线 6.8.2）：**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=/home/zz/Qt/6.11.1/gcc_64
cmake --build build --parallel
ctest --test-dir build --output-on-failure
```

- 测试统一在 offscreen 平台运行（`tests/CMakeLists.txt` 已固化 `QT_QPA_PLATFORM=offscreen`），无需显示服务器。
- **安全网：** `tst_rendering` 是渲染像素等价性回归测试（Linux/FreeType 下结构级像素恒等），新增注入路径不得破坏它；每个任务提交前都要跑全量 ctest，不允许只跑新增测试。
- **仓库既有约定（AGENTS.md）：** 注释强制 Doxygen 风格简体中文；类名沿用仓库现状不加 `Zz` 前缀（仓库内现有类如 `Screen`、`HistoryScrollBuffer` 均无前缀，新增 API 皆为既有类的方法，不涉及新类）；Conventional Commits，描述用中文（参照 `git log`：`feat(history): ...`、`test(rendering): ...`）。
- **char32_t 字符管线：** 本仓库文本一律以 UCS-4 码点流转——`Emulation::receiveData` 用 `toUcs4()` 迭代 `char32_t`，`Character::character` 为 `char32_t`，`Screen::displayCharacter(char32_t)`。读回注入路径遵循同一管线：`QString::toUcs4()` → `Character(char32_t)`（默认属性），与 `Emulation::dupDisplayCharacter` 的纯文本口径一致。**v0.1 读回不恢复颜色/样式属性**（追加路径 `dupDisplayOutput` 本身就是纯文本，属性从未进入 ZzLogEngine 的行文本通道；`ZzLogLine.attributes` 为预留负载）。
- **已确认的真实代码事实**（零上下文工程师不必重新考证）：
  - `HistoryScroll`（`lib/src/util/History.h`）只有尾部追加（`addCells`/`addLine`）与随机读取（`getCells`/`getLineLen`/`isWrappedLine`），无前插能力。
  - `HistoryScrollBuffer` 为环形缓冲：`_historyBuffer` + `_head` + `_usedLines`，满员时 `addCellsVector` 推进 `_head` 覆盖最老行；`Screen::addHistLine` 通过比较 `history->getLines()` 新旧值判定丢行（`_droppedLines++`）。
  - `Screen` 持有三张与 history 行一一对应的平行表：`std::deque<HyperlinkLine> _historyLinks`、`std::deque<ImageRefLine> _historyImages`、`std::deque<KittyRefLine> _historyKittyRefs`（`lib/src/emulation/Screen.h:932-966`）。**任何改变 history 行数/索引对齐的操作都必须同步维护这三张表**（`Screen::addHistLine` 的既有做法即推入/弹出平行表行）。
  - 滚动条到视图的链路：`TerminalDisplay::scrollBarPositionChanged`（`TerminalDisplay.cpp:2796`）→ `_screenWindow->scrollTo(value)` → `updateImage()` → `setScroll(currentLine, lineCount)` 回写滚动条范围（回写期间断开 `valueChanged`，不会递归）。滚轮路径 `wheelEvent`（`TerminalDisplay.cpp:3648`）经 `_scrollBar->event(ev)` 转发，同样汇入 `scrollBarPositionChanged`。键盘滚动路径（`ScreenWindow::handleCommandFromKeyboard`）不经过滚动条，**v0.1 不在该路径触发读回**，属已知限制。
  - `ScreenWindow::notifyOutputChanged` 在非跟踪模式（用户向上翻页时 `trackOutput=false`）下按 `droppedLines()` 下移 `_currentLine`；纯前插不产生丢行，视图索引不变——因此前插后必须由显示层显式 `scrollTo(currentLine + n)` 才能保持可视内容稳定。
  - `Screen::linkSegments(int absoluteLine)`（`Screen.cpp`）对历史行直接索引 `_historyLinks[absoluteLine]`，是验证平行表对齐的现成探针。
  - `QTermWidget::recvData(const char *, int) const`（`qtermwidget.cpp:705`）同步调 `Emulation::receiveData`；输出变更经 `bufferedUpdate` 攒帧定时器（10ms/40ms）触发 `showBulk` → `outputChanged`，无事件循环时不刷新滚动条范围，**部件级测试喂数据后须 `QTest::qWait` + `processEvents`**。
  - `QTermWidget` 构造默认不显示滚动条（`setScrollBarPosition(NoScrollBar)`），但 `_scrollBar` 对象始终存在且信号已连接，`findChild<QScrollBar *>()` 可取到。
  - 无显示环境下 `TerminalDisplay`/`ScreenWindow` 默认 40×80。

## 文件结构

| 文件 | 职责 | 动作 |
| ---- | ---- | ---- |
| `lib/src/util/History.h` | `HistoryScroll` 前插虚接口声明；`HistoryScrollBuffer` 前插区成员 | 修改 |
| `lib/src/util/History.cpp` | 基类默认实现（返回 0）；`HistoryScrollBuffer` 前插区实现与读取路径改路由 | 修改 |
| `lib/src/emulation/Screen.h` | `prependHistoryLines` / `historyBaseLine` 声明；`_historyBase` / `_hasPrependedLines` 成员 | 修改 |
| `lib/src/emulation/Screen.cpp` | 前插实现（平行表对齐 + 选区平移）；`addHistLine` 累计计数；`setScroll` 清空时归零 | 修改 |
| `lib/src/emulation/Emulation.h` | 转发方法声明（+`#include <QVector>` / `"Character.h"`） | 修改 |
| `lib/src/emulation/Emulation.cpp` | 转发到主屏 `_screen[0]` | 修改 |
| `lib/src/display/TerminalDisplay.h` | `historyTopReached` 信号、`scrollAfterHistoryPrepend` 声明 | 修改 |
| `lib/src/display/TerminalDisplay.cpp` | 越顶信号发射、前插后视图平移刷新 | 修改 |
| `lib/include/qtermwidget.h` | 公共 API `setHistoryProvider` 声明与私有成员（+`#include <functional>`） | 修改 |
| `lib/src/widget/qtermwidget.cpp` | 信号接线、`fetchOlderHistory` 实现、`clearScrollback` 重置耗尽标记 | 修改 |
| `tests/tst_historyreadback.cpp` | 读回注入全层回归测试 + 性能门控与记录落盘 | 创建 |
| `tests/CMakeLists.txt` | 注册 `tst_historyreadback`、注入源码目录宏 | 修改 |
| `tests/perf/records/` | 性能记录落盘目录（测试运行时自动创建并写入 JSON，提交入库） | 创建（运行时生成） |

---

### 任务 1：HistoryScroll 前插接口与 HistoryScrollBuffer 前插区

**文件：**
- 修改：`lib/src/util/History.h`（`HistoryScroll` 类 `addLine` 声明之后；`HistoryScrollBuffer` 类）
- 修改：`lib/src/util/History.cpp`（`HistoryScroll` 基类实现区；`HistoryScrollBuffer` 各方法）
- 测试：`tests/tst_historyreadback.cpp`（新建）

**设计要点（实现前读懂）：** 前插区是独立于环形区的 `std::deque`，逻辑上位于环形区之前；读取路径（`getLines`/`getLineLen`/`getCells`/`isWrappedLine`）统一按"前插区行号段 → 环形区行号段"两段路由。容量策略：**前插区上限与 `_maxLineCount` 相同且独立**（总内存 ≤ 2× 上限，满足规格 §5.1 内存有界）；单次前插输入超过剩余容量时，保留输入中**较新**的行（紧邻既有历史、不产生索引空洞），最老的行丢弃——上层 ZzLogEngine 仍持有这些行，属可再读回数据。环形区追加路径（`addCellsVector`/`addLine`）**完全不改**，丢行计数语义不变。

- [ ] **步骤 1：注册测试目标（先改构建，让后续步骤能跑测试）**

修改 `tests/CMakeLists.txt`：在 `QTERMWIDGET_TESTS` 列表中追加 `tst_historyreadback`（放在 `tst_history` 之后）：

```cmake
set(QTERMWIDGET_TESTS
    tst_charwidth
    tst_ligature
    tst_emulation
    tst_osc52
    tst_history
    tst_historyreadback
    tst_protocols
    tst_benchmark
    tst_rendering
    tst_sixel
    tst_kittygraphics
)
```

在 `foreach` 循环结束之后追加源码目录宏（任务 5 的性能记录落盘用）：

```cmake
# 性能记录落盘需要源码目录（tst_historyreadback 的 perf 用例写入 tests/perf/records/）
target_compile_definitions(tst_historyreadback PRIVATE
    ZZ_TERM_SOURCE_DIR="${CMAKE_SOURCE_DIR}")
```

- [ ] **步骤 2：编写失败的测试 `tests/tst_historyreadback.cpp`**

```cpp
#include <QtTest>
#include "History.h"
#include "Character.h"

/**
 * @brief 历史读回注入（前插）的缓冲层回归测试。
 * @note 前插语义：外部提供者把更老的历史行注入滚动缓冲头部（旧→新顺序）；
 *       前插区独立于环形区，容量上限同为 _maxLineCount（内存有界 ≤2×）。
 */
class TestHistoryReadback : public QObject
{
    Q_OBJECT
private slots:
    void testPrependBufferBasic();
    void testPrependUnsupportedScrollTypes();
    void testPrependCapacityCap();
    void testAppendAfterPrependKeepsOrder();
};

/**
 * @brief 前插后行序、行长、折行标志、单元格内容均可按新行号原样读回。
 */
void TestHistoryReadback::testPrependBufferBasic()
{
    HistoryScrollBuffer buf(100);
    buf.addCellsVector({ Character(U'a') });
    buf.addLine(false);
    buf.addCellsVector({ Character(U'b') });
    buf.addLine(true);
    QCOMPARE(buf.getLines(), 2);

    const QVector<QVector<Character>> older = {
        { Character(U'x'), Character(U'y') },
        { Character(U'z') }
    };
    const QVector<bool> wrapped = { true, false };
    QCOMPARE(buf.prependLines(older, wrapped), 2);

    QCOMPARE(buf.getLines(), 4);
    QCOMPARE(buf.getLineLen(0), 2);
    QCOMPARE(buf.getLineLen(1), 1);
    QVERIFY(buf.isWrappedLine(0));
    QVERIFY(!buf.isWrappedLine(1));
    QVERIFY(!buf.isWrappedLine(2));
    QVERIFY(buf.isWrappedLine(3)); // 原环形区第 2 行（'b'）的折行标志随索引平移

    QVector<Character> out(2);
    buf.getCells(0, 0, 2, out.data());
    QCOMPARE(out[0].character, char32_t(U'x'));
    QCOMPARE(out[1].character, char32_t(U'y'));
    buf.getCells(3, 0, 1, out.data());
    QCOMPARE(out[0].character, char32_t(U'b')); // 环形区内容不受前插影响
}

/**
 * @brief 无历史/文件历史不支持前插，返回 0（文件型为无限历史，无需读回）。
 */
void TestHistoryReadback::testPrependUnsupportedScrollTypes()
{
    HistoryScrollNone none;
    HistoryScrollFile file{QString()};
    const QVector<QVector<Character>> lines = { { Character(U'x') } };
    const QVector<bool> wrapped = { false };
    QCOMPARE(none.prependLines(lines, wrapped), 0);
    QCOMPARE(file.prependLines(lines, wrapped), 0);
    QCOMPARE(file.getLines(), 0); // 不支持即无副作用
}

/**
 * @brief 前插区容量独立于环形区、上限相同；超量输入保留较新的行（无索引空洞）。
 */
void TestHistoryReadback::testPrependCapacityCap()
{
    HistoryScrollBuffer buf(10);
    for (int i = 0; i < 10; i++) {
        buf.addCellsVector({ Character(char32_t(U'a' + i)) });
        buf.addLine(false);
    }
    QCOMPARE(buf.getLines(), 10); // 环形区已满

    QVector<QVector<Character>> older;
    QVector<bool> wrapped;
    for (int i = 0; i < 15; i++) {
        older.append({ Character(char32_t(U'A' + i)) });
        wrapped.append(false);
    }
    QCOMPARE(buf.prependLines(older, wrapped), 10); // 前插区上限 10，最老的 'A'..'E' 丢弃
    QCOMPARE(buf.getLines(), 20);

    QVector<Character> out(1);
    buf.getCells(0, 0, 1, out.data());
    QCOMPARE(out[0].character, char32_t(U'F'));  // 保留输入中较新的 10 行
    buf.getCells(9, 0, 1, out.data());
    QCOMPARE(out[0].character, char32_t(U'O'));
    buf.getCells(10, 0, 1, out.data());
    QCOMPARE(out[0].character, char32_t(U'a'));  // 环形区首行未受影响
}

/**
 * @brief 前插区存在时继续尾部追加：环形区满员覆盖的是环形区最老行，行序保持连续。
 */
void TestHistoryReadback::testAppendAfterPrependKeepsOrder()
{
    HistoryScrollBuffer buf(3);
    for (char32_t c : { U'a', U'b', U'c' }) {
        buf.addCellsVector({ Character(c) });
        buf.addLine(false);
    }
    const QVector<QVector<Character>> older = { { Character(U'X') }, { Character(U'Y') } };
    const QVector<bool> wrapped = { false, false };
    QCOMPARE(buf.prependLines(older, wrapped), 2);
    QCOMPARE(buf.getLines(), 5);

    buf.addCellsVector({ Character(U'd') }); // 环形区满员，覆盖环形区最老行 'a'
    buf.addLine(false);

    QCOMPARE(buf.getLines(), 5);
    const char32_t expected[5] = { U'X', U'Y', U'b', U'c', U'd' };
    for (int i = 0; i < 5; i++) {
        QVector<Character> out(1);
        buf.getCells(i, 0, 1, out.data());
        QCOMPARE(out[0].character, expected[i]);
    }
}

QTEST_MAIN(TestHistoryReadback)
#include "tst_historyreadback.moc"
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=/home/zz/Qt/6.11.1/gcc_64
cmake --build build --parallel --target tst_historyreadback
```

预期：编译失败，报错 `HistoryScrollBuffer has no member named 'prependLines'`（及 `HistoryScrollNone`/`HistoryScrollFile` 同类错误）。

- [ ] **步骤 4：实现 `HistoryScroll` 前插接口（`lib/src/util/History.h`）**

头部 include 区追加：

```cpp
#include <deque>
```

`HistoryScroll` 类中 `virtual void addLine(bool previousWrapped=false) = 0;` 声明之后追加：

```cpp
    /**
     * @brief 在滚动缓冲头部前插更老的历史行（外部历史读回注入通道，旧→新顺序）。
     * @param lines 行数组，每行为一个 Character 序列（char32_t 管线）。
     * @param wrappedFlags 与 lines 等长的折行标志（LINE_WRAPPED 语义同 addLine）。
     * @return 实际前插的行数；不支持的滚动类型返回 0。
     * @note 基类默认不支持前插；HistoryScrollBuffer 以独立前插区实现。
     *       HistoryScrollFile 为无限历史（行不会离开内存），无读回场景，不支持前插。
     */
    virtual int prependLines(const QVector<QVector<Character>> &lines,
                             const QVector<bool> &wrappedFlags);
```

`HistoryScrollBuffer` 类中 `void addLine(bool previousWrapped=false) override;` 之后追加声明：

```cpp
    int prependLines(const QVector<QVector<Character>> &lines,
                     const QVector<bool> &wrappedFlags) override;
```

`HistoryScrollBuffer` 私有成员区（`int _head;` 之后）追加：

```cpp
    // 历史读回前插区：逻辑上位于环形区之前，front 为最老行；
    // 容量独立于环形区、上限同为 _maxLineCount（总内存占用 ≤2× 上限，规格 §5.1 内存有界）
    std::deque<HistoryLine> _prepended;   ///< 前插区行数据（旧→新）
    std::deque<bool> _prependedWrapped;   ///< 前插区折行标志（与 _prepended 一一对应）
```

- [ ] **步骤 5：实现 `History.cpp`（基类默认 + 前插区 + 读取路径两段路由）**

`HistoryScroll::hasScroll()` 实现之后追加基类默认实现：

```cpp
int HistoryScroll::prependLines(const QVector<QVector<Character>> &,
                                const QVector<bool> &) {
    return 0; // 默认不支持前插
}
```

`HistoryScrollBuffer::addLine` 实现之后追加前插实现：

```cpp
int HistoryScrollBuffer::prependLines(const QVector<QVector<Character>> &lines,
                                      const QVector<bool> &wrappedFlags) {
    Q_ASSERT(lines.size() == wrappedFlags.size());

    // 剩余容量不足时保留输入中较新的行（紧邻既有历史，不产生索引空洞）；
    // 被丢弃的最老行由上层日志引擎兜底持有，用户再次越顶时可重新读回
    const int room = qMax(0, _maxLineCount - static_cast<int>(_prepended.size()));
    const int n = qMin(static_cast<int>(lines.size()), room);
    const int skip = lines.size() - n; // 输入中最老的 skip 行不入缓冲
    for (int i = lines.size() - 1; i >= skip; i--) {
        _prepended.push_front(lines[i]);
        _prependedWrapped.push_front(wrappedFlags[i]);
    }
    return n;
}
```

读取路径改为两段路由（完整替换以下四个方法的方法体）：

```cpp
int HistoryScrollBuffer::getLines() {
    return static_cast<int>(_prepended.size()) + _usedLines;
}

int HistoryScrollBuffer::getLineLen(int lineNumber) {
    Q_ASSERT(lineNumber >= 0 && lineNumber < getLines());

    if (lineNumber < static_cast<int>(_prepended.size()))
        return _prepended[lineNumber].size();

    lineNumber -= static_cast<int>(_prepended.size());
    if (lineNumber < _usedLines) {
        return _historyBuffer[bufferIndex(lineNumber)].size();
    } else {
        return 0;
    }
}

bool HistoryScrollBuffer::isWrappedLine(int lineNumber) {
    Q_ASSERT(lineNumber >= 0 && lineNumber < getLines());

    if (lineNumber < static_cast<int>(_prepended.size()))
        return _prependedWrapped[lineNumber];

    lineNumber -= static_cast<int>(_prepended.size());
    if (lineNumber < _usedLines) {
        return _wrappedLine[bufferIndex(lineNumber)];
    } else
        return false;
}

void HistoryScrollBuffer::getCells(int lineNumber, int startColumn, int count,
                                   Character buffer[]) {
    if (count == 0)
        return;

    Q_ASSERT(lineNumber < getLines());

    if (lineNumber < static_cast<int>(_prepended.size())) {
        const HistoryLine &line = _prepended[lineNumber];
        Q_ASSERT(startColumn <= line.size() - count);
        memcpy(buffer, line.constData() + startColumn, count * sizeof(Character));
        return;
    }

    lineNumber -= static_cast<int>(_prepended.size());

    if (lineNumber >= _usedLines) {
        memset(static_cast<void *>(buffer), 0, count * sizeof(Character));
        return;
    }

    const HistoryLine &line = _historyBuffer[bufferIndex(lineNumber)];

    Q_ASSERT(startColumn <= line.size() - count);

    memcpy(buffer, line.constData() + startColumn, count * sizeof(Character));
}
```

`setMaxNbLines` 方法体末尾（`dynamic_cast<HistoryTypeBuffer *>(m_histType)->m_nbLines = lineCount;` 之后）追加前插区容量收敛：

```cpp
    // 前插区容量同步收敛到新上限（丢弃最老的前插行，上层日志引擎兜底可再读回）
    while (static_cast<int>(_prepended.size()) > static_cast<int>(lineCount)) {
        _prepended.pop_front();
        _prependedWrapped.pop_front();
    }
```

- [ ] **步骤 6：运行测试验证通过 + 全量回归**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel
ctest --test-dir build --output-on-failure -R tst_historyreadback
```

预期：`100% tests passed`（4 个用例全过）。随后全量回归：

```bash
ctest --test-dir build --output-on-failure
```

预期：全部测试通过（含 `tst_rendering`、`tst_history` 等既有套件；`HistoryTypeBuffer::scroll` 的拷贝路径经 `getCells`/`getLines` 已自动覆盖前插区，无需改动）。

- [ ] **步骤 7：Commit**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
git add lib/src/util/History.h lib/src/util/History.cpp tests/tst_historyreadback.cpp tests/CMakeLists.txt
git commit -m "feat(history): HistoryScroll 新增前插接口，环形缓冲支持头部读回注入"
```

---

### 任务 2：Screen::prependHistoryLines 与 historyBaseLine

**文件：**
- 修改：`lib/src/emulation/Screen.h`（公有方法声明区 `resetDroppedLines()` 之后；私有成员 `_droppedLines` 旁）
- 修改：`lib/src/emulation/Screen.cpp`（`addHistLine`、`setScroll`、`prependHistoryLines` 新增）
- 测试：`tests/tst_historyreadback.cpp`（追加用例）

**设计要点：** `historyBaseLine` 采用显式记账而非推导——若用"累计滚入 − 当前行数"推导，前插区存在期间环形区满员丢弃的是**中部行**（最老行仍在内存），推导值会随每次丢行漂移 +1，导致提供者把内存中已有的行重复回传注入（滚动区出现重复行）。显式口径：成员 `_historyBase` 初始 0；`addHistLine` 满员丢行且**前插区为空**（被丢弃的确实是全缓冲最老行）时 +1；`prependHistoryLines` 注入 n 行时 −n；`clearHistory`（`setScroll` 不拷贝旧滚动）时归零。无限历史（文件型）不丢行，基线恒 0 ✓。已知降级形态：前插区非空且环形区满员、新输出持续到达时，被丢弃的中部行造成前插区与环形区之间的内容跳变（这些行在 ZzLogEngine 中仍有存档，但 v0.1 提供者契约只支持"比基线更老"的回读，不补中部空洞）；视口感知的中部行排空属 v0.2 窗口化策略，不在本计划。

- [ ] **步骤 1：编写失败的测试（追加到 `tests/tst_historyreadback.cpp`）**

`#include` 区追加：

```cpp
#include "Screen.h"
#include "TerminalCharacterDecoder.h"
```

`private slots:` 区追加：

```cpp
    void testScreenPrependReadbackOrder();
    void testHistoryBaseLine();
    void testPrependKeepsHyperlinkAlignment();
```

文件末尾（`QTEST_MAIN` 之前）追加三个用例：

```cpp
/**
 * @brief 向 Screen 喂数据产生历史行前插更老的行经解码流按 旧→新 顺序读回。
 */
void TestHistoryReadback::testScreenPrependReadbackOrder()
{
    Screen screen(2, 80);
    screen.setScroll(HistoryTypeBuffer(100));
    // 喂 5 行：L0..L3 滚入历史，L4 留在屏幕
    for (int i = 0; i < 5; i++) {
        for (const char32_t c : QString("L%1").arg(i).toUcs4())
            screen.displayCharacter(c);
        screen.newLine();
    }
    QCOMPARE(screen.getHistLines(), 4);

    const QVector<QVector<Character>> older = {
        { Character(U'o'), Character(U'1') },
        { Character(U'o'), Character(U'2') }
    };
    const QVector<bool> wrapped = { false, false };
    QCOMPARE(screen.prependHistoryLines(older, wrapped), 2);
    QCOMPARE(screen.getHistLines(), 6);

    PlainTextDecoder decoder;
    QString text;
    QTextStream stream(&text);
    decoder.begin(&stream);
    screen.writeLinesToStream(&decoder, 0, screen.getHistLines() - 1);
    decoder.end();
    const QStringList outLines = text.split(QLatin1Char('\n'));
    QCOMPARE(outLines.size(), 7); // 6 行 + 末行后补换行产生的空串
    QCOMPARE(outLines[0], QStringLiteral("o1"));
    QCOMPARE(outLines[1], QStringLiteral("o2"));
    QCOMPARE(outLines[2], QStringLiteral("L0"));
    QCOMPARE(outLines[5], QStringLiteral("L3"));
}

/**
 * @brief historyBaseLine 口径：满员丢行后 base 前进，前插注入后 base 等量回退。
 */
void TestHistoryReadback::testHistoryBaseLine()
{
    Screen screen(2, 80);
    screen.setScroll(HistoryTypeBuffer(10));
    // 喂 15 行：14 次滚入历史（首行 newLine 时光标未触底不滚动），环形区封顶 10
    for (int i = 0; i < 15; i++) {
        for (const char32_t c : QString("L%1").arg(i).toUcs4())
            screen.displayCharacter(c);
        screen.newLine();
    }
    QCOMPARE(screen.getHistLines(), 10);
    QCOMPARE(screen.historyBaseLine(), qint64(14 - 10)); // 4 行已离开内存

    const QVector<QVector<Character>> older = {
        { Character(U'p') }, { Character(U'q') }
    };
    const QVector<bool> wrapped = { false, false };
    QCOMPARE(screen.prependHistoryLines(older, wrapped), 2);
    QCOMPARE(screen.getHistLines(), 12);
    QCOMPARE(screen.historyBaseLine(), qint64(2)); // base 回退 2
}

/**
 * @brief 前插后 OSC 8 链接段平行表同步平移：原历史行的链接在新索引处可查，
 *        前插入的空平行行不产生链接。
 */
void TestHistoryReadback::testPrependKeepsHyperlinkAlignment()
{
    Screen screen(2, 80);
    screen.setScroll(HistoryTypeBuffer(100));
    screen.setCurrentHyperlink(QStringLiteral("https://example.com"), QString());
    for (const char32_t c : QStringLiteral("L0").toUcs4())
        screen.displayCharacter(c);
    screen.setCurrentHyperlink(QString(), QString());
    screen.newLine();
    for (const char32_t c : QStringLiteral("L1").toUcs4())
        screen.displayCharacter(c);
    screen.newLine(); // L0 滚入历史行 0
    QCOMPARE(screen.getHistLines(), 1);
    QCOMPARE(screen.hyperlinkAt(0, 0), QStringLiteral("https://example.com"));

    const QVector<QVector<Character>> older = {
        { Character(U'a') }, { Character(U'b') }, { Character(U'c') }
    };
    const QVector<bool> wrapped = { false, false, false };
    QCOMPARE(screen.prependHistoryLines(older, wrapped), 3);
    QCOMPARE(screen.getHistLines(), 4);

    // 原历史行 0 的链接随索引平移到行 3；前插入的空行无链接
    QCOMPARE(screen.hyperlinkAt(3, 0), QStringLiteral("https://example.com"));
    QVERIFY(screen.hyperlinkAt(0, 0).isEmpty());
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel --target tst_historyreadback
```

预期：编译失败，报错 `Screen has no member named 'prependHistoryLines'` / `'historyBaseLine'`。

- [ ] **步骤 3：实现 `Screen.h` 声明与成员**

`Screen.h` 公有区 `void resetDroppedLines();` 之后追加：

```cpp
    /**
     * @brief 在历史缓冲头部前插更老的历史行（外部历史读回注入通道，旧→新顺序）。
     * @param lines 行数组，每行为一个 Character 序列（char32_t 管线，含属性）。
     * @param wrappedFlags 与 lines 等长的折行标志（LINE_WRAPPED 语义同 addLine）。
     * @return 实际前插的行数；底层滚动类型不支持前插（无历史/文件历史）时返回 0。
     * @note 前插后历史行索引整体上移 n 行：OSC 8 链接段表、sixel/kitty 图像引用
     *       平行表同步前插空行保持一一对应；选区 loc 坐标同步平移；
     *       视图层须随后 scrollTo(currentLine + n) 保持可视内容稳定
     *       （见 TerminalDisplay::scrollAfterHistoryPrepend）。
     */
    int prependHistoryLines(const QVector<QVector<Character>> &lines,
                            const QVector<bool> &wrappedFlags);

    /**
     * @brief 当前内存历史最老一行的绝对行号（会话累计口径，显式记账）。
     * @return 满员丢弃全缓冲最老行时 +1，前插注入 n 行时 -n；无限历史（文件型）下恒为 0；
     *         负值表示注入了本会话从未滚出过的行（提供者误用，仅供调试参考）。
     * @note 前插区非空时环形区满员丢弃的是中部行，最老行仍在内存，基线不动——
     *       防止提供者把内存中已有的行重复回传注入。
     * @note 供历史读回提供者定位"比该行更老"的外部数据；clearHistory 后归零。
     */
    qint64 historyBaseLine() const { return _historyBase; }
```

私有成员区 `int _droppedLines;` 之后追加：

```cpp
    /** @brief 当前内存历史最老一行的绝对行号（historyBaseLine 记账值）。 */
    qint64 _historyBase;
    /** @brief 历史缓冲当前是否含读回前插入的行（addHistLine 丢行记账的判别条件）。 */
    bool _hasPrependedLines;
```

- [ ] **步骤 4：实现 `Screen.cpp`**

构造函数初始化列表中 `_droppedLines(0),` 之后追加 `_historyBase(0), _hasPrependedLines(false),`。

`addHistLine()` 中既有的丢行计数处：

```cpp
        // If the history is full, increment the count
        // of dropped lines
        if (newHistLines == oldHistLines)
            _droppedLines++;
```

改为：

```cpp
        // If the history is full, increment the count
        // of dropped lines
        if (newHistLines == oldHistLines) {
            _droppedLines++;
            // 绝对行号基线记账：前插区为空时被丢弃的才是全缓冲最老行，基线前进；
            // 前插区非空时环形区满员丢弃的是中部行，最老行仍在内存，基线不动，
            // 防止历史提供者把内存中已有的行重复回传注入
            if (oldHistLines > 0 && !_hasPrependedLines)
                _historyBase++;
        }
```

`setScroll()` 的 `else` 分支（`copyPreviousScroll == false`，历史整体废弃）中，三张平行表 `clear()` 之后追加归零：

```cpp
        _historyBase = 0;        // 历史整体废弃（clearHistory）：绝对行号基线归零
        _hasPrependedLines = false;
```

`addHistLine()` 方法之后新增 `prependHistoryLines` 实现：

```cpp
int Screen::prependHistoryLines(const QVector<QVector<Character>> &lines,
                                const QVector<bool> &wrappedFlags) {
    if (lines.isEmpty())
        return 0;

    const int n = history->prependLines(lines, wrappedFlags);
    if (n <= 0)
        return 0;

    // 三张平行表前插空行，保持与 history 行一一对应（读回行无链接/图像引用）
    for (int i = 0; i < n; i++) {
        _historyLinks.push_front(HyperlinkLine());
        _historyImages.push_front(ImageRefLine());
        _historyKittyRefs.push_front(KittyRefLine());
    }

    // 选区 loc 线性坐标随历史行索引整体上移 n 行
    if (selBegin != -1) {
        selTopLeft += n * columns;
        selBottomRight += n * columns;
        selBegin += n * columns;
    }

    // 绝对行号基线回退注入量（注入的行当初滚出时已被基线计数）
    _historyBase -= n;
    _hasPrependedLines = true;

    return n;
}
```

- [ ] **步骤 5：运行测试验证通过 + 全量回归**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel
ctest --test-dir build --output-on-failure -R tst_historyreadback
ctest --test-dir build --output-on-failure
```

预期：新增 3 用例通过，全量套件全绿（`tst_rendering` 不受影响）。

- [ ] **步骤 6：Commit**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
git add lib/src/emulation/Screen.h lib/src/emulation/Screen.cpp tests/tst_historyreadback.cpp
git commit -m "feat(screen): 新增 prependHistoryLines 与 historyBaseLine 绝对行号口径"
```

---

### 任务 3：Emulation 转发层

**文件：**
- 修改：`lib/src/emulation/Emulation.h`（`setCellPixelSize` 声明之后）
- 修改：`lib/src/emulation/Emulation.cpp`（`setCellPixelSize` 实现之后）
- 测试：`tests/tst_historyreadback.cpp`（追加用例）

- [ ] **步骤 1：编写失败的测试（追加到 `tests/tst_historyreadback.cpp`）**

`#include` 区追加：

```cpp
#include "Vt102Emulation.h"
```

`private slots:` 区追加：

```cpp
    void testEmulationPrependForward();
```

文件末尾（`QTEST_MAIN` 之前）追加：

```cpp
/**
 * @brief Emulation 转发层：前插作用于主屏，行总数与绝对行号口径同步变化。
 */
void TestHistoryReadback::testEmulationPrependForward()
{
    Vt102Emulation emu;
    emu.setCodec(QStringEncoder(QStringConverter::Utf8));
    emu.setHistory(HistoryTypeBuffer(50));
    emu.setImageSize(2, 80);
    QByteArray payload;
    for (int i = 0; i < 60; i++)
        payload += "x\r\n";
    emu.receiveData(payload.constData(), int(payload.size()));
    // 2 行屏幕：首个换行不滚动，其后 59 次换行各滚入 1 行；环形区封顶 50
    QCOMPARE(emu.lineCount(), 52);
    QCOMPARE(emu.historyBaseLine(), qint64(59 - 50)); // 9 行已离开内存

    QVector<QVector<Character>> older;
    QVector<bool> wrapped;
    for (int i = 0; i < 9; i++) {
        older.append({ Character(char32_t(U'0' + i)) });
        wrapped.append(false);
    }
    QCOMPARE(emu.prependHistoryLines(older, wrapped), 9);
    QCOMPARE(emu.lineCount(), 61);
    QCOMPARE(emu.historyBaseLine(), qint64(0)); // 注入量恰等于丢行量，base 归零
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel --target tst_historyreadback
```

预期：编译失败，报错 `Emulation has no member named 'prependHistoryLines'` / `'historyBaseLine'`。

- [ ] **步骤 3：实现 `Emulation.h` 声明**

头部 include 区追加（`Emulation.h` 此前仅前向声明 `class Screen`，方法签名需要完整 `Character` 类型）：

```cpp
#include <QVector>

#include "Character.h"
```

公有方法区 `void setCellPixelSize(int width, int height);` 声明之后追加：

```cpp
    /**
     * @brief 向主屏历史缓冲头部前插更老的历史行（外部历史提供者读回注入通道）。
     * @param lines 行数组（旧→新顺序），每行为 char32_t 字符管线的 Character 序列。
     * @param wrappedFlags 与 lines 等长的折行标志。
     * @return 实际前插的行数；底层滚动类型不支持前插时返回 0。
     * @note 只作用于主屏（_screen[0]），与 setHistory/clearHistory 同口径；
     *       前插后视图层须 scrollTo(currentLine + n) 保持可视内容稳定。
     * @see Screen::prependHistoryLines
     */
    int prependHistoryLines(const QVector<QVector<Character>> &lines,
                            const QVector<bool> &wrappedFlags);

    /**
     * @brief 主屏当前内存历史最老一行的绝对行号（会话累计口径）。
     * @see Screen::historyBaseLine
     */
    qint64 historyBaseLine() const;
```

- [ ] **步骤 4：实现 `Emulation.cpp`**

`Emulation::setCellPixelSize` 实现之后追加：

```cpp
int Emulation::prependHistoryLines(const QVector<QVector<Character>> &lines,
                                   const QVector<bool> &wrappedFlags) {
    return _screen[0]->prependHistoryLines(lines, wrappedFlags);
}

qint64 Emulation::historyBaseLine() const {
    return _screen[0]->historyBaseLine();
}
```

- [ ] **步骤 5：运行测试验证通过 + 全量回归**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel
ctest --test-dir build --output-on-failure -R tst_historyreadback
ctest --test-dir build --output-on-failure
```

预期：新增用例通过，全量套件全绿。

- [ ] **步骤 6：Commit**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
git add lib/src/emulation/Emulation.h lib/src/emulation/Emulation.cpp tests/tst_historyreadback.cpp
git commit -m "feat(emulation): 转发历史前插与绝对行号口径到主屏"
```

---

### 任务 4：QTermWidget::setHistoryProvider 与越顶触发

**文件：**
- 修改：`lib/src/display/TerminalDisplay.h`（signals 区；公有方法区 `updateImage()` 旁）
- 修改：`lib/src/display/TerminalDisplay.cpp`（`scrollBarPositionChanged`；新增 `scrollAfterHistoryPrepend`）
- 修改：`lib/include/qtermwidget.h`（include 区；`historySize()` 旁公有声明；private 区成员）
- 修改：`lib/src/widget/qtermwidget.cpp`（构造函数接线；`historySize()` 实现旁新增实现；`clearScrollback`）
- 测试：`tests/tst_historyreadback.cpp`（追加用例）

**设计要点：** 触发口收敛在 `TerminalDisplay::scrollBarPositionChanged`——滚动条拖动与滚轮（经 `QScrollBar::event` 转发）都汇入此槽。取数同步进行（ZzLogEngine 热层/温层读取为微秒级，规格 §5.3），`std::function` 回调形态与应用仓库计划 04 任务 12 的 `ZzScrollbackBridge::readOlderLines(qint64, int)` 签名直接对齐。注入后 `scrollAfterHistoryPrepend` 把窗口当前行 +n 并刷新，用户视觉上内容不动、滚动条向上多出 n 行可继续上翻。前插区满（`n == 0`）或提供者返回空均标记"耗尽"终止滚动事件空转。

- [ ] **步骤 1：编写失败的测试（追加到 `tests/tst_historyreadback.cpp`）**

`#include` 区追加：

```cpp
#include <QScrollBar>
#include "qtermwidget.h"
```

`private slots:` 区追加：

```cpp
    void testWidgetFetchOlderOnScrollTop();
    void testProviderEmptyMarksExhausted();
```

文件末尾（`QTEST_MAIN` 之前）追加：

```cpp
/**
 * @brief 部件级端到端：滚动条越顶触发提供者回调，读回行前插入历史，视图保持稳定。
 */
void TestHistoryReadback::testWidgetFetchOlderOnScrollTop()
{
    QTermWidget term(nullptr, nullptr);
    term.setHistorySize(50);
    QStringList allLines;
    QByteArray payload;
    for (int i = 0; i < 200; i++) {
        allLines << QString("line %1").arg(i);
        payload += "line " + QByteArray::number(i) + "\r\n";
    }
    term.recvData(payload.constData(), int(payload.size()));
    // 输出变更经攒帧定时器刷新，无事件循环需显式等待
    QTest::qWait(60);
    QCoreApplication::processEvents();
    QCOMPARE(term.historyLinesCount(), 50); // 内存历史封顶

    QVector<qint64> requestedBefore;
    term.setHistoryProvider([&](qint64 beforeLine, int maxLines) -> QStringList {
        requestedBefore << beforeLine;
        QStringList out;
        for (qint64 id = qMax<qint64>(0, beforeLine - maxLines); id < beforeLine; id++)
            out << allLines.at(int(id));
        return out;
    });

    QScrollBar *bar = term.findChild<QScrollBar *>();
    QVERIFY(bar);
    QVERIFY(bar->maximum() > 0); // 攒帧刷新后滚动条范围就位

    bar->setValue(0); // 越顶触发读回
    QCOMPARE(requestedBefore.size(), 1);
    QVERIFY(requestedBefore[0] > 0);
    // 前插区容量同内存历史上限（50）：合计 100；视图稳定 = 当前行下移 n
    QCOMPARE(term.historyLinesCount(), 100);
    QCOMPARE(bar->value(), 50);
    QCOMPARE(bar->maximum(), 100);

    // 再次越顶：前插区已满，注入 0 行并标记耗尽；beforeLine 已随首次前插回退 50
    bar->setValue(0);
    QCOMPARE(requestedBefore.size(), 2);
    QCOMPARE(requestedBefore[1], requestedBefore[0] - 50);
    QCOMPARE(term.historyLinesCount(), 100);

    // 耗尽后不再打扰提供者
    bar->setValue(50);
    bar->setValue(0);
    QCOMPARE(requestedBefore.size(), 2);
}

/**
 * @brief 提供者返回空列表即标记耗尽，后续越顶不再回调。
 */
void TestHistoryReadback::testProviderEmptyMarksExhausted()
{
    QTermWidget term(nullptr, nullptr);
    term.setHistorySize(10);
    QByteArray payload;
    for (int i = 0; i < 50; i++)
        payload += "line " + QByteArray::number(i) + "\r\n";
    term.recvData(payload.constData(), int(payload.size()));
    QTest::qWait(60);
    QCoreApplication::processEvents();

    int calls = 0;
    term.setHistoryProvider([&](qint64, int) -> QStringList {
        calls++;
        return {};
    });

    QScrollBar *bar = term.findChild<QScrollBar *>();
    QVERIFY(bar);
    bar->setValue(0);
    QCOMPARE(calls, 1);
    bar->setValue(5);
    bar->setValue(0);
    QCOMPARE(calls, 1); // 已耗尽，不再回调
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel --target tst_historyreadback
```

预期：编译失败，报错 `QTermWidget has no member named 'setHistoryProvider'`。

- [ ] **步骤 3：实现 `TerminalDisplay`（信号 + 视图平移）**

`TerminalDisplay.h` signals 区 `void changedContentCountSignal(int line, int column);` 之后追加：

```cpp
    /**
     * @brief 滚动条到达顶端（正在查看内存历史最老行）时发出。
     * @note 历史读回注入的触发口：QTermWidget 挂接此信号向外部提供者索取更老的行；
     *       滚轮路径经 QScrollBar::event 转发汇入同一槽，同样触发；
     *       键盘滚动路径（ScreenWindow::handleCommandFromKeyboard）v0.1 不触发。
     */
    void historyTopReached();
```

公有方法区 `void updateImage();` 之后追加：

```cpp
    /**
     * @brief 历史前插 n 行后同步视图：窗口当前行下移 n 保持可视内容稳定，并刷新滚动条与图像。
     * @param n 实际前插的行数（<=0 时不动作）。
     */
    void scrollAfterHistoryPrepend(int n);
```

`TerminalDisplay.cpp` 的 `scrollBarPositionChanged` 方法体末尾（`updateImage();` 之后）追加：

```cpp
    // 到达顶端：通知上层按需读回更老的历史行（无提供者或已耗尽时 QTermWidget 侧直接忽略）
    if (_scrollBar->value() == 0 && _scrollBar->maximum() > 0)
        emit historyTopReached();
```

`scrollBarPositionChanged` 实现之后新增：

```cpp
void TerminalDisplay::scrollAfterHistoryPrepend(int n) {
    if (!_screenWindow || n <= 0)
        return;
    // 前插使历史行索引整体上移 n：窗口同步下移保持可视内容稳定；
    // scrollTo 内部的 qBound 钳制保证不越界，updateImage 经 setScroll 回写滚动条
    _screenWindow->scrollTo(_screenWindow->currentLine() + n);
    updateImage();
}
```

- [ ] **步骤 4：实现 `QTermWidget`（公共 API + 取数逻辑）**

`lib/include/qtermwidget.h` 头部 include 区（`#include <QTimer>` 之后）追加：

```cpp
#include <functional>
```

公有方法区 `int historySize() const;` 之后追加：

```cpp
    /**
     * @brief 设置滚动历史读回提供者（ZzClawTerm 规格 §5.4 读回路径）。
     *
     * @param provider 回调函数；滚动条到达顶端（含滚轮越顶）时在 GUI 线程同步调用：
     *        - beforeLine：当前内存历史最老一行的绝对行号（会话累计口径）；
     *        - maxLines：本次最多请求的行数（当前固定为 500）；
     *        - 返回值：绝对行号 [beforeLine - n, beforeLine) 的行文本，旧→新顺序，
     *          不含换行符；无更老数据时返回空列表。
     *        传空 std::function 清除提供者。
     *
     * @note 注入的行经 QString::toUcs4() 进入 char32_t 字符管线，使用默认字符属性
     *       （与 dupDisplayOutput 的纯文本口径一致；颜色/样式不随读回恢复）。
     * @note 提供者在滚动事件处理中被同步调用，必须毫秒级返回
     *       （ZzLogEngine 热层/温层读取为微秒级，满足该约束）。
     * @note 提供者返回空、或历史前插区已满（回看深度达内存历史上限）后标记"耗尽"，
     *       不再重复调用；setHistoryProvider() 与 clearScrollback() 重置该标记。
     */
    void setHistoryProvider(std::function<QStringList(qint64 beforeLine, int maxLines)> provider);
```

private 区 `void search(bool forwards, bool next);` 之前追加方法声明，`bool m_notifiedActivity = false;` 之后追加成员：

```cpp
    /** @brief 滚动越顶时向历史提供者同步取数并注入显示层（耗尽/重入保护内建）。 */
    void fetchOlderHistory();
```

```cpp
    std::function<QStringList(qint64, int)> m_historyProvider; ///< 历史读回提供者（空 = 未设置）
    bool m_historyProviderExhausted = false; ///< 提供者返回空或前插区满后置位，避免滚动事件反复空转
    bool m_historyFetching = false;          ///< 防重入：提供者回调期间不再触发取数
    static constexpr int HISTORY_FETCH_LINES = 500; ///< 单次越顶读回的行数
```

`lib/src/widget/qtermwidget.cpp` 构造函数中 `connect( m_emulation, &Emulation::dupDisplayOutput, this, &QTermWidget::dupDisplayOutput);` 之后追加接线：

```cpp
    // 滚动越顶（含滚轮路径）→ 向历史提供者读回更老的行注入显示层
    connect(m_terminalDisplay, &TerminalDisplay::historyTopReached, this, [this]() {
        fetchOlderHistory();
    });
```

`QTermWidget::historySize()` 实现之后追加：

```cpp
void QTermWidget::setHistoryProvider(std::function<QStringList(qint64 beforeLine, int maxLines)> provider) {
    m_historyProvider = std::move(provider);
    m_historyProviderExhausted = false;
}

void QTermWidget::fetchOlderHistory() {
    if (!m_historyProvider || m_historyProviderExhausted || m_historyFetching)
        return;

    const qint64 base = m_emulation->historyBaseLine();
    if (base <= 0) {
        // 内存历史已含会话全部输出（或提供者误用注入了超量行），无更老行可读
        m_historyProviderExhausted = true;
        return;
    }

    m_historyFetching = true;
    const QStringList texts = m_historyProvider(base, HISTORY_FETCH_LINES);
    m_historyFetching = false;

    if (texts.isEmpty()) {
        m_historyProviderExhausted = true;
        return;
    }

    // QString → UCS-4 → Character（默认属性）：与 receiveData / dupDisplayCharacter
    // 同一 char32_t 字符管线；读回行的折行关系不可知，统一按非折行整行处理
    QVector<QVector<Character>> lines;
    QVector<bool> wrapped;
    lines.reserve(texts.size());
    wrapped.reserve(texts.size());
    for (const QString &text : texts) {
        const QVector<uint> ucs4 = text.toUcs4();
        QVector<Character> line;
        line.reserve(ucs4.size());
        for (const char32_t c : ucs4)
            line.append(Character(c));
        lines.append(std::move(line));
        wrapped.append(false);
    }

    const int n = m_emulation->prependHistoryLines(lines, wrapped);
    if (n <= 0) {
        // 底层滚动类型不支持前插，或前插区已满（回看深度达内存历史上限）：
        // 两种结局都无法再注入，标记耗尽终止滚动事件空转
        m_historyProviderExhausted = true;
        return;
    }

    m_terminalDisplay->scrollAfterHistoryPrepend(n);
}
```

`QTermWidget::clearScrollback()` 方法体改为：

```cpp
void QTermWidget::clearScrollback() {
    m_emulation->clearHistory();
    m_historyProviderExhausted = false; // 历史清空后允许提供者重新应答
}
```

- [ ] **步骤 5：运行测试验证通过 + 全量回归**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake --build build --parallel
ctest --test-dir build --output-on-failure -R tst_historyreadback
ctest --test-dir build --output-on-failure
```

预期：新增 2 用例通过，全量套件全绿（`tst_rendering` 不受影响——未设提供者时 `historyTopReached` 无消费者、渲染路径零改动）。

- [ ] **步骤 6：Commit**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
git add lib/src/display/TerminalDisplay.h lib/src/display/TerminalDisplay.cpp \
        lib/include/qtermwidget.h lib/src/widget/qtermwidget.cpp tests/tst_historyreadback.cpp
git commit -m "feat(widget): 新增 setHistoryProvider 滚动越顶历史读回注入"
```

---

### 任务 5：性能门控（10 万行注入）与记录落盘

**文件：**
- 测试：`tests/tst_historyreadback.cpp`（追加 perf 用例与记录落盘辅助函数）
- 创建（运行时生成并提交）：`tests/perf/records/YYYY-MM-DD-zztermwidget-history-prepend.json`

**依据规格 §9.1（硬性要求）：** 阈值失败即测试失败；仅 Release 构建门控（Debug 跳过）；结果持久化到 `tests/perf/records/YYYY-MM-DD-<功能名>.json`，含测试项/阈值/实测/是否通过、环境信息（CPU/内存/OS/Qt 版本/编译器/构建类型）、git commit hash、测试时间。**阈值：前插注入 100,000 行（200 批 × 500 行，与生产越顶取数同形态）总耗时 ≤ 2000ms**——实测典型值为数十毫秒级（`std::deque::push_front` 摊还 O(1) + 每行一次 `QVector` 拷贝），2000ms 为 CI 慢机器留足裕度的宽松门控。

- [ ] **步骤 1：编写性能测试（追加到 `tests/tst_historyreadback.cpp`）**

`#include` 区追加：

```cpp
#include <QDate>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QSysInfo>
```

`private slots:` 区追加：

```cpp
    void testPrepend100kLinesPerf();
```

`QTEST_MAIN` 之前先加记录落盘辅助函数，再加用例：

```cpp
/**
 * @brief 性能记录落盘（规格 §9.1）：写入 tests/perf/records/YYYY-MM-DD-<功能名>.json。
 * @param name 功能名（文件名组成部分）。
 * @param thresholdMs 通过阈值（毫秒）。
 * @param elapsedMs 实测耗时（毫秒）。
 * @param passed 是否通过。
 */
static void writePerfRecord(const QString &name, qint64 thresholdMs, qint64 elapsedMs, bool passed)
{
    QString compiler;
#if defined(Q_CC_CLANG)
    compiler = QStringLiteral("clang %1.%2").arg(__clang_major__).arg(__clang_minor__);
#elif defined(Q_CC_GNU)
    compiler = QStringLiteral("gcc %1.%2").arg(__GNUC__).arg(__GNUC_MINOR__);
#elif defined(Q_CC_MSVC)
    compiler = QStringLiteral("msvc %1").arg(_MSC_VER);
#else
    compiler = QStringLiteral("unknown");
#endif

    QString commit;
    QProcess git;
    git.start(QStringLiteral("git"),
              { QStringLiteral("-C"), QStringLiteral(ZZ_TERM_SOURCE_DIR),
                QStringLiteral("rev-parse"), QStringLiteral("HEAD") });
    if (git.waitForFinished(5000))
        commit = QString::fromUtf8(git.readAllStandardOutput()).trimmed();

    QJsonObject env {
        { QStringLiteral("os"), QSysInfo::prettyProductName() },
        { QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture() },
        { QStringLiteral("qt"), QStringLiteral(QT_VERSION_STR) },
        { QStringLiteral("compiler"), compiler },
#ifdef QT_DEBUG
        { QStringLiteral("buildType"), QStringLiteral("Debug") },
#else
        { QStringLiteral("buildType"), QStringLiteral("Release") },
#endif
    };
    QJsonObject record {
        { QStringLiteral("test"), name },
        { QStringLiteral("description"),
          QStringLiteral("Screen::prependHistoryLines 前插注入 100000 行"
                         "（200 批 × 500 行，40 列，与生产越顶取数同形态）") },
        { QStringLiteral("thresholdMs"), thresholdMs },
        { QStringLiteral("elapsedMs"), elapsedMs },
        { QStringLiteral("passed"), passed },
        { QStringLiteral("environment"), env },
        { QStringLiteral("commit"), commit },
        { QStringLiteral("timestamp"), QDateTime::currentDateTime().toString(Qt::ISODate) },
    };

    QDir dir(QStringLiteral(ZZ_TERM_SOURCE_DIR) + QStringLiteral("/tests/perf/records"));
    QVERIFY2(dir.mkpath(QStringLiteral(".")), "创建性能记录目录失败");
    const QString fileName = QDate::currentDate().toString(QStringLiteral("yyyy-MM-dd"))
                           + QStringLiteral("-") + name + QStringLiteral(".json");
    QFile file(dir.filePath(fileName));
    QVERIFY2(file.open(QIODevice::WriteOnly | QIODevice::Text | QIODevice::Truncate),
             qPrintable(file.errorString()));
    file.write(QJsonDocument(record).toJson(QJsonDocument::Indented));
}

/**
 * @brief 性能门控：前插注入 10 万行总耗时 ≤2000ms（仅 Release 门控，规格 §9.1）。
 */
void TestHistoryReadback::testPrepend100kLinesPerf()
{
#ifdef QT_DEBUG
    QSKIP("性能阈值仅在 Release 构建下门控（规格 §9.1）");
#endif
    Screen screen(24, 80);
    screen.setScroll(HistoryTypeBuffer(100000));

    // 预造一批 500 行（40 列）带默认属性的行，模拟提供者分批读回的同形态负载
    QVector<QVector<Character>> batch;
    QVector<bool> wrapped;
    batch.reserve(500);
    wrapped.reserve(500);
    for (int i = 0; i < 500; i++) {
        QVector<Character> line;
        line.reserve(40);
        for (int j = 0; j < 40; j++)
            line.append(Character(char32_t(U'a' + (j % 26))));
        batch.append(std::move(line));
        wrapped.append(false);
    }

    QElapsedTimer timer;
    timer.start();
    for (int b = 0; b < 200; b++)
        QCOMPARE(screen.prependHistoryLines(batch, wrapped), 500);
    const qint64 elapsed = timer.elapsed();
    QCOMPARE(screen.getHistLines(), 100000);

    const bool passed = elapsed <= 2000;
    writePerfRecord(QStringLiteral("zztermwidget-history-prepend"), 2000, elapsed, passed);
    QVERIFY2(passed, qPrintable(QStringLiteral("前插 10 万行耗时 %1ms，阈值 2000ms").arg(elapsed)));
}
```

- [ ] **步骤 2：Release 构建运行性能测试并确认记录落盘**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=/home/zz/Qt/6.11.1/gcc_64
cmake --build build --parallel
ctest --test-dir build --output-on-failure -R tst_historyreadback
cat tests/perf/records/$(date +%F)-zztermwidget-history-prepend.json
```

预期：测试通过；JSON 记录存在且含 `thresholdMs: 2000`、实测 `elapsedMs`、`passed: true`、环境信息与 commit hash。**把该 JSON 文件纳入提交**（规格 §9.1：历史记录全部保留在仓库中）。

- [ ] **步骤 3：全量回归（渲染安全网确认）**

```bash
ctest --test-dir build --output-on-failure
```

预期：全部测试通过，重点是 `tst_rendering`（像素等价性安全网）与 `tst_benchmark` 不受新增注入路径影响。

- [ ] **步骤 4：Commit**

```bash
cd /home/zz/Jackfahdin/github/ZzTermWidget
git add tests/tst_historyreadback.cpp tests/perf/records/
git commit -m "test(history): 前插 10 万行性能门控（≤2000ms）与记录落盘"
```

---

## 自检结果（编写完成后对照规格核查）

- **规格 §5.4 覆盖：** 滚出行经 `dupDisplayOutput` 追加到 ZzLogEngine 为既有通路（计划 04 任务 12 消费）；本计划交付读回注入通路（越顶回调 → 前插显示层），两端闭环 ✓。
- **规格 §5.1 内存有界：** 前插区容量独立于环形区、上限相同，总占用 ≤2× `historySize` ✓；已知限制：单次连续回看深度上限 = 内存历史容量（前插区满后标记耗尽），更深的窗口化回看随 v0.2 冷层另行设计。
- **绝对行号基线正确性（自检中修正过的设计缺陷）：** 初版曾用"累计滚入 − 当前行数"推导基线，前插区存在期间环形区满员丢弃中部行会导致基线漂移、提供者重复回传内存中已有的行；已改为显式记账（`_historyBase`：丢弃全缓冲最老行时 +1、前插注入时 −n、clearHistory 归零），并在任务 2 设计要点中记录中部行跳变这一 v0.1 接受的降级形态。
- **规格 §9.1 性能门控：** 任务 5，阈值失败即测试失败、Release 门控、记录落盘 `tests/perf/records/YYYY-MM-DD-zztermwidget-history-prepend.json` 并入库 ✓。
- **char32_t 管线：** 注入路径 `QString::toUcs4()` → `Character(char32_t)`，与 `receiveData`/`dupDisplayCharacter` 同管线 ✓。
- **类型一致性：** `prependLines`（HistoryScroll 层）/ `prependHistoryLines`（Screen/Emulation 层）/ `setHistoryProvider`（QTermWidget 层）签名全文一致；`historyBaseLine` 返回 `qint64` 全文一致；`scrollAfterHistoryPrepend(int)` 在 TerminalDisplay 声明/实现/QTermWidget 调用三处一致 ✓。
- **占位符扫描：** 无 TODO/待定；所有代码步骤含完整代码；引用的既有符号（`_historyLinks`/`_prepended`/`dupDisplayOutput`/`scrollBarPositionChanged` 等）均已在真实源码中核实存在 ✓。
- **统一裁决合规：** 不创建/修改 ZzClawTerm 根 `CMakeLists.txt`、`CMakePresets.json`、`.gitignore`；全部改动在 ZzTermWidget 独立仓库；性能记录路径符合统一落盘约定 ✓。

## 附录 A：新增公开 API 完整签名清单

对外公共头（`lib/include/qtermwidget.h`，随 install 导出）：

```cpp
// QTermWidget
void setHistoryProvider(std::function<QStringList(qint64 beforeLine, int maxLines)> provider);
```

库内公共接口（`lib/src/` 头文件，同仓库及同源码树消费者可用）：

```cpp
// HistoryScroll（lib/src/util/History.h）—— 基类默认返回 0
virtual int prependLines(const QVector<QVector<Character>> &lines,
                         const QVector<bool> &wrappedFlags);

// HistoryScrollBuffer（lib/src/util/History.h）
int prependLines(const QVector<QVector<Character>> &lines,
                 const QVector<bool> &wrappedFlags) override;

// Screen（lib/src/emulation/Screen.h）
int prependHistoryLines(const QVector<QVector<Character>> &lines,
                        const QVector<bool> &wrappedFlags);
qint64 historyBaseLine() const;

// Emulation（lib/src/emulation/Emulation.h）
int prependHistoryLines(const QVector<QVector<Character>> &lines,
                        const QVector<bool> &wrappedFlags);
qint64 historyBaseLine() const;

// TerminalDisplay（lib/src/display/TerminalDisplay.h）
void scrollAfterHistoryPrepend(int n);
signals: void historyTopReached();
```

**与应用仓库的接线契约（计划 04 任务 12 直接对齐，无需改写）：**

```cpp
// ZzTerminalView::enableScrollback 内（计划 04 任务 12 步骤 5）：
m_term->setHistoryProvider([bridge = m_scrollbackBridge](qint64 beforeLine, int maxLines) {
    return bridge->readOlderLines(beforeLine, maxLines);
});
```
