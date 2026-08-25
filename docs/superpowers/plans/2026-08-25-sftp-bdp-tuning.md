# SFTP BDP 自适应调优 + V0.2 小收尾（M6）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 让 SFTP 在高延迟链路（RTT 25-100ms）下吞吐与系统 OpenSSH sftp 比值 ≥0.95（CI netem 门控），并提供设置页手动块大小覆盖。

**架构：** ZzSshCore 新增纯逻辑组件 `ZzSftpTuner`（RTT→参数档），`ZzSftpEngine` 硬编码常量改为可注入上限；RTT 经 SFTP open 探针 + 下载泵往返计时 EWMA 采样，跨档即对后续传输生效。主仓经 `ZzAppSettings::sftp/blockSize`（0=自动）+ 设置页 combo 提供手动覆盖。

**技术栈：** C++20 / Qt 6.8+ / libssh2（嵌套 submodule，不动）/ CMake Presets / Qt Test / Docker + tc netem。

**规格：** `docs/superpowers/specs/2026-08-25-sftp-bdp-tuning-design.md`

**仓库说明：** 任务 1-5 在 `third_party/ZzSshCore`（gitcode submodule，master 分支）；任务 6 在主仓 docs；任务 7 在主仓 src；任务 8 收尾（先推库再 bump 主仓 gitlink）。

**全局约定（每个任务都要遵守）：**

- ZzSshCore 构建：`cmake --build --preset linux-release`（cwd 为 `third_party/ZzSshCore`）
- ZzSshCore 单测：`ctest --test-dir build/linux-release -L unit --output-on-failure`（基线 15 程序）
- ZzSshCore 集成/perf：`tests/integration/docker/run-integration-tests.sh build/linux-release`（基线 16 项；perf 门控偶有位置漂移噪声红，定向 `ctest -R` 复跑甄别）
- 主仓构建/回归：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release`（基线 45 项）
- 主仓全量 ctest 后必须 `git checkout -- tests/perf/records/` 并删除当天新生成的未跟踪 `tests/perf/records/$(date +%F)-*.json`（逐一核对，绝不用月份通配）
- commit：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详述；类名 Zz 前缀、文件名=类名、Doxygen 简体中文注释
- **push 一律等用户明确确认**，计划中不得自动推送

**关键代码事实（调研已核实，实现者直接引用）：**

- `ZzSftpEngine.cpp:8-13`：`stagingCapFor(block)` = 16×块夹 [1MB, 8MB]（匿名命名空间自由函数）
- `ZzSftpEngine.cpp:293-296`：下载读缓冲 = 4×块夹 4MB（startDownload 局部）
- `ZzSftpEngine.cpp:16`：`kPumpCycleProgressCap = 16MB`（**不动**：16MB/周期已远超需求）
- `ZzSftpEngine.h:126-139`：`startUpload/startDownload(requestId, remote, local, blockSize, errorString)`；派生量每次 pump 由 `t.blockSize` 现算——改后续传输的 blockSize 即生效
- `ZzSftpTypes.h:67-88`：`kDefaultBlockSize=256KB`、`clampBlockSize`（≤0→默认，夹 [16KB,4MB]）
- `ZzSftpSession.h:51,154`：`setTransferBlockSize` 仅写本地成员 `m_blockSize`（默认 kDefaultBlockSize）；`ZzSftpSession.cpp:113-116/:129-132` upload/download 快照进 lambda → worker `doSftpUpload/doSftpDownload`（`ZzSshConnectionWorker.cpp:559-593`）→ engine
- `ZzSshConnectionWorker.cpp:420-448`：`doOpenSftp`（engine 创建/open/回调注册处）；泵 `pumpSftpTransfers :603-607` 由 5ms 定时器驱动
- libssh2：SFTP 通道窗口在 `libssh2/src/sftp.c:782` 硬编码 2MB（嵌套 submodule，**不动**）；下载读侧 libssh2 自动扩窗至 `4×窗口默认=8MB` 上限（`sftp.c:1449-1493`）——下载预取硬顶 8MB
- `tst_ZzSftpPerf.cpp:154-159` 阈值常量区、`:214-245` 系统 sftp 参照（QProcess batch）、`:259-296` 块扫描样板、`:247-257/:503-536` 自写 JSON 记录（prune 留 5 份）
- docker：`tests/integration/docker/Dockerfile:8`（无 iproute2）、`run-integration-tests.sh:28-30`（三容器，无 --cap-add）

---

### 任务 1：ZzSftpTuner 纯逻辑组件（RTT→参数档 + env 覆盖解析）

**仓库：** `third_party/ZzSshCore`

**文件：**
- 创建：`src/ZzSftpTuner.h`、`src/ZzSftpTuner.cpp`
- 修改：`src/CMakeLists.txt`（照 ZzSftpEngine 两行模式追加）
- 测试：创建 `tests/unit/tst_ZzSftpTuner.cpp`；修改 `tests/CMakeLists.txt`（照现有 unit 注册模式，无 perf 定义注入）

**设计：** 纯值类型，无 QObject 依赖，头文件不 include Qt（只用 cstdint）。档位表为**默认值**（任务 2 实验校准，校准后回写并 commit 说明）。env 覆盖供任务 2 参数扫描实验与人工调试使用。

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSftpTuner.cpp`：

