# ZzSshCore 库 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 新建独立仓库 ZzSshCore，实现 libssh2 的 C++20/Qt 6 异步封装库，提供 `ZzSshConnection`（连接生命周期、agent→公钥→密码认证、keepalive、主机密钥验证）与 `ZzSshShellChannel`（shell 收发、resize），并附带完整 QTest 单元测试与 Docker 集成测试。

**架构：** 每个 SSH 连接独占一个 `QThread` 工作线程，libssh2 以阻塞模式在该线程内串行调用；GUI 线程与工作线程之间只通过 Qt 信号槽（queued connection）通信；密码索取与主机密钥确认通过 `QWaitCondition` 阻塞等待上层决策。所有 C 资源（session / channel / socket）以 C++20 RAII 封装，析构即释放。传输层抽象为 `ZzSshTransport` 接口，单元测试用 `ZzMockTransport`，生产用 `ZzTcpTransport`（原生 socket，阻塞语义不被 Qt 缓冲干扰）。

**技术栈：** C++20 / Qt 6.8+（仅 Core + Network，不依赖 Widgets、不依赖 QCoro）/ CMake 3.25+（CMakePresets.json）/ libssh2（CMake 移植版，OpenSSL 后端，add_subdirectory 子模块）/ QTest（Qt6::Test）/ Docker（openssh-server 集成测试）。

**前置条件：**
- 本机已安装 Qt 6.8+（本机路径写入未跟踪的 `CMakeUserPresets.json`，禁止提交进仓库）、CMake 3.25+、Ninja、Docker。
- ZzSshCore 是一个**新独立仓库**。任务 1 会执行 `git init` 与 `git submodule add`。以下所有相对路径均相对于 ZzSshCore 仓库根目录。
- libssh2 CMake 移植版地址按规格 §十一：`https://gitcode.com/JackfahdinImport/libssh2`（gitcode 导入的 CMake 移植版；clone 后确认是 libssh2 而非其他库再继续，CMake 中的 target 名探测逻辑已做双分支兜底）。其加密后端 OpenSSL 使用 `https://gitcode.com/ZzThirdParty/openssl`（当前缺 macOS 构建）。
- 示例命令统一使用 `linux-release` preset；Windows/macOS 执行时替换为 `windows-release` / `macos-release`。

**性能门控（规格 §9.1，硬性）：** 任务 13 的性能测试进 ctest，仅 Release 构建数字有效，阈值失败即测试失败；结果写入 `tests/perf/records/YYYY-MM-DD-zzsshcore.json`（含阈值、实测值、环境信息、git commit hash）并提交仓库。

---

## 文件结构

| 文件 | 职责 |
| ---- | ---- |
| `CMakeLists.txt` | 顶层构建：C++20、Qt Core/Network、libssh2 子模块（OpenSSL 后端）、`zzsshcore` 静态库目标 |
| `CMakePresets.json` | 共享构建矩阵（三平台 × Debug/Release），提交仓库 |
| `CMakeUserPresets.json.example` | 本机路径模板（Qt SDK），不提交真实文件 |
| `.gitignore` | 忽略 build/、CMakeUserPresets.json 等 |
| `src/ZzSshCore.h/.cpp` | 库级信息（版本号、libssh2 全局初始化） |
| `src/ZzSshError.h/.cpp` | 错误码枚举 `ZzSshErrorCode` 与中文描述映射（含 libssh2 负数透传码） |
| `src/ZzSshTransport.h` | 传输层抽象接口（open/waitReadable/abortBlocking/close/socketDescriptor） |
| `src/ZzTcpTransport.h/.cpp` | 生产传输层：原生 socket（POSIX/Winsock），阻塞 connect 带超时 |
| `src/ZzSshSession.h/.cpp` | `LIBSSH2_SESSION` 的 RAII 封装（握手、主机密钥、keepalive） |
| `src/ZzSshChannel.h/.cpp` | `LIBSSH2_CHANNEL` 的 RAII 封装（openShell/read/write/resize/EOF） |
| `src/ZzSshAuthConfig.h` | 认证配置结构体（agent 开关、密钥路径、密码开关） |
| `src/ZzSshConnectParams.h` | 一次连接的全部参数结构体 |
| `src/ZzSshHostKeyStore.h/.cpp` | known_hosts.json 存取、验证（Trusted/Unknown/Changed）、SHA256 指纹 |
| `src/ZzSshConnectionShared.h/.cpp` | GUI 线程与工作线程的共享状态（密码/主机密钥决策的 QWaitCondition、abort 原子标志） |
| `src/ZzSshConnectionWorker.h/.cpp` | 工作线程内的执行体：连接、认证、主机密钥验证、channel 读取泵、keepalive |
| `src/ZzSshConnection.h/.cpp` | 对外的连接对象（GUI 线程），信号槽 API，持有 QThread 与 worker |
| `src/ZzSshShellChannel.h/.cpp` | 对外的 shell 通道对象（GUI 线程），转发调用到 worker |
| `tests/CMakeLists.txt` | 测试注册函数 `zz_add_test` 与各测试目标 |
| `tests/helpers/ZzMockTransport.h` | 单元测试用脚本化 mock 传输层（header-only） |
| `tests/unit/tst_*.cpp` | QTest 单元测试（无网络或仅 loopback） |
| `tests/integration/ZzSshTestServerConfig.h/.cpp` | 集成测试公共配置（环境变量读取）与辅助函数 |
| `tests/integration/tst_*IT.cpp` | Docker openssh-server 集成测试 |
| `tests/integration/docker/Dockerfile` | 测试用 openssh-server 镜像（用户 zztest、密码/密钥认证、构建期固定主机密钥） |
| `tests/integration/docker/entrypoint.sh` | 容器入口（支持 `ZZ_REGEN_HOSTKEYS=1` 重新生成主机密钥） |
| `tests/integration/docker/run-integration-tests.sh` | 一键构建镜像、起容器、跑 ctest、清理 |
| `tests/integration/docker/keys/id_ed25519{,.pub}` | 测试专用密钥对（任务 7 生成后提交，仅用于测试容器） |
| `tests/perf/tst_ZzSshPerf.cpp` | 性能门控测试（连接耗时、shell 吞吐），写 JSON 记录 |
| `tests/perf/records/` | 性能历史记录目录（含 `.gitkeep`，记录文件按日期追加） |

依赖方向：`ZzSshConnection` → `ZzSshConnectionWorker` → (`ZzSshSession` / `ZzSshChannel` / `ZzSshTransport`) → libssh2。`ZzSshShellChannel` 只持有连接指针并转发调用。

---

### 任务 1：仓库骨架与构建系统

**文件：**
- 创建：`.gitignore`
- 创建：`CMakeLists.txt`
- 创建：`CMakePresets.json`
- 创建：`CMakeUserPresets.json.example`
- 创建：`src/ZzSshCore.h`、`src/ZzSshCore.cpp`
- 创建：`tests/CMakeLists.txt`
- 创建：`tests/unit/tst_ZzSshCore.cpp`

- [ ] **步骤 1：初始化仓库并添加 libssh2 子模块**

在 ZzSshCore 仓库根目录执行：

```bash
git init
git submodule add https://gitcode.com/JackfahdinImport/libssh2 third_party/libssh2
git submodule update --init --recursive
ls third_party/libssh2/CMakeLists.txt
```

预期：子模块检出成功，最后一行输出 `third_party/libssh2/CMakeLists.txt`。若该地址不是 libssh2 的 CMake 移植版，停止并向主会话确认正确地址。

- [ ] **步骤 2：创建 `.gitignore`**

```gitignore
# 构建产物
build/
out/

# 本机 preset（含本机绝对路径，禁止提交）
CMakeUserPresets.json

# IDE
.user
.idea/
.vs/
```

- [ ] **步骤 3：创建 `CMakeLists.txt`**

```cmake
cmake_minimum_required(VERSION 3.25)
project(ZzSshCore VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "构建类型" FORCE)
endif()

option(ZZSSHCORE_BUILD_TESTS "构建 ZzSshCore 测试" ON)
option(ZZSSHCORE_USE_SYSTEM_LIBSSH2 "使用系统 libssh2 而非 third_party 子模块" OFF)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Network)
find_package(OpenSSL REQUIRED)

if(ZZSSHCORE_USE_SYSTEM_LIBSSH2)
    find_package(libssh2 REQUIRED)
    set(ZZSSHCORE_LIBSSH2_TARGET libssh2::libssh2)
else()
    if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/libssh2/CMakeLists.txt")
        message(FATAL_ERROR "third_party/libssh2 子模块不存在，请先执行 git submodule update --init --recursive")
    endif()
    # libssh2 使用 OpenSSL 作为加密后端（规格 §十一）
    set(CRYPTO_BACKEND "OpenSSL" CACHE STRING "libssh2 加密后端" FORCE)
    set(BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
    set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
    add_subdirectory(third_party/libssh2)
    if(TARGET libssh2::libssh2)
        set(ZZSSHCORE_LIBSSH2_TARGET libssh2::libssh2)
    elseif(TARGET libssh2)
        set(ZZSSHCORE_LIBSSH2_TARGET libssh2)
    else()
        message(FATAL_ERROR "libssh2 子项目未导出 libssh2 或 libssh2::libssh2 目标，请检查移植版的 CMake target 名")
    endif()
endif()

add_library(zzsshcore STATIC
    src/ZzSshCore.h
    src/ZzSshCore.cpp
)
target_include_directories(zzsshcore PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/src")
target_link_libraries(zzsshcore
    PUBLIC Qt6::Core Qt6::Network ${ZZSSHCORE_LIBSSH2_TARGET}
)
# 偏差说明（执行后同步）：libssh2 为 PUBLIC——公开头 ZzSshSession.h/ZzSshChannel.h
# 直接暴露 LIBSSH2_* 类型，PRIVATE 会导致消费方编译失败。
if(WIN32)
    target_link_libraries(zzsshcore PRIVATE ws2_32)
endif()

# === 源文件（后续任务按 target_sources 追加） ===

if(ZZSSHCORE_BUILD_TESTS)
    enable_testing()
    find_package(Qt6 6.8 REQUIRED COMPONENTS Test)
    add_subdirectory(tests)
endif()
```

- [ ] **步骤 4：创建 `CMakePresets.json`**

```json
{
  "version": 4,
  "cmakeMinimumRequired": { "major": 3, "minor": 25, "patch": 0 },
  "configurePresets": [
    {
      "name": "base",
      "hidden": true,
      "generator": "Ninja",
      "binaryDir": "${sourceDir}/build/${presetName}",
      "cacheVariables": { "CMAKE_EXPORT_COMPILE_COMMANDS": "ON" }
    },
    {
      "name": "linux-debug",
      "inherits": "base",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Linux" },
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug" }
    },
    {
      "name": "linux-release",
      "inherits": "base",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Linux" },
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" }
    },
    {
      "name": "windows-debug",
      "inherits": "base",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Windows" },
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug" }
    },
    {
      "name": "windows-release",
      "inherits": "base",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Windows" },
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" }
    },
    {
      "name": "macos-debug",
      "inherits": "base",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Darwin" },
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug" }
    },
    {
      "name": "macos-release",
      "inherits": "base",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Darwin" },
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" }
    }
  ],
  "buildPresets": [
    { "name": "linux-debug", "configurePreset": "linux-debug" },
    { "name": "linux-release", "configurePreset": "linux-release" },
    { "name": "windows-debug", "configurePreset": "windows-debug" },
    { "name": "windows-release", "configurePreset": "windows-release" },
    { "name": "macos-debug", "configurePreset": "macos-debug" },
    { "name": "macos-release", "configurePreset": "macos-release" }
  ],
  "testPresets": [
    { "name": "linux-debug", "configurePreset": "linux-debug", "output": { "outputOnFailure": true } },
    { "name": "linux-release", "configurePreset": "linux-release", "output": { "outputOnFailure": true } },
    { "name": "windows-debug", "configurePreset": "windows-debug", "output": { "outputOnFailure": true } },
    { "name": "windows-release", "configurePreset": "windows-release", "output": { "outputOnFailure": true } },
    { "name": "macos-debug", "configurePreset": "macos-debug", "output": { "outputOnFailure": true } },
    { "name": "macos-release", "configurePreset": "macos-release", "output": { "outputOnFailure": true } }
  ]
}
```

- [ ] **步骤 5：创建 `CMakeUserPresets.json.example`**

```json
{
  "version": 4,
  "configurePresets": [
    {
      "name": "local-release",
      "displayName": "本机 Release（示例：填入你的 Qt 路径）",
      "inherits": "linux-release",
      "cacheVariables": {
        "CMAKE_PREFIX_PATH": "/path/to/Qt/6.8.x/gcc_64"
      }
    }
  ]
}
```

- [ ] **步骤 6：创建 `src/ZzSshCore.h` 与 `src/ZzSshCore.cpp`**

`src/ZzSshCore.h`：

```cpp
#pragma once

#include <QString>

/**
 * @brief ZzSshCore 库级信息与全局初始化。
 */
namespace ZzSshCore {

/**
 * @brief 返回库版本号。
 * @return 版本字符串，如 "0.1.0"。
 */
QString version();

/**
 * @brief 返回底层 libssh2 的版本字符串。
 * @return 版本字符串；libssh2 未正确链接时返回空字符串。
 */
QString libssh2Version();

/**
 * @brief 执行 libssh2 全局初始化（进程内仅一次，线程安全）。
 * @return 初始化成功返回 true。
 */
bool globalInit();

} // namespace ZzSshCore
```

`src/ZzSshCore.cpp`：

```cpp
#include "ZzSshCore.h"

#include <libssh2.h>

#include <mutex>

QString ZzSshCore::version()
{
    return QStringLiteral("0.1.0");
}

QString ZzSshCore::libssh2Version()
{
    const char *v = libssh2_version(0);
    return v ? QString::fromLatin1(v) : QString();
}

bool ZzSshCore::globalInit()
{
    static std::once_flag flag;
    static bool ok = false;
    std::call_once(flag, [] { ok = (libssh2_init(0) == 0); });
    return ok;
}
```

- [ ] **步骤 7：编写冒烟测试 `tests/CMakeLists.txt` 与 `tests/unit/tst_ZzSshCore.cpp`**

`tests/CMakeLists.txt`：

```cmake
# 注册一个 QTest 测试可执行文件并加入 ctest
function(zz_add_test name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE zzsshcore Qt6::Core Qt6::Network Qt6::Test)
    target_include_directories(${name} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/helpers")
    add_test(NAME ${name} COMMAND ${name})
endfunction()

zz_add_test(tst_ZzSshCore unit/tst_ZzSshCore.cpp)
set_tests_properties(tst_ZzSshCore PROPERTIES LABELS "unit")
```

`tests/unit/tst_ZzSshCore.cpp`：

```cpp
#include <QtTest>

#include "ZzSshCore.h"

/**
 * @brief 冒烟测试：验证库与 libssh2 链接、初始化正常。
 */
class tst_ZzSshCore : public QObject
{
    Q_OBJECT

private slots:
    void versionString();
    void libssh2Linked();
    void globalInitSucceeds();
};

void tst_ZzSshCore::versionString()
{
    QCOMPARE(ZzSshCore::version(), QStringLiteral("0.1.0"));
}

void tst_ZzSshCore::libssh2Linked()
{
    QVERIFY(!ZzSshCore::libssh2Version().isEmpty());
}

void tst_ZzSshCore::globalInitSucceeds()
{
    QVERIFY(ZzSshCore::globalInit());
}

QTEST_GUILESS_MAIN(tst_ZzSshCore)
#include "tst_ZzSshCore.moc"
```

- [ ] **步骤 8：配置、构建并运行测试**

先复制本机 preset 并填入 Qt 路径（`CMakeUserPresets.json` 不入库）：

```bash
cp CMakeUserPresets.json.example CMakeUserPresets.json
# 编辑 CMakeUserPresets.json，把 CMAKE_PREFIX_PATH 改为本机 Qt 6.8+ 路径
cmake --preset linux-release
cmake --build --preset linux-release
ctest --preset linux-release
```

预期：配置与构建零错误；ctest 输出 `1/1 Test #1: tst_ZzSshCore ... Passed`，末尾 `100% tests passed, 0 tests failed out of 1`。若 Qt 在系统路径中可直接用 `linux-release`；否则用 `--preset local-release`。

- [ ] **步骤 9：Commit**

```bash
git add .gitignore CMakeLists.txt CMakePresets.json CMakeUserPresets.json.example .gitmodules third_party/libssh2 src/ZzSshCore.h src/ZzSshCore.cpp tests/CMakeLists.txt tests/unit/tst_ZzSshCore.cpp
git commit -m "feat: 初始化 ZzSshCore 仓库骨架与 CMake 构建系统"
```

---

### 任务 2：ZzSshError 错误码与描述映射

**文件：**
- 创建：`src/ZzSshError.h`、`src/ZzSshError.cpp`
- 测试：`tests/unit/tst_ZzSshError.cpp`
- 修改：`CMakeLists.txt`（追加 target_sources）
- 修改：`tests/CMakeLists.txt`（追加测试注册）

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSshError.cpp`**

```cpp
#include <QtTest>

#include "ZzSshError.h"

/**
 * @brief ZzSshError 错误码与中文描述映射的单元测试。
 */
class tst_ZzSshError : public QObject
{
    Q_OBJECT

private slots:
    void noErrorMessage();
    void customCodeMessages();
    void libssh2KnownCode();
    void libssh2UnknownCodeFallback();
    void negativeCodeDispatchesToLibssh2();
};

void tst_ZzSshError::noErrorMessage()
{
    QCOMPARE(ZzSshError::message(0), QStringLiteral("无错误"));
}

void tst_ZzSshError::customCodeMessages()
{
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::TransportError)),
             QStringLiteral("传输层连接失败"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::HandshakeFailed)),
             QStringLiteral("SSH 握手失败"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::HostKeyRejected)),
             QStringLiteral("用户拒绝信任主机密钥"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::HostKeyMismatch)),
             QStringLiteral("主机密钥与已知记录不一致"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::AuthenticationFailed)),
             QStringLiteral("所有认证方式均失败"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::AuthenticationCancelled)),
             QStringLiteral("用户取消认证"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::ChannelOpenFailed)),
             QStringLiteral("打开 SSH 通道失败"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::DisconnectedByPeer)),
             QStringLiteral("远程主机断开连接"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::Cancelled)),
             QStringLiteral("操作已取消"));
    QCOMPARE(ZzSshError::message(static_cast<int>(ZzSshErrorCode::InternalError)),
             QStringLiteral("内部错误"));
}

void tst_ZzSshError::libssh2KnownCode()
{
    QCOMPARE(ZzSshError::messageForLibssh2(-9), QStringLiteral("操作超时"));
    QCOMPARE(ZzSshError::messageForLibssh2(-18), QStringLiteral("认证失败"));
    QCOMPARE(ZzSshError::messageForLibssh2(-51), QStringLiteral("算法不受支持（后端缺少实现）"));
}

void tst_ZzSshError::libssh2UnknownCodeFallback()
{
    const QString msg = ZzSshError::messageForLibssh2(-999);
    QVERIFY(msg.contains(QStringLiteral("-999")));
    QVERIFY(msg.contains(QStringLiteral("libssh2")));
}

void tst_ZzSshError::negativeCodeDispatchesToLibssh2()
{
    // 负数错误码一律走 libssh2 映射
    QCOMPARE(ZzSshError::message(-9), QStringLiteral("操作超时"));
}

QTEST_GUILESS_MAIN(tst_ZzSshError)
#include "tst_ZzSshError.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshError unit/tst_ZzSshError.cpp)
set_tests_properties(tst_ZzSshError PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错找不到 `ZzSshError.h`（实现尚不存在）。

- [ ] **步骤 3：创建 `src/ZzSshError.h` 与 `src/ZzSshError.cpp`**

`src/ZzSshError.h`：

```cpp
#pragma once

#include <QString>

/**
 * @brief ZzSshCore 封装层错误码。
 *
 * 0 表示无错误；负数为 libssh2 错误码透传；1000 及以上为封装层自定义码。
 * 所有异步操作的 errorOccurred(int code, QString message) 信号均使用本约定。
 */
enum class ZzSshErrorCode {
    NoError = 0,                ///< 无错误
    TransportError = 1000,      ///< 底层 socket 连接失败
    HandshakeFailed,            ///< SSH 握手失败
    HostKeyRejected,            ///< 用户拒绝信任首次出现的主机密钥
    HostKeyMismatch,            ///< 主机密钥与 known_hosts 记录不一致且被拒绝
    AuthenticationFailed,       ///< 所有可用认证方式均失败
    AuthenticationCancelled,    ///< 用户取消密码输入
    ChannelOpenFailed,          ///< 打开 channel 失败
    DisconnectedByPeer,         ///< 对端主动断开
    Cancelled,                  ///< 本地主动取消/中断
    InternalError               ///< 内部错误
};

/**
 * @brief 错误码到中文描述的映射。
 */
namespace ZzSshError {

/**
 * @brief 将错误码转换为中文描述。
 * @param code 错误码（ZzSshErrorCode 或 libssh2 负数透传码）。
 * @return 中文描述；未知 libssh2 码返回 "libssh2 错误 <码>"。
 */
QString message(int code);

/**
 * @brief 将 libssh2 错误码（负数）转换为中文描述。
 * @param rc libssh2 返回值（LIBSSH2_ERROR_* 常量）。
 * @return 中文描述。
 */
QString messageForLibssh2(int rc);

} // namespace ZzSshError
```

`src/ZzSshError.cpp`：

```cpp
#include "ZzSshError.h"

QString ZzSshError::message(int code)
{
    if (code < 0)
        return messageForLibssh2(code);
    switch (static_cast<ZzSshErrorCode>(code)) {
    case ZzSshErrorCode::NoError:                return QStringLiteral("无错误");
    case ZzSshErrorCode::TransportError:         return QStringLiteral("传输层连接失败");
    case ZzSshErrorCode::HandshakeFailed:        return QStringLiteral("SSH 握手失败");
    case ZzSshErrorCode::HostKeyRejected:        return QStringLiteral("用户拒绝信任主机密钥");
    case ZzSshErrorCode::HostKeyMismatch:        return QStringLiteral("主机密钥与已知记录不一致");
    case ZzSshErrorCode::AuthenticationFailed:   return QStringLiteral("所有认证方式均失败");
    case ZzSshErrorCode::AuthenticationCancelled:return QStringLiteral("用户取消认证");
    case ZzSshErrorCode::ChannelOpenFailed:      return QStringLiteral("打开 SSH 通道失败");
    case ZzSshErrorCode::DisconnectedByPeer:     return QStringLiteral("远程主机断开连接");
    case ZzSshErrorCode::Cancelled:              return QStringLiteral("操作已取消");
    case ZzSshErrorCode::InternalError:          return QStringLiteral("内部错误");
    }
    return QStringLiteral("未知错误 %1").arg(code);
}

QString ZzSshError::messageForLibssh2(int rc)
{
    switch (rc) {
    case -2:  return QStringLiteral("接收 SSH 横幅失败");
    case -3:  return QStringLiteral("发送 SSH 横幅失败");
    case -5:  return QStringLiteral("密钥交换失败");
    case -7:  return QStringLiteral("socket 发送失败");
    case -9:  return QStringLiteral("操作超时");
    case -13: return QStringLiteral("socket 已断开");
    case -18: return QStringLiteral("认证失败");
    case -30: return QStringLiteral("socket 超时");
    case -33: return QStringLiteral("算法或方法不受支持");
    case -43: return QStringLiteral("socket 接收失败");
    case -48: return QStringLiteral("密钥文件认证失败");
    case -51: return QStringLiteral("算法不受支持（后端缺少实现）");
    default:  return QStringLiteral("libssh2 错误 %1").arg(rc);
    }
}
```

在 `CMakeLists.txt` 的 `# === 源文件（后续任务按 target_sources 追加） ===` 一行下方追加：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshError.h src/ZzSshError.cpp)
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzSshError
```

预期：`Passed`；`100% tests passed, 0 tests failed out of 1`。

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSshError.h src/ZzSshError.cpp tests/unit/tst_ZzSshError.cpp CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshError 错误码与中文描述映射"
```

