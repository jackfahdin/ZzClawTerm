# SFTP 小文件批量传输优化 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 为 ZzSshCore 增加目录递归传输与有界并发调度，小文件场景（2000×20KB）vs OpenSSH `sftp -r` 回环比值 ≥1.5、WAN 50ms 比值 ≥2。

**架构：** 纯逻辑遍历器 `ZzSftpDirWalker` 产任务计划；纯逻辑调度器 `ZzSftpBatchScheduler` 维持 N 在飞；`ZzSftpEngine` 仅加入口与批收尾接线，单文件路径零改动；链路透传到主仓面板两个最小入口。

**技术栈：** C++20、Qt 6.8+（QtTest）、libssh2、CMake；ZzSshCore 仓库（gitcode submodule）+ 主仓接线。

**规格：** `docs/superpowers/specs/2026-08-25-sftp-smallfile-batch-design.md`（已批准）

**仓库与回归基线：**
- ZzSshCore：`cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit`（基线 16 程序全绿）；docker 集成 `tests/integration/docker/run-integration-tests.sh build/linux-release`（基线 17 项；perf 门控偶发噪声红需定向复跑甄别）。
- ZzSshCore perf records 是**入库滚动基线**（prune 留 5 份随 commit 提交），与主仓规矩相反，勿删勿 checkout。
- 主仓：`cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release`（基线 45）；跑完必须 `git checkout -- tests/perf/records/` 并按当天日期前缀删未跟踪新记录（禁用月份通配）。
- commit：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详述；**不 push**（push 需用户逐项确认）。库改动先提库，主仓最后 bump gitlink。

---

## 文件结构

**ZzSshCore（`third_party/ZzSshCore/`）：**

| 文件 | 职责 |
| --- | --- |
| 创建 `src/ZzSftpDirWalker.h/.cpp` | 纯逻辑：本地树遍历产计划；远端 listing 增量装配计划 |
| 创建 `src/ZzSftpBatchScheduler.h/.cpp` | 纯逻辑：任务队列 + 有界并发 + 进度/错误聚合 |
| 修改 `src/ZzSftpEngine.h/.cpp` | 批入口 startDirUpload/startDirDownload、Transfer 批标记、批收尾接线 |
| 修改 `src/ZzSftpSession.h/.cpp` | 透传 uploadDir/downloadDir |
| 修改 `src/ZzSshConnectionWorker.h/.cpp` | doSftpUploadDir/doSftpDownloadDir |
| 创建 `tests/unit/tst_ZzSftpDirWalker.cpp` | walker 单测 |
| 创建 `tests/unit/tst_ZzSftpBatchScheduler.cpp` | scheduler 单测 |
| 修改 `tests/integration/tst_ZzSftpIT.cpp` | 目录递归 docker 集成用例 |
| 创建 `tests/perf/tst_ZzSftpSmallFilesPerf.cpp` | 小文件比值门控 |
| 修改 `tests/CMakeLists.txt` | 注册三个新测试 |
| 修改 `README.md` | 小文件场景能力一句话（数据待任务 5 实测后填） |

**主仓：**

| 文件 | 职责 |
| --- | --- |
| 修改 `src/panel/ZzSftpOps.h` | 抽象接口加 uploadDir/downloadDir |
| 修改 `src/panel/ZzSftpSessionOps.h/.cpp` | 生产适配器透传 |
| 修改 `tests/mocks/ZzMockSftpOps.h/.cpp` | mock 记录 uploadedDirs/downloadedDirs |
| 修改 `src/panel/ZzSftpPanel.h/.cpp` | 「上传文件夹」按钮 + 目录右键「下载」+ 可测试公开方法 |
| 修改 `tests/unit/tst_ZzSftpPanel.cpp` | 面板新入口用例 |
| 修改 `third_party/ZzSshCore`（gitlink） | bump 到库侧最终提交 |

---

### 任务 1：ZzSftpDirWalker（纯逻辑遍历器）

**文件：**
- 创建：`third_party/ZzSshCore/src/ZzSftpDirWalker.h`
- 创建：`third_party/ZzSshCore/src/ZzSftpDirWalker.cpp`
- 测试：`third_party/ZzSshCore/tests/unit/tst_ZzSftpDirWalker.cpp`
- 修改：`third_party/ZzSshCore/tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSftpDirWalker.cpp`（在 ZzSshCore 仓库内）：

```cpp
#include <QtTest>
#include <QTemporaryDir>
#include "ZzSftpDirWalker.h"

class tst_ZzSftpDirWalker : public QObject
{
    Q_OBJECT
private slots:
    void walkLocalBuildsDepthSortedPlan();
    void walkLocalSkipsEmptyDirsAndCountsBytes();
    void walkLocalRejectsMissingDir();
    void appendRemoteListingQueuesSubdirs();
};

/** @brief 造一个文件并写入指定字节数（内容重复填充）。 */
static bool makeFile(const QString &path, qint64 bytes)
{
    QFile f(path);
    if (!f.open(QIODevice::WriteOnly))
        return false;
    QByteArray chunk(4096, 'x');
    while (bytes > 0) {
        const qint64 n = qMin<qint64>(bytes, chunk.size());
        if (f.write(chunk.constData(), n) != n)
            return false;
        bytes -= n;
    }
    return true;
}

void tst_ZzSftpDirWalker::walkLocalBuildsDepthSortedPlan()
{
    QTemporaryDir tmp;
    QVERIFY(tmp.isValid());
    const QString root = tmp.filePath(QStringLiteral("src"));
    QVERIFY(QDir().mkpath(root + QStringLiteral("/a/b")));
    QVERIFY(QDir().mkpath(root + QStringLiteral("/c")));
    QVERIFY(makeFile(root + QStringLiteral("/top.txt"), 100));
    QVERIFY(makeFile(root + QStringLiteral("/a/mid.txt"), 200));
    QVERIFY(makeFile(root + QStringLiteral("/a/b/leaf.txt"), 300));

    ZzSftpDirPlan plan;
    QString err;
    QVERIFY(ZzSftpDirWalker::walkLocal(root, QStringLiteral("/remote/dst"), &plan, &err));

    // 目录按深度升序：/remote/dst/a 与 /remote/dst/c 先于 /remote/dst/a/b
    QCOMPARE(plan.dirs.size(), 3);
    const int idxA = plan.dirs.indexOf(QStringLiteral("/remote/dst/a"));
    const int idxB = plan.dirs.indexOf(QStringLiteral("/remote/dst/a/b"));
    QVERIFY(idxA >= 0 && idxB >= 0 && idxA < idxB);
    QVERIFY(plan.dirs.contains(QStringLiteral("/remote/dst/c")));

    QCOMPARE(plan.files.size(), 3);
    QCOMPARE(plan.totalBytes, 600);
    // 文件任务路径成对（本地 → 远端）
    bool foundLeaf = false;
    for (const ZzSftpDirTask &t : plan.files) {
        if (t.remotePath == QStringLiteral("/remote/dst/a/b/leaf.txt")) {
            foundLeaf = true;
            QCOMPARE(t.size, 300);
            QVERIFY(t.localPath.endsWith(QStringLiteral("/a/b/leaf.txt")));
        }
    }
    QVERIFY(foundLeaf);
}

void tst_ZzSftpDirWalker::walkLocalSkipsEmptyDirsAndCountsBytes()
{
    QTemporaryDir tmp;
    QVERIFY(tmp.isValid());
    const QString root = tmp.filePath(QStringLiteral("src"));
    QVERIFY(QDir().mkpath(root + QStringLiteral("/empty")));
    QVERIFY(makeFile(root + QStringLiteral("/f.bin"), 42));

    ZzSftpDirPlan plan;
    QString err;
    QVERIFY(ZzSftpDirWalker::walkLocal(root, QStringLiteral("/r"), &plan, &err));
    // 空目录也要建（目录结构保真）
    QVERIFY(plan.dirs.contains(QStringLiteral("/r/empty")));
    QCOMPARE(plan.files.size(), 1);
    QCOMPARE(plan.totalBytes, 42);
}

void tst_ZzSftpDirWalker::walkLocalRejectsMissingDir()
{
    ZzSftpDirPlan plan;
    QString err;
    QVERIFY(!ZzSftpDirWalker::walkLocal(QStringLiteral("/nonexistent/zz-dir"),
                                        QStringLiteral("/r"), &plan, &err));
    QVERIFY(!err.isEmpty());
}

void tst_ZzSftpDirWalker::appendRemoteListingQueuesSubdirs()
{
    // 模拟 /remote/src 的一次 listDir 结果：一个子目录 + 一个文件 + 一个符号链接
    ZzSftpFileInfo sub;
    sub.name = QStringLiteral("sub");
    sub.permissions = LIBSSH2_SFTP_S_IFDIR | 0755;
    ZzSftpFileInfo file;
    file.name = QStringLiteral("f.txt");
    file.permissions = LIBSSH2_SFTP_S_IFREG | 0644;
    file.size = 123;
    ZzSftpFileInfo link;
    link.name = QStringLiteral("lnk");
    link.permissions = LIBSSH2_SFTP_S_IFLNK | 0777;

    QStringList pendingDirs;
    ZzSftpDirPlan plan;
    ZzSftpDirWalker::appendRemoteListing(QStringLiteral("/remote/src"),
                                         QStringLiteral("/local/dst"),
                                         {sub, file, link}, &pendingDirs, &plan);

    // 子目录：本地 mkdir 目标入 plan.dirs，远端路径入待遍历队列
    QCOMPARE(pendingDirs, QStringList{QStringLiteral("/remote/src/sub")});
    QVERIFY(plan.dirs.contains(QStringLiteral("/local/dst/sub")));
    // 常规文件入任务；符号链接跳过（V1 不跟随）
    QCOMPARE(plan.files.size(), 1);
    QCOMPARE(plan.files.first().remotePath, QStringLiteral("/remote/src/f.txt"));
    QCOMPARE(plan.files.first().localPath, QStringLiteral("/local/dst/f.txt"));
    QCOMPARE(plan.totalBytes, 123);
}

QTEST_GUILESS_MAIN(tst_ZzSftpDirWalker)
#include "tst_ZzSftpDirWalker.moc"
```

