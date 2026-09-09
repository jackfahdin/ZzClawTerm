# 编译指南

本文档介绍 ZzClawTerm 在各平台上的环境准备、编译、测试与打包流程。

目录：

- [环境依赖](#环境依赖)
- [拉取源码与依赖说明](#拉取源码与依赖说明)
- [编译](#编译)
- [测试与质量检查](#测试与质量检查)
- [打包](#打包)
- [CI 发布流程](#ci-发布流程)
- [文档站（docs-site）](#文档站docs-site)
- [常见问题](#常见问题)

---

## 环境依赖

### 通用（所有平台）

| 依赖 | 说明 |
|------|------|
| Rust stable | 通过 [rustup](https://rustup.rs/) 安装；项目使用 Rust 2024 edition，建议保持 stable 为最新 |
| Git | 克隆仓库；cargo 拉取 git 依赖时也会调用 |
| Python 3.12+ | 仅打包、产物校验和脚本测试需要（`scripts/release/`、`scripts/tests/`） |

### Windows

1. **Visual Studio 生成工具**：安装"使用 C++ 的桌面开发"工作负载
   （包含 MSVC 编译器、链接器与 Windows SDK）。装完整版 Visual Studio
   或独立的 Build Tools for Visual Studio 都可以
2. Rust 工具链使用默认的 `stable-x86_64-pc-windows-msvc` 即可
3. **NSIS**（仅打包安装包时需要）：可用仓库脚本安装

   ```powershell
   ./scripts/ci/install-nsis.ps1
   ```

### macOS

安装 Xcode 命令行工具即可，打包用的 `hdiutil` 为系统自带：

```bash
xcode-select --install
```

### Linux（Debian / Ubuntu）

GPUI 依赖一组 X11/Wayland 系统库。推荐直接运行仓库内的 CI 脚本：

```bash
bash scripts/ci/install-linux-deps.sh
```

或手动安装（Ubuntu 20.04+）：

```bash
sudo apt-get install -y \
  build-essential clang pkg-config \
  libdbus-1-dev libfontconfig1-dev libfreetype6-dev libssl-dev libudev-dev \
  libwayland-dev libx11-dev libx11-xcb-dev libxcb-cursor-dev libxcb-icccm4-dev \
  libxcb-image0-dev libxcb-keysyms1-dev libxcb-randr0-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xinerama0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libzstd-dev \
  libvulkan1 mesa-vulkan-drivers
```

**打包还需要**额外的工具和库：

```bash
sudo apt-get install -y curl file patchelf desktop-file-utils dpkg-dev rpm
bash scripts/ci/install-appimagetool.sh x86_64-unknown-linux-gnu   # 按目标架构传参
```

其他发行版（Fedora、Arch 等）请安装对应的等价开发包。

---

## 拉取源码与依赖说明

```bash
git clone https://github.com/jackfahdin/ZzClawTerm.git
cd ZzClawTerm
```

项目依赖 9 个打过补丁的第三方 fork：zed（gpui、gpui_platform）、gpui-kit
（gpui-component 等）、alacritty、IronRDP、sspi-rs、russh、russh-sftp、
vnc-rs、zmodem2。它们在根 `Cargo.toml` 中被锁定到
[GitCode 镜像](https://gitcode.com/JackfahdinImport) 各仓库 `nyaterm` 分支的
固定修订版本，`cargo build` 时自动拉取，**不需要手动克隆**。

crates.io 默认走官方源。网络较慢时可配置镜像，在 `~/.cargo/config.toml`
（Windows 为 `%USERPROFILE%\.cargo\config.toml`）中写入：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
```

---

## 编译

### 开发构建

```bash
cargo build
```

工作区的 `default-members` 包含应用本体和 RDP/VNC 两个 helper 进程，
一条 `cargo build` 即可构建全部可执行文件。

运行：

```bash
cargo run -p zzclawterm-app --bin zzclawterm
```

> **注意**：`cargo run` 只构建应用本体。RDP/VNC 功能是独立的 helper
> 进程，应用会在自身 exe 同目录查找它们；缺失时报 `HelperMissing`。
> 请先执行过一次完整的 `cargo build`，或单独构建：
>
> ```bash
> cargo build -p zzclawterm-rdp-helper -p zzclawterm-vnc-helper
> ```
>
> 也可以用环境变量显式指定 helper 路径：
> `ZZCLAWTERM_RDP_HELPER` / `ZZCLAWTERM_VNC_HELPER`

迭代开发时可以用更细的命令：

```bash
cargo check -p zzclawterm-app     # 只检查某个包
cargo test -p zzclawterm-core     # 只跑某个包的测试
```

### 发布构建

```bash
cargo build --release
```

打包脚本 `package_native.py` 会自动以 release 模式构建应用和全部 helper，
一般不需要手动执行 release 构建。

支持的构建目标（发布矩阵覆盖全部 6 个）：

| 目标三元组 | 平台 |
|------------|------|
| `x86_64-pc-windows-msvc` | Windows x64 |
| `aarch64-pc-windows-msvc` | Windows ARM64 |
| `x86_64-apple-darwin` | macOS Intel |
| `aarch64-apple-darwin` | macOS Apple Silicon |
| `x86_64-unknown-linux-gnu` | Linux x64 |
| `aarch64-unknown-linux-gnu` | Linux ARM64 |

构建非本机架构的目标需要先安装对应 target（例如在 x64 Windows 上构建
ARM64 安装包）：

```bash
rustup target add aarch64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
```

### 构建期环境变量

| 变量 | 必需性 | 说明 |
|------|--------|------|
| `ZZCLAWTERM_GITHUB_GIST_CLIENT_ID` | 可选 | GitHub Gist 云同步的 OAuth Client ID。不设置也能正常编译运行，仅 Gist 同步功能在界面中提示当前构建缺少 Client ID。CI 发布流程要求必须配置 |

---

## 测试与质量检查

提交代码前建议按顺序执行（与 CI 一致）：

```bash
# 1. 全工作区编译检查
cargo check --workspace --locked

# 2. 全工作区测试
cargo test --workspace --locked --no-fail-fast

# 3. 格式检查（如有差异用 cargo fmt --all 修复）
cargo fmt --all -- --check

# 4. lint 检查
cargo clippy --workspace --all-targets -- -D warnings
```

> **代理环境注意**：部分测试会在 `127.0.0.1` 起本地 mock 服务器。如果 shell
> 里设置了 `HTTP_PROXY` 等代理变量，请求会被代理劫走并报
> `502 Bad Gateway`。请先设置：
>
> ```powershell
> # PowerShell
> $env:NO_PROXY="localhost,127.0.0.1"
> ```
>
> ```bash
> # bash
> export NO_PROXY="localhost,127.0.0.1"
> ```

发布/打包脚本（Python）有独立测试：

```bash
python -m unittest \
  scripts.tests.test_check_release_assets \
  scripts.tests.test_package_native \
  scripts.tests.test_verify_native_package \
  scripts.tests.test_generate_release_metadata
```

仓库还带有两个结构性检查脚本：

```bash
python scripts/ci/check_architecture.py        # crate 边界/架构约束
python scripts/ci/check_docs_translations.py   # 文档中英文翻译一致性
```

修改 GitHub Actions 工作流后可用 actionlint 校验语法（需要 Docker）：

```bash
docker run --rm -v "${PWD}:/repo" -w /repo rhysd/actionlint:1.7.7
```

---

## 打包

统一入口是 `scripts/release/package_native.py`，产物输出到 `dist/`。
它会自动以 release 模式构建应用与 3 个 helper
（`zzclawterm-rdp-helper`、`zzclawterm-vnc-helper`、`zzclawterm-mcp`），
并保证 helper 放在应用旁边——这是运行时解析 helper 的要求：

```bash
python scripts/release/package_native.py <rust-target>
```

版本相关环境变量：

| 变量 | 说明 |
|------|------|
| `ZZCLAWTERM_VERSION` | 包元数据版本，默认取 workspace 版本；若显式设置则**必须**与 `Cargo.toml` 的 workspace 版本一致 |
| `ZZCLAWTERM_ARTIFACT_VERSION` | 产物文件名中的版本标签，默认同上；快照构建可用 `main-snapshot` |

各平台产物与前置要求：

### Windows

前置：NSIS（`./scripts/ci/install-nsis.ps1`）。

```bash
python scripts/release/package_native.py x86_64-pc-windows-msvc
```

产物：`ZzClawTerm_<版本>_windows_x64-setup.exe`（NSIS 安装包）和
`..._windows_x64-portable.zip`（便携版，内含应用、helper、LICENSE 等）。
ARM64 换用 `aarch64-pc-windows-msvc`。

#### 随包附带 VcXsrv（X Server，可选）

Windows 包可以附带 VcXsrv，供 SSH X11 转发时以独立进程方式自动拉起。
打包脚本按以下顺序解析 VcXsrv 编译产物目录（目录根下必须有
`vcxsrv.exe`，并包含 `fonts/`、`locale/` 等数据目录）：

1. 环境变量 `ZZCLAWTERM_VCXSRV_DIST` 指向的目录；
2. 仓库内的 `vendor/vcxsrv/`（已被 `.gitignore` 排除，不会入库）。

两者都不存在时脚本会打印醒目 warning 并继续打出不含 X Server 的包。
来源可以是自行编译的 VcXsrv（`msbuild` Release x64 产物目录），也可以是
官方发布的安装目录解压结果。找到后整个目录会复制到包内的 `vcxsrv/`
子目录，并自动写入一份 `NOTICE.txt`。

注意：VcXsrv 采用 GPLv3，随包分发时必须附带源码出处说明——包内的
`vcxsrv/NOTICE.txt` 即为此用途，不要删除。

### macOS

前置：无额外工具（`hdiutil` 系统自带）。需在对应架构的 macOS 上构建。

```bash
python scripts/release/package_native.py aarch64-apple-darwin
```

产物：`..._macos_arm64.dmg`（安装镜像）和 `..._macos_arm64.app.tar.gz`
（更新器使用的 .app 压缩包）。Intel 换用 `x86_64-apple-darwin`。

### Linux

前置：额外的 apt 包与 appimagetool（见 [Linux 环境依赖](#linuxdebian--ubuntu)）。

```bash
python scripts/release/package_native.py x86_64-unknown-linux-gnu
```

产物：`..._linux_x64.AppImage`、`..._linux_x64.deb`、`..._linux_x64.rpm`。
ARM64 换用 `aarch64-unknown-linux-gnu`。

> 在没有 FUSE 的环境（容器、部分 CI）中运行 appimagetool 需要：
>
> ```bash
> export APPIMAGE_EXTRACT_AND_RUN=1
> ```

### 校验产物

打包完成后可执行完整性校验（检查文件清单、helper 是否齐全、归档路径
安全性、最小体积等）：

```bash
python scripts/release/verify_native_package.py \
  --target x86_64-pc-windows-msvc \
  --version 0.0.1 --artifact-version 0.0.1 --dist dist
```

---

## CI 发布流程

推送 `v*` 标签（或在 Actions 页面手动触发 `Release` 工作流）即可自动完成
三平台发布：

```bash
git tag v0.0.1
git push origin v0.0.1
```

> 标签版本必须与根 `Cargo.toml` 的 workspace 版本一致，否则 preflight
> 会直接失败。

工作流分三个阶段（`.github/workflows/release.yml`）：

1. **preflight**（Ubuntu）：解析并校验版本号、跑工作区测试与 Python
   脚本测试、actionlint 校验全部工作流
2. **package**（6 个平台矩阵）：macOS arm64/x64、Linux x64/arm64、
   Windows x64/arm64，分别执行 `package_native.py` +
   `verify_native_package.py` 并上传产物
3. **release**（仅 tag 或手动选择发布时执行）：对 6 个更新器产物做
   Tauri/minisign 签名、生成 `checksums.txt`、`downloads.json`、
   `latest.json`，创建或更新 GitHub Release 并上传全部产物

发布前需要在仓库配置以下内容：

| 位置 | 名称 | 用途 |
|------|------|------|
| Settings → Variables | `ZZCLAWTERM_GITHUB_GIST_CLIENT_ID` | 构建期注入 Gist 同步的 OAuth Client ID；**可选**，未配置时构建继续但不含 Gist 同步功能 |
| Settings → Secrets | `TAURI_SIGNING_PRIVATE_KEY_B64` | 更新器签名私钥（base64） |
| Settings → Secrets | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 签名私钥口令 |

另有 `publish-gitcode-release.yml` 可把发布同步到 GitCode，需要额外配置
变量 `GITCODE_OWNER`、`GITCODE_REPO` 和密钥 `GITCODE_TOKEN`。

---

## 文档站（docs-site）

文档站基于 Docusaurus，使用 pnpm：

```bash
cd docs-site
pnpm install
pnpm start:zh      # 中文预览（开发服务器）
pnpm start:en      # 英文预览
pnpm build         # 产出静态站点
```

---

## 常见问题

**RDP/VNC 报 `HelperMissing`**
`cargo run` 只构建了应用本体。先执行一次完整的 `cargo build`（或单独
构建两个 helper），确保 helper 与应用 exe 在同一目录。详见
[开发构建](#开发构建)。

**测试报 `502 Bad Gateway`**
shell 的代理环境变量劫持了发往 `127.0.0.1` 的本地 mock 请求。设置
`NO_PROXY="localhost,127.0.0.1"` 后重跑，详见[测试与质量检查](#测试与质量检查)。

**启动日志出现 `ERROR gpui::window: window not found`**
GPUI 在窗口创建/销毁的时序窗口内向活动窗口句柄派发动作时打出的非致命
日志，上游即存在，不影响功能。

**Windows 任务栏图标不显示**
exe 的图标资源由 `crates/zzclawterm-app/build.rs` 通过 winresource 嵌入，
正常构建即包含。若替换图标后任务栏仍显示旧图标，是 Windows 图标缓存
问题，重启资源管理器或清理图标缓存即可。

**`cargo check --locked` 报 lockfile 需要更新**
本地新增/升级过依赖后 `Cargo.lock` 已变化。先不带 `--locked` 跑一次
`cargo check` 让 lockfile 更新，并把它一起提交。