---

### 任务 3：ZzSshTransport 抽象接口与 ZzTcpTransport 生产实现

设计说明：传输层用**原生 socket** 而非 QTcpSocket——QTcpSocket 会把数据预读进内部缓冲区，与 libssh2 直接读 fd 的行为冲突；原生 socket 保证阻塞语义干净可控。Qt Network 依赖用于 DNS（`QHostInfo`）。`abortBlocking()` 是唯一允许跨线程调用的方法（通过 `shutdown()` 打断对端线程的阻塞调用），其余方法由工作线程独占调用。

**文件：**
- 创建：`src/ZzSshTransport.h`
- 创建：`src/ZzTcpTransport.h`、`src/ZzTcpTransport.cpp`
- 测试：`tests/unit/tst_ZzTcpTransport.cpp`
- 修改：`CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzTcpTransport.cpp`**

```cpp
#include <QtTest>
#include <QTcpServer>
#include <QTcpSocket>

#include "ZzTcpTransport.h"

/**
 * @brief ZzTcpTransport 的回环单元测试（本机 QTcpServer，无需外部网络）。
 */
class tst_ZzTcpTransport : public QObject
{
    Q_OBJECT

private slots:
    void openSuccess();
    void openRefused();
    void dnsFailure();
    void waitReadableTimeout();
    void waitReadableData();
    void closeIsIdempotent();
    void abortInterruptsWait();
};

void tst_ZzTcpTransport::openSuccess()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    ZzTcpTransport t;
    QString err;
    QVERIFY2(t.open(QStringLiteral("127.0.0.1"), server.serverPort(), 3000, &err), qPrintable(err));
    QVERIFY(t.isOpen());
    QVERIFY(t.socketDescriptor() >= 0);
}

void tst_ZzTcpTransport::openRefused()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    const quint16 port = server.serverPort();
    server.close(); // 立即释放端口，随后连接必被拒绝
    ZzTcpTransport t;
    QString err;
    QVERIFY(!t.open(QStringLiteral("127.0.0.1"), port, 2000, &err));
    QVERIFY(!err.isEmpty());
    QVERIFY(!t.isOpen());
}

void tst_ZzTcpTransport::dnsFailure()
{
    ZzTcpTransport t;
    QString err;
    QVERIFY(!t.open(QStringLiteral("nonexistent-zzsshcore.invalid"), 22, 3000, &err));
    QVERIFY(err.contains(QStringLiteral("DNS")));
}

void tst_ZzTcpTransport::waitReadableTimeout()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    ZzTcpTransport t;
    QString err;
    QVERIFY(t.open(QStringLiteral("127.0.0.1"), server.serverPort(), 3000, &err));
    QElapsedTimer timer;
    timer.start();
    QCOMPARE(t.waitReadable(80), ZzSshWaitResult::Timeout);
    QVERIFY(timer.elapsed() < 2000);
}

void tst_ZzTcpTransport::waitReadableData()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    ZzTcpTransport t;
    QString err;
    QVERIFY(t.open(QStringLiteral("127.0.0.1"), server.serverPort(), 3000, &err));
    QTcpSocket *peer = server.nextPendingConnection();
    QVERIFY(peer != nullptr);
    peer->write("x");
    peer->flush();
    QCOMPARE(t.waitReadable(3000), ZzSshWaitResult::Readable);
}

void tst_ZzTcpTransport::closeIsIdempotent()
{
    ZzTcpTransport t;
    t.close();
    t.close();
    QVERIFY(!t.isOpen());
}

void tst_ZzTcpTransport::abortInterruptsWait()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    ZzTcpTransport t;
    QString err;
    QVERIFY(t.open(QStringLiteral("127.0.0.1"), server.serverPort(), 3000, &err));
    QTimer::singleShot(50, [&t] { t.abortBlocking(); });
    QElapsedTimer timer;
    timer.start();
    t.waitReadable(5000); // 应被 abortBlocking 提前打断
    QVERIFY(timer.elapsed() < 2000);
}

QTEST_MAIN(tst_ZzTcpTransport)
#include "tst_ZzTcpTransport.moc"
```

