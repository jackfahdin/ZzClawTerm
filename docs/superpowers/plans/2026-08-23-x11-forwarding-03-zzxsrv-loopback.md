# ZzXsrv 回环绑定（M4a）实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在 ZzXsrv fork 上建立 Windows CI 构建链，对 xtrans 打回环绑定 patch（默认仅监听 127.0.0.1/::1），发布自有 release 产物，主仓库下载源切换并解除 Windows 端 X11 门禁。

**架构：** ZzXsrv（`github.com/jackfahdin/ZzXsrv`，工作副本在主仓库 `third_party/vcxsrv`，remote `zzxsrv`）承载全部构建与魔改：GitHub Actions windows-2022 runner 上按上游 `buildall.sh` 原链（MSVC + mhmake + WSL）先跑未修改基线，再打 `X11/xtrans/Xtranssock.c` 回环绑定 patch，同一 workflow 在 tag 推送时产出 NSIS noadmin 安装包并发 GitHub Release。主仓库只改三个发布物常量 + 删除门禁代码块，下载/安装机制（NSIS `/S /D=` 静默安装）不变。

**技术栈：** GitHub Actions（windows-2022）、WSL、MSVC 2022（v143）、mhmake、Cygwin 工具包、Strawberry Perl、NSIS、C（xorg 代码，非 C++）。

**前置事实（调研已实证，执行时不必重新调研）：**

- 构建入口：`./buildall.sh <1=64位> <并行数> <R=Release> [N=只编 server]`，在 WSL bash 中运行，源码必须在 Windows 盘 DrvFs 路径（大小写不敏感）
- 64 位 Release 产物：`xorg-server/obj64/servrelease/vcxsrv.exe`；NSIS 打包：`xorg-server/installer/packageall.sh nox86`（自动用 `noadmin.patch` 生成免管理员变体 `vcxsrv-64.<VERSION>.installer.noadmin.exe`）
- 版本号 21.1.16.1 分散在 6 个文件（`versionchanges.btm` 有清单），M4a **不改版本号**（NSI VERSION 不动，靠 release tag `zz-21.1.16.1-N` 区分自有构建）
- 未文档化的硬依赖：**winflexbison**（`c:\winflexbison\win_bison.exe`，被 `tools/mhmake/makebison.bat:6`、`xkbcomp/bison.bat:8` 等硬编码）和 **jom**（`buildall.sh:105,128` 编 openssl 用）
- `setenv.bat:5` 硬编码 VS2022 **Community** 路径，runner 预装的是 Enterprise → 必须改 vswhere 探测（任务 1 步骤 2）
- `setenv.sh` 硬编码 `/mnt/c/perl`、`/mnt/c/Python39`、`/mnt/c/nasm` → CI 用目录联接/复制复刻这些路径
- Cygwin 装完必须把 `C:\cygwin64\bin\link.exe` 改名（与 MSVC link.exe 冲突），并设 `CYGWIN=winsymlinks:lnk`
- Cygwin 源可能已无 `python27` 包；它只服务 `buildall.sh:48` 的 `which python.exe` 检查，可用 `cp python3.x.exe python.exe` 顶替
- runner 4 vCPU → 并行数取 5；全量构建（含 mesa 软渲染）预计 1-3 小时，`timeout-minutes` 放宽到 360
- 上游**没有任何现成 CI**，`docker/` 是 Windows 容器方案，GitHub runner 跑不了 Windows 容器，仅作依赖清单蓝本（`docker/Dockerfile`）
- patch 点实证：`X11/xtrans/Xtranssock.c:1372` `htonl(INADDR_ANY)`（IPv4）、`:1381` `in6addr_any`（IPv6），同在 `TRANS(SocketINETCreateListener)` 内（`#ifdef TCPCONN` 段）

**里程碑完成定义（对应规格 §十 M4a）：** CI 原链基线构建通过；patch 后产物冒烟断言仅监听 127.0.0.1:6000+N；主仓库下载源切至 ZzXsrv release、Windows 门禁解除；Windows 端 SSH 会话跑远端 GUI 程序人工验收通过。