- [ ] **步骤 2：运行测试验证失败**

在 `third_party/ZzSshCore/` 内运行：

```bash
cmake --build --preset linux-release --target tst_ZzSftpDirWalker 2>&1 | tail -5
```

预期：编译失败，`ZzSftpDirWalker.h: No such file or directory`（先完成步骤 3 的 CMake 注册再运行；或先注册 CMake 再跑，报错为头文件缺失）。

- [ ] **步骤 3：编写实现**

创建 `src/ZzSftpDirWalker.h`：

```cpp
#pragma once

#include <QList>
#include <QString>
#include <QStringList>

#include "ZzSftpTypes.h"

/**
 * @brief 单个文件传输任务（目录递归批的子项）。
 */
struct ZzSftpDirTask {
    QString localPath;  ///< 本地路径（上传=源，下载=目标）
    QString remotePath; ///< 远端路径（上传=目标，下载=源）
    qint64 size = 0;    ///< 文件字节数（批总进度基数）
};

/**
 * @brief 目录递归传输计划：待建目录（深度升序）+ 文件任务 + 批总字节。
 */
struct ZzSftpDirPlan {
    QStringList dirs;              ///< 待创建目录（上传=远端路径；下载=本地路径），父先子后
    QList<ZzSftpDirTask> files;    ///< 文件任务列表
    qint64 totalBytes = 0;         ///< 批总字节（files.size 求和）
};

/**
 * @brief 目录树遍历器（纯逻辑，不依赖网络）。
 *
 * 上传：walkLocal 一次性遍历本地树产完整计划。
 * 下载：远端树只能逐层 listDir，appendRemoteListing 把每层结果增量装配进计划，
 *       子目录远端路径经 pendingDirs 队列返回（广度优先，天然父先子后）。
 * 符号链接一律跳过（V1 不跟随，避免环与越界写入）。
 */
class ZzSftpDirWalker
{
public:
    /**
     * @brief 遍历本地目录树，生成上传计划。
     * @param localDir 本地源目录（须存在且为目录）。
     * @param remoteDir 远端目标目录（计划内路径以其为前缀，不含尾部 "/"）。
     * @param out 输出计划。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    static bool walkLocal(const QString &localDir, const QString &remoteDir,
                          ZzSftpDirPlan *out, QString *errorString);

    /**
     * @brief 把一层远端目录列举结果装配进下载计划。
     * @param remoteDir 本次列举的远端目录。
     * @param localDir 对应的本地目录。
     * @param entries listDir 返回的条目（已过滤 "." 与 ".."）。
     * @param pendingDirs 输出：需继续列举的子目录远端路径（追加到队列尾）。
     * @param out 累积中的计划（dirs 追加本地子目录路径，files 追加文件任务）。
     */
    static void appendRemoteListing(const QString &remoteDir, const QString &localDir,
                                    const QList<ZzSftpFileInfo> &entries,
                                    QStringList *pendingDirs, ZzSftpDirPlan *out);
};
```

创建 `src/ZzSftpDirWalker.cpp`：

```cpp
#include "ZzSftpDirWalker.h"

#include <QDir>
#include <QDirIterator>
#include <QFileInfo>

namespace {

/** @brief 拼接远端路径（统一 "/" 分隔，基路径去尾斜杠）。 */
QString joinRemote(const QString &base, const QString &name)
{
    return base.endsWith(QLatin1Char('/')) ? base + name : base + QLatin1Char('/') + name;
}

} // namespace

bool ZzSftpDirWalker::walkLocal(const QString &localDir, const QString &remoteDir,
                                ZzSftpDirPlan *out, QString *errorString)
{
    const QFileInfo rootInfo(localDir);
    if (!rootInfo.isDir()) {
        if (errorString)
            *errorString = QStringLiteral("本地目录不存在或不是目录：%1").arg(localDir);
        return false;
    }

    const QString rootAbs = rootInfo.absoluteFilePath();
    QList<QPair<int, QString>> dirsByDepth; // (深度, 远端路径)，排序保证父先子后
    QList<ZzSftpDirTask> files;
    qint64 total = 0;

    QDirIterator it(rootAbs, QDir::Dirs | QDir::Files | QDir::NoDotAndDotDot | QDir::NoSymlinks,
                    QDirIterator::Subdirectories);
    while (it.hasNext()) {
        const QString abs = it.next();
        const QString rel = QDir(rootAbs).relativeFilePath(abs);
        const QString remotePath = joinRemote(remoteDir, QString(rel).replace(QLatin1Char('\\'), QLatin1Char('/')));
        const QFileInfo fi = it.fileInfo();
        if (fi.isDir()) {
            dirsByDepth.append({rel.count(QLatin1Char('/')), remotePath});
        } else if (fi.isFile()) {
            ZzSftpDirTask task;
            task.localPath = abs;
            task.remotePath = remotePath;
            task.size = fi.size();
            files.append(task);
            total += task.size;
        }
        // 其余类型（套接字/设备等）跳过
    }

    std::sort(dirsByDepth.begin(), dirsByDepth.end(),
              [](const auto &a, const auto &b) { return a.first < b.first; });

    if (out) {
        out->dirs.clear();
        for (const auto &d : dirsByDepth)
            out->dirs.append(d.second);
        out->files = files;
        out->totalBytes = total;
    }
    return true;
}

void ZzSftpDirWalker::appendRemoteListing(const QString &remoteDir, const QString &localDir,
                                          const QList<ZzSftpFileInfo> &entries,
                                          QStringList *pendingDirs, ZzSftpDirPlan *out)
{
    for (const ZzSftpFileInfo &e : entries) {
        if (e.isSymlink())
            continue; // V1 不跟随符号链接
        if (e.isDir()) {
            const QString childRemote = joinRemote(remoteDir, e.name);
            out->dirs.append(QDir(localDir).filePath(e.name));
            if (pendingDirs)
                pendingDirs->append(childRemote);
        } else {
            ZzSftpDirTask task;
            task.remotePath = joinRemote(remoteDir, e.name);
            task.localPath = QDir(localDir).filePath(e.name);
            task.size = qMax<qint64>(e.size, 0);
            out->files.append(task);
            out->totalBytes += task.size;
        }
    }
}
```

在 `tests/CMakeLists.txt` 中（紧跟 `tst_ZzSftpTypes` 注册块之后）追加：

