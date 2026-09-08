# 编译指南

本文档介绍如何从源码构建 ZzClawTerm。

## 环境依赖

### 所有平台

- **Rust stable 工具链**：通过 [rustup](https://rustup.rs/) 安装。项目使用 Rust 2024
  edition，建议使用较新的 stable 版本
- **Git**：克隆仓库及拉取 git 依赖

### Windows

- **Visual Studio 生成工具**：安装"使用 C++ 的桌面开发"工作负载
  （含 MSVC 编译器与 Windows SDK），Visual Studio 或独立的 Build Tools 均可
- 默认工具链 `stable-x86_64-pc-windows-msvc` 即可，无需额外配置

### macOS

- **Xcode 命令行工具**：

  ```bash
  xcode-select --install
  ```

### Linux（Debian / Ubuntu）

GPUI 需要一组系统库，可直接运行仓库内的 CI 脚本安装：

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

其他发行版请安装对应的等价开发包。

## 拉取源码并编译

```bash
git clone https://github.com/jackfahdin/ZzClawTerm.git
cd ZzClawTerm
cargo build
```

工作区的 `default-members` 包含应用本体与 RDP/VNC 两个 helper 进程，
因此一条 `cargo build` 即可构建全部可执行文件。

## 运行

```bash
cargo run -p zzclawterm-app --bin zzclawterm
```

注意：`cargo run` 只构建应用本体。RDP 和 VNC 依赖独立于应用进程的 helper
可执行文件，应用会在自身 exe 旁边查找；如果 helper 不存在，对应功能会报
`HelperMissing`。请先执行过一次完整的 `cargo build`，或单独构建：

```bash
cargo build -p zzclawterm-rdp-helper -p zzclawterm-vnc-helper
```

也可以用环境变量显式指定 helper 路径：

- `ZZCLAWTERM_RDP_HELPER`：RDP helper 的完整路径
- `ZZCLAWTERM_VNC_HELPER`：VNC helper 的完整路径

## 依赖来源说明

项目依赖 9 个打过补丁的第三方 fork（zed/gpui、gpui-kit、alacritty、IronRDP、
sspi-rs、russh、russh-sftp、vnc-rs、zmodem2），由 `Cargo.toml` 锁定到
[GitCode 镜像](https://gitcode.com/JackfahdinImport) 各仓库 `nyaterm` 分支的
固定修订版本，`cargo build` 时自动拉取，无需手动克隆。

crates.io 默认走官方源。网络较慢时可配置镜像，例如在
`~/.cargo/config.toml` 中：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
```

## 质量检查

提交代码前建议运行：

```bash
cargo check --workspace              # 全工作区编译检查
cargo test --workspace               # 全工作区测试
cargo fmt --all -- --check           # 格式检查（用 cargo fmt --all 修复）
cargo clippy --workspace --all-targets -- -D warnings   # lint 检查
```

## 发布打包

`scripts/release/package_native.py` 负责把应用与 helper 打包为各平台发布产物
（Windows 便携 ZIP / 安装包、macOS `.dmg`、Linux `.deb` / `.AppImage` / `.rpm`），
其 `HELPER_BINS` 列表必须包含全部 helper。三平台的完整发布流程由
`.github/workflows/release.yml` 在推送 `v*` 标签时自动执行。