---

### 任务 1：ZzXsrv CI 原链基线（未修改源码构建通过）

**文件：**
- 创建：`third_party/vcxsrv/.github/workflows/build.yml`
- 修改：`third_party/vcxsrv/setenv.bat`（VS 路径改 vswhere 探测）
- 修改：`third_party/vcxsrv/setenv.sh`（如 CI 路径复刻后与硬编码不一致才改，优先复刻路径不改文件）

**说明：** 本任务在 ZzXsrv 工作副本（`third_party/vcxsrv`）内操作，commit 后 `git push zzxsrv master`。全程**不改任何构建输入源码**，目标是在 CI 复刻上游手工构建。CI 迭代周期以「push → 看 Actions 日志 → 修 workflow」循环进行，失败优先看 `buildall.sh` 各阶段的 `which` 检查输出。

- [ ] **步骤 1：写 CI workflow**

创建 `third_party/vcxsrv/.github/workflows/build.yml`：

```yaml
name: build

on:
  push:
    branches: [master]
    tags: ['zz-*']
  workflow_dispatch:

jobs:
  build:
    runs-on: windows-2022
    timeout-minutes: 360
    steps:
      - name: git 行尾（必须在 checkout 前）
        run: git config --global core.autocrlf false

      - uses: actions/checkout@v4
        with:
          path: vcxsrv

      - name: 安装 Windows 侧构建依赖
        shell: powershell
        run: |
          choco install -y strawberryperl
          choco install -y jom nasm winflexbison3 nsis
          choco install -y python3 --version 3.9.13
          C:\Python39\python.exe -m pip install lxml mako

      - name: 复刻 setenv 硬编码路径
        shell: powershell
        run: |
          # setenv.sh 期望 /mnt/c/perl、/mnt/c/nasm、/mnt/c/winflexbison、/mnt/c/Python39
          New-Item -ItemType Junction -Path C:\perl -Target C:\Strawberry
          New-Item -ItemType Directory -Path C:\nasm -Force
          Copy-Item "C:\Program Files\NASM\nasm.exe" C:\nasm\
          New-Item -ItemType Directory -Path C:\winflexbison -Force
          Copy-Item "$env:ProgramData\chocolatey\lib\winflexbison3\tools\win_bison.exe" C:\winflexbison\
          Copy-Item "$env:ProgramData\chocolatey\lib\winflexbison3\tools\win_flex.exe" C:\winflexbison\
          Copy-Item "$env:ProgramData\chocolatey\lib\winflexbison3\tools\data" C:\winflexbison\ -Recurse -ErrorAction SilentlyContinue

      - name: 安装 Cygwin 工具包
        shell: powershell
        run: |
          choco install -y cygwin
          C:\tools\cygwin\cygwinsetup.exe -q -n -R C:\tools\cygwin -P bison,flex,gawk,gperf,gzip,nasm,sed,python38-lxml
          # cygwin link.exe 与 MSVC link.exe 冲突，改名
          Rename-Item C:\tools\cygwin\bin\link.exe link-cygwin.exe
          [Environment]::SetEnvironmentVariable('CYGWIN', 'winsymlinks:lnk', 'Machine')
          $env:CYGWIN = 'winsymlinks:lnk'

      - name: 安装 WSL（Ubuntu）
        shell: powershell
        run: |
          wsl --install -d Ubuntu-22.04 --no-launch
          wsl --set-default-version 1

      - name: 构建 vcxsrv（WSL + mhmake 原链）
        shell: powershell
        run: |
          $ws = (Get-Location).Path -replace '^([A-Za-z]):', { '/mnt/' + $_.Groups[1].Value.ToLower() } -replace '\\', '/'
          wsl -d Ubuntu-22.04 bash -lc "cd '$ws/vcxsrv' && ./buildall.sh 1 5 R"

      - name: 核验基线产物
        shell: powershell
        run: |
          $exe = "vcxsrv\xorg-server\obj64\servrelease\vcxsrv.exe"
          if (-not (Test-Path $exe)) { throw "vcxsrv.exe 未产出" }
          (Get-Item $exe).Length

      - uses: actions/upload-artifact@v4
        with:
          name: vcxsrv-baseline
          path: |
            vcxsrv/xorg-server/obj64/servrelease/vcxsrv.exe
            vcxsrv/xorg-server/installer/*.exe
          if-no-files-found: warn
```