注意：本测试用 `QTEST_MAIN`（QTimer::singleShot 需要事件循环）。

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzTcpTransport unit/tst_ZzTcpTransport.cpp)
set_tests_properties(tst_ZzTcpTransport PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错找不到 `ZzTcpTransport.h`。

- [ ] **步骤 3：创建 `src/ZzSshTransport.h`（抽象接口）**

```cpp
#pragma once

#include <QString>

/**
 * @brief 传输层等待结果。
 */
enum class ZzSshWaitResult {
    Readable,   ///< socket 可读（含对端关闭，读取方会看到 EOF）
    Timeout,    ///< 超时
    Error       ///< 出错（含被 abortBlocking() 中止）
};

/**
 * @brief SSH 传输层抽象接口：对上层提供底层 socket 能力。
 *
 * 实现类的实例由工作线程独占使用；仅 abortBlocking() 允许从其他线程调用，
 * 用于打断进行中的阻塞操作（取消连接、主动断开）。
 */
class ZzSshTransport
{
public:
    virtual ~ZzSshTransport() = default;

    /**
     * @brief 建立 TCP 连接（阻塞，带超时）。
     * @param host 主机地址（IP 或域名）。
     * @param port 端口号。
     * @param timeoutMs 超时毫秒数。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    virtual bool open(const QString &host, quint16 port, int timeoutMs, QString *errorString) = 0;

    /**
     * @brief 等待 socket 可读（阻塞，带超时）。
     * @param timeoutMs 超时毫秒数；0 表示立即返回。
     * @return 等待结果。
     */
    virtual ZzSshWaitResult waitReadable(int timeoutMs) = 0;

    /**
     * @brief 中止进行中的阻塞调用（线程安全，可从任意线程调用）。
     */
    virtual void abortBlocking() = 0;

    /**
     * @brief 关闭传输并释放 socket（幂等；由拥有线程调用）。
     */
    virtual void close() = 0;

    /**
     * @brief 是否处于已连接状态。
     */
    virtual bool isOpen() const = 0;

    /**
     * @brief 返回底层 socket 描述符（交给 libssh2 使用）；无效时返回 -1。
     */
    virtual qintptr socketDescriptor() const = 0;
};
```

- [ ] **步骤 4：创建 `src/ZzTcpTransport.h` 与 `src/ZzTcpTransport.cpp`**

`src/ZzTcpTransport.h`：

```cpp
#pragma once

#include "ZzSshTransport.h"

#include <atomic>

/**
 * @brief 基于原生 socket 的生产传输层实现。
 *
 * 跨平台（POSIX / Winsock2）。connect 阶段使用非阻塞 socket + poll 实现超时，
 * 连接成功后恢复阻塞模式并开启 TCP_NODELAY。
 * 不使用 QTcpSocket：避免 Qt 内部缓冲与 libssh2 直接读 fd 冲突。
 */
class ZzTcpTransport : public ZzSshTransport
{
public:
    ZzTcpTransport();
    ~ZzTcpTransport() override;

    bool open(const QString &host, quint16 port, int timeoutMs, QString *errorString) override;
    ZzSshWaitResult waitReadable(int timeoutMs) override;
    void abortBlocking() override;
    void close() override;
    bool isOpen() const override { return m_socket.load() >= 0; }
    qintptr socketDescriptor() const override { return m_socket.load(); }

private:
    std::atomic<qintptr> m_socket{-1};
    std::atomic<bool> m_aborted{false};
};
```

`src/ZzTcpTransport.cpp`：

```cpp
#include "ZzTcpTransport.h"

#include <QElapsedTimer>
#include <QHostInfo>

#include <cstring>

#ifdef Q_OS_WIN
#  include <winsock2.h>
#  include <ws2tcpip.h>
#else
#  include <arpa/inet.h>
#  include <cerrno>
#  include <fcntl.h>
#  include <netinet/in.h>
#  include <netinet/tcp.h>
#  include <poll.h>
#  include <sys/socket.h>
#  include <unistd.h>
#endif

namespace {

#ifdef Q_OS_WIN
/** @brief 确保 Winsock 已初始化（进程内仅一次）。 */
bool ensureWinsock()
{
    static const bool ok = [] {
        WSADATA d;
        return WSAStartup(MAKEWORD(2, 2), &d) == 0;
    }();
    return ok;
}
#endif

} // namespace

ZzTcpTransport::ZzTcpTransport() = default;

ZzTcpTransport::~ZzTcpTransport()
{
    close();
}

bool ZzTcpTransport::open(const QString &host, quint16 port, int timeoutMs, QString *errorString)
{
#ifdef Q_OS_WIN
    if (!ensureWinsock()) {
        if (errorString) *errorString = QStringLiteral("WSAStartup 失败");
        return false;
    }
#endif
    close();
    m_aborted.store(false);

    const QHostInfo info = QHostInfo::fromName(host);
    if (info.error() != QHostInfo::NoError || info.addresses().isEmpty()) {
        if (errorString)
            *errorString = QStringLiteral("DNS 解析失败：%1").arg(info.errorString());
        return false;
    }
    const QHostAddress addr = info.addresses().constFirst();
    const bool ipv4 = (addr.protocol() == QAbstractSocket::IPv4Protocol);

    const qintptr fd = static_cast<qintptr>(::socket(ipv4 ? AF_INET : AF_INET6, SOCK_STREAM, IPPROTO_TCP));
    if (fd < 0) {
        if (errorString) *errorString = QStringLiteral("创建 socket 失败");
        return false;
    }

    // 非阻塞 connect + poll，实现可超时、可中止的连接
#ifdef Q_OS_WIN
    u_long nonblock = 1;
    ioctlsocket(static_cast<SOCKET>(fd), FIONBIO, &nonblock);
#else
    const int flags = fcntl(static_cast<int>(fd), F_GETFL, 0);
    fcntl(static_cast<int>(fd), F_SETFL, flags | O_NONBLOCK);
#endif

    sockaddr_storage ss{};
    socklen_t sslen = 0;
    if (ipv4) {
        auto *sin = reinterpret_cast<sockaddr_in *>(&ss);
        sin->sin_family = AF_INET;
        sin->sin_port = htons(port);
        sin->sin_addr.s_addr = htonl(addr.toIPv4Address());
        sslen = sizeof(sockaddr_in);
    } else {
        auto *sin6 = reinterpret_cast<sockaddr_in6 *>(&ss);
        sin6->sin6_family = AF_INET6;
        sin6->sin6_port = htons(port);
        const Q_IPV6ADDR a6 = addr.toIPv6Address();
        std::memcpy(&sin6->sin6_addr, a6.c, 16);
        sslen = sizeof(sockaddr_in6);
    }
#ifdef Q_OS_WIN
    ::connect(static_cast<SOCKET>(fd), reinterpret_cast<sockaddr *>(&ss), sslen);
#else
    ::connect(static_cast<int>(fd), reinterpret_cast<sockaddr *>(&ss), sslen);
#endif

    QElapsedTimer timer;
    timer.start();
    bool ok = false;
    while (timer.elapsed() < timeoutMs && !m_aborted.load()) {
#ifdef Q_OS_WIN
        pollfd pfd{static_cast<SOCKET>(fd), POLLOUT, 0};
        const int rc = WSAPoll(&pfd, 1, 100);
#else
        pollfd pfd{static_cast<int>(fd), POLLOUT, 0};
        const int rc = ::poll(&pfd, 1, 100);
#endif
        if (rc > 0 && (pfd.revents & POLLOUT)) {
            int soErr = 0;
            socklen_t len = sizeof(soErr);
#ifdef Q_OS_WIN
            getsockopt(static_cast<SOCKET>(fd), SOL_SOCKET, SO_ERROR, reinterpret_cast<char *>(&soErr), &len);
#else
            getsockopt(static_cast<int>(fd), SOL_SOCKET, SO_ERROR, &soErr, &len);
#endif
            if (soErr == 0) {
                ok = true;
            } else if (errorString) {
                *errorString = QStringLiteral("连接被拒绝或不可达（系统错误 %1）").arg(soErr);
            }
            break;
        }
        if (rc < 0)
            break;
    }

    if (!ok) {
#ifdef Q_OS_WIN
        ::closesocket(static_cast<SOCKET>(fd));
#else
        ::close(static_cast<int>(fd));
#endif
        if (errorString && errorString->isEmpty()) {
            *errorString = m_aborted.load() ? QStringLiteral("连接被中止")
                                            : QStringLiteral("连接超时（%1 ms）").arg(timeoutMs);
        }
        return false;
    }

    // 恢复阻塞模式，开启 TCP_NODELAY
#ifdef Q_OS_WIN
    u_long blocking = 0;
    ioctlsocket(static_cast<SOCKET>(fd), FIONBIO, &blocking);
#else
    fcntl(static_cast<int>(fd), F_SETFL, flags);
#endif
    const int one = 1;
#ifdef Q_OS_WIN
    setsockopt(static_cast<SOCKET>(fd), IPPROTO_TCP, TCP_NODELAY, reinterpret_cast<const char *>(&one), sizeof(one));
#else
    setsockopt(static_cast<int>(fd), IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));
#endif

    m_socket.store(fd);
    return true;
}

ZzSshWaitResult ZzTcpTransport::waitReadable(int timeoutMs)
{
    const qintptr fd = m_socket.load();
    if (fd < 0)
        return ZzSshWaitResult::Error;
#ifdef Q_OS_WIN
    pollfd pfd{static_cast<SOCKET>(fd), POLLIN, 0};
    const int rc = WSAPoll(&pfd, 1, timeoutMs);
#else
    pollfd pfd{static_cast<int>(fd), POLLIN, 0};
    const int rc = ::poll(&pfd, 1, timeoutMs);
#endif
    if (rc > 0) {
        if (pfd.revents & (POLLIN | POLLHUP))
            return ZzSshWaitResult::Readable;
        return ZzSshWaitResult::Error;
    }
    return rc == 0 ? ZzSshWaitResult::Timeout : ZzSshWaitResult::Error;
}

void ZzTcpTransport::abortBlocking()
{
    m_aborted.store(true);
    const qintptr fd = m_socket.load();
    if (fd < 0)
        return;
#ifdef Q_OS_WIN
    ::shutdown(static_cast<SOCKET>(fd), SD_BOTH);
#else
    ::shutdown(static_cast<int>(fd), SHUT_RDWR);
#endif
}

void ZzTcpTransport::close()
{
    const qintptr fd = m_socket.exchange(-1);
    if (fd < 0)
        return;
#ifdef Q_OS_WIN
    ::shutdown(static_cast<SOCKET>(fd), SD_BOTH);
    ::closesocket(static_cast<SOCKET>(fd));
#else
    ::shutdown(static_cast<int>(fd), SHUT_RDWR);
    ::close(static_cast<int>(fd));
#endif
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshTransport.h src/ZzTcpTransport.h src/ZzTcpTransport.cpp)
```

- [ ] **步骤 5：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzTcpTransport
```

预期：`Passed`；`100% tests passed, 0 tests failed out of 1`。

- [ ] **步骤 6：Commit**

```bash
git add src/ZzSshTransport.h src/ZzTcpTransport.h src/ZzTcpTransport.cpp tests/unit/tst_ZzTcpTransport.cpp CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshTransport 抽象接口与 ZzTcpTransport 原生 socket 实现"
```

---

### 任务 4：ZzMockTransport 单元测试用 mock 传输层

`ZzMockTransport` 是脚本化的 `ZzSshTransport` 实现（header-only，仅供测试）：open 结果、waitReadable 序列由测试预置，close/abort 调用计数可断言。它同时是任务 8 中 `ZzSshConnectionWorker` 错误路径单测的注入点。

**文件：**
- 创建：`tests/helpers/ZzMockTransport.h`
- 测试：`tests/unit/tst_ZzMockTransport.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzMockTransport.cpp`**

```cpp
#include <QtTest>

#include "ZzMockTransport.h"

/**
 * @brief ZzMockTransport 脚本化行为的单元测试。
 */
class tst_ZzMockTransport : public QObject
{
    Q_OBJECT

private slots:
    void openSuccessByDefault();
    void openFailureCarriesErrorString();
    void waitReadableScriptedSequence();
    void waitReadableDefaultsToTimeout();
    void closeAndAbortAreCounted();
    void descriptorPassthrough();
};

void tst_ZzMockTransport::openSuccessByDefault()
{
    ZzMockTransport t;
    QVERIFY(t.open(QStringLiteral("h"), 22, 100, nullptr));
    QVERIFY(t.isOpen());
}

void tst_ZzMockTransport::openFailureCarriesErrorString()
{
    ZzMockTransport t;
    t.setOpenResult(false, QStringLiteral("mock 连接失败"));
    QString err;
    QVERIFY(!t.open(QStringLiteral("h"), 22, 100, &err));
    QCOMPARE(err, QStringLiteral("mock 连接失败"));
    QVERIFY(!t.isOpen());
}

void tst_ZzMockTransport::waitReadableScriptedSequence()
{
    ZzMockTransport t;
    t.enqueueWaitResult(ZzSshWaitResult::Readable);
    t.enqueueWaitResult(ZzSshWaitResult::Error);
    QCOMPARE(t.waitReadable(0), ZzSshWaitResult::Readable);
    QCOMPARE(t.waitReadable(0), ZzSshWaitResult::Error);
}

void tst_ZzMockTransport::waitReadableDefaultsToTimeout()
{
    ZzMockTransport t;
    QCOMPARE(t.waitReadable(0), ZzSshWaitResult::Timeout);
}

void tst_ZzMockTransport::closeAndAbortAreCounted()
{
    ZzMockTransport t;
    t.open(QStringLiteral("h"), 22, 100, nullptr);
    t.abortBlocking();
    t.close();
    t.close();
    QCOMPARE(t.abortCallCount(), 1);
    QCOMPARE(t.closeCallCount(), 2);
    QVERIFY(!t.isOpen());
}

void tst_ZzMockTransport::descriptorPassthrough()
{
    ZzMockTransport t;
    t.setSocketDescriptor(42);
    QCOMPARE(t.socketDescriptor(), static_cast<qintptr>(42));
}

QTEST_GUILESS_MAIN(tst_ZzMockTransport)
#include "tst_ZzMockTransport.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错找不到 `ZzMockTransport.h`。

- [ ] **步骤 3：创建 `tests/helpers/ZzMockTransport.h`**

```cpp
#pragma once

#include "ZzSshTransport.h"

#include <QQueue>

/**
 * @brief 测试用脚本化传输层：所有行为由测试预置脚本决定。
 *
 * - open 的成功/失败与错误文案通过 setOpenResult() 预置；
 * - waitReadable 按 enqueueWaitResult() 的队列依次返回，队空时返回 Timeout；
 * - close / abortBlocking 的调用次数可断言；
 * - socketDescriptor 可注入一个真实 fd（用于让 libssh2 握手在受控对端上失败）。
 */
class ZzMockTransport : public ZzSshTransport
{
public:
    /** @brief 预置 open 的结果与失败文案。 */
    void setOpenResult(bool ok, const QString &errorString = QString())
    {
        m_openOk = ok;
        m_openError = errorString;
    }

    /** @brief 注入 socket 描述符（不拥有所有权）。 */
    void setSocketDescriptor(qintptr fd) { m_fd = fd; }

    /** @brief 追加一次 waitReadable 的返回值。 */
    void enqueueWaitResult(ZzSshWaitResult r) { m_waitResults.enqueue(r); }

    /** @brief close 被调用的次数。 */
    int closeCallCount() const { return m_closeCalls; }

    /** @brief abortBlocking 被调用的次数。 */
    int abortCallCount() const { return m_abortCalls; }

    bool open(const QString &, quint16, int, QString *errorString) override
    {
        if (!m_openOk && errorString)
            *errorString = m_openError;
        m_open = m_openOk;
        return m_openOk;
    }

    ZzSshWaitResult waitReadable(int) override
    {
        if (!m_waitResults.isEmpty())
            return m_waitResults.dequeue();
        return ZzSshWaitResult::Timeout;
    }

    void abortBlocking() override { ++m_abortCalls; }

    void close() override
    {
        ++m_closeCalls;
        m_open = false;
    }

    bool isOpen() const override { return m_open; }
    qintptr socketDescriptor() const override { return m_fd; }

private:
    bool m_openOk = true;
    bool m_open = false;
    QString m_openError;
    qintptr m_fd = -1;
    QQueue<ZzSshWaitResult> m_waitResults;
    int m_closeCalls = 0;
    int m_abortCalls = 0;
};
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzMockTransport unit/tst_ZzMockTransport.cpp)
set_tests_properties(tst_ZzMockTransport PROPERTIES LABELS "unit")
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzMockTransport
```

预期：`Passed`。

- [ ] **步骤 5：Commit**

```bash
git add tests/helpers/ZzMockTransport.h tests/unit/tst_ZzMockTransport.cpp tests/CMakeLists.txt
git commit -m "test: 新增 ZzMockTransport 脚本化 mock 传输层"
```

---

### 任务 5：ZzSshSession 与 ZzSshChannel 的 C++20 RAII 封装

`ZzSshSession` 封装 `LIBSSH2_SESSION`（构造即 init，析构即 disconnect+free，禁拷贝）；`ZzSshChannel` 封装 `LIBSSH2_CHANNEL`（析构即 close+free）。禁止裸指针泄漏出封装层：`handle()` 仅供同库内部使用，不转移所有权。

**文件：**
- 创建：`src/ZzSshSession.h`、`src/ZzSshSession.cpp`
- 创建：`src/ZzSshChannel.h`、`src/ZzSshChannel.cpp`
- 测试：`tests/unit/tst_ZzSshSession.cpp`（含 channel 生命周期用例）
- 修改：`CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSshSession.cpp`**

```cpp
#include <QtTest>

#include "ZzSshChannel.h"
#include "ZzSshSession.h"

/**
 * @brief ZzSshSession / ZzSshChannel RAII 生命周期的单元测试（无网络）。
 */
class tst_ZzSshSession : public QObject
{
    Q_OBJECT

private slots:
    void sessionConstructDestruct();
    void handshakeWithoutSocketFails();
    void hostKeyBeforeHandshakeIsEmpty();
    void channelOpenWithoutHandshakeFails();
    void channelCloseIsIdempotent();
    void channelReadWriteWithoutChannelFail();
};

void tst_ZzSshSession::sessionConstructDestruct()
{
    {
        ZzSshSession s;
        QVERIFY(s.isValid());
        QVERIFY(s.handle() != nullptr);
    }
    // 析构不崩溃即通过
    QVERIFY(true);
}

void tst_ZzSshSession::handshakeWithoutSocketFails()
{
    ZzSshSession s;
    QString err;
    QVERIFY(!s.handshake(-1, &err));
    QVERIFY(!err.isEmpty());
}

void tst_ZzSshSession::hostKeyBeforeHandshakeIsEmpty()
{
    ZzSshSession s;
    QString keyType;
    QVERIFY(s.hostKey(&keyType).isEmpty());
}

void tst_ZzSshSession::channelOpenWithoutHandshakeFails()
{
    ZzSshSession s; // 有效但未握手
    ZzSshChannel ch;
    QString err;
    QVERIFY(!ch.openShell(s, QStringLiteral("xterm-256color"), 80, 24, &err));
    QVERIFY(!ch.isOpen());
}

void tst_ZzSshSession::channelCloseIsIdempotent()
{
    ZzSshChannel ch;
    ch.close();
    ch.close();
    QVERIFY(!ch.isOpen());
}

void tst_ZzSshSession::channelReadWriteWithoutChannelFail()
{
    ZzSshChannel ch;
    QByteArray out;
    QCOMPARE(ch.read(&out, 1024), static_cast<qint64>(-1));
    QString err;
    QVERIFY(!ch.write(QByteArray("abc"), &err));
    QVERIFY(!ch.resize(100, 40, &err));
}

QTEST_GUILESS_MAIN(tst_ZzSshSession)
#include "tst_ZzSshSession.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshSession unit/tst_ZzSshSession.cpp)
set_tests_properties(tst_ZzSshSession PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错找不到 `ZzSshSession.h`。

- [ ] **步骤 3：创建 `src/ZzSshSession.h` 与 `src/ZzSshSession.cpp`**

`src/ZzSshSession.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QString>

#include <libssh2.h>

/**
 * @brief LIBSSH2_SESSION 的 RAII 封装。
 *
 * 构造即 libssh2_session_init，析构即 disconnect + free；禁止拷贝与移动。
 * handle() 返回的裸指针仅供 ZzSshCore 内部（channel/worker）使用，不转移所有权。
 * @note 非线程安全：同一实例的所有方法必须在持有它的工作线程内串行调用。
 */
class ZzSshSession
{
public:
    ZzSshSession();
    ~ZzSshSession();

    ZzSshSession(const ZzSshSession &) = delete;
    ZzSshSession &operator=(const ZzSshSession &) = delete;
    ZzSshSession(ZzSshSession &&) = delete;
    ZzSshSession &operator=(ZzSshSession &&) = delete;

    /** @brief 会话是否成功初始化。 */
    bool isValid() const { return m_session != nullptr; }

    /** @brief 返回底层句柄（不转移所有权，仅供库内部使用）。 */
    LIBSSH2_SESSION *handle() const { return m_session; }

    /**
     * @brief 在已连接的 socket 上执行 SSH 握手（阻塞）。
     * @param socketFd 已建立 TCP 连接的 socket 描述符。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    bool handshake(qintptr socketFd, QString *errorString);

    /**
     * @brief 获取主机密钥原始字节。
     * @param keyTypeOut 输出密钥类型（如 "ssh-ed25519"，可为 nullptr）。
     * @return 密钥原始字节；握手未完成时返回空。
     */
    QByteArray hostKey(QString *keyTypeOut) const;

    /**
     * @brief 配置 keepalive（要求对端应答）。
     * @param intervalSeconds 发送间隔秒数。
     */
    void enableKeepalive(int intervalSeconds);

    /**
     * @brief 立即发送一次 keepalive。
     * @return 发送成功返回 true；失败说明连接已断开。
     */
    bool sendKeepalive();

private:
    LIBSSH2_SESSION *m_session = nullptr;
};
```

`src/ZzSshSession.cpp`：

```cpp
#include "ZzSshSession.h"

#include "ZzSshCore.h"
#include "ZzSshError.h"

ZzSshSession::ZzSshSession()
{
    ZzSshCore::globalInit();
    m_session = libssh2_session_init_ex(nullptr, nullptr, nullptr, nullptr);
}

ZzSshSession::~ZzSshSession()
{
    if (!m_session)
        return;
    libssh2_session_disconnect_ex(m_session, SSH_DISCONNECT_BY_APPLICATION, "ZzSshCore 关闭", "");
    libssh2_session_free(m_session);
}

bool ZzSshSession::handshake(qintptr socketFd, QString *errorString)
{
    if (!m_session) {
        if (errorString) *errorString = QStringLiteral("libssh2 会话初始化失败");
        return false;
    }
    const int rc = libssh2_session_handshake(m_session, static_cast<libssh2_socket_t>(socketFd));
    if (rc != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = ZzSshError::messageForLibssh2(rc);
        return false;
    }
    return true;
}

QByteArray ZzSshSession::hostKey(QString *keyTypeOut) const
{
    if (!m_session)
        return {};
    int type = 0;
    size_t len = 0;
    const char *key = libssh2_session_hostkey(m_session, &len, &type);
    if (!key || len == 0)
        return {};
    if (keyTypeOut) {
        switch (type) {
        case LIBSSH2_HOSTKEY_TYPE_RSA:        *keyTypeOut = QStringLiteral("ssh-rsa"); break;
        case LIBSSH2_HOSTKEY_TYPE_DSS:        *keyTypeOut = QStringLiteral("ssh-dss"); break;
        case LIBSSH2_HOSTKEY_TYPE_ECDSA_256:  *keyTypeOut = QStringLiteral("ecdsa-sha2-nistp256"); break;
        case LIBSSH2_HOSTKEY_TYPE_ECDSA_384:  *keyTypeOut = QStringLiteral("ecdsa-sha2-nistp384"); break;
        case LIBSSH2_HOSTKEY_TYPE_ECDSA_521:  *keyTypeOut = QStringLiteral("ecdsa-sha2-nistp521"); break;
        case LIBSSH2_HOSTKEY_TYPE_ED25519:    *keyTypeOut = QStringLiteral("ssh-ed25519"); break;
        default:                              *keyTypeOut = QStringLiteral("unknown(%1)").arg(type); break;
        }
    }
    return QByteArray(key, static_cast<qsizetype>(len));
}

void ZzSshSession::enableKeepalive(int intervalSeconds)
{
    if (m_session)
        libssh2_keepalive_config(m_session, 1 /* 要求对端应答 */, static_cast<unsigned int>(intervalSeconds));
}

bool ZzSshSession::sendKeepalive()
{
    if (!m_session)
        return false;
    int secondsToNext = 0;
    return libssh2_keepalive_send(m_session, &secondsToNext) == LIBSSH2_ERROR_NONE;
}
```

- [ ] **步骤 4：创建 `src/ZzSshChannel.h` 与 `src/ZzSshChannel.cpp`**

`src/ZzSshChannel.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QString>

#include <libssh2.h>

class ZzSshSession;

/**
 * @brief LIBSSH2_CHANNEL 的 RAII 封装。
 *
 * 析构即 close + free；禁止拷贝与移动。
 * @note 非线程安全：同一实例的所有方法必须在持有它的工作线程内串行调用。
 */
class ZzSshChannel
{
public:
    ZzSshChannel() = default;
    ~ZzSshChannel();

    ZzSshChannel(const ZzSshChannel &) = delete;
    ZzSshChannel &operator=(const ZzSshChannel &) = delete;
    ZzSshChannel(ZzSshChannel &&) = delete;
    ZzSshChannel &operator=(ZzSshChannel &&) = delete;

    /** @brief channel 是否处于打开状态。 */
    bool isOpen() const { return m_channel != nullptr; }

    /**
     * @brief 打开交互式 shell channel（session channel + PTY + shell 进程，阻塞）。
     * @param session 已完成握手与认证的会话。
     * @param term 终端类型（如 "xterm-256color"）。
     * @param cols 终端列数。
     * @param rows 终端行数。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    bool openShell(ZzSshSession &session, const QString &term, int cols, int rows, QString *errorString);

    /**
     * @brief 读取 channel 数据（阻塞模式：socket 已确认可读时调用，返回本次到达的数据）。
     * @param out 输出缓冲区。
     * @param maxBytes 单次最多读取字节数。
     * @return 读取的字节数；0 表示对端 EOF 或暂无数据；负数表示错误。
     */
    qint64 read(QByteArray *out, int maxBytes);

    /**
     * @brief 写入数据（循环写直至全部发出）。
     * @param data 待写数据。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 全部写入成功返回 true。
     */
    bool write(const QByteArray &data, QString *errorString);

    /**
     * @brief 调整 PTY 尺寸。
     * @param cols 列数。
     * @param rows 行数。
     * @param errorString 失败时输出错误描述（可为 nullptr）。
     * @return 成功返回 true。
     */
    bool resize(int cols, int rows, QString *errorString);

    /** @brief 对端是否已发送 EOF。 */
    bool isEof() const;

    /** @brief 关闭并释放 channel（幂等）。 */
    void close();

private:
    LIBSSH2_CHANNEL *m_channel = nullptr;
};
```

`src/ZzSshChannel.cpp`：

```cpp
#include "ZzSshChannel.h"

#include "ZzSshError.h"
#include "ZzSshSession.h"

ZzSshChannel::~ZzSshChannel()
{
    close();
}

bool ZzSshChannel::openShell(ZzSshSession &session, const QString &term, int cols, int rows, QString *errorString)
{
    close();
    if (!session.isValid()) {
        if (errorString) *errorString = QStringLiteral("SSH 会话无效");
        return false;
    }
    m_channel = libssh2_channel_open_session(session.handle());
    if (!m_channel) {
        if (errorString) *errorString = QStringLiteral("打开 session channel 失败");
        return false;
    }
    const QByteArray t = term.toUtf8();
    if (libssh2_channel_request_pty_ex(m_channel, t.constData(), static_cast<unsigned int>(t.size()),
                                       nullptr, 0, cols, rows, 0, 0) != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = QStringLiteral("PTY 请求失败");
        close();
        return false;
    }
    if (libssh2_channel_process_startup(m_channel, "shell", 5, nullptr, 0) != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = QStringLiteral("启动 shell 进程失败");
        close();
        return false;
    }
    return true;
}

qint64 ZzSshChannel::read(QByteArray *out, int maxBytes)
{
    if (!m_channel || !out || maxBytes <= 0)
        return -1;
    QByteArray buf(maxBytes, Qt::Uninitialized);
    const ssize_t rc = libssh2_channel_read_ex(m_channel, 0, buf.data(), static_cast<size_t>(buf.size()));
    if (rc > 0) {
        buf.truncate(static_cast<qsizetype>(rc));
        *out = buf;
    }
    return static_cast<qint64>(rc);
}

bool ZzSshChannel::write(const QByteArray &data, QString *errorString)
{
    if (!m_channel) {
        if (errorString) *errorString = QStringLiteral("channel 未打开");
        return false;
    }
    qsizetype sent = 0;
    while (sent < data.size()) {
        const ssize_t rc = libssh2_channel_write_ex(m_channel, 0, data.constData() + sent,
                                                    static_cast<size_t>(data.size() - sent));
        if (rc < 0) {
            if (errorString) *errorString = ZzSshError::messageForLibssh2(static_cast<int>(rc));
            return false;
        }
        sent += rc;
    }
    return true;
}

bool ZzSshChannel::resize(int cols, int rows, QString *errorString)
{
    if (!m_channel) {
        if (errorString) *errorString = QStringLiteral("channel 未打开");
        return false;
    }
    if (libssh2_channel_request_pty_size_ex(m_channel, cols, rows, 0, 0) != LIBSSH2_ERROR_NONE) {
        if (errorString) *errorString = QStringLiteral("调整 PTY 尺寸失败");
        return false;
    }
    return true;
}

bool ZzSshChannel::isEof() const
{
    return m_channel && libssh2_channel_eof(m_channel) == 1;
}

void ZzSshChannel::close()
{
    if (!m_channel)
        return;
    libssh2_channel_close(m_channel);
    libssh2_channel_free(m_channel);
    m_channel = nullptr;
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshSession.h src/ZzSshSession.cpp src/ZzSshChannel.h src/ZzSshChannel.cpp)
```

- [ ] **步骤 5：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzSshSession
```

预期：`Passed`。

- [ ] **步骤 6：Commit**

```bash
git add src/ZzSshSession.h src/ZzSshSession.cpp src/ZzSshChannel.h src/ZzSshChannel.cpp tests/unit/tst_ZzSshSession.cpp CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshSession 与 ZzSshChannel 的 RAII 封装"
```

---

### 任务 6：ZzSshHostKeyStore 主机密钥存储（known_hosts.json）

known_hosts 风格的主机密钥存储：JSON 文件，按 host+port 索引；验证结果为三态（Trusted / Unknown / Changed）；指纹为 OpenSSH 风格的 `SHA256:<base64 无填充>`。这是规格 §八 的安全底线组件，逻辑全部可脱离网络单测。

**文件：**
- 创建：`src/ZzSshHostKeyStore.h`、`src/ZzSshHostKeyStore.cpp`
- 测试：`tests/unit/tst_ZzSshHostKeyStore.cpp`
- 修改：`CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSshHostKeyStore.cpp`**

```cpp
#include <QtTest>
#include <QTemporaryDir>

#include "ZzSshHostKeyStore.h"

/**
 * @brief ZzSshHostKeyStore 的单元测试（纯逻辑，无网络）。
 */
class tst_ZzSshHostKeyStore : public QObject
{
    Q_OBJECT

private slots:
    void verifyUnknownWhenEmpty();
    void addThenTrusted();
    void changedWhenKeyDiffers();
    void unknownWhenPortDiffers();
    void addOverwritesExisting();
    void saveLoadRoundTrip();
    void loadMissingFileSucceeds();
    void loadCorruptFileFails();
    void storedEntryReturnsWhatWasAdded();
    void fingerprintFormat();
};

void tst_ZzSshHostKeyStore::verifyUnknownWhenEmpty()
{
    ZzSshHostKeyStore store(QStringLiteral("/nonexistent/known_hosts.json"));
    QVERIFY(store.load());
    QCOMPARE(store.verify(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"),
                          QByteArray("keybytes")),
             ZzSshHostKeyStore::VerifyResult::Unknown);
}

void tst_ZzSshHostKeyStore::addThenTrusted()
{
    ZzSshHostKeyStore store(QStringLiteral("/nonexistent/known_hosts.json"));
    store.add(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("keybytes"));
    QCOMPARE(store.verify(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"),
                          QByteArray("keybytes")),
             ZzSshHostKeyStore::VerifyResult::Trusted);
    QCOMPARE(store.count(), 1);
}

void tst_ZzSshHostKeyStore::changedWhenKeyDiffers()
{
    ZzSshHostKeyStore store(QStringLiteral("/nonexistent/known_hosts.json"));
    store.add(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("oldkey"));
    QCOMPARE(store.verify(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"),
                          QByteArray("newkey")),
             ZzSshHostKeyStore::VerifyResult::Changed);
}

void tst_ZzSshHostKeyStore::unknownWhenPortDiffers()
{
    ZzSshHostKeyStore store(QStringLiteral("/nonexistent/known_hosts.json"));
    store.add(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("keybytes"));
    QCOMPARE(store.verify(QStringLiteral("example.com"), 2222, QStringLiteral("ssh-ed25519"),
                          QByteArray("keybytes")),
             ZzSshHostKeyStore::VerifyResult::Unknown);
}

void tst_ZzSshHostKeyStore::addOverwritesExisting()
{
    ZzSshHostKeyStore store(QStringLiteral("/nonexistent/known_hosts.json"));
    store.add(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("oldkey"));
    store.add(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("newkey"));
    QCOMPARE(store.count(), 1);
    QCOMPARE(store.verify(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"),
                          QByteArray("newkey")),
             ZzSshHostKeyStore::VerifyResult::Trusted);
}

void tst_ZzSshHostKeyStore::saveLoadRoundTrip()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("known_hosts.json"));
    {
        ZzSshHostKeyStore store(path);
        store.add(QStringLiteral("a.example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("key-a"));
        store.add(QStringLiteral("b.example.com"), 2222, QStringLiteral("ssh-rsa"), QByteArray("key-b"));
        QVERIFY(store.save());
    }
    ZzSshHostKeyStore loaded(path);
    QVERIFY(loaded.load());
    QCOMPARE(loaded.count(), 2);
    QCOMPARE(loaded.verify(QStringLiteral("a.example.com"), 22, QStringLiteral("ssh-ed25519"),
                           QByteArray("key-a")),
             ZzSshHostKeyStore::VerifyResult::Trusted);
    QCOMPARE(loaded.verify(QStringLiteral("b.example.com"), 2222, QStringLiteral("ssh-rsa"),
                           QByteArray("key-b")),
             ZzSshHostKeyStore::VerifyResult::Trusted);
}

void tst_ZzSshHostKeyStore::loadMissingFileSucceeds()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSshHostKeyStore store(dir.filePath(QStringLiteral("missing.json")));
    QVERIFY(store.load());
    QCOMPARE(store.count(), 0);
}

void tst_ZzSshHostKeyStore::loadCorruptFileFails()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = dir.filePath(QStringLiteral("corrupt.json"));
    QFile f(path);
    QVERIFY(f.open(QIODevice::WriteOnly));
    f.write("这不是 JSON {{{");
    f.close();
    ZzSshHostKeyStore store(path);
    QVERIFY(!store.load());
}

void tst_ZzSshHostKeyStore::storedEntryReturnsWhatWasAdded()
{
    ZzSshHostKeyStore store(QStringLiteral("/nonexistent/known_hosts.json"));
    store.add(QStringLiteral("example.com"), 22, QStringLiteral("ssh-ed25519"), QByteArray("keybytes"));
    QString keyType;
    QByteArray rawKey;
    QVERIFY(store.storedEntry(QStringLiteral("example.com"), 22, &keyType, &rawKey));
    QCOMPARE(keyType, QStringLiteral("ssh-ed25519"));
    QCOMPARE(rawKey, QByteArray("keybytes"));
    QVERIFY(!store.storedEntry(QStringLiteral("other.com"), 22, nullptr, nullptr));
}

void tst_ZzSshHostKeyStore::fingerprintFormat()
{
    // SHA256 摘要 32 字节 → base64 无填充固定 43 字符
    const QByteArray fp = ZzSshHostKeyStore::fingerprintSha256(QByteArray("abc"));
    QVERIFY(fp.startsWith("SHA256:"));
    QCOMPARE(fp.size(), 7 + 43);
    QVERIFY(!fp.endsWith('='));
    // 相同输入指纹稳定
    QCOMPARE(fp, ZzSshHostKeyStore::fingerprintSha256(QByteArray("abc")));
}

QTEST_GUILESS_MAIN(tst_ZzSshHostKeyStore)
#include "tst_ZzSshHostKeyStore.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshHostKeyStore unit/tst_ZzSshHostKeyStore.cpp)
set_tests_properties(tst_ZzSshHostKeyStore PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错找不到 `ZzSshHostKeyStore.h`。

- [ ] **步骤 3：创建 `src/ZzSshHostKeyStore.h` 与 `src/ZzSshHostKeyStore.cpp`**

`src/ZzSshHostKeyStore.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QList>
#include <QString>

/**
 * @brief known_hosts 风格的主机密钥存储（JSON 文件）。
 *
 * 按 host+port 索引；密钥以原始字节的 base64 存储，比对用原始字节精确比较。
 * 文件格式：{"hosts": [{"host": "...", "port": 22, "key_type": "...", "key": "<base64>"}]}
 * @note 非线程安全：由工作线程在连接流程中独占使用。
 */
class ZzSshHostKeyStore
{
public:
    /**
     * @brief 主机密钥验证结果。
     */
    enum class VerifyResult {
        Trusted,    ///< 已有记录且密钥一致
        Unknown,    ///< 无记录（首次连接）
        Changed     ///< 已有记录但密钥不一致（安全警告）
    };

    /**
     * @brief 构造函数。
     * @param filePath known_hosts.json 文件路径。
     */
    explicit ZzSshHostKeyStore(const QString &filePath);

    /**
     * @brief 从文件加载。
     * @return 文件不存在视为空库并返回 true；文件损坏或不可读返回 false。
     */
    bool load();

    /**
     * @brief 原子写入文件（QSaveFile）。
     * @return 成功返回 true。
     */
    bool save() const;

    /**
     * @brief 验证主机密钥。
     * @param host 主机地址。
     * @param port 端口号。
     * @param keyType 密钥类型（如 "ssh-ed25519"）。
     * @param rawKey 密钥原始字节。
     * @return 验证结果。
     */
    VerifyResult verify(const QString &host, quint16 port, const QString &keyType,
                        const QByteArray &rawKey) const;

    /**
     * @brief 添加或覆盖一条记录。
     */
    void add(const QString &host, quint16 port, const QString &keyType, const QByteArray &rawKey);

    /**
     * @brief 读取已存储的记录。
     * @return 存在返回 true 并输出 keyType/rawKey（输出参数可为 nullptr）。
     */
    bool storedEntry(const QString &host, quint16 port, QString *keyTypeOut, QByteArray *rawKeyOut) const;

    /** @brief 记录条数。 */
    int count() const { return static_cast<int>(m_entries.size()); }

    /**
     * @brief 计算 OpenSSH 风格的 SHA256 指纹。
     * @param rawKey 密钥原始字节。
     * @return "SHA256:<base64 无填充>"。
     */
    static QByteArray fingerprintSha256(const QByteArray &rawKey);

private:
    struct Entry {
        QString host;
        quint16 port = 0;
        QString keyType;
        QByteArray rawKey;
    };

    qsizetype indexOf(const QString &host, quint16 port) const;

    QString m_filePath;
    QList<Entry> m_entries;
};
```

`src/ZzSshHostKeyStore.cpp`：

```cpp
#include "ZzSshHostKeyStore.h"

#include <QCryptographicHash>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSaveFile>

ZzSshHostKeyStore::ZzSshHostKeyStore(const QString &filePath)
    : m_filePath(filePath)
{
}

bool ZzSshHostKeyStore::load()
{
    m_entries.clear();
    QFile f(m_filePath);
    if (!f.exists())
        return true; // 文件不存在视为空库（首次连接语义）
    if (!f.open(QIODevice::ReadOnly))
        return false;
    QJsonParseError parseError{};
    const QJsonDocument doc = QJsonDocument::fromJson(f.readAll(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isObject())
        return false;
    const QJsonArray hosts = doc.object().value(QStringLiteral("hosts")).toArray();
    for (const QJsonValue &v : hosts) {
        const QJsonObject o = v.toObject();
        Entry e;
        e.host = o.value(QStringLiteral("host")).toString();
        e.port = static_cast<quint16>(o.value(QStringLiteral("port")).toInt());
        e.keyType = o.value(QStringLiteral("key_type")).toString();
        e.rawKey = QByteArray::fromBase64(o.value(QStringLiteral("key")).toString().toLatin1());
        if (!e.host.isEmpty() && e.port != 0 && !e.rawKey.isEmpty())
            m_entries.append(e);
    }
    return true;
}

bool ZzSshHostKeyStore::save() const
{
    QJsonArray hosts;
    for (const Entry &e : m_entries) {
        QJsonObject o;
        o.insert(QStringLiteral("host"), e.host);
        o.insert(QStringLiteral("port"), static_cast<int>(e.port));
        o.insert(QStringLiteral("key_type"), e.keyType);
        o.insert(QStringLiteral("key"), QString::fromLatin1(e.rawKey.toBase64()));
        hosts.append(o);
    }
    QJsonObject root;
    root.insert(QStringLiteral("hosts"), hosts);
    QSaveFile f(m_filePath);
    if (!f.open(QIODevice::WriteOnly))
        return false;
    f.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    return f.commit();
}

ZzSshHostKeyStore::VerifyResult ZzSshHostKeyStore::verify(const QString &host, quint16 port,
                                                          const QString &keyType,
                                                          const QByteArray &rawKey) const
{
    const qsizetype idx = indexOf(host, port);
    if (idx < 0)
        return VerifyResult::Unknown;
    const Entry &e = m_entries.at(idx);
    return (e.keyType == keyType && e.rawKey == rawKey) ? VerifyResult::Trusted : VerifyResult::Changed;
}

void ZzSshHostKeyStore::add(const QString &host, quint16 port, const QString &keyType,
                            const QByteArray &rawKey)
{
    const qsizetype idx = indexOf(host, port);
    if (idx >= 0)
        m_entries.removeAt(idx);
    m_entries.append({host, port, keyType, rawKey});
}

bool ZzSshHostKeyStore::storedEntry(const QString &host, quint16 port, QString *keyTypeOut,
                                    QByteArray *rawKeyOut) const
{
    const qsizetype idx = indexOf(host, port);
    if (idx < 0)
        return false;
    const Entry &e = m_entries.at(idx);
    if (keyTypeOut) *keyTypeOut = e.keyType;
    if (rawKeyOut) *rawKeyOut = e.rawKey;
    return true;
}

QByteArray ZzSshHostKeyStore::fingerprintSha256(const QByteArray &rawKey)
{
    const QByteArray digest = QCryptographicHash::hash(rawKey, QCryptographicHash::Sha256);
    return QByteArray("SHA256:") + digest.toBase64(QByteArray::Base64Encoding | QByteArray::OmitTrailingEquals);
}

qsizetype ZzSshHostKeyStore::indexOf(const QString &host, quint16 port) const
{
    for (qsizetype i = 0; i < m_entries.size(); ++i) {
        if (m_entries.at(i).host == host && m_entries.at(i).port == port)
            return i;
    }
    return -1;
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshHostKeyStore.h src/ZzSshHostKeyStore.cpp)
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R tst_ZzSshHostKeyStore
```

预期：`Passed`。

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSshHostKeyStore.h src/ZzSshHostKeyStore.cpp tests/unit/tst_ZzSshHostKeyStore.cpp CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshHostKeyStore 主机密钥存储（known_hosts.json）"
```

---

### 任务 7：Docker 集成测试基建

用 Docker 起 openssh-server 容器作为集成测试对端。镜像基于完整 openssh-server，自带 sftp-server 子系统，v0.2 的 SFTP 测试可直接复用同一镜像（规格 §九，此处不新增任何 SFTP 代码）。主机密钥在**构建期**生成并固化在镜像中（同一镜像多次 run 密钥稳定）；变更测试容器通过 `ZZ_REGEN_HOSTKEYS=1` 在启动时重新生成密钥。

环境变量约定（集成/性能测试统一读取）：
- `ZZSSH_TEST_HOST` / `ZZSSH_TEST_PORT`：主容器地址
- `ZZSSH_TEST_CHANGED_PORT`：密钥变更测试容器端口
- `ZZSSH_TEST_USER` / `ZZSSH_TEST_PASSWORD`：测试账号
- `ZZSSH_TEST_KEY_PATH`：测试私钥路径（仓库内 `tests/integration/docker/keys/id_ed25519`）
- 环境变量缺失时所有集成测试 QSKIP（无 Docker 的机器上单测照常跑）

**文件：**
- 创建：`tests/integration/docker/Dockerfile`
- 创建：`tests/integration/docker/entrypoint.sh`
- 创建：`tests/integration/docker/run-integration-tests.sh`
- 创建：`tests/integration/docker/keys/id_ed25519`、`tests/integration/docker/keys/id_ed25519.pub`（步骤 2 生成）
- 创建：`tests/integration/ZzSshTestServerConfig.h`、`tests/integration/ZzSshTestServerConfig.cpp`
- 测试：`tests/integration/tst_ZzSshConnectivityIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：创建 `tests/integration/docker/Dockerfile` 与 `tests/integration/docker/entrypoint.sh`**

`tests/integration/docker/Dockerfile`：

```dockerfile
# ZzSshCore 集成测试用 openssh-server 镜像
# 账号：zztest / zzpass123；密钥：keys/id_ed25519.pub
# 主机密钥在构建期生成并固化（同一镜像多次运行密钥稳定）
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends openssh-server procps \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m zztest && echo 'zztest:zzpass123' | chpasswd

RUN mkdir -p /home/zztest/.ssh && chmod 700 /home/zztest/.ssh
COPY keys/id_ed25519.pub /home/zztest/.ssh/authorized_keys
RUN chmod 600 /home/zztest/.ssh/authorized_keys && chown -R zztest:zztest /home/zztest/.ssh

RUN mkdir -p /run/sshd \
    && sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PubkeyAuthentication.*/PubkeyAuthentication yes/' /etc/ssh/sshd_config

# 构建期生成并固化主机密钥
RUN ssh-keygen -A

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22
ENTRYPOINT ["/entrypoint.sh"]
```

`tests/integration/docker/entrypoint.sh`：

```bash
#!/bin/bash
# 容器入口：ZZ_REGEN_HOSTKEYS=1 时启动前重新生成主机密钥（用于密钥变更测试）
set -e

if [ "${ZZ_REGEN_HOSTKEYS}" = "1" ]; then
    rm -f /etc/ssh/ssh_host_*
    ssh-keygen -A
fi

exec /usr/sbin/sshd -D -e
```

- [ ] **步骤 2：生成测试密钥对并提交**

```bash
mkdir -p tests/integration/docker/keys
ssh-keygen -t ed25519 -f tests/integration/docker/keys/id_ed25519 -N "" -C "zzsshcore-test"
```

预期：生成 `tests/integration/docker/keys/id_ed25519` 与 `id_ed25519.pub`（无 passphrase）。该密钥仅用于测试容器，直接提交仓库。

- [ ] **步骤 3：创建 `tests/integration/docker/run-integration-tests.sh`**

```bash
#!/bin/bash
# 用法: tests/integration/docker/run-integration-tests.sh <build目录>
# 构建测试镜像、启动两个容器（主容器 + 密钥变更容器）、运行 integration/perf 标签的 ctest、清理容器。
set -uo pipefail

BUILD_DIR="${1:?用法: $0 <build目录>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

IMAGE="zzsshcore-test-sshd:latest"
CONTAINER="zzsshcore-sshd"
CHANGED_CONTAINER="zzsshcore-sshd-chg"
PORT=2222
CHANGED_PORT=2223

echo "== 构建测试镜像 =="
docker build -t "${IMAGE}" "${SCRIPT_DIR}" || exit 1

docker rm -f "${CONTAINER}" "${CHANGED_CONTAINER}" >/dev/null 2>&1 || true

echo "== 启动测试容器（127.0.0.1:${PORT} / 127.0.0.1:${CHANGED_PORT}）=="
docker run -d --name "${CONTAINER}" -p "127.0.0.1:${PORT}:22" "${IMAGE}" || exit 1
docker run -d --name "${CHANGED_CONTAINER}" -e ZZ_REGEN_HOSTKEYS=1 -p "127.0.0.1:${CHANGED_PORT}:22" "${IMAGE}" || exit 1

# 等待 sshd 就绪
READY=0
for _ in $(seq 1 30); do
    if (exec 3<>"/dev/tcp/127.0.0.1/${PORT}") 2>/dev/null; then READY=1; break; fi
    sleep 1
done
if [ "${READY}" != "1" ]; then
    echo "错误：测试容器 30 秒内未就绪"
    docker rm -f "${CONTAINER}" "${CHANGED_CONTAINER}" >/dev/null 2>&1
    exit 1
fi

export ZZSSH_TEST_HOST=127.0.0.1
export ZZSSH_TEST_PORT="${PORT}"
export ZZSSH_TEST_CHANGED_PORT="${CHANGED_PORT}"
export ZZSSH_TEST_USER=zztest
export ZZSSH_TEST_PASSWORD=zzpass123
export ZZSSH_TEST_KEY_PATH="${SCRIPT_DIR}/keys/id_ed25519"
export ZZSSH_TEST_MAIN_CONTAINER="${CONTAINER}"
export ZZSSH_TEST_CHANGED_CONTAINER="${CHANGED_CONTAINER}"

echo "== 运行集成与性能测试 =="
ctest --test-dir "${BUILD_DIR}" -L 'integration|perf' --output-on-failure
STATUS=$?

echo "== 清理容器 =="
docker rm -f "${CONTAINER}" "${CHANGED_CONTAINER}" >/dev/null 2>&1 || true
exit ${STATUS}
```

执行 `chmod +x tests/integration/docker/run-integration-tests.sh tests/integration/docker/entrypoint.sh`。

- [ ] **步骤 4：创建公共配置 `tests/integration/ZzSshTestServerConfig.h` 与 `.cpp`**

`tests/integration/ZzSshTestServerConfig.h`：

```cpp
#pragma once

#include <QString>

/**
 * @brief 集成测试服务器配置（从环境变量读取）。
 */
struct ZzSshTestServerConfig {
    QString host;               ///< ZZSSH_TEST_HOST
    quint16 port = 0;           ///< ZZSSH_TEST_PORT
    quint16 changedPort = 0;    ///< ZZSSH_TEST_CHANGED_PORT（密钥变更容器）
    QString user;               ///< ZZSSH_TEST_USER
    QString password;           ///< ZZSSH_TEST_PASSWORD
    QString privateKeyPath;     ///< ZZSSH_TEST_KEY_PATH
    QString mainContainer;      ///< ZZSSH_TEST_MAIN_CONTAINER
    QString changedContainer;   ///< ZZSSH_TEST_CHANGED_CONTAINER

    /** @brief 从环境变量构造。 */
    static ZzSshTestServerConfig fromEnvironment();

    /** @brief 配置是否有效（无效时集成测试应 QSKIP）。 */
    bool isValid() const { return !host.isEmpty() && port != 0 && !user.isEmpty(); }
};
```

`tests/integration/ZzSshTestServerConfig.cpp`：

```cpp
#include "ZzSshTestServerConfig.h"

ZzSshTestServerConfig ZzSshTestServerConfig::fromEnvironment()
{
    ZzSshTestServerConfig c;
    c.host = qEnvironmentVariable("ZZSSH_TEST_HOST");
    c.port = static_cast<quint16>(qEnvironmentVariableIntValue("ZZSSH_TEST_PORT"));
    c.changedPort = static_cast<quint16>(qEnvironmentVariableIntValue("ZZSSH_TEST_CHANGED_PORT"));
    c.user = qEnvironmentVariable("ZZSSH_TEST_USER");
    c.password = qEnvironmentVariable("ZZSSH_TEST_PASSWORD");
    c.privateKeyPath = qEnvironmentVariable("ZZSSH_TEST_KEY_PATH");
    c.mainContainer = qEnvironmentVariable("ZZSSH_TEST_MAIN_CONTAINER", "zzsshcore-sshd");
    c.changedContainer = qEnvironmentVariable("ZZSSH_TEST_CHANGED_CONTAINER", "zzsshcore-sshd-chg");
    return c;
}
```

- [ ] **步骤 5：编写基建验证测试 `tests/integration/tst_ZzSshConnectivityIT.cpp`（先用 QTcpSocket 验证容器可用，与库实现解耦）**

```cpp
#include <QtTest>
#include <QTcpSocket>

#include "ZzSshTestServerConfig.h"

/**
 * @brief 集成测试基建验证：容器可达且返回 SSH 横幅。
 */
class tst_ZzSshConnectivityIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void serverSendsSshBanner();

private:
    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshConnectivityIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试（使用 run-integration-tests.sh 运行）");
}

void tst_ZzSshConnectivityIT::serverSendsSshBanner()
{
    QTcpSocket socket;
    socket.connectToHost(m_cfg.host, m_cfg.port);
    QVERIFY2(socket.waitForConnected(5000), qPrintable(socket.errorString()));
    QVERIFY(socket.waitForReadyRead(5000));
    const QByteArray banner = socket.readAll();
    QVERIFY2(banner.startsWith("SSH-"), qPrintable(QString::fromLatin1(banner.left(64))));
}

QTEST_GUILESS_MAIN(tst_ZzSshConnectivityIT)
#include "tst_ZzSshConnectivityIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
# 集成测试公共配置库
add_library(zzsshcore_itconfig STATIC
    integration/ZzSshTestServerConfig.h
    integration/ZzSshTestServerConfig.cpp
)
target_link_libraries(zzsshcore_itconfig PUBLIC Qt6::Core)
target_include_directories(zzsshcore_itconfig PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/integration")

zz_add_test(tst_ZzSshConnectivityIT integration/tst_ZzSshConnectivityIT.cpp)
target_link_libraries(tst_ZzSshConnectivityIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshConnectivityIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 6：构建并运行集成测试验证基建可用**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：镜像构建成功、容器启动、ctest 输出 `1/1 Test ...: tst_ZzSshConnectivityIT ... Passed`。无 Docker 环境下改为直接 `ctest --preset linux-release`，预期该集成测试显示 `Skipped` 而非失败。

- [ ] **步骤 7：Commit**

```bash
git add tests/integration/docker/Dockerfile tests/integration/docker/entrypoint.sh tests/integration/docker/run-integration-tests.sh tests/integration/docker/keys/id_ed25519 tests/integration/docker/keys/id_ed25519.pub tests/integration/ZzSshTestServerConfig.h tests/integration/ZzSshTestServerConfig.cpp tests/integration/tst_ZzSshConnectivityIT.cpp tests/CMakeLists.txt
git commit -m "test: 新增 Docker openssh-server 集成测试基建"
```

---

### 任务 8：ZzSshConnection 与 ZzSshConnectionWorker（线程模型、连接、密码认证）

核心任务。结构：`ZzSshConnection`（GUI 线程的 QObject）持有 `QThread` 与 `ZzSshConnectionWorker`（moveToThread）；所有阻塞调用在 worker 内串行执行；跨线程只走 queued 信号槽。密码索取与主机密钥确认用 `ZzSshConnectionShared` 里的 `QWaitCondition` 让 worker 阻塞等待 GUI 决策。取消/断开通过 `abortRequested` 原子标志 + `transport->abortBlocking()`（shutdown socket）打断阻塞调用。

认证策略（规格 §4.2）：agent → 公钥（配置路径）→ 密码（信号回调索取，密码错误则再次索取，直到成功或用户取消）。本任务先落地：连接 + 密码认证 + 三种结局信号 + 取消；主机密钥验证在任务 9 补全信号流（本任务的 doConnect 骨架已预留 verifyHostKey 调用点，初版恒返回 true——下一步任务 9 以 TDD 方式替换）。

**文件：**
- 创建：`src/ZzSshAuthConfig.h`
- 创建：`src/ZzSshConnectParams.h`
- 创建：`src/ZzSshConnectionShared.h`、`src/ZzSshConnectionShared.cpp`
- 创建：`src/ZzSshConnectionWorker.h`、`src/ZzSshConnectionWorker.cpp`
- 创建：`src/ZzSshConnection.h`、`src/ZzSshConnection.cpp`
- 测试：`tests/unit/tst_ZzSshConnectionWorker.cpp`（错误路径单测）
- 测试：`tests/integration/tst_ZzSshConnectionIT.cpp`（密码认证集成测试）
- 修改：`CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的单元测试 `tests/unit/tst_ZzSshConnectionWorker.cpp`**

```cpp
#include <QtTest>
#include <QTcpServer>

#include "ZzMockTransport.h"
#include "ZzSshConnectionShared.h"
#include "ZzSshConnectionWorker.h"
#include "ZzSshError.h"
#include "ZzTcpTransport.h"

/**
 * @brief ZzSshConnectionWorker 错误路径的单元测试（不依赖真实 SSH 服务器）。
 */
class tst_ZzSshConnectionWorker : public QObject
{
    Q_OBJECT

private slots:
    void transportOpenFailureEmitsTransportError();
    void handshakeFailureEmitsHandshakeFailed();
    void passwordCancelEmitsAuthenticationCancelled();
    void waitForPasswordRoundTrip();
    void hostKeyDecisionRoundTrip();
};

void tst_ZzSshConnectionWorker::transportOpenFailureEmitsTransportError()
{
    ZzSshConnectionShared shared;
    ZzSshConnectionWorker worker(&shared, [] {
        auto t = std::make_unique<ZzMockTransport>();
        t->setOpenResult(false, QStringLiteral("mock 连接失败"));
        return t;
    });
    QSignalSpy spy(&worker, &ZzSshConnectionWorker::errorOccurred);

    ZzSshConnectParams params;
    params.host = QStringLiteral("127.0.0.1");
    params.port = 22;
    params.user = QStringLiteral("u");
    params.knownHostsPath = QStringLiteral("/nonexistent/kh.json");
    worker.doConnect(params);

    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::TransportError));
    QCOMPARE(spy.first().at(1).toString(), QStringLiteral("mock 连接失败"));
}

void tst_ZzSshConnectionWorker::handshakeFailureEmitsHandshakeFailed()
{
    // 对端是普通 TCP 服务器，接受后立即断开 → libssh2 握手必失败
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost));
    auto tcp = std::make_unique<ZzTcpTransport>();
    QString err;
    QVERIFY(tcp->open(QStringLiteral("127.0.0.1"), server.serverPort(), 3000, &err));
    const qintptr fd = tcp->socketDescriptor();
    QVERIFY(server.waitForNewConnection(1000));
    delete server.nextPendingConnection(); // 服务端立即关闭

    ZzSshConnectionShared shared;
    ZzSshConnectionWorker worker(&shared, [fd] {
        auto t = std::make_unique<ZzMockTransport>();
        t->setSocketDescriptor(fd); // mock 不拥有 fd，由外层 tcp 管理生命周期
        return t;
    });
    QSignalSpy spy(&worker, &ZzSshConnectionWorker::errorOccurred);

    ZzSshConnectParams params;
    params.host = QStringLiteral("127.0.0.1");
    params.port = 22;
    params.user = QStringLiteral("u");
    params.knownHostsPath = QStringLiteral("/nonexistent/kh.json");
    worker.doConnect(params);

    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::HandshakeFailed));
    tcp->close();
}

