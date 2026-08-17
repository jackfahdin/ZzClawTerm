# ZzClawTerm Windows 打包：Release 构建 → windeployqt 收集依赖 → zip 绿色包
# 用法：pwsh scripts/package-windows.ps1 [-QtRoot "D:\Qt\6.8.2\msvc2022_64"]
# 前置条件：third_party/openssl 子模块已初始化（vendored 预编译 bundle，构建默认
#       静态链接，无需额外拷贝 OpenSSL DLL）；若 bundle 缺失则回退系统 OpenSSL，
#       需自行保证其可被 find_package 找到并处理运行时分发
# 注意：本脚本在 Windows 平台人工验收时执行，当前未实机验证
param(
    [string]$QtRoot = $env:QT_ROOT
)
$ErrorActionPreference = "Stop"
if (-not $QtRoot) { throw "请通过 -QtRoot 或 QT_ROOT 环境变量提供 Qt 前缀" }

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cmake --preset windows-msvc2022-release
    cmake --build --preset windows-msvc2022-release

    $dist = "$root/dist/windows"
    if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
    New-Item -ItemType Directory -Force $dist | Out-Null
    Copy-Item "$root/build/windows-msvc2022-release/src/Release/ZzClawTerm.exe" $dist

    & "$QtRoot/bin/windeployqt.exe" --release --no-translations `
        --compiler-runtime "$dist/ZzClawTerm.exe"
    if ($LASTEXITCODE -ne 0) { throw "windeployqt 失败" }

    Compress-Archive -Path "$dist/*" `
        -DestinationPath "$root/dist/ZzClawTerm-v0.1-windows-x64.zip" -Force
    Write-Host "产出：dist/ZzClawTerm-v0.1-windows-x64.zip"
} finally {
    Pop-Location
}