注意（执行时按 CI 实际报错迭代，以下为已知风险点预案）：

- `wsl --install` 若报需重启/不可用：改用 `Vampire/setup-wsl` action（其处理了 runner 上 WSL 引导）；WSL1 即可且 DrvFs 性能优于 WSL2，**不要**追 WSL2
- Cygwin choco 包安装后 `cygwinsetup.exe` 路径若为 `C:\tools\cygwin\setup-x86_64.exe` 则按实际调整；包清单里 `python38-lxml` 若源里缺货，删掉它（Windows 侧 Python39 已有 lxml，`setenv.sh` 的 `PYTHON3` 指向 Windows Python）
- `buildall.sh:48` 的 `which python.exe` 若失败：在 Cygwin bin 里 `cp python3.8.exe python.exe`（追加到 Cygwin 步骤）
- `setenv.sh:10` 还引用 `/mnt/c/gnuwin32/bin`（遗留），先不加，报错再补

- [ ] **步骤 2：修 setenv.bat 的 VS 路径硬编码**

读 `third_party/vcxsrv/setenv.bat`，把硬编码的 `C:\Program Files\Microsoft Visual Studio\2022\Community\...vcvarsall.bat` 调用改为 vswhere 探测：

```bat
for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSINSTALL=%%i"
call "%VSINSTALL%\VC\Auxiliary\Build\vcvarsall.bat" %1
```

保持文件其余行不变（`setenv.py` 以 `cmd /c setenv.bat amd64` 调用并 diff 环境变量，接口不变）。

- [ ] **步骤 3：commit 并推送触发 CI**

```bash
cd third_party/vcxsrv
git add .github/workflows/build.yml setenv.bat
git commit -m "ci: 新增 GitHub Actions Windows 构建链

- 复刻上游手工构建链到 windows-2022 runner：VS2022(vswhere 探测)
  + Strawberry Perl + NASM + winflexbison + jom + Cygwin 工具包
  + Python3.9(lxml/mako) + WSL1 承载 buildall.sh
- setenv.bat 去除 VS Community 路径硬编码，改 vswhere 定位
  （runner 预装 Enterprise，原硬编码路径不存在）"
git push zzxsrv master
```

- [ ] **步骤 4：观察 CI 并迭代至绿**

```bash
gh run list --repo jackfahdin/ZzXsrv --limit 3
gh run watch --repo jackfahdin/ZzXsrv <run-id> --exit-status
```

失败时 `gh run view --repo jackfahdin/ZzXsrv <run-id> --log-failed` 定位，按步骤 1 的预案修。每次修复单独 commit（说明修的是哪个阶段）。**全量构建 1-3 小时，迭代要有耐心；若 mesa 阶段反复失败且与目标无关，允许临时在 buildall.sh 调用后注释 installer 阶段做二分，但基线任务完成定义是完整 `./buildall.sh 1 5 R` 绿。**

- [ ] **步骤 5：记录基线构建时长**

CI 绿后在 workflow 运行页记录全量耗时，写进 commit 或本文件执行记录（后续增量构建与缓存策略的参照）。

---

### 任务 2：xtrans 回环绑定 patch + CI 监听断言

**文件：**
- 修改：`third_party/vcxsrv/X11/xtrans/Xtranssock.c:1287-1406`（`TRANS(SocketINETCreateListener)`）
- 修改：`third_party/vcxsrv/.github/workflows/build.yml`（构建 job 末尾加冒烟步骤）