void tst_ZzSshConnectionWorker::waitForPasswordRoundTrip()
{
    ZzSshConnectionShared shared;
    shared.reset();
    // 模拟 GUI 线程稍后提供密码（必须用真线程：等待会阻塞本线程的事件循环）
    QThread *helper = QThread::create([&shared] {
        QThread::msleep(50);
        shared.providePassword(QStringLiteral("s3cret"));
    });
    helper->start();
    bool cancelled = true;
    const QString pwd = shared.waitForPassword(&cancelled);
    helper->wait();
    delete helper;
    QVERIFY(!cancelled);
    QCOMPARE(pwd, QStringLiteral("s3cret"));
}

void tst_ZzSshConnectionWorker::hostKeyDecisionRoundTrip()
{
    ZzSshConnectionShared shared;
    shared.reset();
    QThread *helper = QThread::create([&shared] {
        QThread::msleep(50);
        shared.decideHostKey(ZzSshConnectionShared::HostKeyDecision::Trust);
    });
    helper->start();
    const auto decision = shared.waitForHostKeyDecision();
    helper->wait();
    delete helper;
    QCOMPARE(decision, ZzSshConnectionShared::HostKeyDecision::Trust);
}

void tst_ZzSshConnectionWorker::passwordCancelEmitsAuthenticationCancelled()
{
    // abort 打断等待：直接置位后 waitForPassword 立即以取消返回
    ZzSshConnectionShared shared;
    shared.reset();
    shared.abortRequested.store(true);
    bool cancelled = false;
    shared.waitForPassword(&cancelled);
    QVERIFY(cancelled);
}

