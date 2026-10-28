# GPUI 2.0 迁移指南

ZzClawTerm 0.2.0 起，上游 NyaTerm 已将 GPUI 版本纳入 2.0 主线，本项目也同步以 GPUI 原生实现作为技术基线。早期版本中的 Tauri/WebView 内容仅用于历史参考。

## 如何升级

- **安装版**：在应用内检查更新，下载后选择“安装并重启”。也可以从 [Releases](https://github.com/jackfahdin/ZzClawTerm/releases) 下载新版安装包覆盖安装。
- **便携版**：下载完整的新便携 ZIP 并解压，将旧目录中的 `data/` 复制到新目录，然后运行新版 `ZzClawTerm.exe`。不要只替换主程序，因为运行还依赖随包提供的 helper。

## 数据兼容性

现有连接、凭据、设置、OTP、已知主机、备份和云同步数据继续沿用。升级前仍建议先备份重要数据；便携版的全部用户数据都位于 `data/` 目录。

## 更新日志

版本变化与功能说明见[更新日志](/changelog)。上游 GPUI 主线化不等于 ZzClawTerm 的界面和交互会完全跟随上游，项目自己的品牌、X11、更新渠道和打包方式会继续独立维护。