```cpp
#include <QtTest/QtTest>

#include "ZzSftpTuner.h"

/**
 * @brief ZzSftpTuner 单测：RTT 分档、滞回、env 覆盖解析。
 */
class tst_ZzSftpTuner : public QObject
{
    Q_OBJECT
private slots:
    /** @brief 回环/局域网（RTT<5ms）必须返回与现状逐字节一致的默认值。 */
    void loopbackKeepsCurrentDefaults()
    {
        const ZzSftpTuning t = ZzSftpTuner::tuningForRtt(0.0);
        QCOMPARE(t.blockSize, 256 * 1024);
        QCOMPARE(t.readBufferCap, qint64(4) * 1024 * 1024);
        QCOMPARE(t.stagingCap, qint64(8) * 1024 * 1024);
        QCOMPARE(ZzSftpTuner::tuningForRtt(4.9), t);
    }

    /** @brief 三档升档：5-30 / 30-80 / >80ms。 */
    void tiersEscalateWithRtt()
    {
        QCOMPARE(ZzSftpTuner::tuningForRtt(5.0).blockSize, 512 * 1024);
        QCOMPARE(ZzSftpTuner::tuningForRtt(29.9).blockSize, 512 * 1024);
        QCOMPARE(ZzSftpTuner::tuningForRtt(30.0).blockSize, 1024 * 1024);
        QCOMPARE(ZzSftpTuner::tuningForRtt(79.9).blockSize, 1024 * 1024);
        QCOMPARE(ZzSftpTuner::tuningForRtt(80.0).blockSize, 1024 * 1024);
        QCOMPARE(ZzSftpTuner::tuningForRtt(80.0).stagingCap, qint64(16) * 1024 * 1024);
        // 下载预取受 libssh2 8MB 硬顶约束（sftp.c:1449-1453），任何档不得超过
        QVERIFY(ZzSftpTuner::tuningForRtt(200.0).readBufferCap <= qint64(8) * 1024 * 1024);
    }

    /** @brief 非法输入（负 RTT、NaN 语义外的极端值）按回环默认处理。 */
    void invalidRttFallsBackToDefault()
    {
        QCOMPARE(ZzSftpTuner::tuningForRtt(-1.0), ZzSftpTuner::tuningForRtt(0.0));
    }

    /** @brief 滞回：从高档降档需 EWMA 跌破升档阈值的 0.6 倍（升快降慢防振荡）。 */
    void hysteresisPreventsOscillation()
    {
        ZzSftpTuner tuner;
        tuner.noteRtt(40.0);   // 首采样直接采用：升 1MB 档
        QCOMPARE(tuner.currentTuning().blockSize, 1024 * 1024);
        tuner.noteRtt(20.0);   // EWMA=35：仍 1MB 档
        QCOMPARE(tuner.currentTuning().blockSize, 1024 * 1024);
        tuner.noteRtt(10.0);   // EWMA=28.75：低于 30 但高于 30×0.6=18，不降
        QCOMPARE(tuner.currentTuning().blockSize, 1024 * 1024);
        tuner.noteRtt(10.0);   // EWMA=24.06：不降
        tuner.noteRtt(10.0);   // EWMA=20.55：不降
        tuner.noteRtt(10.0);   // EWMA=17.91 < 18：降档，档位按 17.91 命中 512KB 档
        QCOMPARE(tuner.currentTuning().blockSize, 512 * 1024);
    }

    /** @brief env 覆盖解析：合法三元组、缺项补 0、畸形整串无效。 */
    void envOverrideParsing()
    {
        const auto ok = ZzSftpTuner::parseTuneOverride("512,8,16");
        QVERIFY(ok.valid);
        QCOMPARE(ok.blockSize, 512 * 1024);
        QCOMPARE(ok.readBufferCap, qint64(8) * 1024 * 1024);
        QCOMPARE(ok.stagingCap, qint64(16) * 1024 * 1024);

        const auto partial = ZzSftpTuner::parseTuneOverride("1024,,8");
        QVERIFY(partial.valid);
        QCOMPARE(partial.blockSize, 1024 * 1024);
        QCOMPARE(partial.readBufferCap, qint64(0)); // 缺项=该项默认
        QCOMPARE(partial.stagingCap, qint64(8) * 1024 * 1024);

        QVERIFY(!ZzSftpTuner::parseTuneOverride("abc").valid);
        QVERIFY(!ZzSftpTuner::parseTuneOverride("").valid);
        QVERIFY(!ZzSftpTuner::parseTuneOverride("1,2").valid);   // 必须三元组
    }
};

QTEST_MAIN(tst_ZzSftpTuner)
#include "tst_ZzSftpTuner.moc"
```

`tests/CMakeLists.txt` 照 `zz_add_test(tst_ZzSftpTypes ...)` 同类 unit 注册模式追加 `tst_ZzSftpTuner`（LABELS "unit"，**不**加 perf 定义注入）。

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-release`（cwd `third_party/ZzSshCore`）
预期：编译失败（`src/ZzSftpTuner.h` 不存在）——此即失败验证

- [ ] **步骤 3：实现 ZzSftpTuner**

创建 `src/ZzSftpTuner.h`：

```cpp
#pragma once

#include <cstdint>

/**
 * @brief SFTP 传输参数（BDP 自适应输出；任务 2 实验校准档位表）。
 */
struct ZzSftpTuning
{
    int blockSize = 256 * 1024;            ///< 每块字节数（[16KB,4MB]，见 ZzSftpTypes.h）
    int64_t readBufferCap = 4LL << 20;     ///< 下载读缓冲上限（字节；预取硬顶 8MB 由 libssh2 决定）
    int64_t stagingCap = 8LL << 20;        ///< 上传 staging 在途上限（字节）

    bool operator==(const ZzSftpTuning &) const = default;
};

/**
 * @brief RTT → SFTP 调参的纯逻辑（无 QObject 依赖，可独立单测）。
 *
 * 分档规则（默认表，任务 2 实验校准后回写）：
 *   RTT < 5ms   → {256KB, 4MB, 8MB}（与调优前现状逐字节一致）
 *   5-30ms      → {512KB, 8MB, 8MB}
 *   30-80ms     → {1MB,   8MB, 8MB}
 *   >80ms       → {1MB,   8MB, 16MB}
 * 实例方法 noteRtt 带滞回：升档按上表阈值，降档需跌破对应升档阈值的 0.6 倍
 * （升快降慢，防链路抖动引起参数振荡）。
 */
class ZzSftpTuner
{
public:
    /** @brief 按单次 RTT 采样直接求档位（无状态；非法输入按回环默认）。 */
    static ZzSftpTuning tuningForRtt(double rttMs);

    /** @brief 调参覆盖（实验/调试注入）：解析 "blockKB,readCapMB,stagingCapMB"。 */
    struct TuneOverride
    {
        bool valid = false;       ///< 解析成功（三元组、各项数字或空）
        int blockSize = 0;        ///< 0=该项用默认
        int64_t readBufferCap = 0;
        int64_t stagingCap = 0;
    };
    static TuneOverride parseTuneOverride(const char *text);

    /** @brief 录入一次 RTT 采样（EWMA α=0.25 + 滞回跨档）。 */
    void noteRtt(double rttMs);
    /** @brief 当前生效参数（初始为回环默认档）。 */
    ZzSftpTuning currentTuning() const { return m_current; }
    /** @brief 当前 EWMA 估算的 RTT（毫秒；未采样时为 0）。 */
    double smoothedRtt() const { return m_rtt; }

private:
    double m_rtt = 0.0;          ///< EWMA RTT（0=未采样）
    int m_samples = 0;           ///< 已采样次数（首次直接采用，不做 EWMA）
    int m_tier = 0;              ///< 当前档位索引（0=默认档）
    ZzSftpTuning m_current;      ///< 当前档位参数
};
```

创建 `src/ZzSftpTuner.cpp`：

```cpp
#include "ZzSftpTuner.h"

#include <cstdlib>
#include <string>