QTEST_MAIN(tst_ZzSshConnectionWorker)
#include "tst_ZzSshConnectionWorker.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshConnectionWorker unit/tst_ZzSshConnectionWorker.cpp)
set_tests_properties(tst_ZzSshConnectionWorker PROPERTIES LABELS "unit")
```

- [ ] **步骤 2：编写失败的集成测试 `tests/integration/tst_ZzSshConnectionIT.cpp`**

```cpp
#include <QtTest>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshError.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief ZzSshConnection 密码认证的集成测试（Docker openssh-server）。
 */
class tst_ZzSshConnectionIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void passwordAuthConnects();
    void wrongPasswordThenCancelFails();
    void connectToRefusedPortFails();

private:
    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshConnectionIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

void tst_ZzSshConnectionIT::passwordAuthConnects()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSshConnection conn;
    conn.setKnownHostsFilePath(dir.filePath(QStringLiteral("known_hosts.json")));
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn.setAuthConfig(auth);

    // 自动信任主机密钥、自动提供密码（模拟上层行为）
    connect(&conn, &ZzSshConnection::hostKeyUnknown, &conn, &ZzSshConnection::trustHostKey,
            Qt::QueuedConnection);
    connect(&conn, &ZzSshConnection::passwordRequested, &conn,
            [this, &conn] { conn.providePassword(m_cfg.password); }, Qt::QueuedConnection);

    QSignalSpy connectedSpy(&conn, &ZzSshConnection::connected);
    QSignalSpy errorSpy(&conn, &ZzSshConnection::errorOccurred);
    QCOMPARE(conn.state(), ZzSshConnection::State::Disconnected);

    conn.connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    QCOMPARE(conn.state(), ZzSshConnection::State::Connecting);

    QVERIFY2(connectedSpy.wait(15000), "连接超时");
    QVERIFY(errorSpy.isEmpty());
    QCOMPARE(conn.state(), ZzSshConnection::State::Connected);

    // 主动断开
    QSignalSpy disconnectedSpy(&conn, &ZzSshConnection::disconnected);
    conn.disconnectFromHost();
    QVERIFY(disconnectedSpy.wait(5000));
    QCOMPARE(conn.state(), ZzSshConnection::State::Disconnected);
}

void tst_ZzSshConnectionIT::wrongPasswordThenCancelFails()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    ZzSshConnection conn;
    conn.setKnownHostsFilePath(dir.filePath(QStringLiteral("known_hosts.json")));
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn.setAuthConfig(auth);

    connect(&conn, &ZzSshConnection::hostKeyUnknown, &conn, &ZzSshConnection::trustHostKey,
            Qt::QueuedConnection);

    QSignalSpy errorSpy(&conn, &ZzSshConnection::errorOccurred);
    // 第一次索取给错误密码，第二次索取取消
    int *requestCount = new int(0);
    connect(&conn, &ZzSshConnection::passwordRequested, &conn,
            [&conn, requestCount] {
                ++(*requestCount);
                if (*requestCount == 1)
                    conn.providePassword(QStringLiteral("wrong-password"));
                else
                    conn.cancelPasswordRequest();
            }, Qt::QueuedConnection);

    conn.connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    QVERIFY2(errorSpy.wait(15000), "未收到错误信号");
    QCOMPARE(errorSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::AuthenticationCancelled));
    QCOMPARE(*requestCount, 2); // 密码错误后应再次索取
    delete requestCount;
}

void tst_ZzSshConnectionIT::connectToRefusedPortFails()
{
    ZzSshConnection conn;
    conn.setKnownHostsFilePath(QStringLiteral("/nonexistent/kh.json"));
    QSignalSpy errorSpy(&conn, &ZzSshConnection::errorOccurred);
    conn.connectToHost(QStringLiteral("127.0.0.1"), 1, QStringLiteral("u")); // 端口 1 必拒绝
    QVERIFY2(errorSpy.wait(10000), "未收到错误信号");
    QCOMPARE(errorSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::TransportError));
    QCOMPARE(conn.state(), ZzSshConnection::State::Disconnected);
}

QTEST_GUILESS_MAIN(tst_ZzSshConnectionIT)
#include "tst_ZzSshConnectionIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshConnectionIT integration/tst_ZzSshConnectionIT.cpp)
target_link_libraries(tst_ZzSshConnectionIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshConnectionIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cmake --build --preset linux-release
```

预期：编译失败，报错找不到 `ZzSshConnection.h` / `ZzSshConnectionWorker.h`。

- [ ] **步骤 4：创建 `src/ZzSshAuthConfig.h` 与 `src/ZzSshConnectParams.h`**

`src/ZzSshAuthConfig.h`：

```cpp
#pragma once

#include <QString>

/**
 * @brief 认证配置。尝试顺序固定为：agent → 公钥 → 密码。
 */
struct ZzSshAuthConfig {
    bool tryAgent = true;           ///< 是否先尝试 SSH agent
    QString privateKeyPath;         ///< 私钥路径；为空则跳过公钥认证
    QString publicKeyPath;          ///< 公钥路径；为空则由 libssh2 按 私钥路径+".pub" 推导
    QString passphrase;             ///< 私钥口令（无口令留空）
    bool allowPassword = true;      ///< 是否允许回退到密码认证（密码通过 passwordRequested 信号索取）
};
```

`src/ZzSshConnectParams.h`：

```cpp
#pragma once

#include <QString>

#include "ZzSshAuthConfig.h"

/**
 * @brief 一次连接所需的全部参数（值类型，跨线程传递）。
 */
struct ZzSshConnectParams {
    QString host;                       ///< 主机地址
    quint16 port = 22;                  ///< 端口号
    QString user;                       ///< 登录用户名
    ZzSshAuthConfig auth;               ///< 认证配置
    QString knownHostsPath;             ///< known_hosts.json 路径
    int keepaliveIntervalSeconds = 0;   ///< keepalive 间隔秒数；0 表示关闭
    int connectTimeoutMs = 10000;       ///< TCP 连接超时毫秒数
};
```

- [ ] **步骤 5：创建 `src/ZzSshConnectionShared.h` 与 `src/ZzSshConnectionShared.cpp`**

`src/ZzSshConnectionShared.h`：

```cpp
#pragma once

#include <QMutex>
#include <QString>
#include <QWaitCondition>

#include <atomic>

/**
 * @brief ZzSshConnection（GUI 线程）与 ZzSshConnectionWorker（工作线程）之间的共享状态。
 *
 * 承载两类需要工作线程阻塞等待 GUI 决策的交互：密码索取、主机密钥确认。
 * 等待循环每 100ms 醒一次检查 abortRequested，保证取消/断开可打断等待。
 */
class ZzSshConnectionShared
{
public:
    /**
     * @brief 主机密钥确认决策。
     */
    enum class HostKeyDecision {
        None,           ///< 尚无决策
        Trust,          ///< 信任并存入 known_hosts
        Reject,         ///< 拒绝
        AcceptChange    ///< 接受密钥变更并更新记录
    };

    std::atomic<bool> abortRequested{false}; ///< 中止标志（GUI 置位，worker 轮询）

    /** @brief 新一轮连接前清空全部等待状态。 */
    void reset();

    /**
     * @brief 工作线程调用：阻塞等待上层提供密码。
     * @param cancelled 输出：用户取消或被中止时为 true。
     * @return 密码；取消时返回空字符串。
     */
    QString waitForPassword(bool *cancelled);

    /** @brief GUI 线程调用：提供密码。 */
    void providePassword(const QString &password);

    /** @brief GUI 线程调用：取消密码输入。 */
    void cancelPassword();

    /**
     * @brief 工作线程调用：阻塞等待主机密钥确认决策。
     * @return 决策；被中止时返回 Reject。
     */
    HostKeyDecision waitForHostKeyDecision();

    /** @brief GUI 线程调用：提交主机密钥决策。 */
    void decideHostKey(HostKeyDecision decision);

private:
    QMutex m_mutex;
    QWaitCondition m_condition;
    bool m_passwordReady = false;
    bool m_passwordCancelled = false;
    QString m_password;
    HostKeyDecision m_hostKeyDecision = HostKeyDecision::None;
};
```

`src/ZzSshConnectionShared.cpp`：

```cpp
#include "ZzSshConnectionShared.h"

void ZzSshConnectionShared::reset()
{
    QMutexLocker locker(&m_mutex);
    abortRequested.store(false);
    m_passwordReady = false;
    m_passwordCancelled = false;
    m_password.clear();
    m_hostKeyDecision = HostKeyDecision::None;
}

QString ZzSshConnectionShared::waitForPassword(bool *cancelled)
{
    QMutexLocker locker(&m_mutex);
    while (true) {
        if (abortRequested.load() || m_passwordCancelled) {
            if (cancelled) *cancelled = true;
            return {};
        }
        if (m_passwordReady) {
            if (cancelled) *cancelled = false;
            m_passwordReady = false;
            const QString result = m_password;
            m_password.clear();
            return result;
        }
        m_condition.wait(&m_mutex, 100);
    }
}

void ZzSshConnectionShared::providePassword(const QString &password)
{
    QMutexLocker locker(&m_mutex);
    m_password = password;
    m_passwordReady = true;
    m_condition.wakeAll();
}

void ZzSshConnectionShared::cancelPassword()
{
    QMutexLocker locker(&m_mutex);
    m_passwordCancelled = true;
    m_condition.wakeAll();
}

ZzSshConnectionShared::HostKeyDecision ZzSshConnectionShared::waitForHostKeyDecision()
{
    QMutexLocker locker(&m_mutex);
    while (true) {
        if (abortRequested.load())
            return HostKeyDecision::Reject;
        if (m_hostKeyDecision != HostKeyDecision::None) {
            const HostKeyDecision d = m_hostKeyDecision;
            m_hostKeyDecision = HostKeyDecision::None;
            return d;
        }
        m_condition.wait(&m_mutex, 100);
    }
}

void ZzSshConnectionShared::decideHostKey(HostKeyDecision decision)
{
    QMutexLocker locker(&m_mutex);
    m_hostKeyDecision = decision;
    m_condition.wakeAll();
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE
    src/ZzSshAuthConfig.h
    src/ZzSshConnectParams.h
    src/ZzSshConnectionShared.h
    src/ZzSshConnectionShared.cpp
)
```

- [ ] **步骤 6：创建 `src/ZzSshConnectionWorker.h` 与 `src/ZzSshConnectionWorker.cpp`**

`src/ZzSshConnectionWorker.h`：

```cpp
#pragma once

#include <QHash>
#include <QMutex>
#include <QObject>
#include <QTimer>

#include <functional>
#include <memory>

#include "ZzSshChannel.h"
#include "ZzSshConnectParams.h"
#include "ZzSshSession.h"

class ZzSshTransport;
class ZzSshConnectionShared;

/**
 * @brief SSH 连接的工作线程执行体（规格 §4.1：每连接一个 QThread，libssh2 阻塞模式）。
 *
 * 本对象 moveToThread 到专属 QThread，所有槽函数在该线程内串行执行。
 * 握手、认证、channel 读写均为阻塞调用；读取泵用 QTimer（5ms）+ waitReadable(0) 轮询驱动，
 * GUI 侧的 write/resize 等 queued 调用在事件循环间隙处理。
 * @note v0.1 每连接只开一个 shell channel：socket 可读即可读该 channel。
 *       未来多 channel（SFTP）时需改为按 block directions 调度。
 */
class ZzSshConnectionWorker : public QObject
{
    Q_OBJECT

public:
    /** @brief 传输层工厂（测试注入 mock 用；默认创建 ZzTcpTransport）。 */
    using TransportFactory = std::function<std::unique_ptr<ZzSshTransport>()>;

    explicit ZzSshConnectionWorker(ZzSshConnectionShared *shared, QObject *parent = nullptr);
    ZzSshConnectionWorker(ZzSshConnectionShared *shared, TransportFactory factory, QObject *parent = nullptr);
    ~ZzSshConnectionWorker() override;

    /** @brief 线程安全：中止进行中的阻塞操作（从 GUI 线程调用）。 */
    void interrupt();

public slots:
    /** @brief 执行完整连接流程：TCP → 握手 → 主机密钥验证 → 认证 → keepalive。 */
    void doConnect(const ZzSshConnectParams &params);

    /** @brief 主动断开并清理（发射 disconnected）。 */
    void doDisconnect();

    /** @brief 打开 shell channel。 */
    void doOpenShell(quint32 channelId, const QString &term, int cols, int rows);

    /** @brief 向 channel 写入数据。 */
    void doWriteChannel(quint32 channelId, const QByteArray &data);

    /** @brief 调整 channel 的 PTY 尺寸。 */
    void doResizeChannel(quint32 channelId, int cols, int rows);

    /** @brief 关闭 channel。 */
    void doCloseChannel(quint32 channelId);

    /** @brief 发送一次 keepalive，失败则判定断线。 */
    void doSendKeepalive();

signals:
    void connected();
    void errorOccurred(int code, const QString &message);
    void disconnected(const QString &reason);
    void passwordRequested();
    void hostKeyUnknown(const QString &host, quint16 port, const QString &keyType,
                        const QString &fingerprint);
    void hostKeyChanged(const QString &host, quint16 port, const QString &keyType,
                        const QString &oldFingerprint, const QString &newFingerprint);
    void shellOpened(quint32 channelId);
    void channelDataReceived(quint32 channelId, const QByteArray &data);
    void channelClosed(quint32 channelId);
    void channelErrorOccurred(quint32 channelId, int code, const QString &message);

private slots:
    void onReadTimer();

private:
    bool verifyHostKey(const QString &host, quint16 port, const QString &storePath,
                       int *codeOut, QString *msgOut);
    bool authenticate(const QString &user, const ZzSshAuthConfig &config, int *codeOut, QString *msgOut);
    bool tryAgentAuth(const QString &user);
    bool tryPublicKeyAuth(const QString &user, const ZzSshAuthConfig &config);
    bool tryPasswordAuth(const QString &user, int *codeOut, QString *msgOut);

    /** @brief 清理 channel/session/transport（不发射信号）。 */
    void teardown();

    ZzSshConnectionShared *m_shared; // 非拥有，归属 ZzSshConnection
    TransportFactory m_factory;
    QMutex m_transportMutex;         // 保护 m_transport（interrupt 跨线程访问）
    std::unique_ptr<ZzSshTransport> m_transport;
    std::unique_ptr<ZzSshSession> m_session;
    QHash<quint32, std::unique_ptr<ZzSshChannel>> m_channels;
    QTimer *m_readTimer = nullptr;
    QTimer *m_keepaliveTimer = nullptr;
};
```

`src/ZzSshConnectionWorker.cpp`：

```cpp
#include "ZzSshConnectionWorker.h"

#include "ZzSshConnectionShared.h"
#include "ZzSshError.h"
#include "ZzSshHostKeyStore.h"
#include "ZzSshTransport.h"
#include "ZzTcpTransport.h"

ZzSshConnectionWorker::ZzSshConnectionWorker(ZzSshConnectionShared *shared, QObject *parent)
    : ZzSshConnectionWorker(shared, TransportFactory{}, parent)
{
}

ZzSshConnectionWorker::ZzSshConnectionWorker(ZzSshConnectionShared *shared, TransportFactory factory,
                                             QObject *parent)
    : QObject(parent)
    , m_shared(shared)
    , m_factory(std::move(factory))
{
    m_readTimer = new QTimer(this);
    m_readTimer->setInterval(5);
    connect(m_readTimer, &QTimer::timeout, this, &ZzSshConnectionWorker::onReadTimer);

    m_keepaliveTimer = new QTimer(this);
    connect(m_keepaliveTimer, &QTimer::timeout, this, &ZzSshConnectionWorker::doSendKeepalive);
}

ZzSshConnectionWorker::~ZzSshConnectionWorker()
{
    teardown();
}

void ZzSshConnectionWorker::interrupt()
{
    m_shared->abortRequested.store(true);
    QMutexLocker locker(&m_transportMutex);
    if (m_transport)
        m_transport->abortBlocking();
}

void ZzSshConnectionWorker::doConnect(const ZzSshConnectParams &params)
{
    teardown();

    {
        QMutexLocker locker(&m_transportMutex);
        m_transport = m_factory ? m_factory() : std::make_unique<ZzTcpTransport>();
    }

    QString err;
    if (!m_transport->open(params.host, params.port, params.connectTimeoutMs, &err)) {
        const bool aborted = m_shared->abortRequested.load();
        emit errorOccurred(static_cast<int>(aborted ? ZzSshErrorCode::Cancelled
                                                    : ZzSshErrorCode::TransportError),
                           aborted ? ZzSshError::message(static_cast<int>(ZzSshErrorCode::Cancelled))
                                   : err);
        teardown();
        return;
    }

    m_session = std::make_unique<ZzSshSession>();
    if (!m_session->isValid()) {
        emit errorOccurred(static_cast<int>(ZzSshErrorCode::InternalError),
                           QStringLiteral("libssh2 会话初始化失败"));
        teardown();
        return;
    }

    if (m_shared->abortRequested.load()) {
        emit errorOccurred(static_cast<int>(ZzSshErrorCode::Cancelled),
                           ZzSshError::message(static_cast<int>(ZzSshErrorCode::Cancelled)));
        teardown();
        return;
    }

    if (!m_session->handshake(m_transport->socketDescriptor(), &err)) {
        const bool aborted = m_shared->abortRequested.load();
        emit errorOccurred(static_cast<int>(aborted ? ZzSshErrorCode::Cancelled
                                                    : ZzSshErrorCode::HandshakeFailed),
                           aborted ? ZzSshError::message(static_cast<int>(ZzSshErrorCode::Cancelled))
                                   : err);
        teardown();
        return;
    }

    int code = 0;
    if (!verifyHostKey(params.host, params.port, params.knownHostsPath, &code, &err)) {
        emit errorOccurred(code, err);
        teardown();
        return;
    }

    if (!authenticate(params.user, params.auth, &code, &err)) {
        emit errorOccurred(code, err);
        teardown();
        return;
    }

    if (params.keepaliveIntervalSeconds > 0) {
        m_session->enableKeepalive(params.keepaliveIntervalSeconds);
        m_keepaliveTimer->start(params.keepaliveIntervalSeconds * 1000);
    }

    emit connected();
}

void ZzSshConnectionWorker::doDisconnect()
{
    if (!m_transport && !m_session)
        return;
    teardown();
    emit disconnected(QStringLiteral("用户主动断开"));
}

void ZzSshConnectionWorker::doOpenShell(quint32 channelId, const QString &term, int cols, int rows)
{
    if (!m_session || !m_session->isValid()) {
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::ChannelOpenFailed),
                                  QStringLiteral("会话未连接"));
        return;
    }
    auto channel = std::make_unique<ZzSshChannel>();
    QString err;
    if (!channel->openShell(*m_session, term, cols, rows, &err)) {
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::ChannelOpenFailed), err);
        return;
    }
    m_channels.insert(channelId, std::move(channel));
    if (!m_readTimer->isActive())
        m_readTimer->start();
    emit shellOpened(channelId);
}

void ZzSshConnectionWorker::doWriteChannel(quint32 channelId, const QByteArray &data)
{
    const auto it = m_channels.find(channelId);
    if (it == m_channels.end())
        return;
    QString err;
    if (!it.value()->write(data, &err))
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::InternalError), err);
}

void ZzSshConnectionWorker::doResizeChannel(quint32 channelId, int cols, int rows)
{
    const auto it = m_channels.find(channelId);
    if (it == m_channels.end())
        return;
    QString err;
    if (!it.value()->resize(cols, rows, &err))
        emit channelErrorOccurred(channelId, static_cast<int>(ZzSshErrorCode::InternalError), err);
}

