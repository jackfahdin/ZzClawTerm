<p align="center">
  <img src="./docs-site/static/img/logo.svg" alt="ZzClawTerm" width="128" height="128">
</p>

<h1 align="center">ZzClawTerm</h1>

<p align="center">
  <strong>基于 GPUI 构建的现代原生远程终端工作区</strong>
</p>

## 简介

ZzClawTerm 是一个使用 Rust 和 GPUI 编写的原生桌面终端工作区，在一个客户端中统一处理
SSH、本地 Shell、Telnet、串口、RDP、VNC、SFTP、隧道、OTP、AI 辅助，以及加密同步与备份。

本项目 fork 自 [NyaTerm](https://github.com/nyakang/nyaterm)（上游作者 Kang），
在其基础上进行独立品牌改造与后续维护，感谢原作者的工作。上游项目基于 Apache-2.0 许可证开源。

## 支持平台

| 系统 | 支持情况 |
| :--- | :--- |
| **Windows** | Windows 10/11，x64 / arm64 |
| **macOS** | macOS 12+，Intel / Apple Silicon |
| **Linux** | Ubuntu 20.04+、Fedora 36+、Arch Linux 及类似发行版 |

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