```cmake
zz_add_test(tst_ZzSftpDirWalker unit/tst_ZzSftpDirWalker.cpp)
set_tests_properties(tst_ZzSftpDirWalker PROPERTIES LABELS "unit")
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd third_party/ZzSshCore
cmake --build --preset linux-release --target tst_ZzSftpDirWalker && \
  ctest --test-dir build/linux-release -R tst_ZzSftpDirWalker --output-on-failure
```

预期：`1/1 Test ... Passed`（4 个用例全过）。

- [ ] **步骤 5：回归 + Commit**

```bash
cd third_party/ZzSshCore
cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit
git add src/ZzSftpDirWalker.h src/ZzSftpDirWalker.cpp tests/unit/tst_ZzSftpDirWalker.cpp tests/CMakeLists.txt
git commit -m "feat(sftp): 新增 ZzSftpDirWalker 目录树遍历器（纯逻辑）

目录递归批传输的计划层：
- walkLocal：本地上传树一次性遍历，目录按深度升序（父先子后），
  统计批总字节；符号链接与非常规文件跳过
- appendRemoteListing：远端逐层 listDir 结果增量装配下载计划，
  子目录经 pendingDirs 广度优先入队
- 单测 4 例：深度序/空目录保真/缺失目录报错/远端装配与链接跳过"
```

预期：单测 17 程序（16 旧 + 1 新）全绿。

---

### 任务 2：ZzSftpBatchScheduler（纯逻辑调度器）

**文件：**
- 创建：`third_party/ZzSshCore/src/ZzSftpBatchScheduler.h`
- 创建：`third_party/ZzSshCore/src/ZzSftpBatchScheduler.cpp`
- 测试：`third_party/ZzSshCore/tests/unit/tst_ZzSftpBatchScheduler.cpp`
- 修改：`third_party/ZzSshCore/tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试**

创建 `tests/unit/tst_ZzSftpBatchScheduler.cpp`：

```cpp
#include <QtTest>
#include "ZzSftpBatchScheduler.h"

class tst_ZzSftpBatchScheduler : public QObject
{
    Q_OBJECT
private slots:
    void kickCapsInFlight();
    void finishFeedsNext();
    void failureAggregatesAndContinues();
    void cancelStopsFeeding();
};

/** @brief 造 N 个 20KB 文件任务的计划。 */
static ZzSftpDirPlan makePlan(int fileCount)
{
    ZzSftpDirPlan plan;
    for (int i = 0; i < fileCount; ++i) {
        ZzSftpDirTask t;
        t.localPath = QStringLiteral("/local/f%1").arg(i);
        t.remotePath = QStringLiteral("/remote/f%1").arg(i);
        t.size = 20 * 1024;
        plan.files.append(t);
        plan.totalBytes += t.size;
    }
    return plan;
}

void tst_ZzSftpBatchScheduler::kickCapsInFlight()
{
    ZzSftpBatchScheduler sched(makePlan(20), 4);
    QStringList started;
    sched.setHooks({[&started](const ZzSftpDirTask &t) {
        started.append(t.remotePath);
        return true;
    }});

    QCOMPARE(sched.kick(), 4);          // 补投到上限
    QCOMPARE(sched.inFlight(), 4);
    QCOMPARE(sched.kick(), 0);          // 已满，不再投
    QCOMPARE(started.size(), 4);
    QVERIFY(!sched.isDone());
}

void tst_ZzSftpBatchScheduler::finishFeedsNext()
{
    ZzSftpBatchScheduler sched(makePlan(3), 2);
    QStringList started;
    sched.setHooks({[&started](const ZzSftpDirTask &t) {
        started.append(t.remotePath);
        return true;
    }});

    sched.kick();
    QCOMPARE(started, QStringList({QStringLiteral("/remote/f0"), QStringLiteral("/remote/f1")}));

    sched.onFileFinished(QStringLiteral("/remote/f0"), 20 * 1024);
    sched.kick();                        // 补投 f2
    QCOMPARE(started.size(), 3);
    QCOMPARE(sched.inFlight(), 2);
    QCOMPARE(sched.doneBytes(), 20 * 1024);

    sched.onFileFinished(QStringLiteral("/remote/f1"), 20 * 1024);
    sched.onFileFinished(QStringLiteral("/remote/f2"), 20 * 1024);
    QVERIFY(sched.isDone());
    QCOMPARE(sched.doneBytes(), 60 * 1024);
    QCOMPARE(sched.failedCount(), 0);
}

void tst_ZzSftpBatchScheduler::failureAggregatesAndContinues()
{
    ZzSftpBatchScheduler sched(makePlan(3), 2);
    sched.setHooks({[](const ZzSftpDirTask &) { return true; }});
    sched.kick();

    sched.onFileFailed(QStringLiteral("/remote/f0"), 42, QStringLiteral("权限不足"));
    sched.kick();                        // 失败也补投，批次继续
    QCOMPARE(sched.failedCount(), 1);
    QCOMPARE(sched.firstErrorCode(), 42);

    sched.onFileFinished(QStringLiteral("/remote/f1"), 20 * 1024);
    sched.onFileFailed(QStringLiteral("/remote/f2"), 43, QStringLiteral("磁盘满"));
    QVERIFY(sched.isDone());
    QCOMPARE(sched.failedCount(), 2);
    QCOMPARE(sched.firstErrorCode(), 42); // 首错码不变
    QVERIFY(sched.summary().contains(QStringLiteral("2")));
    QVERIFY(sched.summary().contains(QStringLiteral("权限不足")));
}

void tst_ZzSftpBatchScheduler::cancelStopsFeeding()
{
    ZzSftpBatchScheduler sched(makePlan(10), 2);
    sched.setHooks({[](const ZzSftpDirTask &) { return true; }});
    sched.kick();
    QCOMPARE(sched.inFlight(), 2);

    sched.cancel();
    QVERIFY(sched.cancelled());
    QCOMPARE(sched.kick(), 0);           // 取消后不再补投
    QVERIFY(!sched.isDone());            // 在飞未清，未结束

    sched.onFileFinished(QStringLiteral("/remote/f0"), 20 * 1024);
    sched.onFileFinished(QStringLiteral("/remote/f1"), 20 * 1024);
    QVERIFY(sched.isDone());             // 在飞清零即结束（待投队列已弃）
}

QTEST_GUILESS_MAIN(tst_ZzSftpBatchScheduler)
#include "tst_ZzSftpBatchScheduler.moc"
```

- [ ] **步骤 2：运行测试验证失败**

先注册 CMake（同任务 1 模式，追加 `tst_ZzSftpBatchScheduler`），再：

```bash
cd third_party/ZzSshCore
cmake --build --preset linux-release --target tst_ZzSftpBatchScheduler 2>&1 | tail -5
```

预期：编译失败，头文件缺失。

- [ ] **步骤 3：编写实现**

创建 `src/ZzSftpBatchScheduler.h`：

```cpp
#pragma once

#include <functional>

#include "ZzSftpDirWalker.h"

/**
 * @brief 目录递归批传输调度器（纯逻辑，worker 线程内使用）。
 *
 * 持有文件任务队列，kick() 补投至在飞上限；文件完成/失败经 onFile* 录入，
 * 由调用方（engine）随后再调 kick() 补投。单文件失败记入错误列表继续批次；
 * cancel() 放弃待投队列，在飞传输自然收尾后 isDone()。
 * @note 非线程安全；全部方法须在同一线程调用。
 */
class ZzSftpBatchScheduler
{
public:
    /** @brief 外部动作注入：发起一个文件任务（false=发起失败，记入错误列表）。 */
    struct Hooks {
        std::function<bool(const ZzSftpDirTask &task)> startFile;
    };

    /**
     * @brief 构造调度器。
     * @param plan 传输计划（files 被移入内部队列；dirs 由 engine 先行处理）。
     * @param maxConcurrent 在飞上限（夹取 [1, 32]）。
     */
    explicit ZzSftpBatchScheduler(ZzSftpDirPlan plan, int maxConcurrent = 8);

    void setHooks(Hooks hooks) { m_hooks = std::move(hooks); }

    /**
     * @brief 补投任务至在飞上限。
     * @return 本次新发起的任务数（已取消或已满时为 0）。
     */
    int kick();

    /** @brief 录入一个文件完成（bytes 计入批进度）。 */
    void onFileFinished(const QString &remotePath, qint64 bytes);
    /** @brief 录入一个文件失败（记入错误列表，批次继续）。 */
    void onFileFailed(const QString &remotePath, int code, const QString &message);