namespace {

constexpr int64_t MB = 1LL << 20;

/** @brief 档位边界（升档阈值，毫秒）与对应参数。 */
struct Tier { double rttAbove; ZzSftpTuning tuning; };
constexpr Tier kTiers[] = {
    { 0.0,  { 256 * 1024, 4 * MB,  8 * MB } },  // 回环/局域网：现状默认
    { 5.0,  { 512 * 1024, 8 * MB,  8 * MB } },
    { 30.0, { 1024 * 1024, 8 * MB, 8 * MB } },
    { 80.0, { 1024 * 1024, 8 * MB, 16 * MB } },
};
constexpr int kTierCount = sizeof(kTiers) / sizeof(kTiers[0]);

/** @brief rtt 命中的档位索引（0=默认档；负值/NaN 归 0）。 */
int tierFor(double rttMs)
{
    if (!(rttMs >= 0.0)) // 负值与 NaN
        return 0;
    int idx = 0;
    for (int i = 0; i < kTierCount; ++i) {
        if (rttMs >= kTiers[i].rttAbove)
            idx = i;
    }
    return idx;
}

} // namespace

ZzSftpTuning ZzSftpTuner::tuningForRtt(double rttMs)
{
    return kTiers[tierFor(rttMs)].tuning;
}

ZzSftpTuner::TuneOverride ZzSftpTuner::parseTuneOverride(const char *text)
{
    TuneOverride out;
    if (!text || !*text)
        return out;
    const std::string s(text);
    const auto c1 = s.find(',');
    const auto c2 = c1 == std::string::npos ? c1 : s.find(',', c1 + 1);
    if (c1 == std::string::npos || c2 == std::string::npos
        || s.find(',', c2 + 1) != std::string::npos)
        return out; // 必须恰好三元组
    const auto parseField = [](const std::string &f, long long &v) -> bool {
        if (f.empty()) { v = 0; return true; } // 缺项=该项默认
        char *end = nullptr;
        v = std::strtoll(f.c_str(), &end, 10);
        return end && *end == '\0' && v >= 0;
    };
    long long block = 0, readCap = 0, staging = 0;
    if (!parseField(s.substr(0, c1), block)
        || !parseField(s.substr(c1 + 1, c2 - c1 - 1), readCap)
        || !parseField(s.substr(c2 + 1), staging))
        return out;
    out.valid = true;
    out.blockSize = static_cast<int>(block) * 1024;       // KB → 字节
    out.readBufferCap = readCap * MB;                     // MB → 字节
    out.stagingCap = staging * MB;
    return out;
}

void ZzSftpTuner::noteRtt(double rttMs)
{
    if (!(rttMs >= 0.0))
        return; // 非法采样丢弃
    if (m_samples == 0) {
        m_rtt = rttMs;                                  // 首采样直接采用
    } else {
        m_rtt = m_rtt * 0.75 + rttMs * 0.25;            // EWMA α=0.25
    }
    ++m_samples;

    const int hit = tierFor(m_rtt);
    if (hit > m_tier) {
        m_tier = hit;                                   // 升档快
    } else if (hit < m_tier && m_rtt < kTiers[m_tier].rttAbove * 0.6) {
        m_tier = hit;                                   // 降档慢（滞回）
    }
    m_current = kTiers[m_tier].tuning;
}
```

`src/CMakeLists.txt` 照 `ZzSftpEngine.h/.cpp` 两行模式追加 `ZzSftpTuner.h/.cpp`。

- [ ] **步骤 4：运行测试验证通过**

运行：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit -R ZzSftpTuner --output-on-failure`
预期：PASS（5 用例）

- [ ] **步骤 5：Commit**

```bash
cd third_party/ZzSshCore
git add src/ZzSftpTuner.h src/ZzSftpTuner.cpp src/CMakeLists.txt tests/unit/tst_ZzSftpTuner.cpp tests/CMakeLists.txt
git commit -m "feat(sftp): 新增 ZzSftpTuner 纯逻辑组件（RTT 分档 + 滞回 + env 覆盖解析）

M6 BDP 自适应的决策核心：
- 默认四档（<5/5-30/30-80/>80ms），回环档与现状逐字节一致（行为零变化）
- noteRtt 用 EWMA α=0.25，升档快降档慢（0.6 倍滞回）防链路抖动振荡
- parseTuneOverride 解析 blockKB,readCapMB,stagingCapMB 三元组，
  供任务 2 参数扫描实验与人工调试注入
- 档位表为默认值，任务 2 实验校准后回写"
```

---

### 任务 2：ZzSftpEngine 调参接缝 + netem 设施 + 参数扫描实验 + 结论文档

**仓库：** `third_party/ZzSshCore`

**文件：**
- 修改：`src/ZzSftpEngine.h`（新增 setTuning 接口与成员）、`src/ZzSftpEngine.cpp:6-18,247,293-296,365-366`（硬编码上限改成员）
- 修改：`src/ZzSshConnectionWorker.cpp:420-448`（engine open 后应用 env 覆盖）
- 修改：`tests/integration/docker/Dockerfile:8`、`tests/integration/docker/run-integration-tests.sh:28-30,46-54`
- 创建：`docs/sftp-bdp-tuning.md`（结论文档，ZzSshCore 仓）
- 测试：`tests/unit/` 下既有 SFTP 相关单测文件追加 caps 边界用例（无合适文件则并入 `tst_ZzSftpTuner.cpp`）

**前提：** 任务 1 已完成（ZzSftpTuner 存在）。

- [ ] **步骤 1：engine 硬编码上限改可注入成员**

`src/ZzSftpEngine.h`：公开区追加（Doxygen 简体中文）：

```cpp
    /**
     * @brief 设置调参上限（对之后发起的传输生效；进行中传输不受影响）。
     * @param readBufferCap 下载读缓冲上限（字节），0=恢复默认 4MB。
     * @param stagingCap 上传 staging 在途上限（字节），0=恢复默认 8MB。
     * @note 下载预取另受 libssh2 8MB 硬顶（sftp.c:1449-1453），超过无效。
     */
    void setTuningCaps(qint64 readBufferCap, qint64 stagingCap);
```

`src/ZzSftpEngine.h` 私有成员追加：

```cpp
    qint64 m_readBufferCap = 4LL * 1024 * 1024;  ///< 下载读缓冲上限（setTuningCaps 可调）
    qint64 m_stagingCap = 8LL * 1024 * 1024;     ///< 上传 staging 上限（setTuningCaps 可调）
```

`src/ZzSftpEngine.cpp`：

- `:8-13` `stagingCapFor` 改为成员语境（或保留自由函数加 cap 参数——选择与现状风格接近者）：

```cpp
namespace {
/** @brief 上传 staging 缓冲上限：16×块大小，夹在 [1MB, cap] */
qint64 stagingCapFor(int blockSize, qint64 cap)
{
    const qint64 derived = static_cast<qint64>(blockSize) * 16;
    return qBound<qint64>(qint64(1024 * 1024), derived, cap);
}
}
```

- `:247`（startUpload 预分配）与 `:365-366`（pumpUpload 喂数据上限）的 `stagingCapFor(...)` 调用改传 `m_stagingCap`
- `:293-296`（startDownload 读缓冲）：