void ZzSshConnectionWorker::doCloseChannel(quint32 channelId)
{
    const auto it = m_channels.find(channelId);
    if (it == m_channels.end())
        return;
    m_channels.erase(it);
    emit channelClosed(channelId);
}

void ZzSshConnectionWorker::doSendKeepalive()
{
    if (!m_session || !m_session->isValid())
        return;
    if (!m_session->sendKeepalive()) {
        teardown();
        emit disconnected(QStringLiteral("keepalive 失败，连接已断开"));
    }
}

void ZzSshConnectionWorker::onReadTimer()
{
    if (m_channels.isEmpty()) {
        m_readTimer->stop();
        return;
    }
    QMutexLocker locker(&m_transportMutex);
    if (!m_transport || !m_transport->isOpen())
        return;

    const ZzSshWaitResult wr = m_transport->waitReadable(0);
    if (wr == ZzSshWaitResult::Error) {
        locker.unlock();
        teardown();
        emit disconnected(QStringLiteral("连接中断（传输层错误）"));
        return;
    }
    if (wr != ZzSshWaitResult::Readable)
        return;

    // socket 可读：读取所有 channel 的到达数据（v0.1 每连接仅一个 shell channel）
    for (int round = 0; round < 8 && !m_channels.isEmpty(); ++round) {
        bool anyData = false;
        const QList<quint32> ids = m_channels.keys();
        for (const quint32 id : ids) {
            const auto it = m_channels.find(id);
            if (it == m_channels.end())
                continue;
            QByteArray data;
            const qint64 n = it.value()->read(&data, 65536);
            if (n > 0) {
                emit channelDataReceived(id, data);
                anyData = true;
            } else if (n < 0) {
                emit channelErrorOccurred(id, static_cast<int>(ZzSshErrorCode::InternalError),
                                          QStringLiteral("channel 读取失败"));
            }
            if (it.value()->isEof()) {
                m_channels.erase(it);
                emit channelClosed(id);
            }
        }
        if (!anyData || m_transport->waitReadable(0) != ZzSshWaitResult::Readable)
            break;
    }
}

bool ZzSshConnectionWorker::verifyHostKey(const QString &host, quint16 port, const QString &storePath,
                                          int *codeOut, QString *msgOut)
{
    // 任务 9 以 TDD 方式补全信号流；当前骨架恒信任
    Q_UNUSED(host)
    Q_UNUSED(port)
    Q_UNUSED(storePath)
    Q_UNUSED(codeOut)
    Q_UNUSED(msgOut)
    return true;
}

bool ZzSshConnectionWorker::authenticate(const QString &user, const ZzSshAuthConfig &config,
                                         int *codeOut, QString *msgOut)
{
    *codeOut = 0;
    if (config.tryAgent && tryAgentAuth(user))
        return true;
    if (m_shared->abortRequested.load()) {
        *codeOut = static_cast<int>(ZzSshErrorCode::Cancelled);
        *msgOut = ZzSshError::message(*codeOut);
        return false;
    }
    if (!config.privateKeyPath.isEmpty() && tryPublicKeyAuth(user, config))
        return true;
    if (m_shared->abortRequested.load()) {
        *codeOut = static_cast<int>(ZzSshErrorCode::Cancelled);
        *msgOut = ZzSshError::message(*codeOut);
        return false;
    }
    if (config.allowPassword && tryPasswordAuth(user, codeOut, msgOut))
        return true;
    if (*codeOut == 0) {
        *codeOut = static_cast<int>(ZzSshErrorCode::AuthenticationFailed);
        *msgOut = ZzSshError::message(*codeOut);
    }
    return false;
}

bool ZzSshConnectionWorker::tryAgentAuth(const QString &user)
{
    LIBSSH2_AGENT *agent = libssh2_agent_init(m_session->handle());
    if (!agent)
        return false;
    bool ok = false;
    if (libssh2_agent_connect(agent) == LIBSSH2_ERROR_NONE
        && libssh2_agent_list_identities(agent) == LIBSSH2_ERROR_NONE) {
        struct libssh2_agent_publickey *identity = nullptr;
        struct libssh2_agent_publickey *prev = nullptr;
        const QByteArray u = user.toUtf8();
        while (!m_shared->abortRequested.load()) {
            const int rc = libssh2_agent_get_identity(agent, &identity, prev);
            if (rc != 0)
                break; // 1 = 无更多身份；负数 = 错误，放弃 agent
            if (libssh2_agent_userauth(agent, u.constData(), identity) == LIBSSH2_ERROR_NONE) {
                ok = true;
                break;
            }
            prev = identity;
        }
    }
    libssh2_agent_disconnect(agent);
    libssh2_agent_free(agent);
    return ok && libssh2_userauth_authenticated(m_session->handle());
}

bool ZzSshConnectionWorker::tryPublicKeyAuth(const QString &user, const ZzSshAuthConfig &config)
{
    const QByteArray u = user.toUtf8();
    const QByteArray pub = config.publicKeyPath.toUtf8();
    const QByteArray priv = config.privateKeyPath.toUtf8();
    const QByteArray pass = config.passphrase.toUtf8();
    const int rc = libssh2_userauth_publickey_fromfile_ex(
        m_session->handle(), u.constData(), static_cast<unsigned int>(u.size()),
        pub.isEmpty() ? nullptr : pub.constData(),
        priv.constData(),
        pass.isEmpty() ? nullptr : pass.constData());
    return rc == LIBSSH2_ERROR_NONE && libssh2_userauth_authenticated(m_session->handle());
}

bool ZzSshConnectionWorker::tryPasswordAuth(const QString &user, int *codeOut, QString *msgOut)
{
    const QByteArray u = user.toUtf8();
    while (!m_shared->abortRequested.load()) {
        emit passwordRequested();
        bool cancelled = false;
        const QString password = m_shared->waitForPassword(&cancelled);
        if (cancelled || m_shared->abortRequested.load()) {
            *codeOut = static_cast<int>(ZzSshErrorCode::AuthenticationCancelled);
            *msgOut = ZzSshError::message(*codeOut);
            return false;
        }
        const QByteArray p = password.toUtf8();
        const int rc = libssh2_userauth_password_ex(m_session->handle(), u.constData(),
                                                    static_cast<unsigned int>(u.size()),
                                                    p.constData(), static_cast<unsigned int>(p.size()),
                                                    nullptr);
        if (rc == LIBSSH2_ERROR_NONE && libssh2_userauth_authenticated(m_session->handle()))
            return true;
        // 密码错误：继续循环，再次向上层索取
    }
    *codeOut = static_cast<int>(ZzSshErrorCode::Cancelled);
    *msgOut = ZzSshError::message(*codeOut);
    return false;
}

void ZzSshConnectionWorker::teardown()
{
    m_keepaliveTimer->stop();
    m_readTimer->stop();
    m_channels.clear();
    m_session.reset();
    QMutexLocker locker(&m_transportMutex);
    if (m_transport) {
        m_transport->close();
        m_transport.reset();
    }
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshConnectionWorker.h src/ZzSshConnectionWorker.cpp)
```

- [ ] **步骤 7：创建 `src/ZzSshConnection.h` 与 `src/ZzSshConnection.cpp`**

`src/ZzSshConnection.h`：

```cpp
#pragma once

#include <QObject>
#include <QString>
#include <QThread>

#include "ZzSshAuthConfig.h"
#include "ZzSshConnectionShared.h"

class ZzSshConnectionWorker;
class ZzSshShellChannel;

/**
 * @brief SSH 连接对象（GUI 线程创建与持有，规格 §4.2）。
 *
 * 内部独占一个 QThread 工作线程，所有 libssh2 阻塞调用在该线程串行执行；
 * 跨线程只通过 Qt 信号槽（queued connection）通信。
 * 异步操作统一三种结局信号：connected() / errorOccurred(code, message) / disconnected(reason)。
 *
 * @code
 * auto *conn = new ZzSshConnection(this);
 * connect(conn, &ZzSshConnection::connected, this, [] { ... });
 * connect(conn, &ZzSshConnection::passwordRequested, this, [conn] { conn->providePassword("..."); });
 * conn->connectToHost("example.com", 22, "root");
 * @endcode
 */
class ZzSshConnection : public QObject
{
    Q_OBJECT

public:
    /**
     * @brief 连接状态。
     */
    enum class State {
        Disconnected,   ///< 未连接
        Connecting,     ///< 连接/认证进行中
        Connected       ///< 已连接
    };

    explicit ZzSshConnection(QObject *parent = nullptr);
    ~ZzSshConnection() override;

    /** @brief 当前连接状态。 */
    State state() const { return m_state; }

    /** @brief 设置认证配置（connectToHost 之前调用）。 */
    void setAuthConfig(const ZzSshAuthConfig &config) { m_authConfig = config; }

    /** @brief 设置 known_hosts.json 路径（connectToHost 之前调用）。 */
    void setKnownHostsFilePath(const QString &path) { m_knownHostsPath = path; }

    /** @brief 设置 keepalive 间隔秒数，0 关闭（connectToHost 之前调用）。 */
    void setKeepaliveInterval(int seconds) { m_keepaliveInterval = seconds; }

    /** @brief 设置 TCP 连接超时毫秒数（connectToHost 之前调用）。 */
    void setConnectTimeout(int msecs) { m_connectTimeoutMs = msecs; }

    /**
     * @brief 建立到远程主机的 SSH 连接（异步）。
     * @param host 主机地址（IP 或域名）。
     * @param port 端口号（1-65535）。
     * @param user 登录用户名。
     *
     * 完成后发射 connected() 或 errorOccurred()。
     * @note 同一连接对象需等待上一次调用结束（回到 Disconnected 状态）才可再次调用。
     */
    void connectToHost(const QString &host, quint16 port, const QString &user);

    /**
     * @brief 断开连接。
     *
     * Connecting 状态：中止进行中的连接（结局为 errorOccurred(Cancelled)）；
     * Connected 状态：发射 disconnected("用户主动断开")。
     */
    void disconnectFromHost();

    /**
     * @brief 创建一个 shell channel（仅 Connected 状态可用）。
     * @return 通道对象（parent 为本连接）；未连接时返回 nullptr。
     */
    ZzSshShellChannel *createShellChannel();

signals:
    /** @brief 连接与认证完成。 */
    void connected();

    /** @brief 连接失败。code 为 ZzSshErrorCode 或 libssh2 负数透传码。 */
    void errorOccurred(int code, const QString &message);

    /** @brief 连接断开（主动或对端）。 */
    void disconnected(const QString &reason);

    /** @brief 认证流程索取密码：上层应以 providePassword() 或 cancelPasswordRequest() 回应。 */
    void passwordRequested();

    /** @brief 首次连接的主机密钥待确认：上层应以 trustHostKey() 或 rejectHostKey() 回应。 */
    void hostKeyUnknown(const QString &host, quint16 port, const QString &keyType,
                        const QString &fingerprint);

    /** @brief 主机密钥与 known_hosts 记录不一致（安全警告）：上层应以 acceptHostKeyChange() 或 rejectHostKey() 回应。 */
    void hostKeyChanged(const QString &host, quint16 port, const QString &keyType,
                        const QString &oldFingerprint, const QString &newFingerprint);

public slots:
    /** @brief 回应 passwordRequested：提供密码。 */
    void providePassword(const QString &password);

    /** @brief 回应 passwordRequested：取消密码输入。 */
    void cancelPasswordRequest();

    /** @brief 回应 hostKeyUnknown：信任并存入 known_hosts.json。 */
    void trustHostKey();

    /** @brief 回应 hostKeyUnknown/hostKeyChanged：拒绝。 */
    void rejectHostKey();

    /** @brief 回应 hostKeyChanged：接受变更并更新 known_hosts.json。 */
    void acceptHostKeyChange();

private:
    friend class ZzSshShellChannel;

    /** @brief 返回 worker 指针（仅供 ZzSshShellChannel 转发调用）。 */
    ZzSshConnectionWorker *worker() const { return m_worker; }

    void setState(State s) { m_state = s; }

    QThread *m_thread = nullptr;
    ZzSshConnectionWorker *m_worker = nullptr; // 归属 m_thread
    ZzSshConnectionShared m_shared;
    ZzSshAuthConfig m_authConfig;
    QString m_knownHostsPath;
    int m_keepaliveInterval = 0;
    int m_connectTimeoutMs = 10000;
    State m_state = State::Disconnected;
    quint32 m_nextChannelId = 1;
};
```

`src/ZzSshConnection.cpp`：

```cpp
#include "ZzSshConnection.h"

#include "ZzSshConnectionWorker.h"
#include "ZzSshError.h"
#include "ZzSshShellChannel.h"

ZzSshConnection::ZzSshConnection(QObject *parent)
    : QObject(parent)
{
    m_thread = new QThread(this);
    m_worker = new ZzSshConnectionWorker(&m_shared);
    m_worker->moveToThread(m_thread);
    connect(m_thread, &QThread::finished, m_worker, &QObject::deleteLater);

    connect(m_worker, &ZzSshConnectionWorker::connected, this, [this] {
        setState(State::Connected);
        emit connected();
    });
    connect(m_worker, &ZzSshConnectionWorker::errorOccurred, this, [this](int code, const QString &message) {
        setState(State::Disconnected);
        emit errorOccurred(code, message);
    });
    connect(m_worker, &ZzSshConnectionWorker::disconnected, this, [this](const QString &reason) {
        setState(State::Disconnected);
        emit disconnected(reason);
    });
    connect(m_worker, &ZzSshConnectionWorker::passwordRequested, this, &ZzSshConnection::passwordRequested);
    connect(m_worker, &ZzSshConnectionWorker::hostKeyUnknown, this, &ZzSshConnection::hostKeyUnknown);
    connect(m_worker, &ZzSshConnectionWorker::hostKeyChanged, this, &ZzSshConnection::hostKeyChanged);

    m_thread->start();
}

ZzSshConnection::~ZzSshConnection()
{
    m_shared.abortRequested.store(true);
    if (m_worker)
        m_worker->interrupt();
    m_thread->quit();
    m_thread->wait(5000);
    // 线程结束后 worker 已由 deleteLater 销毁；置空防止 channel 析构时访问悬垂指针
    m_worker = nullptr;
}

void ZzSshConnection::connectToHost(const QString &host, quint16 port, const QString &user)
{
    if (m_state != State::Disconnected) {
        emit errorOccurred(static_cast<int>(ZzSshErrorCode::InternalError),
                           QStringLiteral("连接对象正忙，需等待上一次调用结束"));
        return;
    }
    m_shared.reset();
    setState(State::Connecting);

    ZzSshConnectParams params;
    params.host = host;
    params.port = port;
    params.user = user;
    params.auth = m_authConfig;
    params.knownHostsPath = m_knownHostsPath;
    params.keepaliveIntervalSeconds = m_keepaliveInterval;
    params.connectTimeoutMs = m_connectTimeoutMs;

    ZzSshConnectionWorker *w = m_worker;
    QMetaObject::invokeMethod(w, [w, params] { w->doConnect(params); }, Qt::QueuedConnection);
}

void ZzSshConnection::disconnectFromHost()
{
    if (m_state == State::Connecting) {
        m_worker->interrupt(); // doConnect 将以 Cancelled 结束
        return;
    }
    if (m_state == State::Connected) {
        ZzSshConnectionWorker *w = m_worker;
        QMetaObject::invokeMethod(w, [w] { w->doDisconnect(); }, Qt::QueuedConnection);
    }
}

ZzSshShellChannel *ZzSshConnection::createShellChannel()
{
    if (m_state != State::Connected)
        return nullptr;
    auto *channel = new ZzSshShellChannel(m_nextChannelId++, this, this);
    connect(m_worker, &ZzSshConnectionWorker::shellOpened, channel,
            [channel](quint32 id) {
                if (id == channel->channelId())
                    emit channel->shellOpened();
            });
    connect(m_worker, &ZzSshConnectionWorker::channelDataReceived, channel,
            [channel](quint32 id, const QByteArray &data) {
                if (id == channel->channelId())
                    emit channel->dataReceived(data);
            });
    connect(m_worker, &ZzSshConnectionWorker::channelClosed, channel,
            [channel](quint32 id) {
                if (id == channel->channelId())
                    emit channel->closed();
            });
    connect(m_worker, &ZzSshConnectionWorker::channelErrorOccurred, channel,
            [channel](quint32 id, int code, const QString &message) {
                if (id == channel->channelId())
                    emit channel->errorOccurred(code, message);
            });
    return channel;
}

void ZzSshConnection::providePassword(const QString &password)
{
    m_shared.providePassword(password);
}

void ZzSshConnection::cancelPasswordRequest()
{
    m_shared.cancelPassword();
}

void ZzSshConnection::trustHostKey()
{
    m_shared.decideHostKey(ZzSshConnectionShared::HostKeyDecision::Trust);
}

void ZzSshConnection::rejectHostKey()
{
    m_shared.decideHostKey(ZzSshConnectionShared::HostKeyDecision::Reject);
}

void ZzSshConnection::acceptHostKeyChange()
{
    m_shared.decideHostKey(ZzSshConnectionShared::HostKeyDecision::AcceptChange);
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshConnection.h src/ZzSshConnection.cpp)
```

注意：此时 `ZzSshShellChannel` 尚未实现（任务 11）。为让本任务可编译，先创建最小前向实现文件 `src/ZzSshShellChannel.h` / `src/ZzSshShellChannel.cpp`（任务 11 将以 TDD 方式扩展完整功能）：

`src/ZzSshShellChannel.h`：

```cpp
#pragma once

#include <QByteArray>
#include <QObject>

class ZzSshConnection;

/**
 * @brief SSH 交互式 shell 通道（GUI 线程对象，规格 §4.2）。
 *
 * 由 ZzSshConnection::createShellChannel() 创建；所有操作经 queued 调用
 * 转发到连接的工作线程执行。
 */
class ZzSshShellChannel : public QObject
{
    Q_OBJECT

public:
    ~ZzSshShellChannel() override;

    /** @brief 通道 ID（连接内唯一）。 */
    quint32 channelId() const { return m_channelId; }

    /**
     * @brief 打开交互式 shell（异步）。完成后发射 shellOpened() 或 errorOccurred()。
     * @param term 终端类型（如 "xterm-256color"）。
     * @param cols 终端列数。
     * @param rows 终端行数。
     */
    void openShell(const QString &term, int cols, int rows);

    /** @brief 写入数据（异步）。 */
    void write(const QByteArray &data);

    /** @brief 调整 PTY 尺寸（异步）。 */
    void resize(int cols, int rows);

    /** @brief 关闭通道（异步）。完成后发射 closed()。 */
    void closeChannel();

signals:
    /** @brief shell 已打开。 */
    void shellOpened();

    /** @brief 收到远端输出数据。 */
    void dataReceived(const QByteArray &data);

    /** @brief 通道操作失败。 */
    void errorOccurred(int code, const QString &message);

    /** @brief 通道已关闭（主动或对端 EOF）。 */
    void closed();

private:
    friend class ZzSshConnection;

    /** @brief 私有构造：仅 ZzSshConnection 可创建。 */
    ZzSshShellChannel(quint32 channelId, ZzSshConnection *connection, QObject *parent);

    ZzSshConnection *m_connection; // 非拥有
    quint32 m_channelId;
};
```

`src/ZzSshShellChannel.cpp`：

```cpp
#include "ZzSshShellChannel.h"

#include "ZzSshConnection.h"
#include "ZzSshConnectionWorker.h"

ZzSshShellChannel::ZzSshShellChannel(quint32 channelId, ZzSshConnection *connection, QObject *parent)
    : QObject(parent)
    , m_connection(connection)
    , m_channelId(channelId)
{
}

ZzSshShellChannel::~ZzSshShellChannel()
{
    closeChannel();
}

void ZzSshShellChannel::openShell(const QString &term, int cols, int rows)
{
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return; // 连接已销毁
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id, term, cols, rows] { w->doOpenShell(id, term, cols, rows); },
                              Qt::QueuedConnection);
}

void ZzSshShellChannel::write(const QByteArray &data)
{
    if (data.isEmpty())
        return;
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return;
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id, data] { w->doWriteChannel(id, data); }, Qt::QueuedConnection);
}

void ZzSshShellChannel::resize(int cols, int rows)
{
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return;
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id, cols, rows] { w->doResizeChannel(id, cols, rows); },
                              Qt::QueuedConnection);
}

void ZzSshShellChannel::closeChannel()
{
    ZzSshConnectionWorker *w = m_connection->worker();
    if (!w)
        return;
    const quint32 id = m_channelId;
    QMetaObject::invokeMethod(w, [w, id] { w->doCloseChannel(id); }, Qt::QueuedConnection);
}
```

在 `CMakeLists.txt` 的追加区加入：

```cmake
target_sources(zzsshcore PRIVATE src/ZzSshShellChannel.h src/ZzSshShellChannel.cpp)
```

- [ ] **步骤 8：运行单元测试与集成测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
ctest --preset linux-release -R 'tst_ZzSshConnectionWorker'
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：`tst_ZzSshConnectionWorker` 全部 PASS；集成测试中 `tst_ZzSshConnectivityIT` 与 `tst_ZzSshConnectionIT` 均 PASS（`100% tests passed`）。

- [ ] **步骤 9：Commit**

```bash
git add src/ZzSshAuthConfig.h src/ZzSshConnectParams.h src/ZzSshConnectionShared.h src/ZzSshConnectionShared.cpp src/ZzSshConnectionWorker.h src/ZzSshConnectionWorker.cpp src/ZzSshConnection.h src/ZzSshConnection.cpp src/ZzSshShellChannel.h src/ZzSshShellChannel.cpp tests/unit/tst_ZzSshConnectionWorker.cpp tests/integration/tst_ZzSshConnectionIT.cpp CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 新增 ZzSshConnection 每连接一线程模型与密码认证"
```

---

### 任务 9：主机密钥验证（首次确认 + 变更警告）

规格 §八 安全底线。任务 8 的 `verifyHostKey()` 是恒 true 骨架，本任务以 TDD 替换为真实实现：Unknown → 发射 `hostKeyUnknown` 等待决策；Changed → 发射 `hostKeyChanged`（含新旧指纹）等待决策；Trusted → 直接放行。

**文件：**
- 修改：`src/ZzSshConnectionWorker.cpp`（替换 `verifyHostKey` 函数体）
- 测试：`tests/integration/tst_ZzSshHostKeyIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的集成测试 `tests/integration/tst_ZzSshHostKeyIT.cpp`**