    /** @brief 取消：放弃全部待投任务（在飞的自然收尾）。 */
    void cancel();

    /** @brief 批是否结束：在飞为 0 且（待投空或已取消）。 */
    [[nodiscard]] bool isDone() const;
    [[nodiscard]] int inFlight() const { return m_inFlight; }
    [[nodiscard]] qint64 doneBytes() const { return m_doneBytes; }
    [[nodiscard]] qint64 totalBytes() const { return m_totalBytes; }
    [[nodiscard]] int failedCount() const { return m_failed; }
    [[nodiscard]] int firstErrorCode() const { return m_firstErrorCode; }
    [[nodiscard]] bool cancelled() const { return m_cancelled; }

    /**
     * @brief 批错误汇总文案："N 个文件失败，首个错误：<首错描述>（code <码>）"。
     *        无失败时返回空串。
     */
    [[nodiscard]] QString summary() const;

private:
    Hooks m_hooks;
    QList<ZzSftpDirTask> m_pending;   ///< 待投队列（头部弹出）
    int m_maxConcurrent;
    int m_inFlight = 0;
    qint64 m_doneBytes = 0;
    qint64 m_totalBytes = 0;
    int m_failed = 0;
    int m_firstErrorCode = 0;
    QString m_firstErrorMessage;
    bool m_cancelled = false;
};
```

创建 `src/ZzSftpBatchScheduler.cpp`：

```cpp
#include "ZzSftpBatchScheduler.h"

ZzSftpBatchScheduler::ZzSftpBatchScheduler(ZzSftpDirPlan plan, int maxConcurrent)
    : m_pending(plan.files)
    , m_maxConcurrent(qBound(1, maxConcurrent, 32))
    , m_totalBytes(plan.totalBytes)
{
}

int ZzSftpBatchScheduler::kick()
{
    if (m_cancelled || !m_hooks.startFile)
        return 0;
    int started = 0;
    while (m_inFlight < m_maxConcurrent && !m_pending.isEmpty()) {
        const ZzSftpDirTask task = m_pending.takeFirst();
        if (m_hooks.startFile(task)) {
            ++m_inFlight;
            ++started;
        } else {
            // 发起失败视同文件失败：记入错误列表，继续尝试后续任务
            onFileFailed(task.remotePath, 0, QStringLiteral("传输发起失败"));
        }
    }
    return started;
}

void ZzSftpBatchScheduler::onFileFinished(const QString &remotePath, qint64 bytes)
{
    Q_UNUSED(remotePath);
    if (m_inFlight > 0)
        --m_inFlight;
    m_doneBytes += bytes;
}

void ZzSftpBatchScheduler::onFileFailed(const QString &remotePath, int code, const QString &message)
{
    Q_UNUSED(remotePath);
    if (m_inFlight > 0)
        --m_inFlight;
    ++m_failed;
    if (m_firstErrorMessage.isEmpty()) {
        m_firstErrorCode = code;
        m_firstErrorMessage = message;
    }
}

void ZzSftpBatchScheduler::cancel()
{
    m_cancelled = true;
    m_pending.clear();
}

bool ZzSftpBatchScheduler::isDone() const
{
    return m_inFlight == 0 && (m_pending.isEmpty() || m_cancelled);
}

QString ZzSftpBatchScheduler::summary() const
{
    if (m_failed == 0)
        return QString();
    return QStringLiteral("%1 个文件失败，首个错误：%2（code %3）")
        .arg(m_failed)
        .arg(m_firstErrorMessage)
        .arg(m_firstErrorCode);
}
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd third_party/ZzSshCore
cmake --build --preset linux-release --target tst_ZzSftpBatchScheduler && \
  ctest --test-dir build/linux-release -R tst_ZzSftpBatchScheduler --output-on-failure
```

预期：4 用例全过。

- [ ] **步骤 5：回归 + Commit**

```bash
cd third_party/ZzSshCore
cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit
git add src/ZzSftpBatchScheduler.h src/ZzSftpBatchScheduler.cpp tests/unit/tst_ZzSftpBatchScheduler.cpp tests/CMakeLists.txt
git commit -m "feat(sftp): 新增 ZzSftpBatchScheduler 有界并发调度器（纯逻辑）

批传输调度核心：
- kick() 补投至在飞上限（默认 8，夹取 [1,32]），完成/失败均可触发补投
- 单文件失败记入错误列表批次继续；首错码与汇总文案供批终态上报
- cancel() 放弃待投队列，在飞自然收尾后 isDone()
- 发起失败（Hooks 返回 false）视同文件失败，不阻断批次
- 单测 4 例：在飞上限/完成补投/失败聚合续传/取消停投"
```

预期：单测 18 程序全绿。

---

### 任务 3：Engine 批集成 + 全链路透传

**文件：**
- 修改：`third_party/ZzSshCore/src/ZzSftpEngine.h`
- 修改：`third_party/ZzSshCore/src/ZzSftpEngine.cpp`
- 修改：`third_party/ZzSshCore/src/ZzSftpSession.h/.cpp`
- 修改：`third_party/ZzSshCore/src/ZzSshConnectionWorker.h/.cpp`

本任务无可隔离单测（engine 依赖真实 SFTP），验证 = 编译 + 既有单测/集成不回归；行为由任务 4 的 docker 集成用例证明。

- [ ] **步骤 1：Engine 改造**

`ZzSftpEngine.h` 修改点：

1. 头部 include 两个新头文件；`Transfer` 结构体加成员：

```cpp
        quint64 batchId = 0;        ///< 所属批的 requestId（0=非批传输，走原有回调路径）
        qint64 batchFileSize = 0;   ///< 批子文件大小（完成时计入批进度）
```

2. 公开接口加（Doxygen 注释遵循现有风格）：

```cpp
    /**
     * @brief 发起目录递归上传（异步）：本地目录树 → 远端目录。
     *
     * 两阶段：先按深度序同步创建远端目录树（已存在则跳过），再有界并发
     * （默认 8）传输文件。批进度经 ProgressFn 聚合上报（done=批已确认字节，
     * total=批总字节）；终态：全部成功走 FinishFn，有失败走 ErrorFn
     * （首错码 + 汇总描述）。取消走 cancelTransfer(requestId)。
     * @param requestId 批请求 ID（事件回调携带；批内子传输共享此 ID）。
     * @param localDir 本地源目录。
     * @param remoteDir 远端目标目录（须不存在或为空目录之外的已建目录亦可）。
     * @param errorString 发起失败（遍历失败等）时输出描述。
     * @return 发起成功返回 true。
     */
    bool startDirUpload(quint64 requestId, const QString &localDir, const QString &remoteDir,
                        QString *errorString);

    /**
     * @brief 发起目录递归下载（异步）：远端目录树 → 本地目录。
     *
     * 逐层 listDir 装配计划（本地目录经 QDir::mkpath 即建），随后有界并发下载。
     * 进度/终态/取消语义同 startDirUpload。
     */
    bool startDirDownload(quint64 requestId, const QString &remoteDir, const QString &localDir,
                          QString *errorString);
```

3. 私有区加：

```cpp
    /** @brief 一个进行中的目录批。 */
    struct Batch {
        quint64 requestId = 0;
        bool isUpload = true;
        bool walking = false;           ///< 下载：仍在逐层 listDir 装配计划
        QStringList pendingWalkDirs;    ///< 下载：待列举的远端目录队列
        QString localRoot;              ///< 下载：本地根（walk 时装配用）
        ZzSftpDirPlan walkPlan;         ///< 下载：walk 期累积的计划
        std::unique_ptr<ZzSftpBatchScheduler> sched;  ///< 进入传输阶段后非空
        bool outcomeEmitted = false;
        QElapsedTimer progressTimer;    ///< 批进度节流
        qint64 lastReported = 0;
    };

    /** @brief 推进全部批（walk/补投/终态判定）；pumpTransfers 末尾调用。 */
    void pumpBatches();
    /** @brief 批子传输收尾登记（Done/Failed 时调用，替代 emitOutcome）。 */
    void noteBatchFileOutcome(Batch &b, const Transfer &t);
    /** @brief 批终态发射并移除（成功补发最终进度；失败带汇总）。 */
    void finalizeBatch(size_t index);

    std::vector<std::unique_ptr<Batch>> m_batches;
    int m_batchConcurrency = 8;         ///< 批在飞上限（构造后可调，测试用）