```cpp
t->staging.resize(static_cast<qsizetype>(qMin<qint64>(static_cast<qint64>(blockSize) * 4,
                                                    m_readBufferCap)));
```

- 实现 `setTuningCaps`（0=恢复默认）：

```cpp
void ZzSftpEngine::setTuningCaps(qint64 readBufferCap, qint64 stagingCap)
{
    m_readBufferCap = readBufferCap > 0 ? readBufferCap : qint64(4) * 1024 * 1024;
    m_stagingCap = stagingCap > 0 ? stagingCap : qint64(8) * 1024 * 1024;
}
```

- [ ] **步骤 2：engine open 后应用 env 覆盖（实验/调试注入）**

`src/ZzSshConnectionWorker.cpp:420-448`（`doOpenSftp`，engine `open()` 成功之后、入 map 之前）追加：

```cpp
    // 实验/调试注入（M6 任务 2 参数扫描）：ZZ_SFTP_TUNE_OVERRIDE="blockKB,readCapMB,stagingCapMB"
    if (const QByteArray ov = qgetenv("ZZ_SFTP_TUNE_OVERRIDE"); !ov.isEmpty()) {
        const auto parsed = ZzSftpTuner::parseTuneOverride(ov.constData());
        if (parsed.valid) {
            engine->setTuningCaps(parsed.readBufferCap, parsed.stagingCap);
            // blockSize 维度由 start* 入参决定，不在这里应用（扫描时经
            // setTransferBlockSize 传入）
        }
    }
```

（include `"ZzSftpTuner.h"`。）

- [ ] **步骤 3：netem 设施**

`tests/integration/docker/Dockerfile:8` 的包列表追加 `iproute2`。

`tests/integration/docker/run-integration-tests.sh`：
- 三个 `docker run`（:28-30）各加 `--cap-add NET_ADMIN`
- env 导出区（:46-54）追加（容器名按脚本现有 --name 值，指向 2222 端口对应容器）：

```bash
export ZZSSH_IT_CONTAINER_NAME="<脚本中 2222 端口对应的容器名>"
```

- [ ] **步骤 4：参数扫描实验（产出校准数据）**

用任务 1 的块大小接缝 + 本任务的 caps/env 接缝，在 docker 环境注入三档延迟扫描吞吐。逐档执行（以 100ms 档为例）：

```bash
cd third_party/ZzSshCore
tests/integration/docker/run-integration-tests.sh build/linux-release  # 起环境（或复用已起容器）
docker exec <容器名> tc qdisc replace dev eth0 root netem delay 100ms
# 上传窗口拐点：块 256KB 固定，staging 分别 4/8/16MB（env 注入），
# 用现有 perf 测试的块扫描/吞吐用例或手动 sftp 上传 128MB 计时
# 下载预取验证：读缓冲分别 4/8MB（env 注入，超过 8MB 应无增益——libssh2 硬顶）
docker exec <容器名> tc qdisc del dev eth0 root   # 实验完清理
```

实验可用 `tst_ZzSftpPerf` 的既有用例驱动（`ctest --test-dir build/linux-release -R tst_ZzSftpPerf`），也可用系统 `dd`+`sftp` 手动计时——选能最快拿到干净数据的方式，**原始数据（每档每组参数的吞吐）必须逐条记入结论文档**。

同时跑公平性验证：三档延迟下各跑一次系统 OpenSSH sftp 参照（`opensshBaseline` 用例已具备），确认参照与库内吞吐同向劣化。

- [ ] **步骤 5：结论文档 + 档位表回写**

创建 `docs/sftp-bdp-tuning.md`（ZzSshCore 仓），内容：

1. 实验环境（docker 镜像、netem 注入点与命令、负载大小）
2. 上传方向：服务端有效窗口拐点数据 → 结论（stagingCap 超过多少无增益）
3. 下载方向：读缓冲-吞吐曲线 → 结论（libssh2 8MB 硬顶实证）
4. 公平性：OpenSSH 参照三档吞吐 → 比值口径成立性结论
5. **校准后的档位表**（若与任务 1 默认表不同，逐格给数据依据）

若校准值与默认表不同：回写 `src/ZzSftpTuner.cpp` 的 `kTiers` 并在 commit message 说明数据依据；重跑 `ctest --test-dir build/linux-release -L unit -R ZzSftpTuner` 确认用例仍过（用例断言若引用被校准的具体数值，同步更新并说明）。

- [ ] **步骤 6：回归 + Commit**

运行：`ctest --test-dir build/linux-release -L unit --output-on-failure`（15 程序基线 + 新增全绿）

```bash
cd third_party/ZzSshCore
git add src/ZzSftpEngine.h src/ZzSftpEngine.cpp src/ZzSshConnectionWorker.cpp src/ZzSftpTuner.cpp tests/integration/docker/Dockerfile tests/integration/docker/run-integration-tests.sh docs/sftp-bdp-tuning.md tests/
git commit -m "feat(sftp): 调参接缝参数化 + netem 实验设施 + BDP 校准结论

- ZzSftpEngine：stagingCapFor/下载读缓冲上限改 setTuningCaps 可注入
  （0=恢复默认 8MB/4MB，行为零变化）；kPumpCycleProgressCap 16MB 不动
- doOpenSftp 应用 ZZ_SFTP_TUNE_OVERRIDE 环境覆盖（实验/调试注入）
- docker：iproute2 + --cap-add NET_ADMIN + ZZSSH_IT_CONTAINER_NAME 导出
- 参数扫描实验：上传服务端窗口拐点 / 下载 libssh2 8MB 预取硬顶实证 /
  OpenSSH 参照三档公平性，原始数据与校准档位表见 docs/sftp-bdp-tuning.md"
```

---

### 任务 3：RTT 测量 + 自适应接线 + session "0=自动"语义

**仓库：** `third_party/ZzSshCore`

**文件：**
- 修改：`src/ZzSftpEngine.h/.cpp`（RTT 采样成员 + open 探针 + 下载泵 EWMA + tuner 应用 + start* 的 0=自动块大小）
- 修改：`src/ZzSshConnectionWorker.cpp:420-448`（engine 初始 tuning 应用）
- 修改：`src/ZzSftpSession.h:51,154`、`src/ZzSftpSession.cpp:113-116,129-132`（0=自动透传）
- 测试：`tests/unit/` 追加（RTT EWMA/跨档应用逻辑—— engine 层无 mock 接缝的部分并入 `tst_ZzSftpTuner.cpp` 验证 tuner 侧，engine 侧由任务 4 门控实测覆盖）

**前提：** 任务 1、2 已完成。

- [ ] **步骤 1：session "0=自动"语义**

`src/ZzSftpSession.h`：

```cpp
    /**
     * @brief 设置传输块大小（字节）。0/负值=自动（库内 BDP 自适应，M6）；
     *        正值经 clampBlockSize 夹取 [16KB,4MB]。对已发起的传输不生效。
     */
    void setTransferBlockSize(int bytes) { m_blockSize = bytes <= 0 ? 0 : ZzSftp::clampBlockSize(bytes); }
```