**设计：** 默认绑定回环（127.0.0.1 / ::1），环境变量 `ZZXSRV_LISTEN_ANY=1` 恢复全网卡（与主程序既有 `ZZCLAWTERM_X11_ALLOW_ANY_BIND` 旁路语义对应，仅限隔离调试）。这是 ZzXsrv 与上游的唯一行为差异，保持最小侵入。

- [ ] **步骤 1：写冒烟测试脚本（先于 patch，验证其在官方行为下的反例）**

在 workflow 的「核验基线产物」之后追加（本步骤先入 CI，对未 patch 基线应表现为：存在 `0.0.0.0:6099` 监听——即反例成立；patch 后断言翻转）：

```yaml
      - name: 冒烟：监听地址断言
        shell: powershell
        run: |
          $exe = "$PWD\vcxsrv\xorg-server\obj64\servrelease\vcxsrv.exe"
          $p = Start-Process -FilePath $exe -ArgumentList ':99','-multiwindow','-ac' -PassThru
          Start-Sleep -Seconds 10
          $listen = netstat -ano | Select-String ':6099\s+.*LISTENING'
          $listen | ForEach-Object { $_.Line }
          $hasAny = [bool]($listen | Select-String '0\.0\.0\.0:6099')
          $hasLoop = [bool]($listen | Select-String '127\.0\.0\.1:6099')
          if ($p.HasExited) { throw "vcxsrv 进程过早退出，代码 $($p.ExitCode)" }
          Stop-Process -Id $p.Id -Force
          if ($env:ZZXSRV_LISTEN_ANY -eq '1') {
            if (-not $hasAny) { throw "旁路模式应监听 0.0.0.0:6099" }
          } else {
            if ($hasAny) { throw "默认模式不得监听 0.0.0.0:6099" }
            if (-not $hasLoop) { throw "默认模式应监听 127.0.0.1:6099" }
          }
```

风险预案：若 runner 会话无法创建 GUI 窗口导致 vcxsrv 起不来（进程过早退出），把该步骤改为 `continue-on-error: true` 并记录，最终以任务 5 的 Windows 人工验收为准（规格 §7.3 已有此验收模型）。

- [ ] **步骤 2：确认反例后打 patch**

先跑一次 CI 确认未 patch 时冒烟输出显示 `0.0.0.0:6099`（反例成立，证明断言有效）。然后修改 `X11/xtrans/Xtranssock.c`：在 `TRANS(SocketINETCreateListener)` 函数前（约 1285 行处，`#ifdef TCPCONN` 段内）加 helper，并改两个绑定点：

```c
/*
 * ZzXsrv patch: 默认仅绑定回环地址（安全基线，ZzClawTerm 内嵌场景
 * 只接受本机 SSH 转发回连）。设置环境变量 ZZXSRV_LISTEN_ANY=1 恢复
 * 上游全网卡监听行为（仅限隔离调试，不得用于交付形态）。
 */
static int
TRANS(SocketINETListenAny) (void)
{
    static int cached = -1;
    if (cached < 0) {
        const char *env = getenv("ZZXSRV_LISTEN_ANY");
        cached = (env && env[0] == '1') ? 1 : 0;
    }
    return cached;
}
```

第 1372 行：

```c
	((struct sockaddr_in *)&sockname)->sin_addr.s_addr =
	    htonl(TRANS(SocketINETListenAny)() ? INADDR_ANY : INADDR_LOOPBACK);
```

第 1381 行（`#ifdef IPv6` 分支）：

```c
	((struct sockaddr_in6 *)&sockname)->sin6_addr =
	    TRANS(SocketINETListenAny)() ? in6addr_any : in6addr_loopback;
```

若编译报 `getenv` 未声明，在文件头 include 区补 `#include <stdlib.h>`（先看是否已有）。

- [ ] **步骤 3：commit 并推送**