```

`ZzSftpEngine.cpp` 修改点：

1. `startDirUpload` 实现：

```cpp
bool ZzSftpEngine::startDirUpload(quint64 requestId, const QString &localDir,
                                  const QString &remoteDir, QString *errorString)
{
    ZzSftpDirPlan plan;
    if (!ZzSftpDirWalker::walkLocal(localDir, remoteDir, &plan, errorString))
        return false;

    // 阶段一：按深度序创建远端目录树；已存在（stat 确认是目录）跳过
    for (const QString &dir : plan.dirs) {
        QString err;
        if (makeDir(dir, 0755, &err)) {
            continue;
        }
        ZzSftpFileInfo info;
        if (!stat(dir, false, &info, nullptr) || !info.isDir()) {
            if (errorString)
                *errorString = err;
            return false;
        }
    }

    auto b = std::make_unique<Batch>();
    b->requestId = requestId;
    b->isUpload = true;
    b->sched = std::make_unique<ZzSftpBatchScheduler>(plan, m_batchConcurrency);
    Batch *bp = b.get();
    b->sched->setHooks({[this, bp](const ZzSftpDirTask &task) {
        QString err;
        // 批子传输：requestId 复用批 ID，batchId 标记走批收尾路径；块大小 0=自动
        if (!startUpload(bp->requestId, task.localPath, task.remotePath, 0, &err))
            return false;
        m_transfers.back()->batchId = bp->requestId;
        m_transfers.back()->batchFileSize = task.size;
        return true;
    }});
    b->progressTimer.start();
    m_batches.push_back(std::move(b));
    m_batches.back()->sched->kick();
    return true;
}
```

2. `startDirDownload` 实现（walk 启动 + pumpBatches 续走）：

```cpp
bool ZzSftpEngine::startDirDownload(quint64 requestId, const QString &remoteDir,
                                    const QString &localDir, QString *errorString)
{
    if (!QDir().mkpath(localDir)) {
        if (errorString)
            *errorString = QStringLiteral("无法创建本地目录 %1").arg(localDir);
        return false;
    }
    auto b = std::make_unique<Batch>();
    b->requestId = requestId;
    b->isUpload = false;
    b->walking = true;
    b->pendingWalkDirs.append(remoteDir);
    b->localRoot = localDir;
    b->progressTimer.start();
    m_batches.push_back(std::move(b));
    return true;
}
```

`pumpBatches()` 核心逻辑：

```cpp
void ZzSftpEngine::pumpBatches()
{
    for (size_t i = 0; i < m_batches.size();) {
        Batch &b = *m_batches[i];

        // 下载 walk 阶段：每泵周期处理一层（listDir 同步带 WaitFn，单层很快）
        if (b.walking) {
            if (!b.pendingWalkDirs.isEmpty()) {
                const QString remoteDir = b.pendingWalkDirs.takeFirst();
                const QString localDir = b.walkPlan.dirs.isEmpty()
                    ? b.localRoot : b.localRoot; // walkPlan.dirs 仅装子目录
                QList<ZzSftpFileInfo> entries;
                QString err;
                if (!listDir(remoteDir, &entries, &err)) {
                    // 遍历失败：批即失败
                    if (m_errorFn)
                        m_errorFn(b.requestId, static_cast<int>(ZzSshErrorCode::SftpOperationFailed),
                                  err);
                    b.outcomeEmitted = true;
                    m_batches.erase(m_batches.begin() + static_cast<std::ptrdiff_t>(i));
                    continue;
                }
                const QString rel = /* remoteDir 相对批根的路径 */;
                const QString localBase = rel.isEmpty() ? b.localRoot
                    : b.localRoot + QLatin1Char('/') + rel;
                ZzSftpDirWalker::appendRemoteListing(remoteDir, localBase, entries,
                                                     &b.pendingWalkDirs, &b.walkPlan);
                ++i; // 每周期一层，避免深树阻塞泵
                continue;
            }
            // walk 完成：建本地目录树 + 进入传输阶段
            for (const QString &d : b.walkPlan.dirs)
                QDir().mkpath(d);
            b.walking = false;
            b.sched = std::make_unique<ZzSftpBatchScheduler>(b.walkPlan, m_batchConcurrency);
            Batch *bp = &b;
            b.sched->setHooks({[this, bp](const ZzSftpDirTask &task) {
                QString err;
                if (!startDownload(bp->requestId, task.remotePath, task.localPath, 0, &err))
                    return false;
                m_transfers.back()->batchId = bp->requestId;
                m_transfers.back()->batchFileSize = task.size;
                return true;
            }});
            b.sched->kick();
        }

        // 传输阶段：补投 + 批进度节流上报 + 终态判定
        if (b.sched) {
            b.sched->kick();
            const qint64 done = b.sched->doneBytes();
            if (m_progressFn
                && (done - b.lastReported >= 1024 * 1024 || b.progressTimer.elapsed() >= 100)) {
                b.lastReported = done;
                b.progressTimer.restart();
                m_progressFn(b.requestId, done, b.sched->totalBytes());
            }
            if (b.sched->isDone()) {
                finalizeBatch(i);
                continue; // 已移除，不递增
            }
        }
        ++i;
    }
}
```

注意：上面 `rel` 的计算——实现时在 Batch 加 `QString remoteRoot;`，`rel = remoteDir.mid(remoteRoot.size()).trimmed()` 去掉前导 "/"，walker 的 localBase 由此拼出（实现者按此语义落地，单测已锁定 walker 行为，walk 装配正确性由任务 4 集成测试门控）。

3. `noteBatchFileOutcome` 与 `finalizeBatch`：

```cpp
void ZzSftpEngine::noteBatchFileOutcome(Batch &b, const Transfer &t)
{
    if (t.finishCode == 0)
        b.sched->onFileFinished(t.file.fileName(), t.transferred);
    else
        b.sched->onFileFailed(t.file.fileName(), t.finishCode, t.finishMessage);
}

void ZzSftpEngine::finalizeBatch(size_t index)
{
    std::unique_ptr<Batch> b = std::move(m_batches[index]);
    m_batches.erase(m_batches.begin() + static_cast<std::ptrdiff_t>(index));
    if (b->outcomeEmitted)
        return;
    const bool ok = !b->sched->cancelled() && b->sched->failedCount() == 0;
    if (ok) {
        if (m_progressFn && b->sched->doneBytes() != b->lastReported)
            m_progressFn(b->requestId, b->sched->doneBytes(), b->sched->totalBytes());
        if (m_finishFn)
            m_finishFn(b->requestId);
    } else if (m_errorFn) {
        const int code = b->sched->cancelled()
            ? static_cast<int>(ZzSshErrorCode::Cancelled) : b->sched->firstErrorCode();
        const QString msg = b->sched->cancelled()
            ? ZzSshError::message(code) : b->sched->summary();
        m_errorFn(b->requestId, code, msg);
    }
}
```

4. 既有函数接线（最小侵入）：

- `pumpTransfers()` 循环内 `Done/Failed` 分支：置 closing 前检查 `t.batchId != 0` 时不改流程（closing 收尾不变）；在 `finalizeTransfer` 中分流：

```cpp
void ZzSftpEngine::finalizeTransfer(size_t index)
{
    std::unique_ptr<Transfer> t = std::move(m_transfers[index]);
    m_transfers.erase(m_transfers.begin() + static_cast<std::ptrdiff_t>(index));
    if (t->batchId != 0) {
        for (auto &b : m_batches) {
            if (b->requestId == t->batchId && b->sched) {
                noteBatchFileOutcome(*b, *t);
                break;
            }
        }
        return; // 批子传输不发用户级回调
    }
    emitOutcome(*t);
}
```

- `pumpTransfers()` 函数末尾追加 `pumpBatches();`。
- `cancelTransfer()`：命中批 ID 时取消整个批——先找 `m_batches`：`b->sched->cancel()`，然后**遍历全部** `m_transfers` 把 `requestId == id` 的子传输逐个 `finishTransfer`（Cancelled）；非批 ID 保持原单取消语义。注意遍历时下标处理（erase 后不递增）。
- `close()`：在现有传输循环后，对 `m_batches` 中未发射结局的批补发 Cancelled（经 `m_errorFn`），然后 `m_batches.clear()`。

- [ ] **步骤 2：Session 与 Worker 透传**

`ZzSftpSession.h` 公开区加（注释风格同现有 upload/download）：

```cpp
    /**
     * @brief 递归上传本地目录到远端（有界并发，批进度聚合上报）。
     * @param localDir 本地源目录。
     * @param remoteDir 远端目标目录。
     * @return 批请求 ID；会话未打开时返回 0。
     */
    quint64 uploadDir(const QString &localDir, const QString &remoteDir);

    /**
     * @brief 递归下载远端目录到本地（有界并发，批进度聚合上报）。
     * @param remoteDir 远端源目录。
     * @param localDir 本地目标目录。
     * @return 批请求 ID；会话未打开时返回 0。
     */
    quint64 downloadDir(const QString &remoteDir, const QString &localDir);
