# 打包说明

MVP (v0.1.0) 各平台打包流程。先完成 Release 构建, 再执行对应部署。

## Windows (NSIS + 便携 ZIP)

```powershell
scripts/build_windows.ps1 -BuildType Release
windeployqt build/ZzClawTerm.exe          # 收集 Qt 运行时依赖
# 便携版: 压缩 build 下含 exe 与依赖的目录为 ZzClawTerm-0.1.0-win64.zip
# 安装包: 用 NSIS 脚本 (后续添加 installer.nsi) 生成 setup.exe
```

## Linux (AppImage + deb)

```bash
scripts/build_unix.sh Release
# AppImage: 使用 linuxdeploy + linuxdeploy-plugin-qt
#   linuxdeploy --appdir AppDir -e build/ZzClawTerm --plugin qt --output appimage
# deb: 使用 cpack -G DEB (后续在 CMake 中配置 CPack)
```

## macOS (DMG)

```bash
scripts/build_unix.sh Release
macdeployqt build/ZzClawTerm.app -dmg     # 生成自带依赖的 .app 与 .dmg
```

## 后续

将 CPack 配置纳入 CMake, 由 CI (`.github/workflows/release.yml`) 在打 tag 时自动产出三端安装包。