```bash
cd third_party/vcxsrv
git add X11/xtrans/Xtranssock.c .github/workflows/build.yml
git commit -m "feat: xtrans 默认回环绑定，ZZXSRV_LISTEN_ANY=1 旁路

- SocketINETCreateListener 的 IPv4 INADDR_ANY / IPv6 in6addr_any
  硬编码改为默认绑定 127.0.0.1 / ::1
- 背景：官方二进制无地址绑定 CLI，-listen tcp 必然监听全网卡，
  不满足 ZzClawTerm 交付安全基线（2026-08-23 用户裁决）
- CI 新增 netstat 冒烟断言：默认模式仅 127.0.0.1:6099，
  旁路模式恢复 0.0.0.0:6099"
git push zzxsrv master
```

- [ ] **步骤 4：CI 验证断言通过**

`gh run watch` 等绿，确认日志里冒烟步骤输出 `127.0.0.1:6099 ... LISTENING` 且无 `0.0.0.0:6099`。增量构建（`buildall.sh` 按 obj64 增量）应显著快于基线，记录耗时。

---

### 任务 3：release 流水线（tag 触发打包发布）

**文件：**
- 修改：`third_party/vcxsrv/.github/workflows/build.yml`（追加 release job，复用构建产物）

**设计：** 同一 workflow 追加 `release` job，`if: startsWith(github.ref, 'refs/tags/zz-')`，依赖 build job 成功后从其产出打包。打包走上游现成 `xorg-server/installer/packageall.sh nox86`（NSIS，含 noadmin 变体——与主仓库下载器的 NSIS 静默安装机制完全兼容，下载器零改动）。

- [ ] **步骤 1：build job 末尾追加打包步骤（仅 tag 时执行）**

```yaml
      - name: NSIS 打包（tag 构建）
        if: startsWith(github.ref, 'refs/tags/zz-')
        shell: powershell
        run: |
          $ws = (Get-Location).Path -replace '^([A-Za-z]):', { '/mnt/' + $_.Groups[1].Value.ToLower() } -replace '\\', '/'
          wsl -d Ubuntu-22.04 bash -lc "cd '$ws/vcxsrv/xorg-server/installer' && ./packageall.sh nox86"

      - name: 计算 SHA256（tag 构建）
        if: startsWith(github.ref, 'refs/tags/zz-')
        shell: powershell
        run: |
          Get-ChildItem vcxsrv\xorg-server\installer\*.exe | ForEach-Object {
            $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
            "$h  $($_.Name)" | Out-File -Encoding ascii "$($_.FullName).sha256"
          }
```

- [ ] **步骤 2：追加 release job**

```yaml
  release:
    needs: build
    if: startsWith(github.ref, 'refs/tags/zz-')
    runs-on: windows-2022
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: vcxsrv-baseline
          path: dist
      - name: 收集安装包
        shell: powershell
        run: |
          # build job 的 upload-artifact 已含 installer/*.exe；
          # 此处把 installer 产物与 sha256 归置到 dist/
          New-Item -ItemType Directory -Path dist -Force
          Copy-Item vcxsrv\xorg-server\installer\*.exe* dist\ -ErrorAction SilentlyContinue
      - name: 发布 GitHub Release
        shell: powershell
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          $tag = $env:GITHUB_REF_NAME
          gh release create $tag dist\* --repo jackfahdin/ZzXsrv `
            --title "ZzXsrv $tag（回环绑定构建）" `
            --notes "基于 vcxsrv 21.1.16.1 + xtrans 回环绑定 patch。默认仅监听 127.0.0.1/::1；ZZXSRV_LISTEN_ANY=1 恢复全网卡（仅限隔离调试）。SHA256 见各 .sha256 文件。"
```

注意：release job 与 build job 同 runner 文件系统不共享，安装包须走 artifact 传递——把 build job 的 upload-artifact `path` 扩到 `vcxsrv/xorg-server/installer/*.exe*`（含 .sha256），release job 下载后直接发布。执行时按 artifact 实际内容调整收集步骤，**禁止占位**。

- [ ] **步骤 3：打 tag 触发首发布**

```bash
cd third_party/vcxsrv
git tag zz-21.1.16.1-1
git push zzxsrv zz-21.1.16.1-1
gh run watch --repo jackfahdin/ZzXsrv --exit-status
gh release view zz-21.1.16.1-1 --repo jackfahdin/ZzXsrv
```