```

`ZzSftpSession.cpp` 按 `upload()` 同一模式实现（转发到 `w->doSftpUploadDir(id, reqId, localDir, remoteDir)` / `doSftpDownloadDir`，queued）。

`ZzSshConnectionWorker.h` 加声明、`ZzSshConnectionWorker.cpp` 按 `doSftpUpload` 同一模式实现两个新槽：会话不存在→`sftpTransferError(Cancelled)`；`startDirUpload/startDirDownload` 失败→`sftpTransferError(SftpOperationFailed, err)`；成功后确保 `m_readTimer` 启动。

- [ ] **步骤 3：编译 + 全量回归**

```bash
cd third_party/ZzSshCore
cmake --build --preset linux-release && ctest --test-dir build/linux-release -L unit
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：单测 18 程序全绿；docker 集成 17 项全绿（perf 噪声红定向复跑甄别）。

- [ ] **步骤 4：Commit**

```bash
cd third_party/ZzSshCore
git add src/ZzSftpEngine.h src/ZzSftpEngine.cpp src/ZzSftpSession.h src/ZzSftpSession.cpp \
        src/ZzSshConnectionWorker.h src/ZzSshConnectionWorker.cpp
git commit -m "feat(sftp): engine 目录递归批传输入口 + 全链路透传

- ZzSftpEngine 新增 startDirUpload/startDirDownload：上传先按深度序建
  远端目录树（已存在跳过），下载逐层 listDir 装配计划并 mkpath 本地树；
  文件阶段由 ZzSftpBatchScheduler 有界并发（默认 8）驱动，块大小 0=自动
- 批子传输共享批 requestId 并打 batchId 标记：finalizeTransfer 分流到批
  登记，不发用户级回调；批进度按 ≥1MB/≥100ms 节流聚合上报
- cancelTransfer 命中批 ID 时取消整批（sched->cancel + 全部在飞子传输）；
  close() 为未结局批补发 Cancelled
- ZzSftpSession/ZzSshConnectionWorker 透传 uploadDir/downloadDir，
  语义与单文件路径一致"
```

---

### 任务 4：docker 集成测试（目录递归正确性）

**文件：**
- 修改：`third_party/ZzSshCore/tests/integration/tst_ZzSftpIT.cpp`

- [ ] **步骤 1：编写三个新用例**

在 `tst_ZzSftpIT` 类 private slots 声明并实现在类中（辅助函数）：

```cpp
    void dirUploadDownloadRoundtrip();
    void dirUploadPartialFailureContinues();
    void cancelDirTransferEmitsCancelled();
```

辅助（私有方法）：

```cpp
    /** @brief 造本地目录树：dirs 个子目录 × perDir 个文件 × bytes 字节，返回根路径。 */
    QString makeLocalTree(QTemporaryDir &dir, int dirs, int perDir, qint64 bytes);
    /** @brief 递归校验两目录树逐文件 sha256 一致；不符输出 mismatch。 */
    bool treesEqual(const QString &a, const QString &b, QString *mismatch);
```

`treesEqual` 实现要点：`QDirIterator`（Subdirectories，Files，NoSymlinks）收集 a 的相对路径集合，与 b 比对；逐文件 `QCryptographicHash::hash(读全部, Sha256)` 相等。

`dirUploadDownloadRoundtrip`：

```cpp
void tst_ZzSftpIT::dirUploadDownloadRoundtrip()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSftpSession *sftp = openSftpOrAssert(conn.get());
    QVERIFY(sftp != nullptr);

    // 5 子目录 × 10 文件 × 5KB（含空子目录，验证结构保真）
    const QString localSrc = makeLocalTree(dir, 5, 10, 5 * 1024);
    QVERIFY(!localSrc.isEmpty());
    QVERIFY(QDir().mkpath(localSrc + QStringLiteral("/emptydir")));

    const QString remoteDir = m_remoteBase + QStringLiteral("/tree-up");
    QSignalSpy finishSpy(sftp, &ZzSftpSession::transferFinished);
    QSignalSpy errorSpy(sftp, &ZzSftpSession::transferError);
    QSignalSpy progressSpy(sftp, &ZzSftpSession::transferProgress);

    const quint64 upId = sftp->uploadDir(localSrc, remoteDir);
    QVERIFY(upId != 0);
    QVERIFY(finishSpy.wait(60000));
    QCOMPARE(errorSpy.count(), 0);
    // 批进度：最后一次上报 total = 批总字节（50×5KB）
    QVERIFY(progressSpy.count() >= 1);
    const auto last = progressSpy.last();
    QCOMPARE(last.at(2).toLongLong(), 50 * 5 * 1024);

    // 下载回原树比对 sha256
    const QString localDst = dir.filePath(QStringLiteral("tree-down"));
    finishSpy.clear();
    const quint64 downId = sftp->downloadDir(remoteDir, localDst);
    QVERIFY(downId != 0);
    QVERIFY(finishSpy.wait(60000));
    QCOMPARE(errorSpy.count(), 0);

    QString mismatch;
    QVERIFY2(treesEqual(localSrc, localDst, &mismatch), qPrintable(mismatch));
    QVERIFY(QDir(localDst + QStringLiteral("/emptydir")).exists());
}
```

`dirUploadPartialFailureContinues`：

```cpp
void tst_ZzSftpIT::dirUploadPartialFailureContinues()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSftpSession *sftp = openSftpOrAssert(conn.get());
    QVERIFY(sftp != nullptr);

    // 预置冲突：远端目标下 blocked/ 路径已被一个同名【文件】占据 → 该子目录
    // mkdir 失败且 stat 非目录 → 批发起即失败？——不：此用例改让单文件失败：
    // 预置远端只读目录 noperm（0555），其中文件 open 失败，其余文件正常
    const QString localSrc = makeLocalTree(dir, 2, 5, 1024);
    QVERIFY(!localSrc.isEmpty());
    // 本地造一个将被拒写的子目录名，先在远端建成 0555
    QSignalSpy opSpy(sftp, &ZzSftpSession::operationFinished);
    sftp->makeDir(m_remoteBase + QStringLiteral("/tree-fail/d0"), 0555);
    QVERIFY(opSpy.wait(10000));

    QSignalSpy finishSpy(sftp, &ZzSftpSession::transferFinished);
    QSignalSpy errorSpy(sftp, &ZzSftpSession::transferError);
    const quint64 upId = sftp->uploadDir(localSrc, m_remoteBase + QStringLiteral("/tree-fail"));
    QVERIFY(upId != 0);
    // 批终态为 error（含失败计数汇总），不是 finished
    QVERIFY(errorSpy.wait(60000));
    QCOMPARE(finishSpy.count(), 0);
    QVERIFY(errorSpy.first().at(2).toString().contains(QStringLiteral("失败")));

    // 其余子目录的文件已落盘（批次未被单点失败阻断）
    QSignalSpy listSpy(sftp, &ZzSftpSession::dirListed);
    sftp->listDir(m_remoteBase + QStringLiteral("/tree-fail/d1"));
    QVERIFY(listSpy.wait(10000));
    QCOMPARE(listSpy.first().at(1).value<QList<ZzSftpFileInfo>>().size(), 5);
}
```

（实现注意：`makeLocalTree` 的子目录名固定为 `d0..dN`，保证 d0 与预置只读目录冲突；若 docker 镜像内 sshd 用户 chmod 行为不同导致用例不稳，实现者改为预置同名文件冲突的等价方案并在 commit 说明。）

