# 构建 ZzClawTerm (Windows / MSVC)
# 用法: powershell -ExecutionPolicy Bypass -File scripts/build_windows.ps1 [-BuildType Release]
param(
    [string]$BuildType = "Release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

cmake -S $root -B "$root/build" -DCMAKE_BUILD_TYPE=$BuildType
cmake --build "$root/build" --config $BuildType --parallel

Write-Host "构建完成: $root/build"