`m_blockSize` 默认值改 `0`（h:154），注释改"0=自动（BDP 自适应）；非 0=手动块大小"。

`src/ZzSftpSession.cpp:113-116/:129-132`：upload/download lambda 快照**直接传 `m_blockSize`**（0 或手动值）——检查现有快照代码，若存在 `m_blockSize` 经 clamp 处理的逻辑改为原样透传 0。

- [ ] **步骤 2：engine 侧 0=自动块大小 + tuner 持有**

`src/ZzSftpEngine.h` 私有成员追加：

```cpp
    ZzSftpTuner m_tuner;                 ///< BDP 自适应决策（RTT EWMA + 滞回）
    int m_tuningBlockSize = 256 * 1024;  ///< 当前自动块大小（start* 入参为 0 时采用）
```

公开区追加：

```cpp
    /** @brief 测试观察口：当前 EWMA RTT（毫秒）。 */
    [[nodiscard]] double smoothedRtt() const { return m_tuner.smoothedRtt(); }
    /** @brief 测试观察口：当前自动块大小。 */
    [[nodiscard]] int tuningBlockSize() const { return m_tuningBlockSize; }
```

`src/ZzSftpEngine.cpp`：

- `startUpload/startDownload` 内 `t->blockSize` 赋值处（:216-300 区域）：

```cpp
    // 0=自动：采用 tuner 当前档位块大小（回环档=256KB，与调优前一致）
    t->blockSize = blockSize > 0 ? ZzSftp::clampBlockSize(blockSize) : m_tuningBlockSize;
```

- 新增私有方法（RTT 采样统一入口）：

```cpp
    /**
     * @brief 录入一次 RTT 采样并应用 tuner 结果（跨档时更新 caps 与自动块大小）。
     * @note 仅 worker 线程调用（engine 全部状态均归属该线程）。
     */
    void noteRttSample(double rttMs)
    {
        const int before = m_tuningBlockSize;
        m_tuner.noteRtt(rttMs);
        const ZzSftpTuning t = m_tuner.currentTuning();
        m_tuningBlockSize = t.blockSize;
        setTuningCaps(t.readBufferCap, t.stagingCap);
        if (m_tuningBlockSize != before) {
            // 跨档：对之后发起的传输生效（进行中传输不动，规格 §4.3）
        }
    }
```

- [ ] **步骤 3：open 探针 + 下载泵采样**

- `src/ZzSftpEngine.cpp:28`（`open()` 的 `libssh2_sftp_init` 成功后）：以 `libssh2_sftp_stat(m_sftp, ".", &attrs)` 计时（QElapsedTimer），成功即 `noteRttSample(elapsed)`；失败/超时不采样（不影响 open 语义）。stat 失败不视为 open 失败。
- `pumpDownload`（:406-434）内：对单次 `libssh2_sftp_read` 调用计时（仅返回 >0 的成功读），`noteRttSample()`——成功读的往返天然是 RTT 样本（含服务器处理时间，偏保守可用）。上传泵 write 异步不等 ACK，不采样。

- [ ] **步骤 4：worker 初始应用 + 回归**

`src/ZzSshConnectionWorker.cpp:420-448`：env 覆盖注入后补一行注释说明顺序（env 覆盖在 engine open 探针之前应用；探针采样后 tuner 可能再次 setTuningCaps 覆盖 env——**语义：env 覆盖只用于无自适应参与的实验扫描，自适应生效后以 tuner 为准**。实验扫描时 RTT<5ms 回环档的 tuner 输出恰好等于默认 caps，不会覆盖 env——在结论文档中已注明该用法边界）。

运行：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit --output-on-failure`
预期：15 程序 + 新增全绿；`tst_ZzSftpIT` 等调用 `setTransferBlockSize(具体值)` 的既有测试不受影响（手动语义不变）

再跑集成确认零回归：`tests/integration/docker/run-integration-tests.sh build/linux-release`（16 项；perf 噪声红按全局约定定向复跑甄别）

- [ ] **步骤 5：Commit**

```bash
cd third_party/ZzSshCore
git add src/ZzSftpEngine.h src/ZzSftpEngine.cpp src/ZzSshConnectionWorker.cpp src/ZzSftpSession.h src/ZzSftpSession.cpp tests/
git commit -m "feat(sftp): RTT 采样 + BDP 自适应接线 + 块大小 0=自动语义

- engine：open 成功即 FXP_STAT 探针测初始 RTT；下载泵对成功 read
  计时做 EWMA 滑动采样（α=0.25 + 滞回，上传异步 write 不采样）
- 跨档即对后续传输生效（setTuningCaps + 自动块大小），进行中传输不动
- start* 块大小入参 0=自动（采用 tuner 当前档；回环档 256KB 行为零变化）
- ZzSftpSession::setTransferBlockSize 0/负值=自动语义，默认改 0；
  upload/download 原样透传（手动值语义不变，既有测试不受影响）"
```

---

### 任务 4：netem 三档比值门控（tst_ZzSftpWanPerf）

**仓库：** `third_party/ZzSshCore`

**文件：**
- 创建：`tests/perf/tst_ZzSftpWanPerf.cpp`
- 修改：`tests/CMakeLists.txt:137-144`（照 tst_ZzSftpPerf 注册模式追加，含 ZZ_PERF_RECORDS_DIR 等定义注入，LABELS "perf"）
- 修改：`tests/integration/docker/run-integration-tests.sh`（如需补充 env 导出）

**前提：** 任务 2（设施）、3（自适应）已完成。

- [ ] **步骤 1：编写门控测试**

创建 `tests/perf/tst_ZzSftpWanPerf.cpp`（骨架照 `tst_ZzSftpPerf.cpp`：配置经 `ZzSshTestServerConfig::fromEnvironment()`、负载生成、系统 sftp 参照 `runSystemSftp` 模式、自写 JSON 记录）：

```cpp
/**
 * @brief WAN 高延迟门控（M6 规格 §二/§五）：netem 三档 RTT 下与系统
 *        OpenSSH sftp 的吞吐比值 ≥0.95。
 *
 * 延迟经 `docker exec $ZZSSH_IT_CONTAINER_NAME tc qdisc` 注入容器 eth0
 * （双向中的出站方向，RTT≈注入值）；每档结束清理 qdisc。容器无
 * NET_ADMIN 或 tc 失败 → FAIL（不放行，规格 §4.4）。
 *
 * 仅经 tests/integration/docker/run-integration-tests.sh 运行（labels perf）。
 */