确认 release 资产含 `vcxsrv-64.21.1.16.1.installer.noadmin.exe` 及其 `.sha256`。

- [ ] **步骤 4：记录 noadmin 包 SHA256**

```bash
gh release download zz-21.1.16.1-1 --repo jackfahdin/ZzXsrv \
  --pattern "vcxsrv-64.21.1.16.1.installer.noadmin.exe.sha256" --dir /tmp/zzxsrv-rel
cat /tmp/zzxsrv-rel/*.sha256
```

记下 SHA256 与文件字节数，任务 4 步骤 1 使用。

---

### 任务 4：主仓库下载源切换 + 解除 Windows 门禁

**文件：**
- 修改：`src/x11/ZzXServerDownloader.h:40-65`（发布物常量 + 头注释）
- 修改：`src/transport/ZzSshTransport.cpp:264-289`（删门禁块，恢复下载链路为默认路径）
- 测试：`tests/` 下引用 `ZzXServerRelease` 常量或门禁提示文案的用例（执行时 `grep -rn "ZZCLAWTERM_X11_ALLOW_ANY_BIND\|ZzXServerRelease\|回环绑定就绪" tests/` 定位并按新行为更新）
- 文档：`README.md` X11 特性说明、规格 `docs/superpowers/specs/2026-08-22-x11-forwarding-design.md` §5.1/§十

**说明：** 下载器机制（NSIS `/S /D=` 静默安装 + staging 原子换入）完全复用，只换常量。`kVersion` 改为 `21.1.16.1-zz1` 使已装官方版的用户触发重装（版本串不等即视为未安装）。

- [ ] **步骤 1：更新发布物常量**

`src/x11/ZzXServerDownloader.h` 的 `namespace ZzXServerRelease`：

```cpp
/// ZzXsrv 自有构建版本标识（基于 vcxsrv 21.1.16.1 + xtrans 回环绑定 patch）。
inline constexpr char kVersion[] = "21.1.16.1-zz1";

/// ZzXsrv release 的 64 位免管理员 NSIS 安装包下载 URL。
inline constexpr char kUrl[] =
    "https://github.com/jackfahdin/ZzXsrv/releases/download/zz-21.1.16.1-1/"
    "vcxsrv-64.21.1.16.1.installer.noadmin.exe";

/// kUrl 所指安装包的 SHA256（ZzXsrv CI 发布物随附 .sha256，任务 3 步骤 4 实测值）。
inline constexpr char kSha256[] = "<任务 3 步骤 4 的实测 SHA256>";
```

同步把 :40-48 的头注释（官方发布物决策说明）改写为 ZzXsrv 自有构建说明：默认回环绑定、来源 release tag、`ZZXSRV_LISTEN_ANY` 旁路语义。`<...>` 处填入任务 3 步骤 4 的真实值——**这是数据回填不是占位符，执行时必须先拿到值再 commit。**

- [ ] **步骤 2：解除门禁**

`src/transport/ZzSshTransport.cpp`：把 265-277 行的门禁块（含 `ZZCLAWTERM_X11_ALLOW_ANY_BIND` 判断、提示文案、注释）整体删除，使 Windows 直接走 278-289 行的 downloader 链路。删除后该段为：

```cpp
    m_x11Cookie = m_x11Authority.generateCookie();
#if defined(Q_OS_WIN)
    // 按需下载/校验/安装 ZzXsrv（回环绑定构建），就绪后经 onX11ServerReady 续接 openShell
    if (!m_x11Downloader) {
        m_x11Downloader = new ZzXServerDownloader(this);
        // ... 原有 connect 与 ensureAvailable 调用原样保留
```

- [ ] **步骤 3：更新受影响测试与文档**

- `grep -rn "ZZCLAWTERM_X11_ALLOW_ANY_BIND\|回环绑定就绪" tests/ src/`：凡断言「门禁提示」或依赖旁路环境变量的用例，更新为断言默认走 downloader 链路
- `README.md`：X11 特性说明中「Windows 端待回环绑定」类表述改为「Windows 端内建 ZzXsrv（回环绑定）」
- 规格 §5.1 监听条目与 §十 M3/M4a 完成定义：标注 M4a 已交付（日期 + commit）