`cancelDirTransferEmitsCancelled`：

```cpp
void tst_ZzSftpIT::cancelDirTransferEmitsCancelled()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSftpSession *sftp = openSftpOrAssert(conn.get());
    QVERIFY(sftp != nullptr);

    // 较大树保证取消时仍有在飞：10 子目录 × 20 文件 × 256KB
    const QString localSrc = makeLocalTree(dir, 10, 20, 256 * 1024);
    QVERIFY(!localSrc.isEmpty());

    QSignalSpy errorSpy(sftp, &ZzSftpSession::transferError);
    const quint64 upId = sftp->uploadDir(localSrc, m_remoteBase + QStringLiteral("/tree-cancel"));
    QVERIFY(upId != 0);
    sftp->cancelTransfer(upId);
    QVERIFY(errorSpy.wait(30000));
    QCOMPARE(errorSpy.first().at(0).toULongLong(), upId);
    QCOMPARE(errorSpy.first().at(1).toInt(), static_cast<int>(ZzSshErrorCode::Cancelled));
}
```

- [ ] **步骤 2：运行 docker 集成验证**

```bash
cd third_party/ZzSshCore
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：集成 17 项全绿，其中 `tst_ZzSftpIT` 含 3 个新用例全过。

- [ ] **步骤 3：Commit**

```bash
cd third_party/ZzSshCore
git add tests/integration/tst_ZzSftpIT.cpp
git commit -m "test(sftp): 目录递归传输 docker 集成三用例

- dirUploadDownloadRoundtrip：5 目录×10 文件×5KB（含空目录）上传+下载，
  逐文件 sha256 校验树一致，批进度 total=批总字节
- dirUploadPartialFailureContinues：只读子目录内文件失败不阻断批次，
  批终态 error 带失败计数汇总，其余目录文件完整落盘
- cancelDirTransferEmitsCancelled：大批传输中取消，批终态 Cancelled"
```

---

### 任务 5：perf 门控 zzsftp-smallfiles

**文件：**
- 创建：`third_party/ZzSshCore/tests/perf/tst_ZzSftpSmallFilesPerf.cpp`
- 修改：`third_party/ZzSshCore/tests/CMakeLists.txt`

- [ ] **步骤 1：编写 perf 测试**

参照 `tst_ZzSftpWanPerf.cpp` 的既有骨架复用以下模式（读该文件照搬）：`ZzSshTestServerConfig::fromEnvironment()` 跳过逻辑、`pruneRecords`、`addRecord`、netem qdisc 添加/删除（含退出兜底）、`runSystemSftp` 计时、records JSON 落盘到 `tests/perf/records/`。新文件结构：

```cpp
class tst_ZzSftpSmallFilesPerf : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase();
    void smallFilesRatioGate();
    void cleanupTestCase();
private:
    // 同 WanPerf：makeConnected/openSftp/measure* 辅助；
    /** @brief 造 2000 文件×20KB×16 子目录的本地树，返回根路径。 */
    QString makeSmallFilesTree();
    /** @brief 本库 uploadDir/downloadDir 计时（毫秒）。 */
    qint64 measureLibDirUpload(const QString &localDir, const QString &remoteDir);
    qint64 measureLibDirDownload(const QString &remoteDir, const QString &localDir);
    /** @brief 系统 sftp -r put/get 计时（毫秒），复用 WanPerf 的 batch 文件模式。 */
    qint64 measureSystemSftp(const QString &batchLine);
    // records 辅助（addRecord/pruneRecords 照搬 WanPerf）

    static constexpr int FILE_COUNT = 2000;
    static constexpr qint64 FILE_SIZE = 20 * 1024;
    static constexpr int DIR_COUNT = 16;
    static constexpr double LOOPBACK_RATIO_MIN = 1.5;  ///< 回环比值门控（规格 §1）
    static constexpr double WAN50_RATIO_MIN = 2.0;     ///< WAN 50ms 比值门控（规格 §1）
    static constexpr int RATIO_SAMPLES = 2;            ///< 双方各采样次数（取最优，对等口径）
};
```

`smallFilesRatioGate` 流程：

1. `ZZSSH_IT_CONTAINER_NAME` 校验（同 WanPerf）；本地产树（16 子目录均布 2000 文件，确定性内容）。
2. **回环档**：本库 uploadDir 计时（取 2 次最优）→ 系统 `sftp -r put` 计时（2 次最优）→ `upRatio = 系统MB/s ÷ 本库MB/s` 的比值口径统一为 `本库吞吐 / 系统吞吐 ≥ 1.5`；下载同口径。`addRecord("smallfiles-loopback-upload", "ratio", ...)` / download，QVERIFY2 带双方 MB/s 文案。
3. **WAN 50ms 档**：`tc qdisc add ... netem delay 50ms`（同 WanPerf 的容器内执行与兜底删除）→ 同口径两侧测量 → 门控 ≥2.0，`addRecord("smallfiles-wan50-upload"/"wan50-download", ...)`。
4. 远端/本地测试树清理（系统 sftp rm -rf 等价 batch；本地 QTemporaryDir 自动）。

CMake 注册（perf 标签，同 WanPerf 模式）：

```cmake
zz_add_test(tst_ZzSftpSmallFilesPerf perf/tst_ZzSftpSmallFilesPerf.cpp)
target_link_libraries(tst_ZzSftpSmallFilesPerf PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSftpSmallFilesPerf PROPERTIES LABELS "perf")
```

- [ ] **步骤 2：运行 perf 验证门控通过**

```bash
cd third_party/ZzSshCore
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：`tst_ZzSftpSmallFilesPerf` 通过且打印两侧 MB/s 与比值；若回环比值在 1.5–2.0 之间临界，记录实际值并在 commit 中注明（禁止为通过而改门控——门控值是规格定的，不达标回开发修复，常见抓手：提高 m_batchConcurrency、核查 mkdir 阶段耗时占比）。

- [ ] **步骤 3：Commit（含 records）**

```bash
cd third_party/ZzSshCore
git add tests/perf/tst_ZzSftpSmallFilesPerf.cpp tests/CMakeLists.txt tests/perf/records/
git commit -m "test(sftp): 小文件批传输 perf 门控（zzsftp-smallfiles）

2000 文件×20KB×16 子目录对齐 WindTerm benchmark 形态：
- 回环：vs OpenSSH sftp -r 比值门控 ≥1.5（实测上/下行见 records）
- WAN 50ms（netem）：比值门控 ≥2.0
- 双方各采样 2 次取最优（对等口径），records 入库滚动基线"
```

---

### 任务 6：主仓接线（Ops 接口 + 面板最小入口）

**前置：** 任务 1–5 在 ZzSshCore 全部提交完成（本任务开始前记录库侧 HEAD，任务结束时 bump gitlink 到它）。

**文件：**
- 修改：`src/panel/ZzSftpOps.h`
- 修改：`src/panel/ZzSftpSessionOps.h/.cpp`
- 修改：`tests/mocks/ZzMockSftpOps.h/.cpp`
- 修改：`src/panel/ZzSftpPanel.h/.cpp`
- 修改：`tests/unit/tst_ZzSftpPanel.cpp`

- [ ] **步骤 1：接口与 mock 扩展（先写失败测试）**

`ZzSftpOps.h` 纯虚区加（注释风格同现有）：

```cpp
    /** @brief 递归上传本地目录到远端（批进度聚合）。@return 批请求 ID；未打开返回 0。 */
    virtual quint64 uploadDir(const QString &localDir, const QString &remoteDir) = 0;
    /** @brief 递归下载远端目录到本地（批进度聚合）。@return 批请求 ID；未打开返回 0。 */
    virtual quint64 downloadDir(const QString &remoteDir, const QString &localDir) = 0;
```

`ZzSftpSessionOps.h/.cpp`：两个 override，QPointer 守卫后透传 `m_session->uploadDir/downloadDir`，会话为空返回 0。

`ZzMockSftpOps.h/.cpp`：两个 override 记录并返回 `nextReqId++`（不发结局信号，与 upload/download 一致）：

```cpp
    QList<QPair<QString, QString>> uploadedDirs;    ///< uploadDir（本地，远端）
    QList<QPair<QString, QString>> downloadedDirs;  ///< downloadDir（远端，本地）
```

