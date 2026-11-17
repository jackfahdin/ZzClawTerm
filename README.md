<p align="center">
  <img src="./docs-site/static/img/logo.svg" alt="ZzClawTerm" width="128" height="128">
</p>

<h1 align="center">ZzClawTerm</h1>

<p align="center">
  <strong>基于 GPUI 构建的现代原生远程终端工作区</strong>
</p>

## 简介

ZzClawTerm 是一个使用 Rust 和 GPUI 编写的原生桌面终端工作区，在一个客户端中统一处理
SSH、本地 Shell、Telnet、串口、Android 设备、RDP、VNC、SFTP、隧道、OTP、AI 辅助，以及加密同步与备份。

本项目 fork 自 [NyaTerm](https://github.com/nyakang/nyaterm)（上游作者 Kang），
在其基础上进行独立品牌改造与后续维护，感谢原作者的工作。上游项目基于 Apache-2.0 许可证开源。

## 支持平台

| 系统 | 支持情况 |
| :--- | :--- |
| **Windows** | Windows 10/11，x64 / arm64 |
| **macOS** | macOS 12+，Intel / Apple Silicon |
| **Linux** | x64：Ubuntu 22.04+；arm64：Ubuntu 24.04+。Fedora、Arch Linux 等需满足对应架构的系统库要求 |

定制 Ubuntu 20.04 x86-64 桌面（glibc 2.31）可使用专门的
[focal 离线兼容包](scripts/compat/linux-focal/README.md)，其中包含全部运行时 helper、
ABI 报告和系统 / 软件 OpenGL 启动回退。

## 下载

从 [Releases](https://github.com/jackfahdin/ZzClawTerm/releases) 页面下载对应平台的安装包：

| 平台 | 格式 |
|------|------|
| Windows | `.exe` / 便携版 `.zip` |
| macOS | `.dmg` |
| Linux | `.deb` / `.AppImage` / `.rpm` |

## 自行编译

编译环境要求与完整步骤见 [编译文档](BUILDING.md)。最简单的方式：

```bash
git clone https://github.com/jackfahdin/ZzClawTerm.git
cd ZzClawTerm
cargo run -p zzclawterm-app --bin zzclawterm
```

## 许可证

本项目基于 [Apache License 2.0](LICENSE) 开源。

```text
├── crates/
│   ├── zzclawterm-app/             # Native GPUI application entry point and assets
│   ├── zzclawterm-desktop/         # GPUI application composition and feature state
│   ├── zzclawterm-core/            # UI-independent domain models and policies
│   ├── zzclawterm-terminal/        # Terminal state machine and parsing
│   ├── zzclawterm-terminal-runtime/ # Frame worker, search, and session event routing
│   ├── zzclawterm-terminal-gpui/   # GPUI terminal layout, input, and painting
│   ├── zzclawterm-transport/       # SSH, PTY, SFTP, Telnet, Serial, and transfers
│   ├── zzclawterm-store/           # Persistence and database compatibility
│   ├── zzclawterm-ui/              # Shared GPUI controls and theme integration
│   ├── zzclawterm-remote-desktop/  # UI-independent RDP/VNC sessions and typed IPC
│   ├── zzclawterm-rdp-helper/      # Isolated IronRDP protocol helper process
│   ├── zzclawterm-vnc-helper/      # Isolated VNC protocol helper process
│   └── zzclawterm-otp/             # OTP compatibility implementation
├── docs-site/                   # Docusaurus documentation site
├── Cargo.toml                   # Rust workspace definition
└── Cargo.lock
```

---

<a name="credits"></a>
# Credits
Thanks to the following projects and libraries that make ZzClawTerm possible:
- [WindTerm](https://github.com/kingToolbox/WindTerm) - Inspired the design and features of ZzClawTerm
- [tabby](https://github.com/Eugeny/tabby) - An excellent cross-platform terminal that provided many design inspirations
- [Alacritty](https://github.com/alacritty/alacritty) - Terminal emulation components used by ZzClawTerm
- [russh](https://github.com/warp-tech/russh) - An SSH client and server implementation in Rust

---

<a name="sponsor"></a>
# Sponsor

If ZzClawTerm helps your daily development, operations, or remote terminal workflow, you can support ongoing maintenance from the [Sponsor page](./docs-site/i18n/en/docusaurus-plugin-content-docs/current/sponsor.md).

---

<a name="contributors"></a>
# Contributors

<a href="https://github.com/nyakang/zzclawterm/graphs/contributors">
  <img src="https://contrib-wall.coderkang.workers.dev/image?snapshot=EEZNlyhjBOrARLki-MUhXqKk&amp;sealed_token=eyJ2IjoxLCJ0eXBlIjoic25hcHNob3QiLCJzbmFwc2hvdCI6IkVFWk5seWhqQk9yQVJMa2ktTVVoWHFLayIsImV4cCI6MTgxNTAzNjQ0NH0.UwegiwfiT3KSKQ17n5ivt24EFwAtoaHZGRwx97t0ODA&amp;sealed_github_token=A6pS93BW-53hz_Cs.8rPY0yM6koNS_UowTbu8qkWzmPLRco8HJRJO2_yp5MZH9yz9o1Ob2g5MUL4aLFK24gjbDexjdErw6eR72ruRbNcsFciYuM4DghLZJgi9V5D9f1-_xFT-sGYK6ibc1A4aWz8EyvFIQo0byPAXv-d410cJmhwsH-bqc963L8J-z0kU6oIi-2zCSszxllze23WmJsWnTJBFUBrQbllg5cLwuHm6q-BMkjoVxChLdOMVwEs" alt="Contributors" />
</a>

---

## Star History

<a href="https://www.star-history.com/?repos=nyakang%2Fzzclawterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=nyakang/zzclawterm&type=date&theme=dark&legend=top-left&sealed_token=B9FcRzP_4KoFVgTzgY_L9J4F1huRnhx9N962VMQBHjFsF-VtiApJNnKYWg5IaJDZtDm7iCd3epIU3uEZTSG2XVoU8jVssH-ge-gPjPGq5cRz9xtF2N_piA" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=nyakang/zzclawterm&type=date&legend=top-left&sealed_token=B9FcRzP_4KoFVgTzgY_L9J4F1huRnhx9N962VMQBHjFsF-VtiApJNnKYWg5IaJDZtDm7iCd3epIU3uEZTSG2XVoU8jVssH-ge-gPjPGq5cRz9xtF2N_piA" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=nyakang/zzclawterm&type=date&legend=top-left&sealed_token=B9FcRzP_4KoFVgTzgY_L9J4F1huRnhx9N962VMQBHjFsF-VtiApJNnKYWg5IaJDZtDm7iCd3epIU3uEZTSG2XVoU8jVssH-ge-gPjPGq5cRz9xtF2N_piA" />
 </picture>
</a>

---

<a name="license"></a>
# License

This project is licensed under the [Apache License 2.0](LICENSE).
