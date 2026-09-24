# 更新来源选择实现计划

**目标：** 正式版更新可以从 GitCode 或 GitHub 检查并下载安装包，自动模式优先 GitCode。

**架构：** `zzclawterm-core::updater` 持有来源、清单地址和规范包地址规则；desktop 的进程级 `UpdateStore` 持有选择与实际来源；GitCode 发布工作流在正式版附件全部同步后维护固定通道清单。

**技术栈：** Rust 2024、GPUI、Python 3、GitHub Actions、GitCode Release API。

---

### 任务 1：来源与安全 URL 策略

**文件：** `crates/zzclawterm-core/src/updater.rs`

- [ ] 先写测试：正式版三种来源的清单地址、GitCode 包 URL 只能由规范 GitHub URL 的已验证版本与文件名构造，预览版继续使用 GitHub。
- [ ] 运行 `cargo test -p zzclawterm-core updater`，确认新测试失败。
- [ ] 实现来源枚举和 URL 选择，保持旧清单字段不变。
- [ ] 重跑目标测试。

### 任务 2：检查、下载和界面状态

**文件：** `crates/zzclawterm-desktop/src/http/update.rs`、`crates/zzclawterm-desktop/src/features/update/state.rs`、`crates/zzclawterm-desktop/src/features/update/download.rs`、`crates/zzclawterm-desktop/src/features/panels/update_overlay.rs`

- [ ] 先写状态测试：切换来源使旧检查结果失效，下载锁定检查所得的版本和来源。
- [ ] 运行 desktop 目标测试确认失败。
- [ ] 把来源传入后台检查与下载，保留签名校验，并用 `ZzClawTabs` 展示来源选择。
- [ ] 重跑 desktop 目标测试与 `cargo check -p zzclawterm-app`。

### 任务 3：GitCode 稳定通道清单

**文件：** `scripts/ci/gitcode_release_upload.py`、`scripts/tests/test_gitcode_release_upload.py`、`.github/workflows/publish-gitcode-release.yml`

- [ ] 先写测试：附件同步成功后更新固定通道 `latest.json`；失败时不前移入口。
- [ ] 运行 Python 目标测试确认失败。
- [ ] 发布或替换 GitCode 稳定通道附件，重复执行需幂等。
- [ ] 重跑 Python 测试和 `actionlint`。

### 任务 4：文档和验证

**文件：** `BUILDING.md`、中英文开发文档。

- [ ] 记录来源行为和 GitCode 通道发布前提。
- [ ] 运行相关 Rust 测试、`cargo fmt --all -- --check`、工作区检查、文档翻译检查与构建。
- [ ] 分类提交中文 Conventional Commits；确认 GitHub CI 成功后在线验证 GitCode 通道地址。