```cpp
#include <QtTest>
#include <QProcess>
#include <QTcpSocket>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshError.h"
#include "ZzSshHostKeyStore.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief 主机密钥验证信号流的集成测试。
 */
class tst_ZzSshHostKeyIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void firstConnectEmitsUnknownAndTrustStores();
    void reconnectWithTrustedKeyEmitsNothing();
    void changedKeyEmitsWarningAndRejectFails();
    void changedKeyAcceptUpdatesStore();

private:
    /** @brief 重建密钥变更容器（ZZ_REGEN_HOSTKEYS=1 → 新主机密钥）。 */
    void recreateChangedContainer();
    /** @brief 等待 changedPort 可连接。 */
    bool waitChangedPortReady(int timeoutMs);
    /** @brief 建立一个自动提供密码的连接（不自动处理主机密钥信号）。 */
    std::unique_ptr<ZzSshConnection> makeConnection(const QString &storePath);

    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshHostKeyIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid() || m_cfg.changedPort == 0)
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshHostKeyIT::makeConnection(const QString &storePath)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    return conn;
}

void tst_ZzSshHostKeyIT::firstConnectEmitsUnknownAndTrustStores()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString storePath = dir.filePath(QStringLiteral("known_hosts.json"));

    auto conn = makeConnection(storePath);
    QSignalSpy unknownSpy(conn.get(), &ZzSshConnection::hostKeyUnknown);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    QSignalSpy errorSpy(conn.get(), &ZzSshConnection::errorOccurred);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);

    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    QVERIFY2(connectedSpy.wait(15000), "连接超时");
    QVERIFY(errorSpy.isEmpty());
    QCOMPARE(unknownSpy.count(), 1);
    // 指纹格式：SHA256:<base64>
    QVERIFY(unknownSpy.first().at(3).toString().startsWith(QStringLiteral("SHA256:")));

    // 密钥已存入 known_hosts.json
    ZzSshHostKeyStore store(storePath);
    QVERIFY(store.load());
    QCOMPARE(store.count(), 1);
}

void tst_ZzSshHostKeyIT::reconnectWithTrustedKeyEmitsNothing()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString storePath = dir.filePath(QStringLiteral("known_hosts.json"));

    { // 第一次连接：信任
        auto conn = makeConnection(storePath);
        QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
        QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                         &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
        conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
        QVERIFY(connectedSpy.wait(15000));
    }
    { // 第二次连接：密钥已被信任，不应再发射确认信号
        auto conn = makeConnection(storePath);
        QSignalSpy unknownSpy(conn.get(), &ZzSshConnection::hostKeyUnknown);
        QSignalSpy changedSpy(conn.get(), &ZzSshConnection::hostKeyChanged);
        QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
        conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
        QVERIFY(connectedSpy.wait(15000));
        QCOMPARE(unknownSpy.count(), 0);
        QCOMPARE(changedSpy.count(), 0);
    }
}

void tst_ZzSshHostKeyIT::recreateChangedContainer()
{
    QProcess::execute(QStringLiteral("docker"),
                      {QStringLiteral("rm"), QStringLiteral("-f"), m_cfg.changedContainer});
    const int rc = QProcess::execute(
        QStringLiteral("docker"),
        {QStringLiteral("run"), QStringLiteral("-d"), QStringLiteral("--name"), m_cfg.changedContainer,
         QStringLiteral("-e"), QStringLiteral("ZZ_REGEN_HOSTKEYS=1"),
         QStringLiteral("-p"), QStringLiteral("127.0.0.1:%1:22").arg(m_cfg.changedPort),
         QStringLiteral("zzsshcore-test-sshd:latest")});
    QCOMPARE(rc, 0);
    QVERIFY(waitChangedPortReady(30000));
}

bool tst_ZzSshHostKeyIT::waitChangedPortReady(int timeoutMs)
{
    // 偏差说明（执行后同步）：必须等到读出 SSH banner，而非仅 TCP 连通——
    // docker-proxy 在 sshd 就绪前就会接受连接，裸 waitForConnected 会假就绪。
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < timeoutMs) {
        QTcpSocket socket;
        socket.connectToHost(m_cfg.host, m_cfg.changedPort);
        if (socket.waitForConnected(1000)
                && socket.waitForReadyRead(3000)
                && socket.peek(4) == "SSH-") {
            socket.disconnectFromHost();
            return true;
        }
        QTest::qWait(500);
    }
    return false;
}

void tst_ZzSshHostKeyIT::changedKeyEmitsWarningAndRejectFails()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString storePath = dir.filePath(QStringLiteral("known_hosts.json"));

    // 第一次连接变更容器：信任当前密钥
    {
        auto conn = makeConnection(storePath);
        QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
        QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                         &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
        conn->connectToHost(m_cfg.host, m_cfg.changedPort, m_cfg.user);
        QVERIFY(connectedSpy.wait(15000));
    }

    // 重建容器 → 同地址出现新主机密钥
    recreateChangedContainer();

    // 第二次连接：应发射变更警告；拒绝 → HostKeyMismatch 错误
    auto conn = makeConnection(storePath);
    QSignalSpy changedSpy(conn.get(), &ZzSshConnection::hostKeyChanged);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    QSignalSpy errorSpy(conn.get(), &ZzSshConnection::errorOccurred);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyChanged, conn.get(),
                     [c = conn.get()] {
                         QTimer::singleShot(0, c, &ZzSshConnection::rejectHostKey);
                     },
                     Qt::QueuedConnection);
    conn->connectToHost(m_cfg.host, m_cfg.changedPort, m_cfg.user);

    QVERIFY2(errorSpy.wait(15000), "未收到错误信号");
    QCOMPARE(changedSpy.count(), 1);
    // 新旧指纹都应出现在信号中且不同
    const QString oldFp = changedSpy.first().at(3).toString();
    const QString newFp = changedSpy.first().at(4).toString();
    QVERIFY(oldFp.startsWith(QStringLiteral("SHA256:")));
    QVERIFY(newFp.startsWith(QStringLiteral("SHA256:")));
    QVERIFY(oldFp != newFp);
    QCOMPARE(errorSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::HostKeyMismatch));
    QVERIFY(connectedSpy.isEmpty());
}

void tst_ZzSshHostKeyIT::changedKeyAcceptUpdatesStore()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString storePath = dir.filePath(QStringLiteral("known_hosts.json"));

    // 当前容器密钥先信任，再重建出不同密钥
    {
        auto conn = makeConnection(storePath);
        QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
        QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                         &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
        conn->connectToHost(m_cfg.host, m_cfg.changedPort, m_cfg.user);
        QVERIFY(connectedSpy.wait(15000));
    }
    recreateChangedContainer();

    // 接受变更 → 连接成功
    {
        auto conn = makeConnection(storePath);
        QSignalSpy changedSpy(conn.get(), &ZzSshConnection::hostKeyChanged);
        QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
        QSignalSpy errorSpy(conn.get(), &ZzSshConnection::errorOccurred);
        QObject::connect(conn.get(), &ZzSshConnection::hostKeyChanged, conn.get(),
                         [c = conn.get()] {
                             QTimer::singleShot(0, c, &ZzSshConnection::acceptHostKeyChange);
                         },
                         Qt::QueuedConnection);
        conn->connectToHost(m_cfg.host, m_cfg.changedPort, m_cfg.user);
        QVERIFY2(connectedSpy.wait(15000), "接受变更后应连接成功");
        QCOMPARE(changedSpy.count(), 1);
        QVERIFY(errorSpy.isEmpty());
    }

    // 再次连接：新密钥已记录，无警告直连
    {
        auto conn = makeConnection(storePath);
        QSignalSpy changedSpy(conn.get(), &ZzSshConnection::hostKeyChanged);
        QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
        conn->connectToHost(m_cfg.host, m_cfg.changedPort, m_cfg.user);
        QVERIFY(connectedSpy.wait(15000));
        QCOMPARE(changedSpy.count(), 0);
    }
}

QTEST_MAIN(tst_ZzSshHostKeyIT)
#include "tst_ZzSshHostKeyIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshHostKeyIT integration/tst_ZzSshHostKeyIT.cpp)
target_link_libraries(tst_ZzSshHostKeyIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshHostKeyIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行集成测试验证失败**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：`tst_ZzSshHostKeyIT` 失败——`firstConnectEmitsUnknownAndTrustStores` 中断言 `unknownSpy.count() == 1` 不成立（骨架 verifyHostKey 恒信任，从不发射确认信号）。

- [ ] **步骤 3：替换 `src/ZzSshConnectionWorker.cpp` 中的 `verifyHostKey` 为真实实现**

将整个 `verifyHostKey` 函数（现为恒 true 骨架）替换为：

```cpp
bool ZzSshConnectionWorker::verifyHostKey(const QString &host, quint16 port, const QString &storePath,
                                          int *codeOut, QString *msgOut)
{
    QString keyType;
    const QByteArray rawKey = m_session->hostKey(&keyType);
    if (rawKey.isEmpty()) {
        *codeOut = static_cast<int>(ZzSshErrorCode::HandshakeFailed);
        *msgOut = QStringLiteral("无法获取主机密钥");
        return false;
    }
    const QByteArray fingerprint = ZzSshHostKeyStore::fingerprintSha256(rawKey);

    ZzSshHostKeyStore store(storePath);
    store.load(); // 加载失败按空库处理（首次连接语义）

    const ZzSshHostKeyStore::VerifyResult result = store.verify(host, port, keyType, rawKey);
    if (result == ZzSshHostKeyStore::VerifyResult::Trusted)
        return true;

    if (result == ZzSshHostKeyStore::VerifyResult::Unknown) {
        emit hostKeyUnknown(host, port, keyType, QString::fromLatin1(fingerprint));
        const auto decision = m_shared->waitForHostKeyDecision();
        if (decision == ZzSshConnectionShared::HostKeyDecision::Trust
            && !m_shared->abortRequested.load()) {
            store.add(host, port, keyType, rawKey);
            store.save();
            return true;
        }
        *codeOut = m_shared->abortRequested.load()
                       ? static_cast<int>(ZzSshErrorCode::Cancelled)
                       : static_cast<int>(ZzSshErrorCode::HostKeyRejected);
        *msgOut = ZzSshError::message(*codeOut);
        return false;
    }

    // Changed：已有记录但密钥不一致 → 安全警告
    QString oldKeyType;
    QByteArray oldKey;
    QByteArray oldFingerprint;
    if (store.storedEntry(host, port, &oldKeyType, &oldKey))
        oldFingerprint = ZzSshHostKeyStore::fingerprintSha256(oldKey);
    emit hostKeyChanged(host, port, keyType, QString::fromLatin1(oldFingerprint),
                        QString::fromLatin1(fingerprint));
    const auto decision = m_shared->waitForHostKeyDecision();
    if (decision == ZzSshConnectionShared::HostKeyDecision::AcceptChange
        && !m_shared->abortRequested.load()) {
        store.add(host, port, keyType, rawKey);
        store.save();
        return true;
    }
    *codeOut = m_shared->abortRequested.load()
                   ? static_cast<int>(ZzSshErrorCode::Cancelled)
                   : static_cast<int>(ZzSshErrorCode::HostKeyMismatch);
    *msgOut = ZzSshError::message(*codeOut);
    return false;
}
```

- [ ] **步骤 4：运行集成测试验证通过**

```bash
cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：全部 integration 测试 `Passed`（`100% tests passed`）。

- [ ] **步骤 5：Commit**

```bash
git add src/ZzSshConnectionWorker.cpp tests/integration/tst_ZzSshHostKeyIT.cpp tests/CMakeLists.txt
git commit -m "feat: 实现主机密钥首次确认与变更警告信号流"
```

---

### 任务 10：认证顺序验收（agent → 公钥 → 密码）

认证逻辑已在任务 8 实现，本任务补全集成验收：公钥认证直连、公钥失败回退密码、所有方式关闭时报 `AuthenticationFailed`。若测试红灯则回到任务 8 的实现中修复（不允许改测试以迁就实现）。

**文件：**
- 测试：`tests/integration/tst_ZzSshAuthIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写集成测试 `tests/integration/tst_ZzSshAuthIT.cpp`**

```cpp
#include <QtTest>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshError.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief 认证策略（agent → 公钥 → 密码）的集成测试。
 */
class tst_ZzSshAuthIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void publicKeyAuthConnects();
    void invalidKeyPathFallsBackToPassword();
    void noMethodsAvailableFails();

private:
    std::unique_ptr<ZzSshConnection> makeAutoTrustConnection(const QString &storePath);
    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshAuthIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshAuthIT::makeAutoTrustConnection(const QString &storePath)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    return conn;
}

void tst_ZzSshAuthIT::publicKeyAuthConnects()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeAutoTrustConnection(dir.filePath(QStringLiteral("known_hosts.json")));
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    auth.privateKeyPath = m_cfg.privateKeyPath;
    auth.allowPassword = false; // 只允许公钥：成功即证明公钥路径生效
    conn->setAuthConfig(auth);

    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    QSignalSpy errorSpy(conn.get(), &ZzSshConnection::errorOccurred);
    QSignalSpy pwdSpy(conn.get(), &ZzSshConnection::passwordRequested);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);

    QVERIFY2(connectedSpy.wait(15000), "公钥认证连接超时");
    QVERIFY(errorSpy.isEmpty());
    QCOMPARE(pwdSpy.count(), 0); // 不应索取密码
}

void tst_ZzSshAuthIT::invalidKeyPathFallsBackToPassword()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeAutoTrustConnection(dir.filePath(QStringLiteral("known_hosts.json")));
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    auth.privateKeyPath = QStringLiteral("/nonexistent/id_ed25519"); // 必失败
    auth.allowPassword = true;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);

    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    QSignalSpy pwdSpy(conn.get(), &ZzSshConnection::passwordRequested);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);

    QVERIFY2(connectedSpy.wait(15000), "回退密码认证连接超时");
    QVERIFY(pwdSpy.count() >= 1); // 公钥失败后回退到了密码
}

void tst_ZzSshAuthIT::noMethodsAvailableFails()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeAutoTrustConnection(dir.filePath(QStringLiteral("known_hosts.json")));
    ZzSshAuthConfig auth;
    auth.tryAgent = false;      // 不试 agent
    auth.privateKeyPath.clear(); // 不试公钥
    auth.allowPassword = false;  // 不试密码
    conn->setAuthConfig(auth);

    QSignalSpy errorSpy(conn.get(), &ZzSshConnection::errorOccurred);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);

    QVERIFY2(errorSpy.wait(15000), "未收到错误信号");
    QCOMPARE(errorSpy.first().at(0).toInt(), static_cast<int>(ZzSshErrorCode::AuthenticationFailed));
}

QTEST_GUILESS_MAIN(tst_ZzSshAuthIT)
#include "tst_ZzSshAuthIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshAuthIT integration/tst_ZzSshAuthIT.cpp)
target_link_libraries(tst_ZzSshAuthIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshAuthIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行集成测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：全部 integration 测试 `Passed`。

- [ ] **步骤 3：Commit**

```bash
git add tests/integration/tst_ZzSshAuthIT.cpp tests/CMakeLists.txt
git commit -m "test: 新增认证顺序（agent→公钥→密码）集成验收"
```

---

### 任务 11：ZzSshShellChannel 集成验收（openShell / write / dataReceived / resize / closed）

转发层与 worker 槽已在任务 8 随编译依赖落地，本任务补全集成验收：shell 打开、命令回显、双向收发、PTY resize、对端 exit 关闭通道。若有红灯，修复 `ZzSshShellChannel` / `ZzSshConnectionWorker` 实现（不改测试迁就实现）。

**文件：**
- 测试：`tests/integration/tst_ZzSshShellChannelIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写集成测试 `tests/integration/tst_ZzSshShellChannelIT.cpp`**

```cpp
#include <QtTest>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshShellChannel.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief ZzSshShellChannel 的集成测试（Docker openssh-server）。
 */
class tst_ZzSshShellChannelIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void openShellAndEcho();
    void resizeChangesPtySize();
    void remoteExitClosesChannel();
    void createChannelBeforeConnectReturnsNull();

private:
    /** @brief 建立已认证连接（自动信任主机密钥、自动提供密码）。 */
    std::unique_ptr<ZzSshConnection> makeConnected(const QString &storePath);
    /** @brief 打开 shell 并等待 shellOpened。 */
    ZzSshShellChannel *openShellOrSkip(ZzSshConnection *conn);
    /** @brief 累积通道数据直到包含 needle 或超时。 */
    bool waitForData(ZzSshShellChannel *channel, const QByteArray &needle, int timeoutMs);

    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshShellChannelIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshShellChannelIT::makeConnected(const QString &storePath)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    return conn;
}

ZzSshShellChannel *tst_ZzSshShellChannelIT::openShellOrSkip(ZzSshConnection *conn)
{
    ZzSshShellChannel *channel = conn->createShellChannel();
    if (!channel)
        return nullptr;
    QSignalSpy openSpy(channel, &ZzSshShellChannel::shellOpened);
    channel->openShell(QStringLiteral("xterm-256color"), 80, 24);
    if (!openSpy.wait(10000))
        return nullptr;
    return channel;
}

bool tst_ZzSshShellChannelIT::waitForData(ZzSshShellChannel *channel, const QByteArray &needle,
                                          int timeoutMs)
{
    QSignalSpy spy(channel, &ZzSshShellChannel::dataReceived);
    QByteArray accum;
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < timeoutMs) {
        for (const QList<QVariant> &item : spy)
            accum += item.at(0).toByteArray();
        spy.clear();
        if (accum.contains(needle))
            return true;
        spy.wait(500);
    }
    qWarning() << "waitForData 超时，已收到:" << accum.right(512);
    return false;
}

void tst_ZzSshShellChannelIT::openShellAndEcho()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = openShellOrSkip(conn.get());
    QVERIFY(channel != nullptr);

    channel->write("echo ZZ_MARK_42\n");
    QVERIFY(waitForData(channel, QByteArray("ZZ_MARK_42"), 10000));
}

void tst_ZzSshShellChannelIT::resizeChangesPtySize()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = openShellOrSkip(conn.get());
    QVERIFY(channel != nullptr);

    channel->write("stty size\n");
    QVERIFY(waitForData(channel, QByteArray("24 80"), 10000)); // 初始 80x24

    channel->resize(100, 40);
    QTest::qWait(300); // 等 resize 经 queued 调用抵达工作线程
    channel->write("stty size\n");
    QVERIFY(waitForData(channel, QByteArray("40 100"), 10000)); // resize 后 100x40
}

void tst_ZzSshShellChannelIT::remoteExitClosesChannel()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = openShellOrSkip(conn.get());
    QVERIFY(channel != nullptr);

    QSignalSpy closedSpy(channel, &ZzSshShellChannel::closed);
    channel->write("exit\n");
    QVERIFY2(closedSpy.wait(10000), "对端 exit 后应收到 closed 信号");
}

void tst_ZzSshShellChannelIT::createChannelBeforeConnectReturnsNull()
{
    ZzSshConnection conn; // 未连接
    QCOMPARE(conn.createShellChannel(), nullptr);
}

QTEST_GUILESS_MAIN(tst_ZzSshShellChannelIT)
#include "tst_ZzSshShellChannelIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshShellChannelIT integration/tst_ZzSshShellChannelIT.cpp)
target_link_libraries(tst_ZzSshShellChannelIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshShellChannelIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行集成测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：全部 integration 测试 `Passed`。

- [ ] **步骤 3：Commit**

```bash
git add tests/integration/tst_ZzSshShellChannelIT.cpp tests/CMakeLists.txt
git commit -m "test: 新增 ZzSshShellChannel 收发/resize/关闭集成验收"
```

---

### 任务 12：断线感知、主动断开与 keepalive 验收

覆盖规格 §九 的断线场景：对端会话被杀 → `disconnected`；Connecting 中取消 → `errorOccurred(Cancelled)`；keepalive 开启时连接保持可用。

**文件：**
- 测试：`tests/integration/tst_ZzSshDisconnectIT.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写集成测试 `tests/integration/tst_ZzSshDisconnectIT.cpp`**

```cpp
#include <QtTest>
#include <QProcess>
#include <QTemporaryDir>

#include "ZzSshConnection.h"
#include "ZzSshError.h"
#include "ZzSshShellChannel.h"
#include "ZzSshTestServerConfig.h"

/**
 * @brief 断线与 keepalive 的集成测试。
 */
class tst_ZzSshDisconnectIT : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void remoteSessionKillDisconnects();
    void abortDuringConnectYieldsTerminalSignal();
    void keepaliveKeepsConnectionAlive();

private:
    std::unique_ptr<ZzSshConnection> makeConnected(const QString &storePath, int keepaliveSeconds = 0);
    ZzSshTestServerConfig m_cfg;
};

void tst_ZzSshDisconnectIT::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过集成测试");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshDisconnectIT::makeConnected(const QString &storePath,
                                                                      int keepaliveSeconds)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    if (keepaliveSeconds > 0)
        conn->setKeepaliveInterval(keepaliveSeconds);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    return conn;
}