class tst_ZzSftpWanPerf : public QObject
{
    Q_OBJECT
private slots:
    void wanRatioGate();
};
```

`wanRatioGate` 逻辑（伪码即实现纲——照 perf 样板的连接/参照代码复用）：

```cpp
void tst_ZzSftpWanPerf::wanRatioGate()
{
    const QByteArray container = qgetenv("ZZSSH_IT_CONTAINER_NAME");
    QVERIFY2(!container.isEmpty(), "ZZSSH_IT_CONTAINER_NAME 未设置（需经 docker 脚本运行）");

    const double delays[] = {25.0, 50.0, 100.0};
    for (const double d : delays) {
        // 注入（replace 幂等覆盖上一档；首次 add 失败时 replace 也会创建）
        QProcess tc;
        tc.start("docker", {"exec", container, "tc", "qdisc", "replace",
                            "dev", "eth0", "root", "netem", "delay",
                            QString::number(d) + "ms"});
        QVERIFY2(tc.waitForFinished(10000) && tc.exitCode() == 0,
                 qPrintable(QStringLiteral("tc 注入失败（NET_ADMIN 缺失？）：%1")
                            .arg(QString::fromUtf8(tc.readAllStandardError()))));

        // 负载照 tst_ZzSftpPerf 的 128MB 确定性生成（高延迟下可降为 32MB，
        // 在 initTestCase 按档决定——选择后全文统一）
        // 库内吞吐：自动模式（setTransferBlockSize 不调用/设 0）各跑 2 次取最优
        // 系统参照：runSystemSftp 同款 batch，各跑 2 次取最优
        // 断言：上行比值 ≥0.95 且 下行比值 ≥0.95
        // 记录：addRecord 名称带档位后缀（如 "wan-upload-100ms"）
    }

    // 清理（必须执行，含断言失败路径——用作用域守卫或最后统一 del）
    QProcess::execute("docker", {"exec", container, "tc", "qdisc", "del",
                                 "dev", "eth0", "root"});
}
```

**实现要求：**
- qdisc 清理必须含失败路径（`QScopeGuard` 或每档 finally 语义）
- 比值计算、采样次数（2 次取优）、记录 JSON 字段与 `tst_ZzSftpPerf` 同款（feature 名 `zzsftp-wan`，prune 机制复用则记录文件独立命名 `yyyy-MM-dd-zzsftp-wan-<runId>.json`）
- 负载大小：高延迟下 128MB 单档可能耗时过长（100ms × 20MB/s ≈ 7s/次，可接受；若实测超时改为 32MB 并在文件注释说明）

- [ ] **步骤 2：运行验证**

运行：`tests/integration/docker/run-integration-tests.sh build/linux-release`
预期：16 项基线 + 新门控全绿；三档比值记录入库 `tests/perf/records/`

- [ ] **步骤 3：Commit**

```bash
cd third_party/ZzSshCore
git add tests/perf/tst_ZzSftpWanPerf.cpp tests/CMakeLists.txt tests/integration/docker/run-integration-tests.sh
git commit -m "test(sftp): netem 三档（25/50/100ms）WAN 比值门控

- 新 tst_ZzSftpWanPerf：docker exec tc qdisc replace 注入容器 eth0 延迟，
  每档库内（自动模式）与系统 OpenSSH sftp 各 2 次取最优，比值 ≥0.95 放行
- tc 不可用/NET_ADMIN 缺失 → FAIL 不静默跳过（规格 §4.4）
- qdisc 清理含失败路径；记录独立文件 zzsftp-wan-<runId>.json 入库"
```

---

### 任务 5：V0.2 人工验收清单文档

**仓库：** 主仓

**文件：**
- 创建：`docs/acceptance/v0.2-manual-acceptance.md`

- [ ] **步骤 1：编写清单**

照 `docs/acceptance/v0.1-manual-acceptance.md` 的格式（逐平台打勾、任何一项不通过即打回），内容如下（逐字使用）：

```markdown
# ZzClawTerm v0.2 人工验收清单

> 覆盖 v0.2 五项功能（SFTP 面板、端口转发、终端分屏、密钥环凭据后端、日志冷层）
> 与 M6 SFTP 性能调优。任何一项不通过即打回。与 X11 M5 验收清单一并执行。

## 1. SFTP 面板

- [ ] SSH 会话连接后 SFTP 面板列出远端目录，双击目录可进入，`..` 可返回
- [ ] 上传本地文件到远端：进度条推进、完成后远端内容逐字节一致（`sha256sum` 对比）
- [ ] 下载远端文件到本地：进度条推进、完成后内容逐字节一致
- [ ] 大目录（>1000 条目）滚动不卡顿
- [ ] 本地会话（local shell）下面板提示不可用，不报错

## 2. SFTP 性能（M6 调优后）

- [ ] 对一台 RTT ≥50ms 的真实服务器（`ping` 实测记录数值）上传 100MB 文件，
      速率与同台机器上系统 `sftp` 命令上传同一文件的速率相当（不劣于 95%）
- [ ] 同条件下载 100MB 文件，速率同样不劣于系统 `sftp` 的 95%
- [ ] 设置页把"SFTP 块大小"改为手动 1MB 后重传，行为生效（速率变化或
      与自动模式相当）；改回"自动"恢复
- [ ] 回环/局域网服务器上传下载速率不低于调优前水平（无回归）

## 3. 端口转发

- [ ] 本地转发（-L）：经跳板访问远端内网服务（如 `curl localhost:本地端口`）成功
- [ ] 远程转发（-R）：远端可经转发端口访问本机服务
- [ ] 动态转发（-D）：以 SOCKS5 代理访问（`curl --socks5 localhost:端口`）成功
- [ ] 状态栏隧道计数随规则启停正确变化

## 4. 终端分屏

- [ ] Ctrl+Shift+E/O 左右/上下分屏，各窗格独立会话互不影响
- [ ] Ctrl+Shift+方向键 焦点在窗格间移动
- [ ] 关闭最后一个窗格时整标签关闭

## 5. 密钥环凭据后端

- [ ] Linux（libsecret）：设置页切到"系统密钥环"，密码会话连接正常，
      凭据在系统密钥环工具（seahorse/secret-tool）中可见
- [ ] Windows（wincred，**首次实机验证**）：同上，凭据在 Windows 凭据管理器
      （cmdkey /list）中可见；连接、改密、删除均正常
- [ ] macOS（Keychain，**首次实机验证**）：同上，凭据在"钥匙串访问"中可见
- [ ] 切回 AES 加密文件模式，旧凭据仍可用（不迁移、不丢失）

## 6. 日志冷层（SQLite + FTS5）

- [ ] 会话产生 >10 万行输出后，热层行数有界（设置的热层上限），
      冷层数据库文件（sessions.db）增长
- [ ] 滚动到旧内容（超出热层）显示正常、无卡顿
- [ ] 清理策略生效：超过保留天数的数据被清除
```

- [ ] **步骤 2：Commit**

```bash
git add docs/acceptance/v0.2-manual-acceptance.md
git commit -m "docs(acceptance): V0.2 人工验收清单（五项功能 + SFTP 性能实测）

