# CI/CD 建设 设计规格

- 日期：2026-08-26
- 状态：已批准（头脑风暴四节设计 + Continuous Build 补充，用户全部认可）
- 范围：主仓（GitHub）三个 workflow：ci.yml / perf.yml / release.yml；ZzSshCore（gitcode）不单建 CI，由主仓 CI 统一覆盖
- 前置事实：两仓目前无任何 CI；三平台打包脚本已存在（scripts/package-linux.sh、package-macos.sh、package-windows.ps1），现为手工执行

## 1. 目标与已确认决策

- **库仓 CI 位置**：ZzSshCore 改动按惯例最终 bump gitlink 回主仓，主仓 CI 连同子模块一起构建并跑库测试，一处维护全覆盖。
- **分层**：push/PR 快门禁（构建 + 单测 + docker integration 标签，约 15-20 分钟）；perf 门控 nightly + 手动触发（避免 ~40 分钟拖慢提交，噪声红不堵开发）。
- **平台矩阵**：三平台（ubuntu-24.04 / windows-2022 / macos-14）构建 + 主仓单测；库单测与 docker 集成仅 Ubuntu。
- **CD**：tag `v*` 触发三平台打包并上传 GitHub Release。
- **Continuous Build**（vnote 模式）：master push 且测试全绿后三平台打包，更新固定 pre-release，无需登录即可下载验证包。

## 2. ci.yml——快门禁

触发：push / pull_request 到 master。

- Job 1 `build-test` 矩阵 `[ubuntu-24.04, windows-2022, macos-14]`：
  - `actions/checkout` + `submodules: recursive`。
  - install-qt-action 安装 Qt 6.11.1（与本地一致）。
  - 对应平台 preset 构建（Windows/macOS 若无现成 preset，实现时补进 CMakePresets.json）。
  - ctest 主仓单测（45 程序基线），GUI 测试 `QT_QPA_PLATFORM=offscreen`。
- Job 2 `zzsshcore-it`（仅 ubuntu-24.04）：
  - 库单测 18 程序（`ctest -L unit`）。
  - docker 集成**仅 integration 标签**（12 项）；`run-integration-tests.sh` 需加标签选择开关（如环境变量 `ZZSSH_IT_LABELS=integration`），perf 标签留给 nightly。
- Continuous Build 打包 job（仅 master push、needs 前两 job 全绿）：见 §4。
- perf 不进此门禁（已知噪声源 ZzColdStoragePerfTest、wanRatioGate 漂移天然规避）。

## 3. perf.yml——nightly + 手动

- 触发：`schedule` cron `17 19 * * *`（UTC 19:17 ≈ 北京 03:17，避开整点）+ `workflow_dispatch`。
- ubuntu-24.04 + docker：全量 `run-integration-tests.sh`（integration + perf 全部，含 WAN 三档 ~40 分钟与 zzsftp-smallfiles）。
- **CI 专用滚动基线（已决策）**：nightly 跑门控前用 `dawidd6/action-download-artifact@v3`
  下载上一轮 nightly 自产、固定命名 `perf-records-baseline` 的 records artifact 覆盖到
  `tests/perf/records/`；跑完（`if: always()`）用 `actions/upload-artifact@v4` 把
  `records/*.json` 回传同名 artifact（`if-no-files-found: warn`），下一轮对本轮自比。
  首轮无 artifact 时下载步失败但继续（`continue-on-error`），测试 QSKIP 自动采集
  基线（不门控、退出码 0）→ 上传，下一轮起自愈。仓库内 records 基线仅用于本地实跑；
  CI 另上传 `perf-records-<run_id>` 诊断 artifact 供排查。
- nightly 红不堵开发；不做自动开 issue（YAGNI）。

## 4. Continuous Build（master push 出验证包）

- 位置：ci.yml 内 `package` 矩阵 job（三平台），`needs:` 测试 jobs 全绿，`if: github.ref == 'refs/heads/master' && github.event_name == 'push'`。
- 行为：复用三个打包脚本产出安装包 → 更新固定 pre-release：
  - tag `continuous-build`，标题「持续构建（Continuous Build）」，正文含 commit SHA、构建时间、触发方式。
  - 上传前删除同名旧资产；资产命名 `ZzClawTerm-continuous-<sha8>-<平台标识>`（如 `-linux-x86_64.AppImage`、`-macos-arm64.dmg`、`-win64.zip`）。
  - `permissions: contents: write`；`concurrency: continuous-build` 组防并发覆盖。
- 权限与公开性：GitHub Release 资产无需登录即可下载（与 vnote 一致）。
- PR 不打包（省 runner 时间）；打包失败不阻断已绿的测试结论（job 独立）。

## 5. release.yml——正式发布