void tst_ZzSshDisconnectIT::remoteSessionKillDisconnects()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")));
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = conn->createShellChannel();
    QVERIFY(channel != nullptr);
    QSignalSpy openSpy(channel, &ZzSshShellChannel::shellOpened);
    channel->openShell(QStringLiteral("xterm-256color"), 80, 24);
    QVERIFY(openSpy.wait(10000));

    QSignalSpy channelClosedSpy(channel, &ZzSshShellChannel::closed);
    QSignalSpy disconnectedSpy(conn.get(), &ZzSshConnection::disconnected);

    // 杀掉容器内该用户的 sshd 会话进程（主 sshd 不受影响）
    QProcess::execute(QStringLiteral("docker"),
                      {QStringLiteral("exec"), m_cfg.mainContainer,
                       QStringLiteral("pkill"), QStringLiteral("-9"), QStringLiteral("-f"),
                       QStringLiteral("sshd: zztest")});

    // 读取泵应在 socket EOF 后感知：channel closed（随后可能伴随 disconnected）
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < 10000 && channelClosedSpy.isEmpty() && disconnectedSpy.isEmpty())
        QTest::qWait(200);
    QVERIFY2(!channelClosedSpy.isEmpty() || !disconnectedSpy.isEmpty(),
             "对端会话被杀后 10 秒内未感知断线");
}

void tst_ZzSshDisconnectIT::abortDuringConnectYieldsTerminalSignal()
{
    // 10.255.255.1 不可路由：connect 将阻塞直到超时；我们在中途打断
    ZzSshConnection conn;
    conn.setConnectTimeout(30000);
    QSignalSpy errorSpy(&conn, &ZzSshConnection::errorOccurred);
    conn.connectToHost(QStringLiteral("10.255.255.1"), 22, QStringLiteral("u"));
    QCOMPARE(conn.state(), ZzSshConnection::State::Connecting);
    QTest::qWait(200);
    conn.disconnectFromHost();

    QVERIFY2(errorSpy.wait(10000), "取消连接未产生结局信号");
    const int code = errorSpy.first().at(0).toInt();
    // 取消到达前若内核已判不可达则为 TransportError，否则为 Cancelled，两者都是合法结局
    QVERIFY(code == static_cast<int>(ZzSshErrorCode::Cancelled)
            || code == static_cast<int>(ZzSshErrorCode::TransportError));
    QCOMPARE(conn.state(), ZzSshConnection::State::Disconnected);
}

void tst_ZzSshDisconnectIT::keepaliveKeepsConnectionAlive()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    auto conn = makeConnected(dir.filePath(QStringLiteral("known_hosts.json")), 1 /* 1 秒 keepalive */);
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = conn->createShellChannel();
    QVERIFY(channel != nullptr);
    QSignalSpy openSpy(channel, &ZzSshShellChannel::shellOpened);
    channel->openShell(QStringLiteral("xterm-256color"), 80, 24);
    QVERIFY(openSpy.wait(10000));

    QSignalSpy disconnectedSpy(conn.get(), &ZzSshConnection::disconnected);
    QSignalSpy errorSpy(conn.get(), &ZzSshConnection::errorOccurred);
    QTest::qWait(3000); // 经历约 3 次 keepalive

    // 连接仍可用
    QSignalSpy dataSpy(channel, &ZzSshShellChannel::dataReceived);
    channel->write("echo ZZ_KEEPALIVE_OK\n");
    QByteArray accum;
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < 10000) {
        for (const QList<QVariant> &item : dataSpy)
            accum += item.at(0).toByteArray();
        dataSpy.clear();
        if (accum.contains("ZZ_KEEPALIVE_OK"))
            break;
        dataSpy.wait(500);
    }
    QVERIFY2(accum.contains("ZZ_KEEPALIVE_OK"), "keepalive 期间连接不可用");
    QVERIFY(disconnectedSpy.isEmpty());
    QVERIFY(errorSpy.isEmpty());
}

QTEST_GUILESS_MAIN(tst_ZzSshDisconnectIT)
#include "tst_ZzSshDisconnectIT.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_test(tst_ZzSshDisconnectIT integration/tst_ZzSshDisconnectIT.cpp)
target_link_libraries(tst_ZzSshDisconnectIT PRIVATE zzsshcore_itconfig)
set_tests_properties(tst_ZzSshDisconnectIT PROPERTIES LABELS "integration")
```

- [ ] **步骤 2：运行集成测试验证通过**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：全部 integration 测试 `Passed`。

- [ ] **步骤 3：Commit**

```bash
git add tests/integration/tst_ZzSshDisconnectIT.cpp tests/CMakeLists.txt
git commit -m "test: 新增断线感知/主动断开/keepalive 集成验收"
```

---

### 任务 13：性能测试门控与记录（规格 §9.1）

两个性能项（本地 Docker 回环环境下的 ZzSshCore 层指标）：

| 测试项 | 阈值 | 说明 |
| ------ | ---- | ---- |
| `connect-password-local` | 平均 ≤ 2000 ms | 密码认证完整连接（TCP+握手+主机密钥+认证），5 次取平均；本地容器实际约几十 ms，阈值留足 CI 余量 |
| `shell-echo-throughput` | ≥ 2 MB/s | shell 通道经 `cat` 回显 1MB 数据的双向吞吐 |

门控规则：仅 Release 构建有效（非 Release 直接 QSKIP）；阈值不达标即 `QVERIFY` 失败；结果写入 `tests/perf/records/YYYY-MM-DD-zzsshcore.json`（阈值、实测值、环境信息、git commit hash、时间）并提交。

**文件：**
- 创建：`tests/perf/tst_ZzSshPerf.cpp`
- 创建：`tests/perf/records/.gitkeep`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：创建 `tests/perf/records/.gitkeep`（空文件）**

```bash
mkdir -p tests/perf/records && touch tests/perf/records/.gitkeep
```

- [ ] **步骤 2：创建 `tests/perf/tst_ZzSshPerf.cpp`**

```cpp
#include <QtTest>
#include <QTemporaryDir>

#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

#if defined(Q_OS_MACOS)
#  include <sys/sysctl.h>
#elif defined(Q_OS_WIN)
#  include <windows.h>
#endif

#include "ZzSshConnection.h"
#include "ZzSshShellChannel.h"
#include "ZzSshTestServerConfig.h"

#ifndef ZZ_PERF_RECORDS_DIR
#  define ZZ_PERF_RECORDS_DIR "."
#endif
#ifndef ZZ_GIT_COMMIT
#  define ZZ_GIT_COMMIT "unknown"
#endif
#ifndef ZZ_BUILD_TYPE
#  define ZZ_BUILD_TYPE "unknown"
#endif

namespace {

/** @brief 返回物理内存总量（MB），无法获取时返回 -1。 */
qint64 totalMemoryMB()
{
#if defined(Q_OS_LINUX)
    QFile f(QStringLiteral("/proc/meminfo"));
    if (f.open(QIODevice::ReadOnly)) {
        const QByteArray content = f.readAll();
        const qsizetype pos = content.indexOf("MemTotal:");
        if (pos >= 0) {
            const QByteArray line = content.mid(pos, content.indexOf('\n', pos) - pos);
            return line.split(' ').filter([](const QByteArray &p) { return !p.isEmpty(); })
                       .value(1)
                       .toLongLong()
                   / 1024;
        }
    }
    return -1;
#elif defined(Q_OS_MACOS)
    int mib[2] = {CTL_HW, HW_MEMSIZE};
    int64_t mem = 0;
    size_t len = sizeof(mem);
    if (sysctl(mib, 2, &mem, &len, nullptr, 0) == 0)
        return mem / 1048576;
    return -1;
#elif defined(Q_OS_WIN)
    MEMORYSTATUSEX status{sizeof(status)};
    if (GlobalMemoryStatusEx(&status))
        return static_cast<qint64>(status.ullTotalPhys / 1048576);
    return -1;
#else
    return -1;
#endif
}

/** @brief 返回编译器描述字符串。 */
QString compilerString()
{
#if defined(Q_CC_CLANG)
    return QStringLiteral("Clang %1.%2").arg(__clang_major__).arg(__clang_minor__);
#elif defined(Q_CC_GNU)
    return QStringLiteral("GCC %1.%2").arg(__GNUC__).arg(__GNUC_MINOR__);
#elif defined(Q_CC_MSVC)
    return QStringLiteral("MSVC %1").arg(_MSC_VER);
#else
    return QStringLiteral("unknown");
#endif
}

} // namespace

/**
 * @brief ZzSshCore 性能门控测试（规格 §9.1：Release 构建、阈值失败即测试失败、结果落盘）。
 */
class tst_ZzSshPerf : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void performanceGate();

private:
    std::unique_ptr<ZzSshConnection> connectOnce(const QString &storePath, qint64 *elapsedMs);
    void writeRecord(double connectAvgMs, double throughputMbps);

    ZzSshTestServerConfig m_cfg;

    static constexpr double CONNECT_THRESHOLD_MS = 2000.0;  ///< 连接平均耗时阈值
    static constexpr double THROUGHPUT_THRESHOLD_MBPS = 2.0; ///< shell 吞吐阈值
    static constexpr int CONNECT_SAMPLES = 5;
    static constexpr qint64 THROUGHPUT_BYTES = 1024 * 1024;  ///< 吞吐测量数据量：1MB
};

void tst_ZzSshPerf::initTestCase()
{
    m_cfg = ZzSshTestServerConfig::fromEnvironment();
    if (!m_cfg.isValid())
        QSKIP("未设置 ZZSSH_TEST_* 环境变量，跳过性能测试（使用 run-integration-tests.sh 运行）");
    if (QStringLiteral(ZZ_BUILD_TYPE) != QLatin1String("Release"))
        QSKIP("性能测试仅在 Release 构建下有效（规格 §9.1）");
}

std::unique_ptr<ZzSshConnection> tst_ZzSshPerf::connectOnce(const QString &storePath,
                                                            qint64 *elapsedMs)
{
    auto conn = std::make_unique<ZzSshConnection>();
    conn->setKnownHostsFilePath(storePath);
    ZzSshAuthConfig auth;
    auth.tryAgent = false;
    conn->setAuthConfig(auth);
    QObject::connect(conn.get(), &ZzSshConnection::hostKeyUnknown, conn.get(),
                     &ZzSshConnection::trustHostKey, Qt::QueuedConnection);
    QObject::connect(conn.get(), &ZzSshConnection::passwordRequested, conn.get(),
                     [this, c = conn.get()] { c->providePassword(m_cfg.password); },
                     Qt::QueuedConnection);
    QSignalSpy connectedSpy(conn.get(), &ZzSshConnection::connected);
    QElapsedTimer timer;
    timer.start();
    conn->connectToHost(m_cfg.host, m_cfg.port, m_cfg.user);
    if (!connectedSpy.wait(15000))
        return nullptr;
    *elapsedMs = timer.elapsed();
    return conn;
}

void tst_ZzSshPerf::performanceGate()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());

    // ---- 测试项 1：密码认证连接平均耗时 ----
    qint64 totalMs = 0;
    for (int i = 0; i < CONNECT_SAMPLES; ++i) {
        qint64 elapsed = 0;
        auto conn = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &elapsed);
        QVERIFY2(conn != nullptr, "性能测试连接失败");
        totalMs += elapsed;
    }
    const double connectAvgMs = static_cast<double>(totalMs) / CONNECT_SAMPLES;

    // ---- 测试项 2：shell 通道回显吞吐 ----
    qint64 elapsed = 0;
    auto conn = connectOnce(dir.filePath(QStringLiteral("known_hosts.json")), &elapsed);
    QVERIFY(conn != nullptr);
    ZzSshShellChannel *channel = conn->createShellChannel();
    QVERIFY(channel != nullptr);
    QSignalSpy openSpy(channel, &ZzSshShellChannel::shellOpened);
    channel->openShell(QStringLiteral("xterm-256color"), 80, 24);
    QVERIFY(openSpy.wait(10000));

    QSignalSpy dataSpy(channel, &ZzSshShellChannel::dataReceived);
    channel->write("stty -echo; cat\n"); // 关闭终端回显并进入 cat，保证计量只统计 cat 的返回数据
    QTest::qWait(1000);                 // 等远端命令就绪

    // 按 2KB 一行发送（PTY 规范模式按行缓冲且 MAX_CANON 受限，行尾必须带换行）
    const QByteArray line = QByteArray(2047, 'A') + '\n';
    const int lineCount = static_cast<int>(THROUGHPUT_BYTES / line.size());
    qint64 received = 0;
    QElapsedTimer timer;
    timer.start();
    for (int i = 0; i < lineCount; ++i)
        channel->write(line);
    while (received < qint64(lineCount) * line.size() && timer.elapsed() < 60000) {
        for (const QList<QVariant> &item : dataSpy)
            received += item.at(0).toByteArray().size();
        dataSpy.clear();
        dataSpy.wait(500);
    }
    const double seconds = timer.elapsed() / 1000.0;
    QVERIFY2(received >= qint64(lineCount) * line.size(), "回显数据量不足，连接可能中断");
    const double throughputMbps =
        (static_cast<double>(lineCount) * line.size() / 1048576.0) / seconds;

    // ---- 落盘记录（无论阈值是否通过都记录，便于回退排查）----
    writeRecord(connectAvgMs, throughputMbps);

    // ---- 门控断言 ----
    QVERIFY2(connectAvgMs <= CONNECT_THRESHOLD_MS,
             qPrintable(QStringLiteral("连接平均耗时 %1 ms 超过阈值 %2 ms")
                            .arg(connectAvgMs).arg(CONNECT_THRESHOLD_MS)));
    QVERIFY2(throughputMbps >= THROUGHPUT_THRESHOLD_MBPS,
             qPrintable(QStringLiteral("shell 吞吐 %1 MB/s 低于阈值 %2 MB/s")
                            .arg(throughputMbps).arg(THROUGHPUT_THRESHOLD_MBPS)));
}

void tst_ZzSshPerf::writeRecord(double connectAvgMs, double throughputMbps)
{
    QJsonObject env;
    env.insert(QStringLiteral("os"), QSysInfo::prettyProductName());
    env.insert(QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture());
    env.insert(QStringLiteral("memory_mb"), static_cast<double>(totalMemoryMB()));
    env.insert(QStringLiteral("qt_version"), QString::fromLatin1(qVersion()));
    env.insert(QStringLiteral("compiler"), compilerString());
    env.insert(QStringLiteral("build_type"), QStringLiteral(ZZ_BUILD_TYPE));

    QJsonArray records;
    QJsonObject r1;
    r1.insert(QStringLiteral("name"), QStringLiteral("connect-password-local"));
    r1.insert(QStringLiteral("unit"), QStringLiteral("ms"));
    r1.insert(QStringLiteral("threshold"), CONNECT_THRESHOLD_MS);
    r1.insert(QStringLiteral("measured"), connectAvgMs);
    r1.insert(QStringLiteral("passed"), connectAvgMs <= CONNECT_THRESHOLD_MS);
    records.append(r1);
    QJsonObject r2;
    r2.insert(QStringLiteral("name"), QStringLiteral("shell-echo-throughput"));
    r2.insert(QStringLiteral("unit"), QStringLiteral("MB/s"));
    r2.insert(QStringLiteral("threshold"), THROUGHPUT_THRESHOLD_MBPS);
    r2.insert(QStringLiteral("measured"), throughputMbps);
    r2.insert(QStringLiteral("passed"), throughputMbps >= THROUGHPUT_THRESHOLD_MBPS);
    records.append(r2);

    QJsonObject root;
    root.insert(QStringLiteral("feature"), QStringLiteral("zzsshcore"));
    root.insert(QStringLiteral("timestamp"),
                QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
    root.insert(QStringLiteral("git_commit"), QStringLiteral(ZZ_GIT_COMMIT));
    root.insert(QStringLiteral("environment"), env);
    root.insert(QStringLiteral("records"), records);

    const QString fileName = QDate::currentDate().toString(QStringLiteral("yyyy-MM-dd"))
                             + QStringLiteral("-zzsshcore.json");
    const QString path = QStringLiteral(ZZ_PERF_RECORDS_DIR) + QLatin1Char('/') + fileName;
    QFile f(path);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        qWarning() << "无法写入性能记录文件:" << path;
        return;
    }
    f.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    qInfo() << "性能记录已写入:" << path;
}

QTEST_GUILESS_MAIN(tst_ZzSshPerf)
#include "tst_ZzSshPerf.moc"
```

在 `tests/CMakeLists.txt` 末尾追加：

```cmake
# git commit hash 注入（性能记录用）
execute_process(
    COMMAND git rev-parse --short HEAD
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    OUTPUT_VARIABLE ZZ_GIT_COMMIT
    OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET
)

zz_add_test(tst_ZzSshPerf perf/tst_ZzSshPerf.cpp)
target_link_libraries(tst_ZzSshPerf PRIVATE zzsshcore_itconfig)
target_compile_definitions(tst_ZzSshPerf PRIVATE
    ZZ_PERF_RECORDS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/perf/records"
    ZZ_GIT_COMMIT="${ZZ_GIT_COMMIT}"
    ZZ_BUILD_TYPE="$<CONFIG>"
)
set_tests_properties(tst_ZzSshPerf PROPERTIES LABELS "perf")
```

- [ ] **步骤 3：Release 构建并运行性能测试**

```bash
cmake --preset linux-release && cmake --build --preset linux-release
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：全部 integration + perf 测试 `Passed`；输出中出现 `性能记录已写入: .../tests/perf/records/<今天日期>-zzsshcore.json`。

- [ ] **步骤 4：检查记录文件内容**

```bash
cat tests/perf/records/$(date +%F)-zzsshcore.json
```

预期：JSON 包含 `feature`、`timestamp`、`git_commit`（非 unknown）、`environment`（含 os/cpu/memory_mb/qt_version/compiler/build_type=Release）、`records` 两项且 `passed` 均为 `true`。

- [ ] **步骤 5：Commit（含首条性能基线记录）**

```bash
git add tests/perf/tst_ZzSshPerf.cpp tests/perf/records/.gitkeep "tests/perf/records/$(date +%F)-zzsshcore.json" tests/CMakeLists.txt
git commit -m "test: 新增 ZzSshCore 性能门控测试并记录首次性能基线"
```

---

### 任务 14：全量回归与收尾

**文件：** 无新增（回归验证）。

- [ ] **步骤 1：Debug 与 Release 双构建全量单测**

```bash
cmake --preset linux-debug && cmake --build --preset linux-debug
ctest --preset linux-debug -L unit
cmake --build --preset linux-release
ctest --preset linux-release -L unit
```

预期：Debug/Release 下 unit 标签全部 `Passed`（Debug 下 integration/perf 因无环境变量而 Skipped 属正常）。

- [ ] **步骤 2：Release 全量集成 + 性能回归**

```bash
tests/integration/docker/run-integration-tests.sh build/linux-release
```

预期：`100% tests passed, 0 tests failed`；集成测试覆盖：容器连通性、密码认证、公钥认证、认证回退、主机密钥首次确认/信任重连/变更警告/接受变更、shell 收发/resize/exit 关闭、断线感知、keepalive、性能门控。

- [ ] **步骤 3：验收清单核对（对照规格 §四/§八/§九）**

逐项人工核对并确认：

- [ ] 每连接一个 QThread，libssh2 阻塞模式，跨线程仅信号槽（规格 §4.1）
- [ ] `connected` / `errorOccurred(code, message)` / `disconnected(reason)` 三种结局信号（规格 §4.2/§八）
- [ ] 认证顺序 agent → 公钥 → 密码，密码经信号回调索取（规格 §4.2）
- [ ] 主机密钥首次确认 + 变更警告信号，known_hosts.json 落盘（规格 §4.2/§八）
- [ ] session/channel/socket 全部 RAII，禁拷贝（规格 §4.2）
- [ ] 只依赖 Qt Core/Network，不依赖 Widgets、不依赖 QCoro（规格 §4.3/§十）
- [ ] 单元测试 mock socket 层 + Docker 集成测试（密码/密钥/shell 收发/断线/主机密钥）（规格 §九）
- [ ] 性能测试进 ctest、Release 门控、记录入 `tests/perf/records/`（规格 §9.1）
- [ ] 类名 Zz 前缀、文件名与类名一致、Doxygen 简体中文注释（规格 §十）
- [ ] CMakeLists.txt + CMakePresets.json 共享矩阵 + CMakeUserPresets.json.example 模板（规格 §十）

- [ ] **步骤 4：如清单核对有出入，修复后统一提交**

```bash
git add -A
git commit -m "chore: ZzSshCore v0.1 收尾回归与验收修复"
```

若无任何修改则跳过本步骤（无变更不提交）。

---

## 附：跨计划接口约定（供主会话与其他计划对齐）

以下为本计划锁定、供 ZzClawTerm 应用层消费的公开 API 签名（计划 B/C 如需变更请提前同步）：

- `ZzSshConnection`：`connectToHost(host, port, user)` / `disconnectFromHost()` / `createShellChannel()`；信号 `connected()` / `errorOccurred(int, QString)` / `disconnected(QString)` / `passwordRequested()` / `hostKeyUnknown(host, port, keyType, fingerprint)` / `hostKeyChanged(host, port, keyType, oldFp, newFp)`；槽 `providePassword` / `cancelPasswordRequest` / `trustHostKey` / `rejectHostKey` / `acceptHostKeyChange`；配置 `setAuthConfig` / `setKnownHostsFilePath` / `setKeepaliveInterval` / `setConnectTimeout`。
- `ZzSshShellChannel`：`openShell(term, cols, rows)` / `write(QByteArray)` / `resize(cols, rows)` / `closeChannel()`；信号 `shellOpened()` / `dataReceived(QByteArray)` / `errorOccurred(int, QString)` / `closed()`。
- 错误码约定：`0` 无错误；负数 = libssh2 透传；`1000+` = `ZzSshErrorCode` 自定义码。
- 引入方式：应用仓库以 git submodule + `add_subdirectory` 引入，链接目标 `zzsshcore`（静态库，PUBLIC 依赖 Qt6::Core/Qt6::Network，libssh2 为 PRIVATE）。
