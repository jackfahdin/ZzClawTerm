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
- **只门控不写基线**：CI 容器一次性，records 滚动基线仍由本地实跑维护并随 commit 入库；CI 产生的 records 与 ctest 日志上传 artifacts 供排查。
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