- 填补 V0.2 无人工验收清单的空白（docs/acceptance 此前仅 v0.1）
- SFTP 性能实测项要求真实高延迟服务器（RTT≥50ms）与系统 sftp 对比 ≥95%
- 密钥环 Windows/macOS 标注首次实机验证（此前仅 Linux libsecret 实测）
- 与 X11 M5 验收清单一并由用户执行"
```

---

### 任务 6：主仓设置项 sftp/blockSize + 设置页 + 面板接线

**仓库：** 主仓

**文件：**
- 修改：`src/settings/ZzAppSettings.h/.cpp`（新增 sftp/blockSize，int，0=自动）
- 修改：`src/settings/ZzSettingsPage.h/.cpp`（combo：自动/64K/128K/256K/512K/1M/2M/4M）
- 修改：`src/panel/ZzSftpOps.h`（接口加 setTransferBlockSize）、`src/panel/ZzSftpSessionOps.h/.cpp`（实现）、`tests/mocks/ZzMockSftpOps.h/.cpp`（记录调用）、`src/panel/ZzSftpPanel.h/.cpp`（会话创建时应用 + settingsChanged 更新）
- 测试：`tests/unit/tst_ZzAppSettings.cpp`、`tests/unit/tst_ZzSettingsPage.cpp`、`tests/unit/tst_ZzSftpPanel.cpp`

- [ ] **步骤 1：编写失败的测试**

`tests/unit/tst_ZzAppSettings.cpp` 追加（构造模式照既有用例）：

```cpp
    void sftpBlockSizeDefaultsToAuto()
    {
        ZzAppSettings settings(<临时 INI 路径，照本文件既有模式>);
        QCOMPARE(settings.sftpBlockSize(), 0); // 0=自动（BDP 自适应，M6）
    }

    void sftpBlockSizeRoundtrip()
    {
        ZzAppSettings settings(<临时 INI 路径>);
        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
        settings.setSftpBlockSize(1024 * 1024);
        QCOMPARE(settings.sftpBlockSize(), 1024 * 1024);
        QCOMPARE(spy.count(), 1);
        settings.setSftpBlockSize(1024 * 1024); // 同值短路
        QCOMPARE(spy.count(), 1);
    }
```

`tests/unit/tst_ZzSettingsPage.cpp` 追加：

```cpp
    void sftpBlockSizeComboReflectsAndWrites()
    {
        // …照本文件既有模式构造 settings 与 page…
        auto *combo = page.sftpBlockSizeCombo();
        QVERIFY(combo);
        QCOMPARE(combo->currentData().toInt(), 0); // 默认"自动"
        const int idx = combo->findData(1024 * 1024);
        QVERIFY(idx >= 0);
        combo->setCurrentIndex(idx);              // currentIndexChanged 即写
        QCOMPARE(settings.sftpBlockSize(), 1024 * 1024);
    }
```

`tests/unit/tst_ZzSftpPanel.cpp` 追加（mock 模式照本文件既有用例）：

```cpp
    void appliesSettingsBlockSizeToSession()
    {
        // ZzAppSettings::instance() 设 1MB → 面板创建/切换会话时
        // ZzMockSftpOps 应记录 setTransferBlockSize(1MB)
        // …照本文件既有 mock 装配模式…
        QVERIFY(mock->recordedBlockSizes().contains(1024 * 1024));
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cmake --build --preset linux-gcc-release`
预期：编译失败（`sftpBlockSize`/`sftpBlockSizeCombo`/`setTransferBlockSize` 不存在）——此即失败验证

- [ ] **步骤 3：实现**

`src/settings/ZzAppSettings.h/.cpp`（同值短路模式照 `setCredentialBackend`）：

```cpp
    /** @brief SFTP 块大小（字节）：0=自动（库内 BDP 自适应，M6 默认）；
     *        手动值夹取 [16KB,4MB]（经 ZzSftpSession::setTransferBlockSize 生效）。 */
    [[nodiscard]] int sftpBlockSize() const;
    void setSftpBlockSize(int bytes);
```

```cpp
int ZzAppSettings::sftpBlockSize() const
{
    return m_settings->value(QStringLiteral("sftp/blockSize"), 0).toInt();
}

void ZzAppSettings::setSftpBlockSize(int bytes)
{
    if (sftpBlockSize() == bytes) {
        return;
    }
    m_settings->setValue(QStringLiteral("sftp/blockSize"), bytes);
    emit settingsChanged();
}
```

`src/settings/ZzSettingsPage.h/.cpp`：combo（itemData 存字节数，0=自动），访问器 `sftpBlockSizeCombo()`，连接：

```cpp
    m_sftpBlockSizeCombo = new QComboBox(this);
    m_sftpBlockSizeCombo->addItem(QStringLiteral("自动（BDP 自适应）"), 0);
    for (int kb : {64, 128, 256, 512, 1024, 2048, 4096}) {
        m_sftpBlockSizeCombo->addItem(
            kb >= 1024 ? QStringLiteral("%1 MB").arg(kb / 1024)
                       : QStringLiteral("%1 KB").arg(kb),
            kb * 1024);
    }
    const int bsIndex = m_sftpBlockSizeCombo->findData(m_settings->sftpBlockSize());
    m_sftpBlockSizeCombo->setCurrentIndex(bsIndex >= 0 ? bsIndex : 0);
    m_sftpBlockSizeCombo->setToolTip(QStringLiteral(
        "手动值对高延迟链路可能更优；自动模式按链路 RTT 自适应（推荐）。\n"
        "进行中的传输不受影响，下一传输生效。"));
    layout->addRow(QStringLiteral("SFTP 块大小："), m_sftpBlockSizeCombo);
    // 连接区：
    connect(m_sftpBlockSizeCombo, &QComboBox::activated, this, [this](int index) {
        m_settings->setSftpBlockSize(m_sftpBlockSizeCombo->itemData(index).toInt());
    });
```

`src/panel/ZzSftpOps.h`：接口追加（纯虚；这是接口变更，mock 与生产适配器都必须实现）：

```cpp
    /**
     * @brief 设置传输块大小（字节）：0=自动（BDP 自适应）；手动值夹取 [16KB,4MB]。
     * @note 对已发起的传输不生效。
     */
    virtual void setTransferBlockSize(int bytes) = 0;
```

`src/panel/ZzSftpSessionOps.h/.cpp`：override 实现（QPointer 守卫，空则空操作）：

```cpp
void ZzSftpSessionOps::setTransferBlockSize(int bytes)
{
    if (m_session) {
        m_session->setTransferBlockSize(bytes);
    }
}
```

`tests/mocks/ZzMockSftpOps.h/.cpp`：override + 记录：

```cpp
    void setTransferBlockSize(int bytes) override { m_blockSizes.append(bytes); }
    [[nodiscard]] QList<int> recordedBlockSizes() const { return m_blockSizes; }
```

`src/panel/ZzSftpPanel.cpp`：找到会话创建点（grep `createSftpSession`/`ZzSftpSessionOps` 构造处），在 ops 创建后应用：

```cpp
    ops->setTransferBlockSize(ZzAppSettings::instance().sftpBlockSize());
```

并在面板构造处接 settingsChanged（对已打开会话的 ops 重应用；0=自动同样传递，恢复自适应）：

```cpp
    connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged, this, [this] {
        if (m_ops) {
            m_ops->setTransferBlockSize(ZzAppSettings::instance().sftpBlockSize());
        }
    });