- 触发：push tag `v*`。
- 矩阵三平台复用打包脚本：
  - Linux AppImage 用 **ubuntu-22.04**（规避 package-linux.sh 注释的 libtiff5 依赖坑）；CI 内下载 linuxdeploy 与 plugin-qt。
  - macOS DMG、Windows 安装包（实现时先核实 package-windows.ps1 的 NSIS 依赖并装进 runner）。
- 产物命名 `ZzClawTerm-vX.Y.Z-<平台标识>`（版本号取自 tag），`gh release upload` 上传。
- 与 continuous-build 互不干扰（不同 tag、不同 release）。

## 6. 验证与约束

- workflow 语法先过 actionlint 本地预检；每个 workflow 首跑全绿才算交付（master 直推迭代，跑挂即修）。
- 三 workflow 的 Qt 版本（6.11.1）、presets、子模块拉取方式保持一致。
- CI 环境差异（offscreen、runner 内 docker）都在 workflow 内显式声明，不改测试代码本身。
- 打包脚本的既有手工用法不受影响（脚本参数化不足时实现任务内最小补充，如 QT_ROOT/版本号入参）。
- commit：Conventional Commits 前缀 + 中文首行 + 空行 + 中文详述；push 需用户确认。

## 7. 非目标（YAGNI）

- gitcode 原生 CI / ZzSshCore 镜像 GitHub。
- perf 失败的自动 issue / 通知机器人。
- deb/rpm/Flatpak、Homebrew Cask、自动更新通道。
- PR 打包与 PR 预览环境。
- 代码签名与公证（macOS notarization、Windows 签名）。

## 8. 首跑核销（2026-08-29，任务 1-4 交付后补记）

ci 十跑全绿（run 33244672049）；Continuous Build 全绿且三平台包已传
continuous-build 预发布（run 33251261414）。相对本文档的实际偏差与决策：

1. **快门禁 ctest 加 `-LE perf`**（计划外、预案内）：perf 标签阈值面向开发机
   标定，CI runner 实测偏低（如冷存储写入 38-48 万行/秒 vs 阈值 50 万）。
2. **Qt 安装架构命名**：Qt 6.10+ 元数据改为 `linux_gcc_64` /
   `win64_msvc2022_64`；Windows 因 Qt 服务器 windows_x86 侧聚合 Updates.xml
   与校验和缺口，aqt 固定到上游 master 提交 16db45a（3.3.0 之后修复），
   linux/mac 用稳定 3.3.x。
3. **OpenSSL**：vendored bundle 无 macOS 产物、Windows 产物不完整
   （static/shared 缺 include/与库）→ CMake 注入前加 ssl.h 粗检，缺失时回退
   系统 OpenSSL（macOS brew openssl@3；Windows runner 自带 3.x）。
4. **QT_ROOT 取 `QT_ROOT_DIR`**：install-qt-action v4 不导出 Qt6_DIR，
   原推导恒为空；Windows 反斜杠路径统一转正斜杠。
5. **Linux 打包 runner 实为 ubuntu-24.04**（§5 写的 22.04 因 ZzPureToolsPro
   要求 GCC 13.1+ 不可行）；libtiff5 从 Launchpad 固定版本
   （libtiff5_4.3.0-6ubuntu0.13_amd64.deb）下载解包，LD_LIBRARY_PATH 注入
   供 linuxdeploy-plugin-qt 解析。
6. **Windows 单测**：ZzPureToolsPro 以 DLL 构建（BUILD_SHARED_LIBS 时序），
   测试经 ENVIRONMENT_MODIFICATION 前置 PATH；三处 POSIX 装置用例
   （0600 权限/只读目录/sh 脚本桩）Q_OS_WIN 声明式 QSKIP；ConPTY 用例修复
   （shell 绝对路径 + Enter 用 \r）；QTest 统一 `-o exe.qtest.log,txt` 文件
   输出绕开 actions/runner#1206（Windows runner 丢弃 ctest 孙进程终端输出）。
7. **continuous-build 产物固定命名（VERSION=continuous，不含 SHA）**：三平台
   并行 leg 各以 `--clobber` 覆盖本平台同名资产，无跨 leg 竞态、不累积旧
   资产；commit 信息写在 release notes。（终审曾发现「先清空全部资产再上传」
   方案有跨 leg 竞态，已返工为此方案。）

待决策/验收项：

- ~~**nightly perf 基线门控在 CI runner 上系统性不兼容**~~（已决策：CI 专用滚动
  基线——artifact 回传固定命名 `perf-records-baseline`，首跑 QSKIP 采集、次轮起自愈）。
- release.yml 端到端不打测试 tag（对外动作），人工验收：首个正式 tag 观察。
- fork PR 无 secret 的子模块拉取场景未验证（当前无 PR）。