在 `tests/unit/tst_ZzSftpPanel.cpp` 追加用例（先写，验证编译失败/断言失败）：

```cpp
void tst_ZzSftpPanel::uploadFolderCallsOpsUploadDir();
void tst_ZzSftpPanel::downloadFolderCallsOpsDownloadDir();
```

`uploadFolderCallsOpsUploadDir`：mock `simulateOpened()`、面板 `navigateTo(QStringLiteral("/home/zztest"))` 并注入 dirListed 后，调面板公开方法 `startUploadDir(QStringLiteral("/tmp/localtree"))`，断言 `mock->uploadedDirs == {("/tmp/localtree", "/home/zztest/localtree")}` 且传输队列多一行（复用现有 `addTransferRow` 断言模式）。

`downloadFolderCallsOpsDownloadDir`：注入目录列表含目录项 `subdir`，调 `startDownloadDir(QStringLiteral("/home/zztest/subdir"), QStringLiteral("/tmp/dst"))`，断言 `mock->downloadedDirs == {("/home/zztest/subdir", "/tmp/dst/subdir")}`。

- [ ] **步骤 2：面板实现**

`ZzSftpPanel.h` 公开区加：

```cpp
    /** @brief 上传本地目录到当前远端目录（等价上传文件夹按钮→选目录确认）。 */
    void startUploadDir(const QString &localDir);
    /** @brief 下载远端目录到本地父目录下（等价目录右键→下载）。 */
    void startDownloadDir(const QString &remotePath, const QString &localParentDir);
```

`ZzSftpPanel.cpp` 实现（复用 `zzJoinPath` 与 `addTransferRow` 模式）：

```cpp
void ZzSftpPanel::startUploadDir(const QString &localDir)
{
    if (!m_ops || !m_ops->isOpen() || m_currentPath.isEmpty())
        return;
    const QString name = QFileInfo(localDir).fileName();
    if (name.isEmpty())
        return;
    const quint64 reqId = m_ops->uploadDir(localDir, zzJoinPath(m_currentPath, name));
    if (reqId > 0)
        addTransferRow(reqId, name, QStringLiteral("上传"));
}

void ZzSftpPanel::startDownloadDir(const QString &remotePath, const QString &localParentDir)
{
    if (!m_ops || !m_ops->isOpen() || remotePath.isEmpty() || localParentDir.isEmpty())
        return;
    const QString name = remotePath.section(QLatin1Char('/'), -1);
    const quint64 reqId = m_ops->downloadDir(remotePath, QDir(localParentDir).filePath(name));
    if (reqId > 0)
        addTransferRow(reqId, name, QStringLiteral("下载"));
}
```

UI 接线（最小）：上传按钮改为下拉或新增「上传文件夹」按钮（`addButton`，点击 `QFileDialog::getExistingDirectory` 后调 `startUploadDir`）；目录右键菜单对目录项加「下载」动作（选本地父目录后调 `startDownloadDir`）。

- [ ] **步骤 3：bump gitlink + 全量回归**

```bash
# 主仓根目录
git add third_party/ZzSshCore   # gitlink 指向库侧任务 5 的 HEAD
cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release
git checkout -- tests/perf/records/
ls tests/perf/records/$(date +%F)-*.json 2>/dev/null  # 逐个核对为当天新生成后删除
```

预期：45 基线 + 新增面板用例全绿。

- [ ] **步骤 4：Commit**

```bash
git add src/panel/ZzSftpOps.h src/panel/ZzSftpSessionOps.h src/panel/ZzSftpSessionOps.cpp \
        src/panel/ZzSftpPanel.h src/panel/ZzSftpPanel.cpp \
        tests/mocks/ZzMockSftpOps.h tests/mocks/ZzMockSftpOps.cpp \
        tests/unit/tst_ZzSftpPanel.cpp third_party/ZzSshCore
git commit -m "feat(sftp): 面板目录递归传输最小接线 + ZzSshCore gitlink bump

- ZzSftpOps 接口加 uploadDir/downloadDir；生产适配器与 mock 同步扩展
- 面板新增「上传文件夹」按钮与目录右键「下载」入口，
  公开 startUploadDir/startDownloadDir 供测试直驱
- 批传输在传输队列按单行呈现（进度聚合，库侧节流）
- gitlink 指向 ZzSshCore 小文件批传输里程碑（任务 1-5）"
```

---

### 任务 7：文档收尾 + 规格核销 + 最终审查

**文件：**
- 修改：`third_party/ZzSshCore/README.md`
- 修改：`docs/superpowers/specs/2026-08-25-sftp-smallfile-batch-design.md`

- [ ] **步骤 1：ZzSshCore README 补小文件实测数据**

在 README 性能章节（WAN 三档数据之后）追加一小节，数字取任务 5 实测 records：

```markdown
### 小文件批量传输（2000 文件 × 20KB × 16 子目录）

| 场景 | 本库 | OpenSSH sftp -r | 比值 |
| --- | --- | --- | --- |
| 回环 上传/下载 | 实测值 | 实测值 | ≥1.5（门控） |
| WAN 50ms 上传/下载 | 实测值 | 实测值 | ≥2.0（门控） |

目录递归传输内置有界并发调度（默认 8 在飞），单文件失败不阻断批次。
```

Commit（库侧）：`docs: README 补小文件批传输实测数据`。

- [ ] **步骤 2：规格核销**

规格文件头部状态改为 `已实现（人工验收挂起）`，追加一节：

```markdown
## 8. 实现核销

- 任务 1-5（ZzSshCore）：walker/scheduler/engine 集成/docker IT/perf 门控，全部完成（库侧 <最终SHA>）。
- 任务 6（主仓接线）：面板入口 + gitlink bump，完成（主仓 <最终SHA>）。
- 实测：回环比值 上/下 = X/X；WAN 50ms 上/下 = X/X（records 见 ZzSshCore tests/perf/records/）。
- 人工验收挂起：Windows 实机「上传文件夹/下载文件夹」入口与真实服务器小文件体验，并入 V0.2 验收清单。
```

同时在 `docs/acceptance/v0.2-manual-acceptance.md` 追加一条目录递归传输验收项（Windows 实机：上传一个 ≥100 文件的目录树，校验内容一致与传输队列单行进度）。

- [ ] **步骤 3：最终宽范围审查**

调度一个全新审查子代理，范围 = ZzSshCore `任务1 SHA..HEAD` + 主仓任务 6 提交，对照规格逐节核对；修复发现的问题（修复轮按 SDD 惯例执行）。

- [ ] **步骤 4：主仓 Commit 核销**

```bash
git add docs/superpowers/specs/2026-08-25-sftp-smallfile-batch-design.md docs/acceptance/v0.2-manual-acceptance.md
git commit -m "docs: 核销小文件批传输规格（已实现，人工验收挂起）

记录库侧/主仓最终 SHA 与实测比值；V0.2 验收清单追加目录递归传输项"
```

- [ ] **步骤 5：汇总推送清单给用户**

列出：ZzSshCore 全部待推提交（任务 1-5、7）、主仓全部待推提交（规格 commit 9fa7946、任务 6、任务 7）。**等用户逐项确认后才 push（先库后主仓）。**

---

## 自检记录

- 规格覆盖：§1 门控→任务 5；§2 组件→任务 1/2/3/6；§3 数据流（两阶段/聚合进度/下载 walk）→任务 3；§4 错误处理（续传汇总/取消/遍历失败/内存不变式）→任务 2/3/4；§5 测试→任务 1/2/4/5/6；§6 非目标未引入任何任务；§7 约束→各任务 commit 步骤与头部基线说明。
- 类型一致性：`ZzSftpDirPlan/ZzSftpDirTask`（任务 1 定义）→ 任务 2 构造参数、任务 3 engine 使用同名；`ZzSftpBatchScheduler::kick/onFileFinished/onFileFailed/cancel/isDone/summary/firstErrorCode/failedCount/doneBytes/totalBytes`（任务 2 定义）→ 任务 3 全部同名调用；`uploadDir/downloadDir` 命名在任务 3（session）、任务 6（ops/panel/mock/测试）一致；`startUploadDir/startDownloadDir`（面板公开方法）任务 6 测试与实现一致。
- 占位符：无 TODO/待定；任务 3 中 `rel` 计算已注明实现语义（remoteRoot 前缀剥离），属实现细节的明确说明而非占位。