```

（m_ops 成员名以 panel 现状为准；若 panel 随时跟随连接重建 ops，则创建点应用已覆盖大部分场景，settingsChanged 连接仍保留兜底。）

- [ ] **步骤 4：运行测试验证通过**

运行：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release -R "ZzAppSettings|ZzSettingsPage|ZzSftpPanel" --output-on-failure`
预期：PASS（含既有用例不回退）

- [ ] **步骤 5：全量回归 + 恢复 perf 记录**

运行：`ctest --preset linux-gcc-release`（45 基线 + 新增全绿）；`git checkout -- tests/perf/records/` 并按当天日期前缀删除新生成未跟踪 json。

- [ ] **步骤 6：Commit**

```bash
git add src/settings/ZzAppSettings.h src/settings/ZzAppSettings.cpp src/settings/ZzSettingsPage.h src/settings/ZzSettingsPage.cpp src/panel/ZzSftpOps.h src/panel/ZzSftpSessionOps.h src/panel/ZzSftpSessionOps.cpp src/panel/ZzSftpPanel.h src/panel/ZzSftpPanel.cpp tests/mocks/ZzMockSftpOps.h tests/mocks/ZzMockSftpOps.cpp tests/unit/tst_ZzAppSettings.cpp tests/unit/tst_ZzSettingsPage.cpp tests/unit/tst_ZzSftpPanel.cpp
git commit -m "feat(sftp): 设置页新增 SFTP 块大小（自动/手动）并接线到会话

- ZzAppSettings::sftpBlockSize（键 sftp/blockSize，默认 0=自动 BDP 自适应）
- 设置页 combo：自动/64K-4M，activated 即写；tooltip 说明语义
- ZzSftpOps 接口加 setTransferBlockSize（纯虚）：生产适配器透传
  ZzSftpSession，mock 记录调用供测试断言
- 面板会话创建时应用 + settingsChanged 兜底重应用；0=自动恢复自适应
- 下一传输生效，进行中传输不受影响（规格 §4.3）"
```

---

### 任务 7：ZzSshCore 推送 + 主仓 gitlink bump + README/规格核销

**前提：** 任务 1-6 全部完成；**推送需用户明确确认**。

- [ ] **步骤 1：推送 ZzSshCore（经用户确认）**

```bash
cd third_party/ZzSshCore
git push origin master   # remote 为 gitcode；先向用户说明并经确认
```

- [ ] **步骤 2：主仓 bump gitlink**

```bash
cd /home/zz/Jackfahdin/github/ZzClawTerm
git add third_party/ZzSshCore
```

- [ ] **步骤 3：ZzSshCore README 性能数据更新**

`third_party/ZzSshCore/README.md:56-61` 的性能数据段：在回环数据后追加 WAN 段（数据取自任务 4 入库记录 `tests/perf/records/*zzsftp-wan*.json` 的最新值，逐格引用真实数字）：

```markdown
WAN 高延迟（netem 注入，docker 实测）：
- RTT 25ms：上传 xxx MB/s（系统 sftp xxx，比值 x.xx）；下载 xxx MB/s（比值 x.xx）
- RTT 50ms：……
- RTT 100ms：……
（BDP 自适应调优后；调优前回环口径见上。调参逻辑见 docs/sftp-bdp-tuning.md）
```

- [ ] **步骤 4：M6 规格核销**

`docs/superpowers/specs/2026-08-25-sftp-bdp-tuning-design.md`：
- 状态行改"已实现（自动化部分全部核销，日期）；人工验收（V0.2 清单）挂起待用户时间"
- §六完成定义 1-3 打勾标注实际达成值（三档比值实测数值）；第 4 条挂起标注

- [ ] **步骤 5：主仓回归 + Commit + 推送（经用户确认）**

```bash
cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release
git checkout -- tests/perf/records/   # 并按当天日期前缀删除新生成未跟踪 json
git commit -m "feat(sftp): M6 收尾——ZzSshCore gitlink 升级 + WAN 性能数据 + 规格核销

- ZzSshCore 升至 <hash>：BDP 自适应调优（ZzSftpTuner/RTT 采样/调参接缝）
  + netem 三档门控 + docs/sftp-bdp-tuning.md 校准结论
- README 追加 WAN 三档实测：25ms 比值 x.xx / 50ms x.xx / 100ms x.xx（均 ≥0.95）
- M6 规格核销：完成定义 1-3 达成（实测值见内文），人工验收挂起"
git push origin master   # 经用户确认
```

---

## 自检记录（计划作者已完成）

- **规格覆盖度：** §2.1-1 任务 0 调研→任务 2 步骤 4/5；§2.1-2 RTT 测量→任务 3 步骤 3；§2.1-3 自适应→任务 1/3；§2.1-4 netem 门控→任务 2 步骤 3 + 任务 4；§2.1-5/6 设置项→任务 6；§2.1-7 验收清单→任务 5；§4.2 采样细节→任务 3；§4.3 手动覆盖语义→任务 3 步骤 1 + 任务 6；§4.4 错误处理→任务 3 步骤 3（采样失败不阻断）+ 任务 4（tc 失败 FAIL）；§五测试→各任务；§六完成定义→任务 7。无遗漏。
- **类型一致性：** `ZzSftpTuning{blockSize,readBufferCap,stagingCap}`、`ZzSftpTuner::tuningForRtt/noteRtt/currentTuning/parseTuneOverride`（任务 1 定义）→ 任务 2/3 使用一致；`ZzSftpEngine::setTuningCaps/smoothedRtt/tuningBlockSize`（任务 2/3）→ 任务 4 使用；`setTransferBlockSize`（任务 3 库侧语义）→ 任务 6 主仓接线一致（0=自动）；`ZzSftpOps::setTransferBlockSize`（任务 6）→ mock/panel 一致。
- **已知留白（执行时确认，非占位符）：** 任务 2 的档位表校准值（实验驱动，默认表已给）；任务 6 面板 m_ops 成员名与创建点精确行号（实现者 grep `createSftpSession`）；测试文件构造样板以各文件既有用例为准。
- **任务 2 与任务 3 的 env 覆盖语义边界已写明**（自适应生效后 tuner 覆盖 env；实验扫描走回环档不冲突）。