- [ ] **步骤 4：全量回归**

```bash
cmake --build --preset linux-gcc-release && ctest --preset linux-gcc-release
git checkout -- tests/perf/records/   # 全量 ctest 覆写已入库记录，跑完恢复
```

基线 43 项全绿方可进入下一步。Linux 上 Windows 专属代码不编译，回归验证的是改动未误伤共享路径（profile/传输装配）。

- [ ] **步骤 5：Commit**

```bash
git add src/x11/ZzXServerDownloader.h src/transport/ZzSshTransport.cpp tests/ README.md docs/
git commit -m "feat(x11): Windows 端 X11 转发切换至 ZzXsrv 回环绑定构建并解除门禁

- 下载源常量从官方 vcxsrv 切至 ZzXsrv release（zz-21.1.16.1-1），
  SHA256 同步更新；kVersion 改 21.1.16.1-zz1 触发既有官方版重装
- 删除 ZzSshTransport 的 Windows 门禁块与 ZZCLAWTERM_X11_ALLOW_ANY_BIND
  旁路，downloader→manager→bridge 链路恢复为默认路径
- ZzXsrv 构建默认仅监听 127.0.0.1/::1，满足 2026-08-23 安全裁决"
```

（按惯例 push 前先问用户确认。）

---

### 任务 5：回归收口与 Windows 人工验收

**文件：**
- 修改：`docs/superpowers/specs/2026-08-22-x11-forwarding-design.md`（M4a 完成定义逐项核销）

- [ ] **步骤 1：确认 ZzSshCore 无改动**

M4a 不涉及库代码；`git -C third_party/ZzSshCore status` 应为干净。若意外有改动，查明原因还原。

- [ ] **步骤 2：规格核销**

把 §十 M4a 完成定义逐条标注达成证据（CI run 链接、冒烟日志摘录、主仓库 commit 号）。

- [ ] **步骤 3：Windows 人工验收清单（交给用户执行）**

1. Windows 机上安装 CI 构建的 ZzClawTerm（或现有安装 + 覆盖更新）
2. 会话 profile 勾选 X11 转发 → 连接 SSH → 状态栏无 X11 错误提示
3. 远端跑 `xclock` / `xeyes` → 窗口在 Windows 桌面显示
4. `netstat -ano | findstr :600` 确认仅 `127.0.0.1:6000+N`（无 `0.0.0.0`）
5. 关闭标签页 → vcxsrv 进程随之退出（任务管理器确认无残留）

- [ ] **步骤 4：验收通过后规划 M4b**

M4b（裁剪品牌化 + rootful 嵌入）另行头脑风暴与立项，不在本计划范围。

---

## 自检记录

**规格覆盖度：** M4a 里程碑定义（规格 §二）= fork 建仓（已完成，计划外）→ CI 原链基线（任务 1）→ xtrans 回环绑定 patch（任务 2）→ CI 产物发 release（任务 3）→ 主仓库下载源切换 + 解除门禁（任务 4）→ 人工验收（任务 5）。§十 M4a 完成定义四条均有对应任务。§3.1 已注明 M4a 不引入 submodule。

**占位符扫描：** 任务 4 步骤 1 的 SHA256 为执行期数据回填（获取命令在任务 3 步骤 4），非设计占位。任务 1 的 CI 迭代预案均给出具体命令。

**类型/名称一致性：** 环境变量名全程统一 `ZZXSRV_LISTEN_ANY`（ZzXsrv 侧）/ `ZZCLAWTERM_X11_ALLOW_ANY_BIND`（主仓库侧，本计划删除）；tag 命名 `zz-21.1.16.1-1` 与 release 资产名 `vcxsrv-64.21.1.16.1.installer.noadmin.exe` 在任务 3/4 间一致；`kVersion` = `21.1.16.1-zz1`。
