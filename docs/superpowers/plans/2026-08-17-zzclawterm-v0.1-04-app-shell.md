# 计划 D：应用壳层与装配 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 搭建 ZzClawTerm 应用仓库骨架并完成全部装配：注册接口、会话面板、多标签终端、本地 shell、连接流程胶水、状态栏、全局设置、滚动历史对接、错误处理 UI 与三平台打包。

**架构：** 本仓库只做装配（规格 §三）：通过 git submodule + `add_subdirectory` 引入 ZzSshCore / ZzTermWidget / ZzPureToolsPro / libssh2 / LZ4；SSH 层与终端层互相无感知，由 `ZzTransportInterface` 抽象 + 应用层字节流转发连接；UI 基于 ZzPureToolsPro 的 `ZzApplicationBuilder`/`ZzApplicationWindow`（无边框 + Fluent 主题），终端区与设置页注册为框架页面，会话面板以 QDockWidget 挂到窗口，装配逻辑收敛在组合根 `ZzAppShell`，装配函数只依赖 `QMainWindow &` 因而可用普通 QMainWindow 做离屏测试。

**技术栈：** C++20 / Qt 6.8+（Core、Widgets、Network、Test）/ CMake 3.25+（CMakePresets.json）/ QTest / ZzPureToolsPro / ZzQTermWidget（含 ptyqt）/ ZzSshCore（计划 01）。

**执行顺序：** 计划 01（ZzSshCore）→ 本计划任务 1（仓库骨架）→ 计划 02（ZzLogEngine，交付 `src/log/` 与 `src/log/CMakeLists.txt`，CMake 目标 `ZzLogEngine`）→ 计划 03（会话模型与凭据，交付 `src/session/` 与 `src/session/CMakeLists.txt`，CMake 目标 `ZzSessionCore`）→ 本计划任务 2 起依次执行。计划 05（ZzTermWidget 历史读回注入 API）**必须在任务 12 的读回接线步骤之前完成**。骨架 CMake 对 02/03 未交付的子目标做了条件保护；任务 7 起依赖计划 03 的 `ZzSessionProfile` 头文件，任务 8 起依赖计划 01，任务 10/12 起依赖计划 03/02。

---

## 跨计划接口契约（装配层唯一依赖面）

以下类型由计划 01/02/03 实现，本计划只按此契约引用。**若其他计划最终交付的签名与下表不一致，以其他计划为准并同步修改本计划引用点**（引用点集中在：任务 8 `ZzSshTransport`、任务 10 `ZzSessionPanel`/`ZzSessionEditDialog`、任务 12 `ZzScrollbackBridge`、任务 14 `ZzAppShell`）。

### 计划 01（ZzSshCore，CMake 目标名 `ZzSshCore`）

```cpp
// ZzSshConnection：连接生命周期、认证、keepalive（规格 §4.2）
class ZzSshConnection : public QObject {
public:
    void connectToHost(const QString &host, quint16 port, const QString &user);
    void disconnectFromHost();
    void setKeyPath(const QString &privateKeyPath);        // 公钥认证私钥路径
    void providePassword(const QString &password);          // 响应 passwordRequested
    void acceptHostKey(bool remember);                      // 响应主机密钥确认；remember=写入 known_hosts.json
    void rejectHostKey();
    ZzSshShellChannel *openShellChannel();                  // connected 之后调用
signals:
    void connected();
    void errorOccurred(int code, const QString &message);
    void disconnected(const QString &reason);
    void passwordRequested();                               // agent/公钥失败后向上层索取密码
    void hostKeyUnknown(const QString &host, const QString &fingerprint);
    void hostKeyChanged(const QString &host, const QString &fingerprint);
};

// ZzSshShellChannel：交互式 shell 通道（规格 §4.2）
class ZzSshShellChannel : public QObject {
public:
    bool openShell(const QString &term, int cols, int rows);
    qint64 write(const QByteArray &data);
    void resize(int cols, int rows);
    void close();
signals:
    void dataReceived(const QByteArray &data);
    void closed();
};
```

### 计划 03（会话模型 + 凭据存储，CMake 目标名 `ZzSessionCore`，头文件位于 `src/session/`）

计划 03 同时交付 `src/session/CMakeLists.txt`（定义静态库目标 `ZzSessionCore`）；本计划 `src/CMakeLists.txt` 对其做条件 `add_subdirectory` 引入，逻辑保持不变。

```cpp
// ZzSessionProfile：会话数据模型（规格 §6.1），纯值类型
struct ZzSessionProfile {
    QString id;                    // 稳定唯一标识（创建时生成）
    QString name;                  // 显示名
    QString groupPath;             // 分组路径，如 "生产环境/Web 服务器"，空=未分组
    QString protocol;              // "ssh" 或 "local"（本地 shell 特殊会话类型）
    QString host;                  // SSH 主机；protocol=="local" 时存 shell 程序路径（可空=系统默认）
    quint16 port = 22;
    QString user;
    QString authMethod;            // "agent" / "key" / "password"
    QString keyPath;               // authMethod=="key" 时的私钥路径
    QString credentialId;          // authMethod=="password" 时的密码引用（ZzCredentialStore 键）
    QString terminalType;          // 空=跟随全局设置
    QString encoding;              // 空=跟随全局设置
    QString colorScheme;           // 空=跟随全局设置
    int keepaliveInterval = 0;     // 秒，0=关闭
};

class ZzSessionModel : public QObject {
public:
    explicit ZzSessionModel(const QString &storagePath, QObject *parent = nullptr);
    QVector<ZzSessionProfile> profiles() const;
    ZzSessionProfile profileById(const QString &id) const;   // 未找到返回 id 为空的 profile
    void addProfile(const ZzSessionProfile &profile);
    void updateProfile(const ZzSessionProfile &profile);
    void removeProfile(const QString &id);
    bool load();
    bool save() const;
signals:
    void changed();                // 任意增删改后发射
};

class ZzCredentialStore : public QObject {
public:
    explicit ZzCredentialStore(const QString &storagePath, QObject *parent = nullptr);
    bool hasMasterPassword() const;
    bool isUnlocked() const;
    bool unlock(const QString &masterPassword);
    bool setMasterPassword(const QString &newPassword);      // 仅首次（hasMasterPassword()==false）
    QString addCredential(const QString &password);          // 返回新 credentialId
    QString credential(const QString &credentialId) const;   // 未解锁/不存在返回空串
    void removeCredential(const QString &credentialId);
};
```

### 计划 02（ZzLogEngine，CMake 目标名 `ZzLogEngine`，头文件位于 `src/log/`）

计划 02 同时交付 `src/log/CMakeLists.txt`（定义静态库目标 `ZzLogEngine`）；本计划 `src/CMakeLists.txt` 对其做条件 `add_subdirectory` 引入，逻辑保持不变。

```cpp
class ZzLogEngine : public QObject {
public:
    // sessionId 用于温层 mmap 文件命名；存储目录由引擎内部按 QStandardPaths 解析
    explicit ZzLogEngine(const QString &sessionId, QObject *parent = nullptr);
    void appendLines(const QStringList &lines);              // 追加到热层，溢出自动归档温层
    QStringList readBack(qint64 beforeLine, int maxLines) const; // 读取 beforeLine 之前的至多 maxLines 行
    qint64 totalLines() const;
signals:
    void degradedToMemoryOnly(const QString &reason);        // 磁盘满等 I/O 失败降级（规格 §八）
};
```

### 计划 05（ZzTermWidget 历史读回注入 API，改动落在 ZzTermWidget 仓库）

规格 §5.4 读回路径所需的 ZzTermWidget 侧新 API 由计划 05 提供，预计形态为
`setHistoryProvider` 或等价机制：

```cpp
// 预计形态（以计划 05 实际交付签名为准）：
// 滚动超出内存历史上界时，QTermWidget 回调本提供器索取更老的历史行并注入显示层。
void QTermWidget::setHistoryProvider(
    std::function<QStringList(qint64 beforeLine, int maxLines)> provider);
```

本计划任务 12 的读回接线步骤引用该 API；若计划 05 实际签名不同，以计划 05 为准等价改写接线处（仅此一处）。

### ZzTermWidget / ZzPureToolsPro（已有仓库，直接确认过的真实 API）

- `QTermWidget`（目标 `ZzQTermWidget::qtermwidget`，头文件 `<qtermwidget.h>`）：构造函数 `QTermWidget(QWidget *msgParent, QWidget *parent)`；`recvData(const char *, int)`；信号 `sendData(const char *, int)`；信号 `termSizeChange(int lines, int columns)`；信号 `dupDisplayOutput(const char *data, int len)`（逐行复制输出，UTF-8）；`setTerminalFont/getTerminalFont`、`setColorScheme`、`availableColorSchemes()`（static）、`setTextCodec(QStringEncoder)`、`setHistorySize(int)`、`setScrollBarPosition(ScrollBarPosition)`、`screenColumnsCount()/screenLinesCount()`。
- ptyqt（随 qtermwidget 目标导出，头文件 `"ptyqt.h"`/`"iptyprocess.h"`）：`PtyQt::createPtyProcess()` 返回 `IPtyProcess *`；`startProcess(exe, args, workDir, env, cols, rows)`、`resize(cols, rows)`、`kill()`、`notifier() -> QIODevice *`、`readAll()`、`write(QByteArray)`。
- ZzPureToolsPro（CMake 子目录见任务 1 的嵌套路径探测）：`ZzWindowKit::ZzWindowKitBootstrap::prepare()`；`ZzPureTools::ZzPureApplication`、`ZzApplicationBuilder`（`addModule`/`addPage`/`addNavigationNode`/`setInitialRoute`/`setWindowSetupCallback`/`build`）、`ZzApplicationWindow`（final，继承 QMainWindow）、`ZzPageInstance::create(pageParent, view, viewModel, presenter)`、`ZzApplicationModule`（descriptor/start/requestStop/stop）、`ZzCore::ZzResult<T>`（`if (!result)` 判失败）。链接目标 `ZzPureTools`。

---

## 文件结构

本计划创建/修改的文件（全部位于 ZzClawTerm 仓库根）：

| 文件 | 职责 |
| ---- | ---- |
| `.gitmodules` + `third_party/*` | 五个子模块引用 |
| `CMakeLists.txt` | 顶层构建：子模块引入、选项、全局编译定义 |
| `CMakePresets.json` / `CMakeUserPresets.json.example` | 共享构建矩阵 / 本机路径模板（不入库） |
| `.gitignore` | 追加 `CMakeUserPresets.json`、`install/` |
| `cmake/ZzGitRevision.cmake` | 取 git commit hash 到 `ZZ_GIT_REVISION` 定义 |
| `src/CMakeLists.txt` | `ZzClawTermApp` 静态库 + `ZzClawTerm` 可执行 |
| `src/main.cpp` | 入口：bootstrap、注册传输协议、装配框架页面与窗口回调 |
| `src/transport/ZzTransportEndpoint.h` | 协议无关的连接参数值类型 |
| `src/transport/ZzTransportInterface.h` | 传输协议抽象（SSH / 本地 PTY 都实现它） |
| `src/transport/ZzTransportRegistry.{h,cpp}` | 传输工厂注册表（内部模块与未来插件同一条注册路径） |
| `src/transport/ZzLocalPtyTransport.{h,cpp}` | 本地 PTY 传输（ptyqt） |
| `src/transport/ZzSshTransport.{h,cpp}` | ZzSshConnection/ZzSshShellChannel 到传输接口的适配器 |
| `src/panel/ZzPanelInterface.h` | Dock 面板抽象 |
| `src/panel/ZzPanelRegistry.{h,cpp}` | 面板注册表 |
| `src/panel/ZzSessionPanel.{h,cpp}` | 会话面板（QDockWidget + 树形分组 + 右键菜单） |
| `src/panel/ZzSessionEditDialog.{h,cpp}` | 会话新建/编辑对话框 |
| `src/terminal/ZzTerminalView.{h,cpp}` | QTermWidget + 传输胶水 + 错误横幅 |
| `src/terminal/ZzScrollbackBridge.{h,cpp}` | ZzTermWidget ↔ ZzLogEngine 滚动历史桥 |
| `src/tab/ZzTabManager.{h,cpp}` | 多标签管理（关闭/拖拽/断线变灰/右键重连） |
| `src/settings/ZzAppSettings.{h,cpp}` | 全局设置存储（终端类型/编码/字号/配色/热层行数） |
| `src/settings/ZzSettingsPage.{h,cpp}` | 设置页 UI |
| `src/dialog/ZzHostKeyDialog.{h,cpp}` | 主机密钥首次确认/变更警告（安全底线弹窗） |
| `src/dialog/ZzMasterPasswordDialog.{h,cpp}` | 主密码解锁框 |
| `src/ZzClawTermModule.{h,cpp}` | 框架应用模块（生命周期占位实现） |
| `src/ZzAppShell.{h,cpp}` | 组合根：dock、状态栏、页面工厂、信号装配 |
| `tests/CMakeLists.txt` + `tests/mocks/ZzMockTransport.{h,cpp}` | 测试基建与 mock 传输 |
| `tests/unit/tst_*.cpp` | 各组件 QTest |
| `tests/perf/ZzPerfRecorder.{h,cpp}` + `tests/perf/tst_*.cpp` | 性能门控与记录（写入 `tests/perf/records/`） |
| `scripts/package-windows.ps1` / `package-macos.sh` / `package-linux.sh` | 三平台打包 |
| `docs/acceptance/v0.1-manual-acceptance.md` | 三平台人工验收清单 |

---

## 任务 1：应用仓库骨架（子模块 + CMake + Presets）

**文件：**
- 创建：`.gitmodules`、`CMakeLists.txt`、`CMakePresets.json`、`CMakeUserPresets.json.example`、`cmake/ZzGitRevision.cmake`、`src/CMakeLists.txt`、`src/main.cpp`、`tests/CMakeLists.txt`、`tests/unit/tst_Skeleton.cpp`、`tests/perf/records/.gitkeep`
- 修改：`.gitignore`（追加两条）

- [ ] **步骤 1：添加五个 git 子模块**

```bash
cd /home/zz/Jackfahdin/github/ZzClawTerm
git submodule add https://github.com/Jackfahdin/ZzTermWidget.git third_party/ZzTermWidget
git submodule add https://github.com/Jackfahdin/ZzPureToolsPro.git third_party/ZzPureToolsPro
git submodule add https://gitcode.com/JackfahdinQt/ZzSshCore.git third_party/ZzSshCore
git submodule add https://gitcode.com/JackfahdinImport/libssh2.git third_party/libssh2
git submodule add https://github.com/lz4/lz4.git third_party/lz4
git submodule update --init --recursive
```

注意：`ZzSshCore` 仓库由计划 01 创建，若执行时 URL 尚未存在，先完成计划 01 再执行本计划。libssh2 为 gitcode 导入的 CMake 移植版（`https://gitcode.com/JackfahdinImport/libssh2`，规格 §十一），其加密后端 OpenSSL 使用 `https://gitcode.com/ZzThirdParty/openssl`（当前缺 macOS 构建，macOS 打包前需补齐）。子模块 URL 如有变化以实际远端为准。

- [ ] **步骤 2：创建 `cmake/ZzGitRevision.cmake`**

```cmake
# 提取当前 git commit hash，供性能记录与关于信息使用。
find_package(Git QUIET)
set(ZZ_GIT_REVISION "unknown")
if(GIT_FOUND)
    execute_process(
        COMMAND ${GIT_EXECUTABLE} rev-parse --short HEAD
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
        OUTPUT_VARIABLE ZZ_GIT_REVISION
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )
endif()
```

- [ ] **步骤 3：创建顶层 `CMakeLists.txt`**

```cmake
cmake_minimum_required(VERSION 3.25)

project(ZzClawTerm VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)

option(ZZCLAWTERM_BUILD_TESTS "构建 ZzClawTerm 测试" ON)

include(cmake/ZzGitRevision.cmake)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Widgets Network)

# ---- 第三方子模块 ----
# 子模块未初始化时给出明确指引，而不是漫天报错。
function(zz_require_submodule path)
    if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${path}/CMakeLists.txt")
        message(FATAL_ERROR
            "子模块缺失：${path}\n请先执行：git submodule update --init --recursive")
    endif()
endfunction()

# ZzTermWidget（终端组件）：关闭其自带示例/测试/安装规则
zz_require_submodule(third_party/ZzTermWidget)
set(ZZQTERMWIDGET_BUILD_EXAMPLE OFF CACHE BOOL "" FORCE)
set(ZZQTERMWIDGET_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(ZZQTERMWIDGET_INSTALL OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/ZzTermWidget)

# ZzPureToolsPro（应用框架）：仓库内嵌套一层同名工程目录，做兼容探测
if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/ZzPureToolsPro/ZzPureToolsPro/CMakeLists.txt")
    set(ZZ_PURETOOLS_SUBDIR "third_party/ZzPureToolsPro/ZzPureToolsPro")
else()
    set(ZZ_PURETOOLS_SUBDIR "third_party/ZzPureToolsPro")
endif()
zz_require_submodule(${ZZ_PURETOOLS_SUBDIR})
set(ZZ_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(ZZ_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(ZZ_BUILD_BENCHMARKS OFF CACHE BOOL "" FORCE)
add_subdirectory(${ZZ_PURETOOLS_SUBDIR})

# libssh2（CMake 移植版）与 ZzSshCore（计划 01）：ZzSshCore 链接 libssh2 目标
zz_require_submodule(third_party/libssh2)
add_subdirectory(third_party/libssh2)
zz_require_submodule(third_party/ZzSshCore)
add_subdirectory(third_party/ZzSshCore)

# LZ4（温层压缩，供计划 02 的 ZzLogEngine 链接 lz4_static；官方 CMake 位于 build/cmake）
zz_require_submodule(third_party/lz4/build/cmake)
set(LZ4_BUILD_CLI OFF CACHE BOOL "" FORCE)
set(LZ4_BUILD_SHARED_LZ4_LIBRARY OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/lz4/build/cmake)

add_subdirectory(src)

if(ZZCLAWTERM_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
```

- [ ] **步骤 4：创建 `src/CMakeLists.txt`（骨架版）**

后续任务会向 `ZZCLAWTERM_APP_SOURCES` 追加文件；计划 02/03 各自交付 `src/session/`、`src/log/` 子目录及其 CMakeLists（定义 `ZzSessionCore`、`ZzLogEngine` 静态库目标），此处条件引入，未交付时不阻塞骨架构建。

```cmake
# ZzClawTermApp：除 main.cpp 外的全部应用源码，供可执行与测试共同链接。
set(ZZCLAWTERM_APP_SOURCES
)

# 计划 03（会话模型/凭据）与计划 02（日志引擎）交付的子目录
foreach(sub session log)
    if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${sub}/CMakeLists.txt")
        add_subdirectory(${sub})
    else()
        message(STATUS "缺少 src/${sub}（由计划 02/03 交付），相关装配目标暂不可用")
    endif()
endforeach()

add_library(ZzClawTermApp STATIC ${ZZCLAWTERM_APP_SOURCES})
target_include_directories(ZzClawTermApp PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzClawTermApp
    PUBLIC
        Qt6::Core
        Qt6::Widgets
        Qt6::Network
        ZzQTermWidget::qtermwidget
        ZzPureTools
        ZzSshCore
)
# 计划 02/03 目标存在时才链接
foreach(optional_target ZzSessionCore ZzLogEngine)
    if(TARGET ${optional_target})
        target_link_libraries(ZzClawTermApp PUBLIC ${optional_target})
    endif()
endforeach()
target_compile_definitions(ZzClawTermApp PUBLIC
    ZZ_GIT_REVISION="${ZZ_GIT_REVISION}"
    ZZ_BUILD_TYPE="$<CONFIG>"
)

# Windows 下可执行不带控制台窗口；macOS 生成 .app
qt_add_executable(ZzClawTerm main.cpp)
set_target_properties(ZzClawTerm PROPERTIES
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
)
target_link_libraries(ZzClawTerm PRIVATE ZzClawTermApp)

install(TARGETS ZzClawTerm
    RUNTIME DESTINATION bin
    BUNDLE DESTINATION .
)
```

- [ ] **步骤 5：创建骨架 `src/main.cpp`（任务 14 替换为完整装配版）**

```cpp
#include <QtCore/QTimer>
#include <QtWidgets/QApplication>
#include <QtWidgets/QMainWindow>

/**
 * @brief 应用入口（骨架版，任务 14 替换为框架完整装配）。
 */
int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    QMainWindow window;
    window.setWindowTitle(QStringLiteral("ZzClawTerm"));
    // 冒烟模式：设置环境变量后启动即退出，供 CI 快速验证可执行能跑起来
    if (qEnvironmentVariableIsSet("ZZCLAWTERM_SMOKE_QUIT")) {
        QTimer::singleShot(0, &app, &QApplication::quit);
    }
    window.show();
    return app.exec();
}
```

- [ ] **步骤 6：创建 `tests/CMakeLists.txt` 与骨架测试**

`tests/CMakeLists.txt`：

```cmake
# 统一注册一个 QWidget 级 QTest：离屏运行，链接应用静态库。
function(zz_add_qtest name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE ZzClawTermApp Qt6::Test Qt6::Widgets)
    add_test(NAME ${name} COMMAND ${name})
    # 三平台统一离屏；Windows 上保持控制台子系统以便 ctest 捕获输出
    set_tests_properties(${name} PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endfunction()

add_subdirectory(mocks)

zz_add_qtest(tst_Skeleton unit/tst_Skeleton.cpp)

add_subdirectory(perf)
```

`tests/unit/tst_Skeleton.cpp`：

```cpp
#include <QtTest/QtTest>

/**
 * @brief 骨架冒烟测试：验证构建链路与 Qt 基线版本。
 */
class tst_Skeleton : public QObject
{
    Q_OBJECT
private slots:
    /** @brief Qt 基线必须为 6.8+（规格 §十）。 */
    void qtBaseline()
    {
        QVERIFY2(QT_VERSION >= QT_VERSION_CHECK(6, 8, 0), "Qt 版本低于 6.8");
        QVERIFY2(QStringLiteral(ZZ_BUILD_TYPE).size() > 0, "缺少构建类型定义");
    }
};

QTEST_MAIN(tst_Skeleton)
#include "tst_Skeleton.moc"
```

- [ ] **步骤 7：创建 mocks/perf 子目录 CMake 占位**

`tests/mocks/CMakeLists.txt`（任务 2 改为 STATIC 真实库；骨架阶段先用 INTERFACE 占位，让 `add_subdirectory(mocks)` 与链接引用能通过）：

```cmake
# ZzTestMocks：测试共享 mock 库（任务 2 定义，此处先建空静态库占位）
add_library(ZzTestMocks INTERFACE)
target_include_directories(ZzTestMocks INTERFACE ${CMAKE_CURRENT_SOURCE_DIR})
```

`tests/perf/CMakeLists.txt`：

```cmake
# 性能测试基建（ZzPerfInfra）由任务 3 定义，此处先占位
```

`tests/perf/records/.gitkeep`：空文件。

- [ ] **步骤 8：修改 `.gitignore`，追加**

```gitignore
# ZzClawTerm：本机 CMake 预设与安装输出不入库
CMakeUserPresets.json
install/
```

- [ ] **步骤 9：创建 `CMakePresets.json`（共享三平台矩阵）**

本机 Qt/编译器路径一律走 `$env{QT_ROOT}` 等环境变量或 CMakeUserPresets.json（规格 §十，参照 ZzPureToolsPro 模式）：

```json
{
  "version": 4,
  "cmakeMinimumRequired": { "major": 3, "minor": 25, "patch": 0 },
  "configurePresets": [
    {
      "name": "linux-gcc-base",
      "hidden": true,
      "generator": "Ninja",
      "binaryDir": "${sourceDir}/build/${presetName}",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Linux" },
      "cacheVariables": {
        "CMAKE_PREFIX_PATH": "$env{QT_ROOT}",
        "CMAKE_EXPORT_COMPILE_COMMANDS": true,
        "ZZCLAWTERM_BUILD_TESTS": true
      }
    },
    { "name": "linux-gcc-debug", "inherits": "linux-gcc-base",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug" } },
    { "name": "linux-gcc-release", "inherits": "linux-gcc-base",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" } },
    {
      "name": "windows-msvc2022-base",
      "hidden": true,
      "generator": "Visual Studio 17 2022",
      "architecture": { "value": "x64", "strategy": "set" },
      "binaryDir": "${sourceDir}/build/${presetName}",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Windows" },
      "cacheVariables": {
        "CMAKE_PREFIX_PATH": "$env{QT_ROOT}",
        "ZZCLAWTERM_BUILD_TESTS": true
      }
    },
    { "name": "windows-msvc2022-debug", "inherits": "windows-msvc2022-base" },
    { "name": "windows-msvc2022-release", "inherits": "windows-msvc2022-base" },
    {
      "name": "macos-clang-base",
      "hidden": true,
      "generator": "Ninja",
      "binaryDir": "${sourceDir}/build/${presetName}",
      "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Darwin" },
      "cacheVariables": {
        "CMAKE_PREFIX_PATH": "$env{QT_ROOT}",
        "CMAKE_OSX_DEPLOYMENT_TARGET": "13.3",
        "CMAKE_EXPORT_COMPILE_COMMANDS": true,
        "ZZCLAWTERM_BUILD_TESTS": true
      }
    },
    { "name": "macos-clang-debug", "inherits": "macos-clang-base",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug" } },
    { "name": "macos-clang-release", "inherits": "macos-clang-base",
      "cacheVariables": { "CMAKE_BUILD_TYPE": "Release" } }
  ],
  "buildPresets": [
    { "name": "linux-gcc-debug", "configurePreset": "linux-gcc-debug" },
    { "name": "linux-gcc-release", "configurePreset": "linux-gcc-release" },
    { "name": "windows-msvc2022-debug", "configurePreset": "windows-msvc2022-debug", "configuration": "Debug" },
    { "name": "windows-msvc2022-release", "configurePreset": "windows-msvc2022-release", "configuration": "Release" },
    { "name": "macos-clang-debug", "configurePreset": "macos-clang-debug" },
    { "name": "macos-clang-release", "configurePreset": "macos-clang-release" }
  ],
  "testPresets": [
    { "name": "linux-gcc-debug", "configurePreset": "linux-gcc-debug",
      "output": { "outputOnFailure": true }, "execution": { "noTestsAction": "error" } },
    { "name": "linux-gcc-release", "configurePreset": "linux-gcc-release",
      "output": { "outputOnFailure": true }, "execution": { "noTestsAction": "error" } },
    { "name": "windows-msvc2022-debug", "configurePreset": "windows-msvc2022-debug", "configuration": "Debug",
      "output": { "outputOnFailure": true }, "execution": { "noTestsAction": "error" } },
    { "name": "windows-msvc2022-release", "configurePreset": "windows-msvc2022-release", "configuration": "Release",
      "output": { "outputOnFailure": true }, "execution": { "noTestsAction": "error" } },
    { "name": "macos-clang-debug", "configurePreset": "macos-clang-debug",
      "output": { "outputOnFailure": true }, "execution": { "noTestsAction": "error" } },
    { "name": "macos-clang-release", "configurePreset": "macos-clang-release",
      "output": { "outputOnFailure": true }, "execution": { "noTestsAction": "error" } }
  ]
}
```

- [ ] **步骤 10：创建 `CMakeUserPresets.json.example`**

```json
{
  "version": 4,
  "configurePresets": [
    {
      "name": "local-linux-release",
      "inherits": "linux-gcc-release",
      "cacheVariables": {
        "CMAKE_PREFIX_PATH": "/home/zz/Qt/6.11.1/gcc_64"
      }
    }
  ]
}
```

- [ ] **步骤 11：配置、构建、跑骨架测试**

```bash
cd /home/zz/Jackfahdin/github/ZzClawTerm
export QT_ROOT=/home/zz/Qt/6.11.1/gcc_64   # 本机 Qt 前缀（也可写进自己的 CMakeUserPresets.json）
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug
```

预期：配置通过（无 "子模块缺失" 报错），编译通过，`tst_Skeleton` 1 个测试 PASS。再冒烟验证可执行：

```bash
QT_QPA_PLATFORM=offscreen ZZCLAWTERM_SMOKE_QUIT=1 ./build/linux-gcc-debug/src/ZzClawTerm
```

预期：进程以退出码 0 结束。

- [ ] **步骤 12：Commit**

```bash
git add .gitmodules .gitignore CMakeLists.txt CMakePresets.json CMakeUserPresets.json.example cmake src tests
git commit -m "feat: 应用仓库骨架（子模块引入、CMakePresets 三平台矩阵、骨架冒烟测试）"
```

---

## 任务 2：注册接口（ZzTransportInterface / ZzPanelInterface）与 Mock 传输

规格 §2.3：接口先行，框架后置。内部模块与未来第三方插件走同一条注册路径。

**文件：**
- 创建：`src/transport/ZzTransportEndpoint.h`、`src/transport/ZzTransportInterface.h`、`src/transport/ZzTransportRegistry.h`、`src/transport/ZzTransportRegistry.cpp`、`src/panel/ZzPanelInterface.h`、`src/panel/ZzPanelRegistry.h`、`src/panel/ZzPanelRegistry.cpp`
- 创建：`tests/mocks/ZzMockTransport.h`、`tests/mocks/ZzMockTransport.cpp`、`tests/unit/tst_ZzTransportRegistry.cpp`
- 修改：`src/CMakeLists.txt`（首次填充 `ZZCLAWTERM_APP_SOURCES`）、`tests/CMakeLists.txt`（注册新测试）、`tests/mocks/CMakeLists.txt`（INTERFACE 改为 STATIC 库）

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzTransportRegistry.cpp`**

```cpp
#include <QtTest/QtTest>

#include "transport/ZzTransportInterface.h"
#include "transport/ZzTransportRegistry.h"
#include "ZzMockTransport.h"

/**
 * @brief 验证传输工厂注册表：注册/创建/重复注册拒绝/未知协议返回空（规格 §2.3）。
 */
class tst_ZzTransportRegistry : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        // 每个用例前清空注册表，避免用例间互相污染
        ZzTransportRegistry::instance().clear();
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void registerAndCreate()
    {
        auto &registry = ZzTransportRegistry::instance();
        QVERIFY(registry.registerTransport(QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
        QVERIFY(registry.schemes().contains(QStringLiteral("mock")));

        std::unique_ptr<ZzTransportInterface> transport(
            registry.create(QStringLiteral("mock")));
        QVERIFY(transport != nullptr);
        QCOMPARE(transport->state(), ZzTransportInterface::State::Disconnected);
    }

    void duplicateRegisterRejected()
    {
        auto &registry = ZzTransportRegistry::instance();
        QVERIFY(registry.registerTransport(QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
        QVERIFY(!registry.registerTransport(QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
        QVERIFY(!registry.registerTransport(QString(),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
    }

    void unknownSchemeReturnsNull()
    {
        std::unique_ptr<ZzTransportInterface> transport(
            ZzTransportRegistry::instance().create(QStringLiteral("no-such-scheme")));
        QVERIFY(transport == nullptr);
    }
};

QTEST_MAIN(tst_ZzTransportRegistry)
#include "tst_ZzTransportRegistry.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzTransportRegistry
```

预期：编译失败，报错 `transport/ZzTransportInterface.h: No such file or directory`。

- [ ] **步骤 3：创建接口与注册表**

`src/transport/ZzTransportEndpoint.h`：

```cpp
#pragma once

#include <QtCore/QString>

/**
 * @brief 描述一次传输会话所需的全部参数，与协议无关。
 * @note 由应用层从 ZzSessionProfile（计划 03）映射生成；认证凭据不经过本结构，
 *       由具体传输实现通过回调向上层索取（规格 §4.2）。
 */
struct ZzTransportEndpoint final
{
    QString host;       ///< 远程主机地址；localShell 时忽略。
    quint16 port = 22;  ///< 远程端口。
    QString user;       ///< 登录用户名；localShell 时忽略。
    QString terminalType = QStringLiteral("xterm-256color"); ///< TERM 终端类型。
    int cols = 80;      ///< 初始列数。
    int rows = 24;      ///< 初始行数。
    QString keyPath;    ///< 公钥认证的私钥路径（可空）。
    bool localShell = false; ///< true 表示本地 shell 会话（规格 §七）。
    QString shellProgram;    ///< 本地 shell 可执行路径（可空，空=系统默认）。
};
```

`src/transport/ZzTransportInterface.h`：

```cpp
#pragma once

#include <QtCore/QObject>

#include "ZzTransportEndpoint.h"

/**
 * @brief 传输协议抽象：SSH 与本地 PTY 的统一接口（规格 §2.3）。
 *
 * 实现必须保证：open() 之后状态经 Connecting 到 Connected，或经 errorOccurred 失败；
 * 断开（主动或被动）后进入 Disconnected 并发射 disconnected()（主动 close 除外，
 * 主动 close 只改状态不报断线）。所有方法只在 GUI 线程调用。
 */
class ZzTransportInterface : public QObject
{
    Q_OBJECT
public:
    /** @brief 传输生命周期三态。 */
    enum class State {
        Disconnected,   ///< 未连接或已断开
        Connecting,     ///< 连接/认证进行中
        Connected       ///< 字节流通道就绪
    };
    Q_ENUM(State)

    using QObject::QObject;

    /**
     * @brief 按 endpoint 参数异步打开传输。
     * @param endpoint 连接参数。
     * @note 同一对象可被重复 open（重连场景由调用方先 close 或另建新实例，
     *       见 ZzTabManager::reconnectTab 的新实例策略）。
     */
    virtual void open(const ZzTransportEndpoint &endpoint) = 0;

    /** @brief 向远端/子进程写入字节（终端键盘输入方向）。 */
    virtual void write(const QByteArray &data) = 0;

    /** @brief 通知终端尺寸变化。 */
    virtual void resize(int cols, int rows) = 0;

    /** @brief 主动关闭传输并释放底层资源。 */
    virtual void close() = 0;

    /** @brief 当前状态。 */
    [[nodiscard]] State state() const noexcept { return m_state; }

signals:
    /** @brief 状态迁移通知。 */
    void stateChanged(ZzTransportInterface::State state);
    /** @brief 收到远端/子进程输出（终端显示方向）。 */
    void dataReceived(const QByteArray &data);
    /** @brief 打开或运行期失败。code 为封装层自定义码或透传底层错误码。 */
    void errorOccurred(int code, const QString &message);
    /** @brief 被动断开（远端关闭、网络中断、子进程退出）。 */
    void disconnected(const QString &reason);

protected:
    /** @brief 实现类迁移状态并发射 stateChanged。 */
    void setState(State state)
    {
        if (m_state == state) {
            return;
        }
        m_state = state;
        emit stateChanged(state);
    }

private:
    State m_state = State::Disconnected;
};
```

`src/transport/ZzTransportRegistry.h`：

```cpp
#pragma once

#include <functional>

#include <QtCore/QHash>
#include <QtCore/QString>
#include <QtCore/QStringList>

class ZzTransportInterface;

/** @brief 传输工厂：按给定父对象创建一个未 open 的传输实例。 */
using ZzTransportFactory = std::function<ZzTransportInterface *(QObject *parent)>;

/**
 * @brief 传输协议注册表（规格 §2.3：内部模块与未来插件同一条注册路径）。
 *
 * 进程级单例；内置协议（ssh/local）在 main() 启动时注册，测试可用 clear() 隔离。
 */
class ZzTransportRegistry final
{
public:
    /** @brief 进程级唯一注册表。 */
    static ZzTransportRegistry &instance();

    /**
     * @brief 注册协议工厂。
     * @param scheme 协议名（如 "ssh"、"local"），非空。
     * @param factory 创建函数。
     * @return 注册成功；scheme 为空或已存在返回 false。
     */
    bool registerTransport(const QString &scheme, ZzTransportFactory factory);

    /**
     * @brief 按协议名创建传输实例。
     * @return 未注册协议返回 nullptr。
     */
    [[nodiscard]] ZzTransportInterface *create(const QString &scheme,
                                               QObject *parent = nullptr);

    /** @brief 已注册协议名列表。 */
    [[nodiscard]] QStringList schemes() const;

    /** @brief 清空全部注册（仅测试使用）。 */
    void clear();

private:
    ZzTransportRegistry() = default;
    QHash<QString, ZzTransportFactory> m_factories;
};
```

`src/transport/ZzTransportRegistry.cpp`：

```cpp
#include "ZzTransportRegistry.h"

#include <utility>

#include "ZzTransportInterface.h"

ZzTransportRegistry &ZzTransportRegistry::instance()
{
    static ZzTransportRegistry registry;
    return registry;
}

bool ZzTransportRegistry::registerTransport(const QString &scheme,
                                            ZzTransportFactory factory)
{
    if (scheme.isEmpty() || !factory || m_factories.contains(scheme)) {
        return false;
    }
    m_factories.insert(scheme, std::move(factory));
    return true;
}

ZzTransportInterface *ZzTransportRegistry::create(const QString &scheme,
                                                  QObject *parent)
{
    const auto it = m_factories.constFind(scheme);
    if (it == m_factories.constEnd()) {
        return nullptr;
    }
    return it.value()(parent);
}

QStringList ZzTransportRegistry::schemes() const
{
    return m_factories.keys();
}

void ZzTransportRegistry::clear()
{
    m_factories.clear();
}
```

`src/panel/ZzPanelInterface.h`：

```cpp
#pragma once

#include <QtCore/QString>

class QWidget;

/**
 * @brief Dock 面板抽象（规格 §2.3）：会话面板、未来的 SFTP 面板都实现它。
 *
 * 纯接口（非 QObject），面板实现类同时继承 QDockWidget 与本接口。
 */
class ZzPanelInterface
{
public:
    virtual ~ZzPanelInterface() = default;

    /** @brief 稳定面板标识（如 "sessions"），用于布局持久化键。 */
    [[nodiscard]] virtual QString panelId() const = 0;

    /** @brief 面板显示标题。 */
    [[nodiscard]] virtual QString panelTitle() const = 0;

    /** @brief 面板控件本体（实现类通常返回 this）。 */
    [[nodiscard]] virtual QWidget *panelWidget() = 0;
};
```

`src/panel/ZzPanelRegistry.h`：

```cpp
#pragma once

#include <QtCore/QList>

class ZzPanelInterface;

/**
 * @brief Dock 面板登记册（规格 §2.3）。
 *
 * v0.1 壳层对内置面板显式停靠；本登记册统一面板身份（panelId 用于布局持久化
 * 与重复注册防护），并为未来插件面板提供同一条接入路径。不获得所有权、
 * 不自动停靠；面板随窗口销毁后由调用方 clear()（ZzAppShell 析构时负责）。
 */
class ZzPanelRegistry final
{
public:
    /** @brief 进程级唯一注册表。 */
    static ZzPanelRegistry &instance();

    /**
     * @brief 注册面板（不获得所有权）。
     * @return 重复 panelId 拒绝并返回 false。
     */
    bool registerPanel(ZzPanelInterface *panel);

    /** @brief 已注册面板列表（注册顺序）。 */
    [[nodiscard]] QList<ZzPanelInterface *> panels() const;

    /** @brief 清空（仅测试使用）。 */
    void clear();

private:
    ZzPanelRegistry() = default;
    QList<ZzPanelInterface *> m_panels;
};
```

`src/panel/ZzPanelRegistry.cpp`：

```cpp
#include "ZzPanelRegistry.h"

#include "ZzPanelInterface.h"

ZzPanelRegistry &ZzPanelRegistry::instance()
{
    static ZzPanelRegistry registry;
    return registry;
}

bool ZzPanelRegistry::registerPanel(ZzPanelInterface *panel)
{
    if (panel == nullptr) {
        return false;
    }
    for (const ZzPanelInterface *existing : m_panels) {
        if (existing->panelId() == panel->panelId()) {
            return false;
        }
    }
    m_panels.append(panel);
    return true;
}

QList<ZzPanelInterface *> ZzPanelRegistry::panels() const
{
    return m_panels;
}

void ZzPanelRegistry::clear()
{
    m_panels.clear();
}
```

`tests/mocks/ZzMockTransport.h`：

```cpp
#pragma once

#include "transport/ZzTransportInterface.h"

/**
 * @brief 测试用 mock 传输：脚本化成功/失败，记录写入与尺寸，可手动注入事件。
 *
 * open() 默认在事件循环下一拍进入 Connected；endpoint.host == "fail" 时
 * 以错误码 1001 失败。echoEnabled 时 write 的内容原样经 dataReceived 回显。
 */
class ZzMockTransport : public ZzTransportInterface
{
    Q_OBJECT
public:
    explicit ZzMockTransport(QObject *parent = nullptr);

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

    /** @brief 注入一次被动断开。 */
    void simulateDisconnect(const QString &reason);
    /** @brief 注入一次错误。 */
    void simulateError(int code, const QString &message);
    /** @brief 注入一段远端输出。 */
    void simulateData(const QByteArray &data);

    bool echoEnabled = true;       ///< write 回显开关
    QByteArray writtenData;        ///< 累计写入内容
    int lastCols = 0;              ///< 最近一次 resize 列数
    int lastRows = 0;              ///< 最近一次 resize 行数
    int openCallCount = 0;         ///< open 调用次数（重连断言用）
    int closeCallCount = 0;        ///< close 调用次数
    ZzTransportEndpoint lastEndpoint; ///< 最近一次 open 参数
};
```

`tests/mocks/ZzMockTransport.cpp`：

```cpp
#include "ZzMockTransport.h"

#include <QtCore/QTimer>

ZzMockTransport::ZzMockTransport(QObject *parent)
    : ZzTransportInterface(parent)
{
}

void ZzMockTransport::open(const ZzTransportEndpoint &endpoint)
{
    ++openCallCount;
    lastEndpoint = endpoint;
    setState(State::Connecting);
    // 下一拍完成，模拟异步连接
    QTimer::singleShot(0, this, [this, host = endpoint.host]() {
        if (host == QStringLiteral("fail")) {
            setState(State::Disconnected);
            emit errorOccurred(1001, QStringLiteral("mock 连接失败"));
            return;
        }
        setState(State::Connected);
    });
}

void ZzMockTransport::write(const QByteArray &data)
{
    writtenData += data;
    if (echoEnabled && state() == State::Connected) {
        emit dataReceived(data);
    }
}

void ZzMockTransport::resize(int cols, int rows)
{
    lastCols = cols;
    lastRows = rows;
}

void ZzMockTransport::close()
{
    ++closeCallCount;
    setState(State::Disconnected);
}

void ZzMockTransport::simulateDisconnect(const QString &reason)
{
    setState(State::Disconnected);
    emit disconnected(reason);
}

void ZzMockTransport::simulateError(int code, const QString &message)
{
    setState(State::Disconnected);
    emit errorOccurred(code, message);
}

void ZzMockTransport::simulateData(const QByteArray &data)
{
    emit dataReceived(data);
}
```

- [ ] **步骤 4：更新 CMake**

`src/CMakeLists.txt` 中把 `ZZCLAWTERM_APP_SOURCES` 改为：

```cmake
set(ZZCLAWTERM_APP_SOURCES
    transport/ZzTransportEndpoint.h
    transport/ZzTransportInterface.h
    transport/ZzTransportRegistry.h
    transport/ZzTransportRegistry.cpp
    panel/ZzPanelInterface.h
    panel/ZzPanelRegistry.h
    panel/ZzPanelRegistry.cpp
)
```

`tests/mocks/CMakeLists.txt` 整体替换为：

```cmake
add_library(ZzTestMocks STATIC
    ZzMockTransport.h
    ZzMockTransport.cpp
)
target_include_directories(ZzTestMocks PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzTestMocks PUBLIC ZzClawTermApp Qt6::Core)
```

`tests/CMakeLists.txt` 中 `zz_add_qtest` 函数体的 link 行改为（测试都要用 mock 库）：

```cmake
    target_link_libraries(${name} PRIVATE ZzClawTermApp ZzTestMocks Qt6::Test Qt6::Widgets)
```

并在文件末尾追加：

```cmake
zz_add_qtest(tst_ZzTransportRegistry unit/tst_ZzTransportRegistry.cpp)
```

- [ ] **步骤 5：运行测试验证通过**

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzTransportRegistry --output-on-failure
```

预期：`tst_ZzTransportRegistry` PASS（3 个用例），`tst_Skeleton` 仍 PASS。

- [ ] **步骤 6：Commit**

```bash
git add src/transport src/panel tests/mocks tests/unit/tst_ZzTransportRegistry.cpp src/CMakeLists.txt tests/CMakeLists.txt tests/mocks/CMakeLists.txt
git commit -m "feat: 传输/面板注册接口（ZzTransportInterface、ZzPanelInterface）与 mock 传输"
```

---

## 任务 3：性能门控基建（ZzPerfRecorder）

规格 §9.1：每项功能完成必须附带性能测试并记录到 `tests/perf/records/YYYY-MM-DD-<功能名>.json`；Release 构建数字才有效；阈值失败即测试失败。后续每个功能任务末尾的性能步骤都依赖本基建。

**文件：**
- 创建：`tests/perf/ZzPerfRecorder.h`、`tests/perf/ZzPerfRecorder.cpp`、`tests/perf/tst_PerfRecorder.cpp`
- 修改：`tests/perf/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/perf/tst_PerfRecorder.cpp`**

```cpp
#include <QtTest/QtTest>

#include "ZzPerfRecorder.h"

/**
 * @brief 验证性能记录器：JSON 字段齐全、同日追加、阈值判定。
 */
class tst_PerfRecorder : public QObject
{
    Q_OBJECT
private slots:
    void recordWritesJson()
    {
        // 本用例自身即一次真实记录（功能名固定为基建自检）
        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("perf-infra-selfcheck"),
            QStringLiteral("记录器自检"), 1000.0, 1.0);
        QVERIFY(ok);

        const QString path = ZzPerfRecorder::recordFilePath(
            QStringLiteral("perf-infra-selfcheck"));
        QFile file(path);
        QVERIFY2(file.exists(), qPrintable(path));
        QVERIFY(file.open(QIODevice::ReadOnly));
        const QJsonArray entries =
            QJsonDocument::fromJson(file.readAll()).array();
        QVERIFY(!entries.isEmpty());

        const QJsonObject entry = entries.last().toObject();
        QCOMPARE(entry.value(QStringLiteral("test")).toString(),
                 QStringLiteral("记录器自检"));
        QCOMPARE(entry.value(QStringLiteral("threshold_ms")).toDouble(), 1000.0);
        QCOMPARE(entry.value(QStringLiteral("passed")).toBool(), true);
        QVERIFY(!entry.value(QStringLiteral("git_commit")).toString().isEmpty());
        QVERIFY(!entry.value(QStringLiteral("timestamp")).toString().isEmpty());
        const QJsonObject env =
            entry.value(QStringLiteral("environment")).toObject();
        for (const char *key : {"os", "cpu", "qt", "compiler", "build_type"}) {
            QVERIFY2(env.contains(QLatin1String(key)), key);
        }
    }

    void thresholdViolationReturnsFalse()
    {
        QVERIFY(!ZzPerfRecorder::recordAndCheck(
            QStringLiteral("perf-infra-selfcheck"),
            QStringLiteral("阈值违例自检"), 10.0, 999.0));
    }
};

QTEST_MAIN(tst_PerfRecorder)
#include "tst_PerfRecorder.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_PerfRecorder
```

预期：编译失败，报错 `ZzPerfRecorder.h: No such file or directory`。

- [ ] **步骤 3：实现 `tests/perf/ZzPerfRecorder.h`**

```cpp
#pragma once

#include <QtCore/QString>

/**
 * @brief 性能测试记录器（规格 §9.1）。
 *
 * 每条记录追加写入 tests/perf/records/YYYY-MM-DD-<功能名>.json（JSON 数组），
 * 包含阈值、实测值、是否通过、环境信息、git commit hash 与时间。
 */
class ZzPerfRecorder final
{
public:
    /**
     * @brief 记录一次性能结果。
     * @param feature 功能名（进入文件名，只含字母数字与连字符）。
     * @param testName 测试项中文名。
     * @param thresholdMs 通过阈值（毫秒）。
     * @param measuredMs 实测值（毫秒）。
     * @return 实测值是否达标（measuredMs <= thresholdMs）。
     */
    static bool recordAndCheck(const QString &feature,
                               const QString &testName,
                               double thresholdMs,
                               double measuredMs);

    /** @brief 指定功能当日的记录文件绝对路径（供测试断言）。 */
    [[nodiscard]] static QString recordFilePath(const QString &feature);

    /**
     * @brief 性能门控是否生效：仅 Release 构建返回 true（规格 §9.1）。
     *
     * 非 Release 构建下性能测试应 QSKIP，数字无效、不落记录。
     */
    [[nodiscard]] static constexpr bool gatingEnabled()
    {
#ifdef NDEBUG
        return true;
#else
        return false;
#endif
    }
};
```

- [ ] **步骤 4：实现 `tests/perf/ZzPerfRecorder.cpp`**

```cpp
#include "ZzPerfRecorder.h"

#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QSysInfo>

namespace {

/** @brief 读取物理内存总量（字节），无法获取返回 0。 */
qint64 zzTotalMemoryBytes()
{
#ifdef Q_OS_LINUX
    QFile meminfo(QStringLiteral("/proc/meminfo"));
    if (meminfo.open(QIODevice::ReadOnly)) {
        const QByteArray content = meminfo.readAll();
        const int begin = content.indexOf("MemTotal:");
        if (begin >= 0) {
            const int end = content.indexOf('\n', begin);
            const QByteArray line = content.mid(begin, end - begin);
            // 格式：MemTotal:       16384000 kB
            return line.split(' ').filter(
                [](const QByteArray &part) { return !part.isEmpty(); })
                .value(1).toLongLong() * 1024;
        }
    }
#endif
    return 0; ///< Windows/macOS 由人工验收清单记录，测试机以 Linux 为主
}

/** @brief 组装环境信息对象（规格 §9.1 要求项）。 */
QJsonObject zzEnvironment()
{
    QJsonObject env;
    env.insert(QStringLiteral("os"), QSysInfo::prettyProductName());
    env.insert(QStringLiteral("cpu"), QSysInfo::currentCpuArchitecture());
    env.insert(QStringLiteral("qt"), QStringLiteral(qVersion()));
    env.insert(QStringLiteral("compiler"), QStringLiteral(ZZ_COMPILER_DESCRIPTION));
    env.insert(QStringLiteral("build_type"), QStringLiteral(ZZ_BUILD_TYPE));
    env.insert(QStringLiteral("memory_bytes"), zzTotalMemoryBytes());
    return env;
}

} // namespace

QString ZzPerfRecorder::recordFilePath(const QString &feature)
{
    const QString date =
        QDateTime::currentDateTimeUtc().toString(QStringLiteral("yyyy-MM-dd"));
    return QStringLiteral("%1/%2-%3.json")
        .arg(QStringLiteral(ZZ_PERF_RECORDS_DIR), date, feature);
}

bool ZzPerfRecorder::recordAndCheck(const QString &feature,
                                    const QString &testName,
                                    double thresholdMs,
                                    double measuredMs)
{
    const bool passed = measuredMs <= thresholdMs;

    QJsonObject entry;
    entry.insert(QStringLiteral("test"), testName);
    entry.insert(QStringLiteral("threshold_ms"), thresholdMs);
    entry.insert(QStringLiteral("measured_ms"), measuredMs);
    entry.insert(QStringLiteral("passed"), passed);
    entry.insert(QStringLiteral("environment"), zzEnvironment());
    entry.insert(QStringLiteral("git_commit"), QStringLiteral(ZZ_GIT_REVISION));
    entry.insert(QStringLiteral("timestamp"),
                 QDateTime::currentDateTimeUtc().toString(Qt::ISODate));

    const QString path = recordFilePath(feature);
    QDir().mkpath(QFileInfo(path).absolutePath());

    // 同日同功能文件为 JSON 数组：读出、追加、写回（历史记录全部保留）
    QJsonArray entries;
    {
        QFile file(path);
        if (file.exists() && file.open(QIODevice::ReadOnly)) {
            entries = QJsonDocument::fromJson(file.readAll()).array();
        }
    }
    entries.append(entry);
    {
        QFile file(path);
        if (file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
            file.write(QJsonDocument(entries).toJson(QJsonDocument::Indented));
        }
    }
    return passed;
}
```

- [ ] **步骤 5：更新 `tests/perf/CMakeLists.txt`**

整体替换为：

```cmake
# 性能基建库：携带记录目录、编译器、构建类型等编译期定义
add_library(ZzPerfInfra STATIC
    ZzPerfRecorder.h
    ZzPerfRecorder.cpp
)
target_link_libraries(ZzPerfInfra PUBLIC ZzClawTermApp Qt6::Core)
target_compile_definitions(ZzPerfInfra PUBLIC
    ZZ_PERF_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records"
    ZZ_COMPILER_DESCRIPTION="${CMAKE_CXX_COMPILER_ID} ${CMAKE_CXX_COMPILER_VERSION}"
)

# 注册一个性能测试：链接 ZzPerfInfra，并打上 perf 标签便于筛选
function(zz_add_perf_test name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE ZzClawTermApp ZzTestMocks ZzPerfInfra Qt6::Test Qt6::Widgets)
    add_test(NAME ${name} COMMAND ${name})
    set_tests_properties(${name} PROPERTIES
        LABELS "perf"
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endfunction()

zz_add_perf_test(tst_PerfRecorder tst_PerfRecorder.cpp)
```

- [ ] **步骤 6：运行测试验证通过**

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_PerfRecorder --output-on-failure
```

预期：PASS；仓库出现 `tests/perf/records/<今日>-perf-infra-selfcheck.json` 且字段齐全。

- [ ] **步骤 7：Commit（含首条性能记录，规格 §9.1 要求记录入库）**

```bash
git add tests/perf
git commit -m "test: 性能门控基建 ZzPerfRecorder（JSON 记录、阈值判定、Release 门控）"
```

---

## 任务 4：本地 PTY 传输（ZzLocalPtyTransport）

规格 §七：本地 shell 特殊会话类型，不经 SSH，复用 ZzTermWidget 内置 ptyqt，验证终端层与协议层解耦。

**文件：**
- 创建：`src/transport/ZzLocalPtyTransport.h`、`src/transport/ZzLocalPtyTransport.cpp`
- 创建：`tests/unit/tst_ZzLocalPtyTransport.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzLocalPtyTransport.cpp`**

```cpp
#include <QtTest/QtTest>

#include "transport/ZzLocalPtyTransport.h"

/**
 * @brief 验证本地 PTY 传输：起 shell、收发回显、resize、关闭与被动断开。
 */
class tst_ZzLocalPtyTransport : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    /** @brief 按平台取默认 shell（与生产代码同一套规则）。 */
    static QString platformShell()
    {
#if defined(Q_OS_WIN)
        return QStringLiteral("powershell.exe");
#else
        return QString::fromLocal8Bit(qgetenv("SHELL")).isEmpty()
            ? QStringLiteral("/bin/sh")
            : QString::fromLocal8Bit(qgetenv("SHELL"));
#endif
    }

    void openEchoClose()
    {
        ZzLocalPtyTransport transport;
        QSignalSpy stateSpy(&transport, &ZzTransportInterface::stateChanged);

        ZzTransportEndpoint endpoint;
        endpoint.localShell = true;
        endpoint.shellProgram = platformShell();
        endpoint.cols = 80;
        endpoint.rows = 24;
        transport.open(endpoint);

        QTRY_VERIFY_WITH_TIMEOUT(
            transport.state() == ZzTransportInterface::State::Connected, 5000);
        QVERIFY(stateSpy.count() >= 2); // Disconnected→Connecting→Connected

        // 写命令，应读到回显输出（PTY 默认回显 + 命令输出）
        QByteArray received;
        connect(&transport, &ZzTransportInterface::dataReceived,
                this, [&received](const QByteArray &data) { received += data; });
        transport.write("echo zz-pty-ok\n");
        QTRY_VERIFY_WITH_TIMEOUT(received.contains("zz-pty-ok"), 5000);

        transport.resize(100, 40);
        transport.close();
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }

    void shellExitEmitsDisconnected()
    {
        ZzLocalPtyTransport transport;
        QSignalSpy disconnectSpy(&transport, &ZzTransportInterface::disconnected);

        ZzTransportEndpoint endpoint;
        endpoint.localShell = true;
        endpoint.shellProgram = platformShell();
        transport.open(endpoint);
        QTRY_VERIFY_WITH_TIMEOUT(
            transport.state() == ZzTransportInterface::State::Connected, 5000);

        transport.write("exit\n");
        QTRY_VERIFY_WITH_TIMEOUT(disconnectSpy.count() >= 1, 5000);
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }
};

QTEST_MAIN(tst_ZzLocalPtyTransport)
#include "tst_ZzLocalPtyTransport.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzLocalPtyTransport
```

预期：编译失败，报错 `transport/ZzLocalPtyTransport.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/transport/ZzLocalPtyTransport.h`**

```cpp
#pragma once

#include <memory>

#include "ZzTransportInterface.h"

class IPtyProcess;

/**
 * @brief 本地 shell 传输：包装 ZzTermWidget 内置 ptyqt（规格 §七）。
 *
 * 不走网络、不经 SSH，验证终端层与协议层解耦——对 ZzTerminalView 而言
 * 与 ZzSshTransport 完全同形。
 */
class ZzLocalPtyTransport : public ZzTransportInterface
{
    Q_OBJECT
public:
    explicit ZzLocalPtyTransport(QObject *parent = nullptr);
    ~ZzLocalPtyTransport() override;

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

private:
    std::unique_ptr<IPtyProcess> m_pty; ///< PTY 进程（RAII 持有）
    bool m_closing = false;             ///< 主动关闭标记：抑制重复 disconnected
};
```

- [ ] **步骤 4：实现 `src/transport/ZzLocalPtyTransport.cpp`**

```cpp
#include "ZzLocalPtyTransport.h"

#include <QtCore/QDir>
#include <QtCore/QIODevice>
#include <QtCore/QProcessEnvironment>

#include "ptyqt.h"
#include "iptyprocess.h"

namespace {

/** @brief 按平台取默认本地 shell（与 ZzTermWidget example 同一套规则）。 */
QString zzDefaultShell()
{
#if defined(Q_OS_WIN)
    return QStringLiteral("c:\\Windows\\system32\\WindowsPowerShell\\v1.0\\powershell.exe");
#else
    const QString fromEnv = QString::fromLocal8Bit(qgetenv("SHELL"));
    return fromEnv.isEmpty() ? QStringLiteral("/bin/sh") : fromEnv;
#endif
}

} // namespace

ZzLocalPtyTransport::ZzLocalPtyTransport(QObject *parent)
    : ZzTransportInterface(parent)
{
}

ZzLocalPtyTransport::~ZzLocalPtyTransport()
{
    close();
}

void ZzLocalPtyTransport::open(const ZzTransportEndpoint &endpoint)
{
    if (state() != State::Disconnected) {
        return;
    }
    setState(State::Connecting);
    m_closing = false;

    m_pty.reset(PtyQt::createPtyProcess());
    const QString shell = endpoint.shellProgram.isEmpty()
        ? zzDefaultShell()
        : endpoint.shellProgram;

    // PTY 输出 → dataReceived（终端显示方向）
    connect(m_pty->notifier(), &QIODevice::readyRead, this, [this]() {
        const QByteArray data = m_pty->readAll();
        if (!data.isEmpty()) {
            emit dataReceived(data);
        }
    });
    // 子进程退出 → 被动断开
    connect(m_pty->notifier(), &QIODevice::aboutToClose, this, [this]() {
        const QByteArray rest = m_pty->readAll();
        if (!rest.isEmpty()) {
            emit dataReceived(rest);
        }
        if (!m_closing) {
            setState(State::Disconnected);
            emit disconnected(QStringLiteral("本地 shell 已退出（退出码 %1）")
                                  .arg(m_pty->exitCode()));
        }
    });

    if (!m_pty->startProcess(shell, {}, QDir::homePath(),
                             QProcessEnvironment::systemEnvironment().toStringList(),
                             static_cast<qint16>(endpoint.cols),
                             static_cast<qint16>(endpoint.rows))) {
        const QString reason = m_pty->lastError();
        m_pty.reset();
        setState(State::Disconnected);
        emit errorOccurred(2001, QStringLiteral("启动本地 shell 失败：%1").arg(reason));
        return;
    }
    setState(State::Connected);
}

void ZzLocalPtyTransport::write(const QByteArray &data)
{
    if (m_pty && state() == State::Connected) {
        m_pty->write(data);
    }
}

void ZzLocalPtyTransport::resize(int cols, int rows)
{
    if (m_pty) {
        m_pty->resize(static_cast<qint16>(cols), static_cast<qint16>(rows));
    }
}

void ZzLocalPtyTransport::close()
{
    if (!m_pty) {
        setState(State::Disconnected);
        return;
    }
    m_closing = true;
    m_pty->kill();
    m_pty.reset();
    setState(State::Disconnected);
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加两行：

```cmake
    transport/ZzLocalPtyTransport.h
    transport/ZzLocalPtyTransport.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzLocalPtyTransport unit/tst_ZzLocalPtyTransport.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzLocalPtyTransport --output-on-failure
```

预期：PASS（2 个用例）。若编译报 `ptyqt.h` 找不到，说明 qtermwidget 目标未导出 ptyqt 头路径，在 `ZzClawTermApp` 上补
`target_include_directories(ZzClawTermApp PUBLIC ${CMAKE_SOURCE_DIR}/third_party/ZzTermWidget/lib/third_party/ptyqt)`。

- [ ] **步骤 6：Commit**

```bash
git add src/transport/ZzLocalPtyTransport.h src/transport/ZzLocalPtyTransport.cpp tests/unit/tst_ZzLocalPtyTransport.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 本地 PTY 传输 ZzLocalPtyTransport（ptyqt 起本地 shell，被动断开通知）"
```

---

## 任务 5：全局设置存储（ZzAppSettings）

设置页 UI 在任务 11；本任务先落地存储与通知，因为 ZzTerminalView / ZzTabManager 立即要消费。规格 §七：默认终端类型、编码、字号、配色（会话级覆盖留待后续）。另加热层内存行数（规格 §5.2 热层默认 10,000 行可配置）。

**文件：**
- 创建：`src/settings/ZzAppSettings.h`、`src/settings/ZzAppSettings.cpp`
- 创建：`tests/unit/tst_ZzAppSettings.cpp`、`tests/perf/tst_PerfSettings.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`、`tests/perf/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzAppSettings.cpp`**

```cpp
#include <QtTest/QtTest>

#include "settings/ZzAppSettings.h"

/**
 * @brief 验证全局设置：默认值、往返持久化、变更通知。
 */
class tst_ZzAppSettings : public QObject
{
    Q_OBJECT
private slots:
    void defaults()
    {
        ZzAppSettings settings(QStringLiteral("/nonexistent-dir/never-exists.ini"));
        QCOMPARE(settings.terminalType(), QStringLiteral("xterm-256color"));
        QCOMPARE(settings.encoding(), QStringLiteral("UTF-8"));
        QCOMPARE(settings.fontSize(), 12);
        QCOMPARE(settings.colorScheme(), QStringLiteral("Linux"));
        QCOMPARE(settings.historyLines(), 10000);
    }

    void roundTrip()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-test.ini"));
        QFile::remove(path);
        {
            ZzAppSettings settings(path);
            QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);
            settings.setTerminalType(QStringLiteral("vt100"));
            settings.setEncoding(QStringLiteral("GBK"));
            settings.setFontSize(16);
            settings.setColorScheme(QStringLiteral("QuardCRT"));
            settings.setHistoryLines(20000);
            QCOMPARE(spy.count(), 5); // 每项变更都通知
        }
        ZzAppSettings reloaded(path);
        QCOMPARE(reloaded.terminalType(), QStringLiteral("vt100"));
        QCOMPARE(reloaded.encoding(), QStringLiteral("GBK"));
        QCOMPARE(reloaded.fontSize(), 16);
        QCOMPARE(reloaded.colorScheme(), QStringLiteral("QuardCRT"));
        QCOMPARE(reloaded.historyLines(), 20000);
        QFile::remove(path);
    }
};

QTEST_MAIN(tst_ZzAppSettings)
#include "tst_ZzAppSettings.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzAppSettings
```

预期：编译失败，报错 `settings/ZzAppSettings.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/settings/ZzAppSettings.h`**

```cpp
#pragma once

#include <QtCore/QObject>
#include <QtCore/QString>

class QSettings;

/**
 * @brief 全局设置存储（规格 §七）：默认终端类型、编码、字号、配色、热层内存行数。
 *
 * 生产环境用 instance()（INI 文件落在 QStandardPaths::AppConfigLocation）；
 * 测试用显式路径构造。任何字段变更发射 settingsChanged()，已打开标签实时应用。
 */
class ZzAppSettings : public QObject
{
    Q_OBJECT
public:
    /** @brief 以指定 INI 路径构造（测试与自定义场景）。 */
    explicit ZzAppSettings(const QString &filePath, QObject *parent = nullptr);

    /** @brief 生产单例：路径为 <AppConfigLocation>/settings.ini。 */
    static ZzAppSettings &instance();

    /** @brief 默认终端类型（TERM 值），默认 "xterm-256color"。 */
    [[nodiscard]] QString terminalType() const;
    void setTerminalType(const QString &terminalType);

    /** @brief 默认字符编码名（如 "UTF-8"、"GBK"），默认 "UTF-8"。 */
    [[nodiscard]] QString encoding() const;
    void setEncoding(const QString &encoding);

    /** @brief 默认字号（pt），默认 12。 */
    [[nodiscard]] int fontSize() const;
    void setFontSize(int fontSize);

    /** @brief 默认配色方案名（QTermWidget::availableColorSchemes 之一），默认 "Linux"。 */
    [[nodiscard]] QString colorScheme() const;
    void setColorScheme(const QString &colorScheme);

    /** @brief 终端内存历史行数上限（ZzLogEngine 热层之外的屏幕侧缓存），默认 10000。 */
    [[nodiscard]] int historyLines() const;
    void setHistoryLines(int lines);

signals:
    /** @brief 任一字段变更后发射；UI 层收到后实时应用到已打开标签。 */
    void settingsChanged();

private:
    QSettings *m_settings;
};
```

- [ ] **步骤 4：实现 `src/settings/ZzAppSettings.cpp`**

```cpp
#include "ZzAppSettings.h"

#include <QtCore/QSettings>
#include <QtCore/QStandardPaths>

ZzAppSettings::ZzAppSettings(const QString &filePath, QObject *parent)
    : QObject(parent)
    , m_settings(new QSettings(filePath, QSettings::IniFormat, this))
{
}

ZzAppSettings &ZzAppSettings::instance()
{
    static ZzAppSettings settings(
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        + QStringLiteral("/settings.ini"));
    return settings;
}

QString ZzAppSettings::terminalType() const
{
    return m_settings->value(QStringLiteral("terminal/type"),
                             QStringLiteral("xterm-256color")).toString();
}

void ZzAppSettings::setTerminalType(const QString &terminalType)
{
    m_settings->setValue(QStringLiteral("terminal/type"), terminalType);
    emit settingsChanged();
}

QString ZzAppSettings::encoding() const
{
    return m_settings->value(QStringLiteral("terminal/encoding"),
                             QStringLiteral("UTF-8")).toString();
}

void ZzAppSettings::setEncoding(const QString &encoding)
{
    m_settings->setValue(QStringLiteral("terminal/encoding"), encoding);
    emit settingsChanged();
}

int ZzAppSettings::fontSize() const
{
    return m_settings->value(QStringLiteral("terminal/fontSize"), 12).toInt();
}

void ZzAppSettings::setFontSize(int fontSize)
{
    m_settings->setValue(QStringLiteral("terminal/fontSize"), fontSize);
    emit settingsChanged();
}

QString ZzAppSettings::colorScheme() const
{
    return m_settings->value(QStringLiteral("terminal/colorScheme"),
                             QStringLiteral("Linux")).toString();
}

void ZzAppSettings::setColorScheme(const QString &colorScheme)
{
    m_settings->setValue(QStringLiteral("terminal/colorScheme"), colorScheme);
    emit settingsChanged();
}

int ZzAppSettings::historyLines() const
{
    return m_settings->value(QStringLiteral("terminal/historyLines"), 10000).toInt();
}

void ZzAppSettings::setHistoryLines(int lines)
{
    m_settings->setValue(QStringLiteral("terminal/historyLines"), lines);
    emit settingsChanged();
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    settings/ZzAppSettings.h
    settings/ZzAppSettings.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzAppSettings unit/tst_ZzAppSettings.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzAppSettings --output-on-failure
```

预期：PASS（2 个用例）。

- [ ] **步骤 6：附带性能测试 `tests/perf/tst_PerfSettings.cpp`（规格 §9.1）**

```cpp
#include <QtTest/QtTest>

#include "ZzPerfRecorder.h"
#include "settings/ZzAppSettings.h"

/**
 * @brief 性能门控：设置读写往返 1000 次。阈值 500ms（Release）。
 */
class tst_PerfSettings : public QObject
{
    Q_OBJECT
private slots:
    void settingsReadWriteThroughput()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settings-perf.ini"));
        QFile::remove(path);

        QElapsedTimer timer;
        timer.start();
        {
            ZzAppSettings settings(path);
            for (int i = 0; i < 1000; ++i) {
                settings.setFontSize(10 + (i % 20));
                (void)settings.fontSize();
                (void)settings.terminalType();
            }
        }
        const double elapsed = static_cast<double>(timer.elapsed());
        QFile::remove(path);

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("app-settings"),
            QStringLiteral("设置读写往返 1000 次"), 500.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 500ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfSettings)
#include "tst_PerfSettings.moc"
```

`tests/perf/CMakeLists.txt` 末尾追加：

```cmake
zz_add_perf_test(tst_PerfSettings tst_PerfSettings.cpp)
```

先 Debug 验证逻辑（预期 SKIP 不失败），再 Release 跑真实数字：

```bash
cmake --preset linux-gcc-debug && cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_PerfSettings --output-on-failure
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R tst_PerfSettings --output-on-failure
```

预期：Debug 下 SKIP；Release 下 PASS 且生成 `tests/perf/records/<今日>-app-settings.json`。

- [ ] **步骤 7：Commit**

```bash
git add src/settings tests/unit/tst_ZzAppSettings.cpp tests/perf/tst_PerfSettings.cpp tests/perf/records src/CMakeLists.txt tests/CMakeLists.txt tests/perf/CMakeLists.txt
git commit -m "feat: 全局设置存储 ZzAppSettings（含性能门控记录）"
```

---

## 任务 6：终端视图（ZzTerminalView：QTermWidget + 传输胶水）

规格 §七：每标签持有一个 ZzTerminalView（组合 QTermWidget + 传输）；字节流双向转发是应用层职责（规格 §三：SSH 层与终端层互相无感知）。

**文件：**
- 创建：`src/terminal/ZzTerminalView.h`、`src/terminal/ZzTerminalView.cpp`
- 创建：`tests/unit/tst_ZzTerminalView.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzTerminalView.cpp`**

```cpp
#include <QtTest/QtTest>

#include "qtermwidget.h"
#include "ZzMockTransport.h"
#include "settings/ZzAppSettings.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 验证终端视图胶水：双向字节流、尺寸转发、设置应用、编码查询。
 */
class tst_ZzTerminalView : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void bidirectionalByteStream()
    {
        ZzTerminalView view;
        auto *transport = new ZzMockTransport(&view);
        transport->echoEnabled = false; // 回显关掉，分辨两个方向
        view.setTransport(transport);

        ZzTransportEndpoint endpoint;
        transport->open(endpoint);
        QTRY_VERIFY(transport->state() == ZzTransportInterface::State::Connected);

        // 远端 → 终端：不应崩溃，视图保持存活即通过（像素内容属 ZzTermWidget 测试域）
        transport->simulateData("hello remote\r\n");
        QCoreApplication::processEvents();

        // 终端 → 远端：模拟键盘输入
        emit view.termWidget()->sendData("ls\n", 3);
        QCoreApplication::processEvents();
        QCOMPARE(transport->writtenData, QByteArray("ls\n"));
    }

    void sizeForwardsToTransport()
    {
        ZzTerminalView view;
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        QSignalSpy sizeSpy(&view, &ZzTerminalView::sizeChanged);

        // QTermWidget::termSizeChange(lines, columns) → transport->resize(cols, rows)
        emit view.termWidget()->termSizeChange(40, 100);
        QCOMPARE(transport->lastCols, 100);
        QCOMPARE(transport->lastRows, 40);
        QCOMPARE(sizeSpy.count(), 1);
        QCOMPARE(sizeSpy.first().at(0).toInt(), 100);
        QCOMPARE(sizeSpy.first().at(1).toInt(), 40);
    }

    void applyGlobalSettings()
    {
        const QString path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-view-settings.ini"));
        QFile::remove(path);
        ZzAppSettings settings(path);
        settings.setFontSize(18);
        settings.setEncoding(QStringLiteral("GBK"));
        settings.setHistoryLines(5000);
        if (QTermWidget::availableColorSchemes().contains(QStringLiteral("QuardCRT"))) {
            settings.setColorScheme(QStringLiteral("QuardCRT"));
        }

        ZzTerminalView view;
        view.applySettings(settings);
        QCOMPARE(view.termWidget()->getTerminalFont().pointSize(), 18);
        QCOMPARE(view.termWidget()->historySize(), 5000);
        QCOMPARE(view.encoding(), QStringLiteral("GBK"));
        QFile::remove(path);
    }

    void statePassthrough()
    {
        ZzTerminalView view;
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        QSignalSpy stateSpy(&view, &ZzTerminalView::stateChanged);

        transport->open(ZzTransportEndpoint{});
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
        QCOMPARE(stateSpy.count(), 2); // Connecting + Connected

        transport->simulateDisconnect(QStringLiteral("对端关闭"));
        QCOMPARE(view.transportState(), ZzTransportInterface::State::Disconnected);
    }
};

QTEST_MAIN(tst_ZzTerminalView)
#include "tst_ZzTerminalView.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzTerminalView
```

预期：编译失败，报错 `terminal/ZzTerminalView.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/terminal/ZzTerminalView.h`**

```cpp
#pragma once

#include <QtWidgets/QWidget>

#include "transport/ZzTransportEndpoint.h"
#include "transport/ZzTransportInterface.h"

class QTermWidget;
class ZzAppSettings;

/**
 * @brief 单标签终端视图：组合 QTermWidget 与一个传输实例（规格 §七）。
 *
 * 职责只有胶水：远端输出 → recvData，键盘输入 → transport->write，
 * 尺寸变化 → transport->resize；外加错误横幅（任务 13）与设置应用。
 * 不拥有传输的所有权以外的语义——传输以本视图为 QObject 父对象随视图销毁。
 */
class ZzTerminalView : public QWidget
{
    Q_OBJECT
public:
    explicit ZzTerminalView(QWidget *parent = nullptr);

    /**
     * @brief 绑定传输并接线（可重复调用，用于断线重连换新实例）。
     * @param transport 必须已将本视图设为 QObject 父对象。
     */
    void setTransport(ZzTransportInterface *transport);

    /** @brief 当前绑定的传输（可空）。 */
    [[nodiscard]] ZzTransportInterface *transport() const;

    /** @brief 内部 QTermWidget（测试与滚动历史桥使用）。 */
    [[nodiscard]] QTermWidget *termWidget() const;

    /** @brief 以给定参数打开传输并记忆，供重连复用。 */
    void openEndpoint(const ZzTransportEndpoint &endpoint);

    /** @brief 按当前编码名展示（状态栏用）。 */
    [[nodiscard]] QString encoding() const;

    /** @brief 当前传输状态（未绑定视为 Disconnected）。 */
    [[nodiscard]] ZzTransportInterface::State transportState() const;

    /** @brief 应用全局设置：字号、编码、配色、内存历史行数。 */
    void applySettings(const ZzAppSettings &settings);

signals:
    /** @brief 传输状态透传（ZzTabManager 据此刷新标签外观与状态栏）。 */
    void stateChanged(ZzTransportInterface::State state);
    /** @brief 终端尺寸变化（列、行），状态栏用。 */
    void sizeChanged(int cols, int rows);
    /** @brief 传输错误透传（横幅展示由任务 13 接入本信号链路）。 */
    void errorOccurred(const QString &message);
    /** @brief 被动断开透传。 */
    void disconnected(const QString &reason);

private:
    QTermWidget *m_term = nullptr;
    ZzTransportInterface *m_transport = nullptr;
    ZzTransportEndpoint m_lastEndpoint;  ///< 最近一次 open 参数（重连用）
    QString m_encoding;                  ///< 状态栏展示的编码名
};
```

- [ ] **步骤 4：实现 `src/terminal/ZzTerminalView.cpp`**

```cpp
#include "ZzTerminalView.h"

#include <QtCore/QStringConverter>
#include <QtWidgets/QVBoxLayout>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"

ZzTerminalView::ZzTerminalView(QWidget *parent)
    : QWidget(parent)
{
    m_term = new QTermWidget(this, this);
    m_term->setScrollBarPosition(QTermWidget::ScrollBarRight);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(m_term, 1);

    // 终端 → 传输（键盘输入方向）
    connect(m_term, &QTermWidget::sendData, this,
            [this](const char *data, int size) {
                if (m_transport) {
                    m_transport->write(QByteArray(data, size));
                }
            });
    // 终端尺寸 → 传输 + 状态栏
    connect(m_term, &QTermWidget::termSizeChange, this,
            [this](int lines, int columns) {
                if (m_transport) {
                    m_transport->resize(columns, lines);
                }
                emit sizeChanged(columns, lines);
            });

    applySettings(ZzAppSettings::instance());
}

void ZzTerminalView::setTransport(ZzTransportInterface *transport)
{
    if (m_transport) {
        disconnect(m_transport, nullptr, this, nullptr);
    }
    m_transport = transport;
    if (!m_transport) {
        return;
    }
    // 传输 → 终端（远端输出方向）
    connect(m_transport, &ZzTransportInterface::dataReceived, this,
            [this](const QByteArray &data) {
                m_term->recvData(data.constData(), data.size());
            });
    connect(m_transport, &ZzTransportInterface::stateChanged, this,
            [this](ZzTransportInterface::State state) {
                emit stateChanged(state);
            });
    connect(m_transport, &ZzTransportInterface::errorOccurred, this,
            [this](int, const QString &message) {
                emit errorOccurred(message);
            });
    connect(m_transport, &ZzTransportInterface::disconnected, this,
            [this](const QString &reason) { emit disconnected(reason); });
}

ZzTransportInterface *ZzTerminalView::transport() const
{
    return m_transport;
}

QTermWidget *ZzTerminalView::termWidget() const
{
    return m_term;
}

void ZzTerminalView::openEndpoint(const ZzTransportEndpoint &endpoint)
{
    m_lastEndpoint = endpoint;
    if (m_transport) {
        m_transport->open(endpoint);
    }
}

QString ZzTerminalView::encoding() const
{
    return m_encoding;
}

ZzTransportInterface::State ZzTerminalView::transportState() const
{
    return m_transport ? m_transport->state()
                       : ZzTransportInterface::State::Disconnected;
}

void ZzTerminalView::applySettings(const ZzAppSettings &settings)
{
    QFont font = m_term->getTerminalFont();
    font.setPointSize(settings.fontSize());
    m_term->setTerminalFont(font);

    m_encoding = settings.encoding();
    const auto encoding =
        QStringConverter::encodingForName(settings.encoding().toUtf8().constData());
    if (encoding) {
        m_term->setTextCodec(QStringEncoder(*encoding));
    }

    if (QTermWidget::availableColorSchemes().contains(settings.colorScheme())) {
        m_term->setColorScheme(settings.colorScheme());
    }
    m_term->setHistorySize(settings.historyLines());
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    terminal/ZzTerminalView.h
    terminal/ZzTerminalView.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzTerminalView unit/tst_ZzTerminalView.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzTerminalView --output-on-failure
```

预期：PASS（4 个用例）。

- [ ] **步骤 6：Commit**

```bash
git add src/terminal tests/unit/tst_ZzTerminalView.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 终端视图 ZzTerminalView（QTermWidget 双向字节流胶水、设置应用、错误横幅）"
```

---

## 任务 7：多标签管理（ZzTabManager）

规格 §七：每标签持有一个 ZzTerminalView；支持关闭、拖拽排序；断线标签变灰保留，右键重连，不自动关标签。

**文件：**
- 创建：`src/tab/ZzTabManager.h`、`src/tab/ZzTabManager.cpp`
- 创建：`tests/unit/tst_ZzTabManager.cpp`、`tests/perf/tst_PerfTabLifecycle.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`、`tests/perf/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzTabManager.cpp`**

```cpp
#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 验证标签管理生命周期：开会话、关闭、断线变灰保留、右键重连（规格 §七/§九）。
 */
class tst_ZzTabManager : public QObject
{
    Q_OBJECT
private:
    /** @brief 构造一个 mock 协议会话。 */
    static ZzSessionProfile makeProfile(const QString &id, const QString &name)
    {
        ZzSessionProfile profile;
        profile.id = id;
        profile.name = name;
        profile.groupPath = QStringLiteral("测试分组");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("example.com");
        profile.port = 22;
        profile.user = QStringLiteral("root");
        return profile;
    }

private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        QVERIFY(ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); }));
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void openAddsConnectedTab()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("s1"), QStringLiteral("生产A")));
        QCOMPARE(tabs.count(), 1);
        QCOMPARE(tabs.tabText(0), QStringLiteral("生产A"));

        auto *view = tabs.viewAt(0);
        QVERIFY(view != nullptr);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);
        QVERIFY(tabs.tabsClosable());
        QVERIFY(tabs.isMovable()); // 拖拽排序
    }

    void closeTabDestroysView()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("s2"), QStringLiteral("临时")));
        auto *view = tabs.viewAt(0);
        QPointer<ZzTerminalView> guard(view);
        tabs.closeTab(0);
        QCOMPARE(tabs.count(), 0);
        QCoreApplication::processEvents();
        QVERIFY(guard.isNull());
    }

    void disconnectGreysTabButKeepsIt()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("s3"), QStringLiteral("断线机")));
        auto *view = tabs.viewAt(0);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        mock->simulateDisconnect(QStringLiteral("网络中断"));

        // 不自动关标签（规格 §七）
        QCOMPARE(tabs.count(), 1);
        // 标签变灰
        QCOMPARE(tabs.tabBar()->tabTextColor(0), QColor(Qt::gray));
        QVERIFY(tabs.isTabDisconnected(0));
    }

    void reconnectCreatesFreshTransport()
    {
        ZzTabManager tabs;
        tabs.openSession(makeProfile(QStringLiteral("s4"), QStringLiteral("重连机")));
        auto *view = tabs.viewAt(0);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);
        auto *firstMock = static_cast<ZzMockTransport *>(view->transport());
        firstMock->simulateDisconnect(QStringLiteral("掉线"));

        tabs.reconnectTab(0);
        auto *secondMock = static_cast<ZzMockTransport *>(view->transport());
        // ZzSshConnection 不可重复 connectToHost（规格 §十注释约定），重连必须换新实例
        QVERIFY(secondMock != firstMock);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);
        // 恢复非灰色
        QVERIFY(tabs.tabBar()->tabTextColor(0) != QColor(Qt::gray));
        QVERIFY(!tabs.isTabDisconnected(0));
    }

    void unknownProtocolShowsStatusMessage()
    {
        ZzTabManager tabs;
        QSignalSpy msgSpy(&tabs, &ZzTabManager::statusMessage);
        ZzSessionProfile bad = makeProfile(QStringLiteral("s5"), QStringLiteral("坏协议"));
        bad.protocol = QStringLiteral("telnet-1996");
        tabs.openSession(bad);
        QCOMPARE(tabs.count(), 0); // 未建标签
        QCOMPARE(msgSpy.count(), 1);
        QVERIFY(msgSpy.first().at(0).toString().contains(QStringLiteral("telnet-1996")));
    }

    void currentTabSignalsForStatusBar()
    {
        ZzTabManager tabs;
        QSignalSpy stateSpy(&tabs, &ZzTabManager::currentStateChanged);
        tabs.openSession(makeProfile(QStringLiteral("s6"), QStringLiteral("状态栏")));
        QTRY_VERIFY(stateSpy.count() >= 2); // Connecting + Connected
    }
};

QTEST_MAIN(tst_ZzTabManager)
#include "tst_ZzTabManager.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzTabManager
```

预期：编译失败，报错 `tab/ZzTabManager.h: No such file or directory`（若计划 03 未交付则还会缺 `session/ZzSessionProfile.h`——本任务起依赖计划 03 落地）。

- [ ] **步骤 3：实现 `src/tab/ZzTabManager.h`**

```cpp
#pragma once

#include <functional>

#include <QtCore/QHash>
#include <QtWidgets/QTabWidget>

#include "session/ZzSessionProfile.h"
#include "transport/ZzTransportInterface.h"

class ZzTerminalView;

/**
 * @brief 多标签管理：每标签一个 ZzTerminalView（规格 §七）。
 *
 * 行为约定：关闭即销毁视图与传输；断线标签变灰保留、不自动关；
 * 重连创建全新传输实例（ZzSshConnection 不可重复 connectToHost）；
 * 拖拽排序由 QTabWidget 自带 movable 提供。
 */
class ZzTabManager : public QTabWidget
{
    Q_OBJECT
public:
    /** @brief SSH 密码索取回调：按 profile 返回明文密码（空=取消认证）。 */
    using ZzPasswordProvider =
        std::function<QString(const ZzSessionProfile &profile)>;
    /** @brief 主机密钥确认回调：host/fingerprint/changed → 是否接受。 */
    using ZzHostKeyConfirmer =
        std::function<bool(const QString &host, const QString &fingerprint,
                           bool changed)>;

    explicit ZzTabManager(QWidget *parent = nullptr);

    /** @brief 按会话 profile 新建标签并开始连接（协议未知则只发状态消息）。 */
    void openSession(const ZzSessionProfile &profile);

    /** @brief 关闭并销毁指定标签。 */
    void closeTab(int index);

    /** @brief 重连指定标签：换新传输实例后按记忆 profile 重新打开。 */
    void reconnectTab(int index);

    /** @brief 取标签内的终端视图（测试与状态栏用），越界返回 nullptr。 */
    [[nodiscard]] ZzTerminalView *viewAt(int index) const;

    /** @brief 标签是否处于断线保留状态。 */
    [[nodiscard]] bool isTabDisconnected(int index) const;

    /** @brief 装配 SSH 密码索取回调（ZzAppShell 注入）。 */
    void setPasswordProvider(ZzPasswordProvider provider);

    /** @brief 装配主机密钥确认回调（ZzAppShell 注入）。 */
    void setHostKeyConfirmer(ZzHostKeyConfirmer confirmer);

signals:
    /** @brief 当前标签传输状态变化（状态栏）。 */
    void currentStateChanged(ZzTransportInterface::State state);
    /** @brief 当前标签编码（状态栏）。 */
    void currentEncodingChanged(const QString &encoding);
    /** @brief 当前标签终端尺寸（状态栏）。 */
    void currentSizeChanged(int cols, int rows);
    /** @brief 需要状态栏展示的瞬时消息（错误处理走状态栏，规格 §八）。 */
    void statusMessage(const QString &message);

private slots:
    void showTabContextMenu(const QPoint &pos);

private:
    /** @brief profile → 传输参数映射（终端类型等空值回落到全局设置）。 */
    ZzTransportEndpoint endpointFor(const ZzSessionProfile &profile) const;
    void wireView(int index, ZzTerminalView *view);
    void markTabDisconnected(int index, const QString &reason);

    QHash<ZzTerminalView *, ZzSessionProfile> m_tabProfiles;
    ZzPasswordProvider m_passwordProvider;
    ZzHostKeyConfirmer m_hostKeyConfirmer;
};
```

- [ ] **步骤 4：实现 `src/tab/ZzTabManager.cpp`**

```cpp
#include "ZzTabManager.h"

#include <utility>

#include <QtWidgets/QMenu>
#include <QtWidgets/QTabBar>

#include "settings/ZzAppSettings.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzSshTransport.h"
#include "transport/ZzTransportRegistry.h"

ZzTabManager::ZzTabManager(QWidget *parent)
    : QTabWidget(parent)
{
    setTabsClosable(true);
    setMovable(true);          // 拖拽排序
    setDocumentMode(true);
    tabBar()->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(tabBar(), &QTabBar::customContextMenuRequested,
            this, &ZzTabManager::showTabContextMenu);
    connect(this, &QTabWidget::tabCloseRequested,
            this, &ZzTabManager::closeTab);
    // 切换标签时刷新状态栏三要素
    connect(this, &QTabWidget::currentChanged, this, [this](int index) {
        ZzTerminalView *view = viewAt(index);
        if (!view) {
            return;
        }
        emit currentStateChanged(view->transportState());
        emit currentEncodingChanged(view->encoding());
        emit currentSizeChanged(view->termWidget()->screenColumnsCount(),
                                view->termWidget()->screenLinesCount());
    });
}

void ZzTabManager::openSession(const ZzSessionProfile &profile)
{
    auto *view = new ZzTerminalView(this);
    ZzTransportInterface *transport =
        ZzTransportRegistry::instance().create(profile.protocol, view);
    if (!transport) {
        emit statusMessage(QStringLiteral("未知协议「%1」：会话 %2 未打开")
                               .arg(profile.protocol, profile.name));
        view->deleteLater();
        return;
    }
    // SSH 传输装配认证与主机密钥回调（本地 PTY 不需要）
    if (auto *ssh = qobject_cast<ZzSshTransport *>(transport)) {
        const ZzPasswordProvider provider = m_passwordProvider;
        ssh->setPasswordProvider([provider, profile]() -> QString {
            return provider ? provider(profile) : QString();
        });
        ssh->setHostKeyConfirmer(
            [confirmer = m_hostKeyConfirmer](const QString &host,
                                             const QString &fingerprint,
                                             bool changed) {
                return confirmer ? confirmer(host, fingerprint, changed) : false;
            });
    }
    view->setTransport(transport);
    view->applySettings(ZzAppSettings::instance());

    m_tabProfiles.insert(view, profile);
    const int index = addTab(view, profile.name);
    setCurrentIndex(index);
    wireView(index, view);

    view->openEndpoint(endpointFor(profile));
}

void ZzTabManager::closeTab(int index)
{
    ZzTerminalView *view = viewAt(index);
    if (!view) {
        return;
    }
    if (view->transport()) {
        view->transport()->close();
    }
    m_tabProfiles.remove(view);
    removeTab(index);
    view->deleteLater();
}

void ZzTabManager::reconnectTab(int index)
{
    ZzTerminalView *view = viewAt(index);
    if (!view || !m_tabProfiles.contains(view)) {
        return;
    }
    const ZzSessionProfile profile = m_tabProfiles.value(view);
    if (view->transport()) {
        view->transport()->close();
        view->transport()->deleteLater(); // 旧实例废弃，重连必须新实例
    }
    ZzTransportInterface *transport =
        ZzTransportRegistry::instance().create(profile.protocol, view);
    if (!transport) {
        emit statusMessage(QStringLiteral("重连失败：协议「%1」未注册")
                               .arg(profile.protocol));
        return;
    }
    if (auto *ssh = qobject_cast<ZzSshTransport *>(transport)) {
        const ZzPasswordProvider provider = m_passwordProvider;
        ssh->setPasswordProvider([provider, profile]() -> QString {
            return provider ? provider(profile) : QString();
        });
        ssh->setHostKeyConfirmer(
            [confirmer = m_hostKeyConfirmer](const QString &host,
                                             const QString &fingerprint,
                                             bool changed) {
                return confirmer ? confirmer(host, fingerprint, changed) : false;
            });
    }
    view->setTransport(transport);
    tabBar()->setTabTextColor(index, palette().color(QPalette::WindowText));
    tabBar()->setTabToolTip(index, QString());
    // 注意：视图级信号接线（wireView）在 openSession 已建立，此处不得重复调用，
    // 否则 currentStateChanged 等信号会翻倍发射
    view->openEndpoint(endpointFor(profile));
}

ZzTerminalView *ZzTabManager::viewAt(int index) const
{
    return qobject_cast<ZzTerminalView *>(widget(index));
}

bool ZzTabManager::isTabDisconnected(int index) const
{
    const ZzTerminalView *view = viewAt(index);
    return view && m_tabProfiles.contains(const_cast<ZzTerminalView *>(view))
        && view->transportState() == ZzTransportInterface::State::Disconnected
        && tabBar()->tabTextColor(index) == QColor(Qt::gray);
}

void ZzTabManager::setPasswordProvider(ZzPasswordProvider provider)
{
    m_passwordProvider = std::move(provider);
}

void ZzTabManager::setHostKeyConfirmer(ZzHostKeyConfirmer confirmer)
{
    m_hostKeyConfirmer = std::move(confirmer);
}

void ZzTabManager::showTabContextMenu(const QPoint &pos)
{
    const int index = tabBar()->tabAt(pos);
    if (index < 0) {
        return;
    }
    QMenu menu(this);
    QAction *reconnectAction =
        menu.addAction(QStringLiteral("重新连接"));
    reconnectAction->setEnabled(isTabDisconnected(index));
    QAction *closeAction = menu.addAction(QStringLiteral("关闭标签"));
    QAction *chosen = menu.exec(tabBar()->mapToGlobal(pos));
    if (chosen == reconnectAction) {
        reconnectTab(index);
    } else if (chosen == closeAction) {
        closeTab(index);
    }
}

ZzTransportEndpoint ZzTabManager::endpointFor(const ZzSessionProfile &profile) const
{
    const ZzAppSettings &settings = ZzAppSettings::instance();
    ZzTransportEndpoint endpoint;
    endpoint.host = profile.host;
    endpoint.port = profile.port;
    endpoint.user = profile.user;
    endpoint.terminalType = profile.terminalType.isEmpty()
        ? settings.terminalType() : profile.terminalType;
    endpoint.keyPath = profile.keyPath;
    endpoint.localShell = (profile.protocol == QStringLiteral("local"));
    if (endpoint.localShell) {
        // 契约约定：local 会话的 shell 程序路径存于 host 字段（可空=系统默认）
        endpoint.shellProgram = profile.host;
    }
    // 初始行列以视图当前尺寸为准，open 后由 termSizeChange 信号持续同步
    auto *view = qobject_cast<ZzTerminalView *>(currentWidget());
    endpoint.cols = view ? view->termWidget()->screenColumnsCount() : 80;
    endpoint.rows = view ? view->termWidget()->screenLinesCount() : 24;
    return endpoint;
}

void ZzTabManager::wireView(int index, ZzTerminalView *view)
{
    connect(view, &ZzTerminalView::disconnected, this,
            [this, view](const QString &reason) {
                const int i = indexOf(view);
                if (i >= 0) {
                    markTabDisconnected(i, reason);
                }
            });
    connect(view, &ZzTerminalView::stateChanged, this,
            [this, view](ZzTransportInterface::State state) {
                const int i = indexOf(view);
                if (i < 0) {
                    return;
                }
                // 重新连通：恢复正常颜色
                if (state == ZzTransportInterface::State::Connected) {
                    tabBar()->setTabTextColor(
                        i, palette().color(QPalette::WindowText));
                    tabBar()->setTabToolTip(i, QString());
                }
                if (i == currentIndex()) {
                    emit currentStateChanged(state);
                }
            });
    connect(view, &ZzTerminalView::sizeChanged, this,
            [this, view](int cols, int rows) {
                if (indexOf(view) == currentIndex()) {
                    emit currentSizeChanged(cols, rows);
                }
            });
    connect(view, &ZzTerminalView::errorOccurred, this,
            [this](const QString &message) { emit statusMessage(message); });
}

void ZzTabManager::markTabDisconnected(int index, const QString &reason)
{
    tabBar()->setTabTextColor(index, QColor(Qt::gray)); // 断线变灰保留
    tabBar()->setTabToolTip(index, reason);
    emit statusMessage(QStringLiteral("%1 已断开：%2")
                           .arg(tabText(index), reason));
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    tab/ZzTabManager.h
    tab/ZzTabManager.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzTabManager unit/tst_ZzTabManager.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzTabManager --output-on-failure
```

预期：PASS（6 个用例）。

> **执行顺序（给执行者）：** ZzTabManager 引用任务 8 的 `ZzSshTransport` 做认证回调装配，两任务存在编译期耦合。执行顺序为：先完成任务 7 步骤 1-4（代码全部就位，暂不编译）→ 完成任务 8（ZzSshTransport 就位）→ 回到本步骤编译并跑通 `tst_ZzTabManager` 与 `tst_ZzSshTransport` → 然后依次执行任务 7 步骤 6-7、任务 8 步骤 5-6。commit 顺序保持任务 7 在前、任务 8 在后。

- [ ] **步骤 6：附带性能测试 `tests/perf/tst_PerfTabLifecycle.cpp`（规格 §9.1）**

```cpp
#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "ZzPerfRecorder.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 性能门控：连续打开并关闭 50 个标签。阈值 3000ms（Release）。
 */
class tst_PerfTabLifecycle : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase()
    {
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void openCloseFiftyTabs()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        ZzTabManager tabs;

        QElapsedTimer timer;
        timer.start();
        for (int i = 0; i < 50; ++i) {
            ZzSessionProfile profile;
            profile.id = QStringLiteral("perf-%1").arg(i);
            profile.name = QStringLiteral("性能标签%1").arg(i);
            profile.protocol = QStringLiteral("mock");
            tabs.openSession(profile);
        }
        for (int i = 49; i >= 0; --i) {
            tabs.closeTab(i);
        }
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("tab-lifecycle"),
            QStringLiteral("打开并关闭 50 个标签"), 3000.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 3000ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfTabLifecycle)
#include "tst_PerfTabLifecycle.moc"
```

`tests/perf/CMakeLists.txt` 末尾追加：

```cmake
zz_add_perf_test(tst_PerfTabLifecycle tst_PerfTabLifecycle.cpp)
```

运行（Debug 验 SKIP、Release 出数字）：

```bash
cmake --preset linux-gcc-debug && cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_PerfTabLifecycle --output-on-failure
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R tst_PerfTabLifecycle --output-on-failure
```

预期：Debug SKIP；Release PASS 并生成 `tests/perf/records/<今日>-tab-lifecycle.json`。

- [ ] **步骤 7：Commit（在任务 8 源码就位后执行）**

```bash
git add src/tab tests/unit/tst_ZzTabManager.cpp tests/perf/tst_PerfTabLifecycle.cpp tests/perf/records src/CMakeLists.txt tests/CMakeLists.txt tests/perf/CMakeLists.txt
git commit -m "feat: 多标签管理 ZzTabManager（关闭/拖拽/断线变灰/右键重连，含性能门控）"
```

---

## 任务 8：SSH 传输适配器（ZzSshTransport）

规格 §三：应用层把 ZzSshCore channel 字节流双向接到 QTermWidget；本类即 SSH 侧的 ZzTransportInterface 适配。连接/认证/主机密钥细节全部在 ZzSshCore 内（计划 01），本类只做信号翻译与回调注入。

**文件：**
- 创建：`src/transport/ZzSshTransport.h`、`src/transport/ZzSshTransport.cpp`
- 创建：`tests/unit/tst_ZzSshTransport.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSshTransport.cpp`**

```cpp
#include <QtTest/QtTest>

#include "transport/ZzSshTransport.h"

/**
 * @brief 验证 SSH 适配器的错误透传与状态机（成功路径由计划 01 的 Docker 集成测试覆盖）。
 */
class tst_ZzSshTransport : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void connectionRefusedEmitsError()
    {
        ZzSshTransport transport;
        QSignalSpy errorSpy(&transport, &ZzTransportInterface::errorOccurred);

        ZzTransportEndpoint endpoint;
        endpoint.host = QStringLiteral("127.0.0.1");
        endpoint.port = 1; // 基本必然无人监听的端口
        endpoint.user = QStringLiteral("nobody");
        transport.open(endpoint);

        QCOMPARE(transport.state(), ZzTransportInterface::State::Connecting);
        QTRY_VERIFY_WITH_TIMEOUT(errorSpy.count() >= 1, 10000);
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }

    void writeBeforeConnectedIsSafe()
    {
        // 未连接时 write/resize/close 不得崩溃
        ZzSshTransport transport;
        transport.write("x");
        transport.resize(80, 24);
        transport.close();
        QCOMPARE(transport.state(), ZzTransportInterface::State::Disconnected);
    }
};

QTEST_MAIN(tst_ZzSshTransport)
#include "tst_ZzSshTransport.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzSshTransport
```

预期：编译失败，报错 `transport/ZzSshTransport.h: No such file or directory`。本任务起依赖计划 01（ZzSshCore 目标与头文件）。

- [ ] **步骤 3：实现 `src/transport/ZzSshTransport.h`**

```cpp
#pragma once

#include <functional>

#include "ZzTransportInterface.h"

class ZzSshConnection;
class ZzSshShellChannel;

/**
 * @brief SSH 传输适配器：把 ZzSshConnection/ZzSshShellChannel 包装成
 *        ZzTransportInterface（规格 §2.3/§4.2）。
 *
 * 认证链（agent→公钥→密码）在 ZzSshCore 内部；密码经 m_passwordProvider
 * 向上层索取，主机密钥确认经 m_hostKeyConfirmer 交给 UI（规格 §八安全底线）。
 */
class ZzSshTransport : public ZzTransportInterface
{
    Q_OBJECT
public:
    /** @brief 密码索取回调：返回明文密码，空串表示用户取消。 */
    using ZzPasswordProvider = std::function<QString()>;
    /** @brief 主机密钥确认回调：host/fingerprint/changed → 是否接受并记住。 */
    using ZzHostKeyConfirmer =
        std::function<bool(const QString &host, const QString &fingerprint,
                           bool changed)>;

    explicit ZzSshTransport(QObject *parent = nullptr);
    ~ZzSshTransport() override;

    void open(const ZzTransportEndpoint &endpoint) override;
    void write(const QByteArray &data) override;
    void resize(int cols, int rows) override;
    void close() override;

    void setPasswordProvider(ZzPasswordProvider provider);
    void setHostKeyConfirmer(ZzHostKeyConfirmer confirmer);

private:
    void wireConnection();
    void onConnected();

    ZzSshConnection *m_conn = nullptr;      ///< 本对象为父，随适配器销毁
    ZzSshShellChannel *m_channel = nullptr; ///< 观察指针，连接断开即失效
    ZzTransportEndpoint m_endpoint;
    ZzPasswordProvider m_passwordProvider;
    ZzHostKeyConfirmer m_hostKeyConfirmer;
};
```

- [ ] **步骤 4：实现 `src/transport/ZzSshTransport.cpp`**

```cpp
#include "ZzSshTransport.h"

#include <utility>

#include <ZzSshCore/ZzSshConnection.h>
#include <ZzSshCore/ZzSshShellChannel.h>

ZzSshTransport::ZzSshTransport(QObject *parent)
    : ZzTransportInterface(parent)
{
}

ZzSshTransport::~ZzSshTransport()
{
    close();
}

void ZzSshTransport::setPasswordProvider(ZzPasswordProvider provider)
{
    m_passwordProvider = std::move(provider);
}

void ZzSshTransport::setHostKeyConfirmer(ZzHostKeyConfirmer confirmer)
{
    m_hostKeyConfirmer = std::move(confirmer);
}

void ZzSshTransport::open(const ZzTransportEndpoint &endpoint)
{
    if (state() != State::Disconnected) {
        return;
    }
    // 重试/重连场景：废弃旧连接对象（规格 §4.2 注释：同一连接不可重复 connectToHost）
    if (m_conn) {
        m_conn->disconnectFromHost();
        m_conn->deleteLater();
        m_conn = nullptr;
        m_channel = nullptr;
    }
    m_endpoint = endpoint;
    setState(State::Connecting);

    m_conn = new ZzSshConnection(this);
    wireConnection();
    if (!endpoint.keyPath.isEmpty()) {
        m_conn->setKeyPath(endpoint.keyPath);
    }
    m_conn->connectToHost(endpoint.host, endpoint.port, endpoint.user);
}

void ZzSshTransport::wireConnection()
{
    connect(m_conn, &ZzSshConnection::connected,
            this, &ZzSshTransport::onConnected);
    connect(m_conn, &ZzSshConnection::errorOccurred, this,
            [this](int code, const QString &message) {
                setState(State::Disconnected);
                emit errorOccurred(code, message);
            });
    connect(m_conn, &ZzSshConnection::disconnected, this,
            [this](const QString &reason) {
                m_channel = nullptr;
                setState(State::Disconnected);
                emit disconnected(reason);
            });
    // 认证链密码回调（规格 §4.2：上层不感知尝试顺序，只负责给密码）
    connect(m_conn, &ZzSshConnection::passwordRequested, this, [this]() {
        m_conn->providePassword(m_passwordProvider ? m_passwordProvider()
                                                   : QString());
    });
    // 主机密钥确认（规格 §八安全底线，不可省略）
    connect(m_conn, &ZzSshConnection::hostKeyUnknown, this,
            [this](const QString &host, const QString &fingerprint) {
                const bool accept = m_hostKeyConfirmer
                    ? m_hostKeyConfirmer(host, fingerprint, false) : false;
                accept ? m_conn->acceptHostKey(true) : m_conn->rejectHostKey();
            });
    connect(m_conn, &ZzSshConnection::hostKeyChanged, this,
            [this](const QString &host, const QString &fingerprint) {
                const bool accept = m_hostKeyConfirmer
                    ? m_hostKeyConfirmer(host, fingerprint, true) : false;
                accept ? m_conn->acceptHostKey(true) : m_conn->rejectHostKey();
            });
}

void ZzSshTransport::onConnected()
{
    m_channel = m_conn->openShellChannel();
    if (!m_channel) {
        setState(State::Disconnected);
        emit errorOccurred(3001, QStringLiteral("创建 shell 通道失败"));
        return;
    }
    connect(m_channel, &ZzSshShellChannel::dataReceived, this,
            [this](const QByteArray &data) { emit dataReceived(data); });
    connect(m_channel, &ZzSshShellChannel::closed, this, [this]() {
        m_channel = nullptr;
        setState(State::Disconnected);
        emit disconnected(QStringLiteral("远程 shell 已关闭"));
    });

    if (!m_channel->openShell(m_endpoint.terminalType,
                              m_endpoint.cols, m_endpoint.rows)) {
        m_channel = nullptr;
        setState(State::Disconnected);
        emit errorOccurred(3002, QStringLiteral("打开 shell 失败"));
        return;
    }
    setState(State::Connected);
}

void ZzSshTransport::write(const QByteArray &data)
{
    if (m_channel && state() == State::Connected) {
        m_channel->write(data);
    }
}

void ZzSshTransport::resize(int cols, int rows)
{
    if (m_channel && state() == State::Connected) {
        m_channel->resize(cols, rows);
    }
}

void ZzSshTransport::close()
{
    if (m_channel) {
        m_channel->close();
        m_channel = nullptr;
    }
    if (m_conn) {
        m_conn->disconnectFromHost();
        m_conn->deleteLater();
        m_conn = nullptr;
    }
    setState(State::Disconnected);
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    transport/ZzSshTransport.h
    transport/ZzSshTransport.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzSshTransport unit/tst_ZzSshTransport.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R "tst_ZzSshTransport|tst_ZzTabManager" --output-on-failure
```

预期：两个测试均 PASS（ZzSshTransport 2 个用例 + ZzTabManager 6 个用例）。若 ZzSshCore 头文件路径不是 `<ZzSshCore/ZzSshConnection.h>`，按计划 01 实际导出的 include 布局调整两处 `#include`。

- [ ] **步骤 6：Commit**

```bash
git add src/transport/ZzSshTransport.h src/transport/ZzSshTransport.cpp tests/unit/tst_ZzSshTransport.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: SSH 传输适配器 ZzSshTransport（认证链回调注入、主机密钥确认、shell 通道转发）"
```

---

## 任务 9：安全对话框与连接流程胶水（端到端 mock）

规格 §七连接流程：双击会话 → 新建标签 → 工作线程连接 → 认证（密码经主密码解锁后从 ZzCredentialStore 取，缺主密码弹解锁框）→ 开 shell channel → 字节流双向接到 QTermWidget。规格 §八：主机密钥首次确认、变更警告为安全底线弹窗；其余错误不弹窗。

**文件：**
- 创建：`src/dialog/ZzHostKeyDialog.h`、`src/dialog/ZzHostKeyDialog.cpp`、`src/dialog/ZzMasterPasswordDialog.h`、`src/dialog/ZzMasterPasswordDialog.cpp`
- 创建：`tests/unit/tst_ZzConnectFlow.cpp`、`tests/perf/tst_PerfConnectFlow.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`、`tests/perf/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzConnectFlow.cpp`**

```cpp
#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "qtermwidget.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 端到端连接流程（mock 传输）：profile → 标签 → 连接 → 双向字节流（规格 §七/§九）。
 */
class tst_ZzConnectFlow : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void doubleClickToByteStream()
    {
        ZzTabManager tabs;
        ZzSessionProfile profile;
        profile.id = QStringLiteral("flow-1");
        profile.name = QStringLiteral("流程机");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("10.0.0.1");
        profile.user = QStringLiteral("deploy");

        // 等价于会话面板双击：面板发 connectRequested(profile) → openSession
        tabs.openSession(profile);

        auto *view = tabs.viewAt(0);
        QVERIFY(view != nullptr);
        QTRY_COMPARE(view->transportState(), ZzTransportInterface::State::Connected);

        auto *mock = static_cast<ZzMockTransport *>(view->transport());
        // 映射正确：host/user 进入 endpoint
        QCOMPARE(mock->lastEndpoint.host, QStringLiteral("10.0.0.1"));
        QCOMPARE(mock->lastEndpoint.user, QStringLiteral("deploy"));

        // 双向字节流：键盘输入抵达传输，远端输出不崩溃
        emit view->termWidget()->sendData("pwd\n", 4);
        QCoreApplication::processEvents();
        QCOMPARE(mock->writtenData, QByteArray("pwd\n"));
        mock->simulateData("/home/deploy\r\n");
        QCoreApplication::processEvents();
    }

    void connectFailureKeepsTabWithError()
    {
        ZzTabManager tabs;
        QSignalSpy msgSpy(&tabs, &ZzTabManager::statusMessage);
        ZzSessionProfile profile;
        profile.id = QStringLiteral("flow-2");
        profile.name = QStringLiteral("连不上");
        profile.protocol = QStringLiteral("mock");
        profile.host = QStringLiteral("fail"); // mock 约定：host==fail 触发失败

        tabs.openSession(profile);
        auto *view = tabs.viewAt(0);
        QVERIFY(view != nullptr);
        QTRY_COMPARE(view->transportState(),
                     ZzTransportInterface::State::Disconnected);
        // 连接失败保留标签（规格 §八），错误横幅由任务 13 断言
        QCOMPARE(tabs.count(), 1);
        QTRY_VERIFY(msgSpy.count() >= 1);
    }

    void masterPasswordUnlockLogic()
    {
        // 主密码解锁的纯逻辑部分（对话框本身依赖人工交互，不进自动化）
        const QString dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-cred-flow"));
        QDir().mkpath(dir);
        ZzCredentialStore store(dir + QStringLiteral("/credentials.dat"), this);
        QVERIFY(ZzMasterPasswordDialog::ensureStoreReady(&store,
                QStringLiteral("正确密码")));
        QVERIFY(store.isUnlocked());
        QVERIFY(!ZzMasterPasswordDialog::ensureStoreReady(&store, QString()));
    }
};

QTEST_MAIN(tst_ZzConnectFlow)
#include "tst_ZzConnectFlow.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzConnectFlow
```

预期：编译失败，报错 `dialog/ZzMasterPasswordDialog.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/dialog/ZzMasterPasswordDialog.h/.cpp`**

`ZzMasterPasswordDialog.h`：

```cpp
#pragma once

#include <QtWidgets/QDialog>

class QLineEdit;
class ZzCredentialStore;

/**
 * @brief 主密码解锁框（规格 §七：缺主密码弹解锁框；§6.2：首次启动设主密码）。
 *
 * 两种形态：凭据库尚无主密码时引导设置并确认两次；已有主密码时输入解锁。
 */
class ZzMasterPasswordDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ZzMasterPasswordDialog(ZzCredentialStore *store,
                                    QWidget *parent = nullptr);

    /**
     * @brief 确保凭据库已解锁：未解锁则弹窗（模态），成功或用户取消后返回。
     * @return true 表示已解锁可继续取凭据。
     */
    static bool ensureUnlocked(ZzCredentialStore *store, QWidget *parent = nullptr);

    /**
     * @brief 纯逻辑：设置（首次）或验证主密码。供自动化测试与对话框复用。
     * @param password 用户输入；空串视为取消，返回 false。
     * @return 操作是否成功。
     */
    static bool ensureStoreReady(ZzCredentialStore *store, const QString &password);

private:
    ZzCredentialStore *m_store;
    QLineEdit *m_passwordEdit;
    QLineEdit *m_confirmEdit;   ///< 仅首次设置时可见
};
```

`ZzMasterPasswordDialog.cpp`：

```cpp
#include "ZzMasterPasswordDialog.h"

#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>

#include "session/ZzCredentialStore.h"

ZzMasterPasswordDialog::ZzMasterPasswordDialog(ZzCredentialStore *store,
                                               QWidget *parent)
    : QDialog(parent)
    , m_store(store)
{
    const bool firstRun = !store->hasMasterPassword();
    setWindowTitle(firstRun ? QStringLiteral("设置主密码")
                            : QStringLiteral("解锁凭据库"));

    auto *layout = new QFormLayout(this);
    auto *hint = new QLabel(firstRun
        ? QStringLiteral("首次使用凭据存储，请设置主密码（AES-256-GCM 加密，规格 §6.2）：")
        : QStringLiteral("请输入主密码解锁凭据库："), this);
    layout->addRow(hint);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    layout->addRow(QStringLiteral("主密码："), m_passwordEdit);

    m_confirmEdit = new QLineEdit(this);
    m_confirmEdit->setEchoMode(QLineEdit::Password);
    if (firstRun) {
        layout->addRow(QStringLiteral("确认密码："), m_confirmEdit);
    } else {
        m_confirmEdit->hide();
    }

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, [this, firstRun]() {
        if (firstRun && m_passwordEdit->text() != m_confirmEdit->text()) {
            m_confirmEdit->clear();
            m_confirmEdit->setPlaceholderText(QStringLiteral("两次输入不一致"));
            return;
        }
        if (ensureStoreReady(m_store, m_passwordEdit->text())) {
            accept();
        } else {
            m_passwordEdit->clear();
            m_passwordEdit->setPlaceholderText(QStringLiteral("密码错误或为空"));
        }
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addRow(buttons);
}

bool ZzMasterPasswordDialog::ensureUnlocked(ZzCredentialStore *store,
                                            QWidget *parent)
{
    if (store->isUnlocked()) {
        return true;
    }
    ZzMasterPasswordDialog dialog(store, parent);
    return dialog.exec() == QDialog::Accepted;
}

bool ZzMasterPasswordDialog::ensureStoreReady(ZzCredentialStore *store,
                                              const QString &password)
{
    if (password.isEmpty()) {
        return false; // 用户取消
    }
    if (!store->hasMasterPassword()) {
        return store->setMasterPassword(password); // 首次设置即解锁
    }
    return store->unlock(password);
}
```

- [ ] **步骤 4：实现 `src/dialog/ZzHostKeyDialog.h/.cpp`**

`ZzHostKeyDialog.h`：

```cpp
#pragma once

#include <QtWidgets/QDialog>

/**
 * @brief 主机密钥确认对话框（规格 §八安全底线：首次确认、变更警告，不可省略）。
 *
 * 这是"不弹窗轰炸"原则的唯一例外——主机密钥变更可能意味着中间人攻击，
 * 必须显式打断用户确认。
 */
class ZzHostKeyDialog : public QDialog
{
    Q_OBJECT
public:
    /**
     * @brief 弹窗确认主机密钥。
     * @param host 主机标识（host:port）。
     * @param fingerprint 密钥指纹（SHA256 文本）。
     * @param changed true 表示与 known_hosts 中记录不一致（高危警告样式）。
     * @param parent 父窗口。
     * @return true 接受并写入 known_hosts.json；false 拒绝（连接中止）。
     */
    static bool confirm(const QString &host, const QString &fingerprint,
                        bool changed, QWidget *parent = nullptr);
};
```

`ZzHostKeyDialog.cpp`：

```cpp
#include "ZzHostKeyDialog.h"

#include <QtWidgets/QMessageBox>

bool ZzHostKeyDialog::confirm(const QString &host, const QString &fingerprint,
                              bool changed, QWidget *parent)
{
    if (changed) {
        // 密钥变更：高危警告，默认拒绝（规格 §八）
        const auto choice = QMessageBox::warning(parent,
            QStringLiteral("警告：主机密钥已变更"),
            QStringLiteral("主机 %1 的密钥指纹与本地记录不一致！\n\n"
                           "新指纹：%2\n\n"
                           "这可能意味着中间人攻击或服务器重装。"
                           "确认无误后才可继续。").arg(host, fingerprint),
            QMessageBox::Yes | QMessageBox::No,
            QMessageBox::No);
        return choice == QMessageBox::Yes;
    }
    // 首次连接：指纹确认
    const auto choice = QMessageBox::question(parent,
        QStringLiteral("确认主机密钥"),
        QStringLiteral("首次连接主机 %1。\n\n密钥指纹：%2\n\n"
                       "是否信任并保存该指纹？").arg(host, fingerprint),
        QMessageBox::Yes | QMessageBox::No,
        QMessageBox::No);
    return choice == QMessageBox::Yes;
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    dialog/ZzHostKeyDialog.h
    dialog/ZzHostKeyDialog.cpp
    dialog/ZzMasterPasswordDialog.h
    dialog/ZzMasterPasswordDialog.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzConnectFlow unit/tst_ZzConnectFlow.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzConnectFlow --output-on-failure
```

预期：PASS（3 个用例）。本任务起依赖计划 03（`session/ZzCredentialStore.h`、`session/ZzSessionProfile.h`）。

- [ ] **步骤 6：附带性能测试 `tests/perf/tst_PerfConnectFlow.cpp`（规格 §9.1）**

```cpp
#include <QtTest/QtTest>

#include "ZzMockTransport.h"
#include "ZzPerfRecorder.h"
#include "session/ZzSessionProfile.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 性能门控：开会话到连接就绪的端到端耗时（mock 传输）。阈值 500ms（Release）。
 * @note 真实 SSH 建连耗时由计划 01 的性能测试覆盖；此处门控的是装配层自身开销。
 */
class tst_PerfConnectFlow : public QObject
{
    Q_OBJECT
private slots:
    void initTestCase()
    {
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void openSessionToConnected()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        ZzTabManager tabs;
        ZzSessionProfile profile;
        profile.id = QStringLiteral("perf-flow");
        profile.name = QStringLiteral("性能流程");
        profile.protocol = QStringLiteral("mock");

        QElapsedTimer timer;
        timer.start();
        tabs.openSession(profile);
        auto *view = tabs.viewAt(0);
        QTRY_VERIFY_WITH_TIMEOUT(
            view->transportState() == ZzTransportInterface::State::Connected, 5000);
        const double elapsed = static_cast<double>(timer.elapsed());

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("connect-flow"),
            QStringLiteral("开会话到连接就绪（mock）"), 500.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 500ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfConnectFlow)
#include "tst_PerfConnectFlow.moc"
```

`tests/perf/CMakeLists.txt` 末尾追加：

```cmake
zz_add_perf_test(tst_PerfConnectFlow tst_PerfConnectFlow.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug && cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_PerfConnectFlow --output-on-failure
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R tst_PerfConnectFlow --output-on-failure
```

预期：Debug SKIP；Release PASS 并生成 `tests/perf/records/<今日>-connect-flow.json`。

- [ ] **步骤 7：Commit**

```bash
git add src/dialog tests/unit/tst_ZzConnectFlow.cpp tests/perf/tst_PerfConnectFlow.cpp tests/perf/records src/CMakeLists.txt tests/CMakeLists.txt tests/perf/CMakeLists.txt
git commit -m "feat: 主密码解锁框与主机密钥确认框，连接流程端到端 mock 测试（含性能门控）"
```

---

## 任务 10：会话面板（ZzSessionPanel + ZzSessionEditDialog）

规格 §七：QDockWidget，实现 ZzPanelInterface；树形分组视图，双击连接，右键新建/编辑/删除/复制会话；可折叠、可停靠左右。依赖计划 03 的 ZzSessionModel/ZzCredentialStore。

**文件：**
- 创建：`src/panel/ZzSessionPanel.h`、`src/panel/ZzSessionPanel.cpp`、`src/panel/ZzSessionEditDialog.h`、`src/panel/ZzSessionEditDialog.cpp`
- 创建：`tests/unit/tst_ZzSessionPanel.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSessionPanel.cpp`**

```cpp
#include <QtTest/QtTest>

#include <QtWidgets/QTreeView>

#include "panel/ZzSessionPanel.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionModel.h"

/**
 * @brief 验证会话面板：树形分组、双击发连接请求、增删改后树刷新（规格 §七）。
 */
class tst_ZzSessionPanel : public QObject
{
    Q_OBJECT
private:
    QString m_dir;

    /** @brief 造一条会话记录并加入模型。 */
    static ZzSessionProfile makeProfile(const QString &id, const QString &name,
                                        const QString &groupPath)
    {
        ZzSessionProfile profile;
        profile.id = id;
        profile.name = name;
        profile.groupPath = groupPath;
        profile.protocol = QStringLiteral("ssh");
        profile.host = QStringLiteral("example.com");
        profile.user = QStringLiteral("root");
        return profile;
    }

private slots:
    void init()
    {
        qRegisterMetaType<ZzSessionProfile>(); // connectRequested 信号参数
        m_dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-panel-test"));
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
    }

    void treeGroupsByPath()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        model.addProfile(makeProfile(QStringLiteral("a"), QStringLiteral("Web1"),
                                     QStringLiteral("生产环境/Web 服务器")));
        model.addProfile(makeProfile(QStringLiteral("b"), QStringLiteral("DB1"),
                                     QStringLiteral("生产环境/数据库")));
        model.addProfile(makeProfile(QStringLiteral("c"), QStringLiteral("本机"),
                                     QString()));

        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        QCOMPARE(panel.panelId(), QStringLiteral("sessions"));
        QCOMPARE(panel.panelWidget(), static_cast<QWidget *>(&panel));

        // 树：生产环境（含 Web 服务器、数据库两个子组）+ 未分组"本机"
        QCOMPARE(panel.visibleGroupCount(), 1);          // 顶层分组数
        QCOMPARE(panel.visibleSessionCount(), 3);        // 会话叶子总数
    }

    void doubleClickEmitsConnectRequest()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        model.addProfile(makeProfile(QStringLiteral("a"), QStringLiteral("Web1"),
                                     QStringLiteral("生产环境")));
        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        QSignalSpy spy(&panel, &ZzSessionPanel::connectRequested);

        panel.triggerConnect(QStringLiteral("a")); // 等价于双击该会话项
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.first().at(0).value<ZzSessionProfile>().id,
                 QStringLiteral("a"));

        // 双击分组项不触发
        panel.triggerConnect(QString()); // 分组项无 profile id
        QCOMPARE(spy.count(), 1);
    }

    void modelChangeRebuildsTree()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        QCOMPARE(panel.visibleSessionCount(), 0);

        model.addProfile(makeProfile(QStringLiteral("a"), QStringLiteral("Web1"),
                                     QString()));
        QCOMPARE(panel.visibleSessionCount(), 1);

        model.removeProfile(QStringLiteral("a"));
        QCOMPARE(panel.visibleSessionCount(), 0);
    }

    void deleteViaActionRemovesFromModel()
    {
        ZzSessionModel model(m_dir + QStringLiteral("/sessions.json"));
        model.addProfile(makeProfile(QStringLiteral("a"), QStringLiteral("Web1"),
                                     QString()));
        ZzCredentialStore store(m_dir + QStringLiteral("/credentials.dat"));
        ZzSessionPanel panel(&model, &store);
        panel.triggerDelete(QStringLiteral("a")); // 等价于右键→删除
        QVERIFY(model.profileById(QStringLiteral("a")).id.isEmpty());
        QCOMPARE(panel.visibleSessionCount(), 0);
    }
};

QTEST_MAIN(tst_ZzSessionPanel)
#include "tst_ZzSessionPanel.moc"
```

注意：`ZzSessionProfile` 作为信号参数必须是元类型——计划 03 应在 `ZzSessionProfile.h` 中提供 `Q_DECLARE_METATYPE(ZzSessionProfile)`；若计划 03 遗漏，先补该声明再跑。

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzSessionPanel
```

预期：编译失败，报错 `panel/ZzSessionPanel.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/panel/ZzSessionEditDialog.h/.cpp`**

`ZzSessionEditDialog.h`：

```cpp
#pragma once

#include <QtWidgets/QDialog>

#include "session/ZzSessionProfile.h"

class QComboBox;
class QLineEdit;
class QSpinBox;
class QStackedWidget;
class ZzCredentialStore;

/**
 * @brief 会话新建/编辑对话框（规格 §七右键菜单的载体）。
 *
 * 表单字段与 ZzSessionProfile 一一对应；密码不存 profile，
 * 经 ZzCredentialStore 加密存储、profile 只留 credentialId 引用（规格 §6.2）。
 */
class ZzSessionEditDialog : public QDialog
{
    Q_OBJECT
public:
    /**
     * @brief 构造新建或编辑对话框。
     * @param store 凭据库（authMethod=="password" 时写入/保留密码引用）。
     * @param profile 编辑时传入已有 profile；新建传默认构造值。
     * @param groupPathPrefix 新建时预选的分组路径（在分组项上右键新建）。
     */
    explicit ZzSessionEditDialog(ZzCredentialStore *store,
                                 ZzSessionProfile profile = {},
                                 const QString &groupPathPrefix = {},
                                 QWidget *parent = nullptr);

    /** @brief 表单当前内容（accept 后由调用方写入 ZzSessionModel）。 */
    [[nodiscard]] ZzSessionProfile profile() const;

protected:
    void accept() override;

private:
    ZzCredentialStore *m_store;
    ZzSessionProfile m_profile;        ///< 编辑中的工作副本
    QString m_originalCredentialId;    ///< 原密码引用（未改密码时保留）

    QLineEdit *m_nameEdit;
    QLineEdit *m_groupEdit;
    QComboBox *m_protocolCombo;
    QStackedWidget *m_hostStack;       ///< ssh 页 / local 页
    QLineEdit *m_hostEdit;
    QSpinBox *m_portSpin;
    QLineEdit *m_shellEdit;            ///< local 会话的 shell 路径
    QLineEdit *m_userEdit;
    QComboBox *m_authCombo;            ///< agent / key / password
    QLineEdit *m_keyPathEdit;
    QLineEdit *m_passwordEdit;         ///< 仅输入新密码；留空=保留原引用
};
```

`ZzSessionEditDialog.cpp`：

```cpp
#include "ZzSessionEditDialog.h"

#include <utility>

#include <QtWidgets/QComboBox>
#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QSpinBox>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QWidget>

#include "session/ZzCredentialStore.h"

ZzSessionEditDialog::ZzSessionEditDialog(ZzCredentialStore *store,
                                         ZzSessionProfile profile,
                                         const QString &groupPathPrefix,
                                         QWidget *parent)
    : QDialog(parent)
    , m_store(store)
    , m_profile(std::move(profile))
    , m_originalCredentialId(m_profile.credentialId)
{
    const bool isNew = m_profile.id.isEmpty();
    setWindowTitle(isNew ? QStringLiteral("新建会话") : QStringLiteral("编辑会话"));
    if (isNew) {
        m_profile.protocol = QStringLiteral("ssh");
        m_profile.groupPath = groupPathPrefix;
    }

    auto *layout = new QFormLayout(this);

    m_nameEdit = new QLineEdit(m_profile.name, this);
    layout->addRow(QStringLiteral("名称："), m_nameEdit);

    m_groupEdit = new QLineEdit(m_profile.groupPath, this);
    m_groupEdit->setPlaceholderText(QStringLiteral("如：生产环境/Web 服务器"));
    layout->addRow(QStringLiteral("分组路径："), m_groupEdit);

    m_protocolCombo = new QComboBox(this);
    m_protocolCombo->addItem(QStringLiteral("SSH"), QStringLiteral("ssh"));
    m_protocolCombo->addItem(QStringLiteral("本地 Shell"), QStringLiteral("local"));
    m_protocolCombo->setCurrentIndex(
        m_profile.protocol == QStringLiteral("local") ? 1 : 0);
    layout->addRow(QStringLiteral("协议："), m_protocolCombo);

    // SSH 与本地 Shell 两套字段切换
    m_hostStack = new QStackedWidget(this);
    auto *sshPage = new QWidget(this);
    auto *sshForm = new QFormLayout(sshPage);
    m_hostEdit = new QLineEdit(
        m_profile.protocol == QStringLiteral("local") ? QString() : m_profile.host,
        sshPage);
    m_portSpin = new QSpinBox(sshPage);
    m_portSpin->setRange(1, 65535);
    m_portSpin->setValue(m_profile.port == 0 ? 22 : m_profile.port);
    sshForm->addRow(QStringLiteral("主机："), m_hostEdit);
    sshForm->addRow(QStringLiteral("端口："), m_portSpin);
    auto *localPage = new QWidget(this);
    auto *localForm = new QFormLayout(localPage);
    m_shellEdit = new QLineEdit(
        m_profile.protocol == QStringLiteral("local") ? m_profile.host : QString(),
        localPage);
    m_shellEdit->setPlaceholderText(QStringLiteral("留空使用系统默认 shell"));
    localForm->addRow(QStringLiteral("Shell 程序："), m_shellEdit);
    m_hostStack->addWidget(sshPage);
    m_hostStack->addWidget(localPage);
    m_hostStack->setCurrentIndex(m_protocolCombo->currentIndex());
    layout->addRow(m_hostStack);
    connect(m_protocolCombo, &QComboBox::currentIndexChanged,
            m_hostStack, &QStackedWidget::setCurrentIndex);

    m_userEdit = new QLineEdit(m_profile.user, this);
    layout->addRow(QStringLiteral("用户名："), m_userEdit);

    m_authCombo = new QComboBox(this);
    m_authCombo->addItem(QStringLiteral("SSH Agent"), QStringLiteral("agent"));
    m_authCombo->addItem(QStringLiteral("公钥文件"), QStringLiteral("key"));
    m_authCombo->addItem(QStringLiteral("密码"), QStringLiteral("password"));
    const int authIndex = m_authCombo->findData(m_profile.authMethod);
    m_authCombo->setCurrentIndex(authIndex >= 0 ? authIndex : 0);
    layout->addRow(QStringLiteral("认证方式："), m_authCombo);

    m_keyPathEdit = new QLineEdit(m_profile.keyPath, this);
    m_keyPathEdit->setPlaceholderText(QStringLiteral("私钥路径（公钥认证）"));
    layout->addRow(QStringLiteral("私钥路径："), m_keyPathEdit);

    m_passwordEdit = new QLineEdit(this);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_passwordEdit->setPlaceholderText(
        m_originalCredentialId.isEmpty()
            ? QStringLiteral("登录密码")
            : QStringLiteral("留空保留已保存的密码"));
    layout->addRow(QStringLiteral("密码："), m_passwordEdit);

    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &ZzSessionEditDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addRow(buttons);
}

ZzSessionProfile ZzSessionEditDialog::profile() const
{
    return m_profile;
}

void ZzSessionEditDialog::accept()
{
    const bool isLocal =
        m_protocolCombo->currentData().toString() == QStringLiteral("local");
    if (!isLocal && m_hostEdit->text().trimmed().isEmpty()) {
        m_hostEdit->setPlaceholderText(QStringLiteral("主机不能为空"));
        return;
    }
    if (m_nameEdit->text().trimmed().isEmpty()) {
        m_nameEdit->setPlaceholderText(QStringLiteral("名称不能为空"));
        return;
    }

    m_profile.name = m_nameEdit->text().trimmed();
    m_profile.groupPath = m_groupEdit->text().trimmed();
    m_profile.protocol = m_protocolCombo->currentData().toString();
    // 契约约定：local 会话的 shell 程序路径存于 host 字段
    m_profile.host = isLocal ? m_shellEdit->text().trimmed()
                             : m_hostEdit->text().trimmed();
    m_profile.port = static_cast<quint16>(m_portSpin->value());
    m_profile.user = m_userEdit->text().trimmed();
    m_profile.authMethod = m_authCombo->currentData().toString();
    m_profile.keyPath = m_keyPathEdit->text().trimmed();

    // 密码：输入了新密码则写入凭据库换新引用；留空保留原引用
    if (m_profile.authMethod == QStringLiteral("password")) {
        if (!m_passwordEdit->text().isEmpty()) {
            m_profile.credentialId = m_store->addCredential(m_passwordEdit->text());
        } else {
            m_profile.credentialId = m_originalCredentialId;
        }
    } else {
        m_profile.credentialId.clear();
    }

    QDialog::accept();
}
```

- [ ] **步骤 4：实现 `src/panel/ZzSessionPanel.h`**

```cpp
#pragma once

#include <QtWidgets/QDockWidget>

#include "ZzPanelInterface.h"
#include "session/ZzSessionProfile.h"

class QStandardItemModel;
class QTreeView;
class ZzCredentialStore;
class ZzSessionModel;

/**
 * @brief 会话面板：树形分组、双击连接、右键新建/编辑/删除/复制（规格 §七）。
 *
 * 实现 ZzPanelInterface 注册进壳层；数据完全来自 ZzSessionModel，
 * 模型 changed() 即重建树（v0.1 会话量级下重建成本可忽略）。
 */
class ZzSessionPanel : public QDockWidget, public ZzPanelInterface
{
    Q_OBJECT
public:
    explicit ZzSessionPanel(ZzSessionModel *model,
                            ZzCredentialStore *store,
                            QWidget *parent = nullptr);

    // ---- ZzPanelInterface ----
    [[nodiscard]] QString panelId() const override;
    [[nodiscard]] QString panelTitle() const override;
    [[nodiscard]] QWidget *panelWidget() override;

    // ---- 测试观察口（等价于 UI 操作，离屏环境不用模拟鼠标） ----
    /** @brief 触发连接（等价双击会话项）；id 为空等价双击分组项。 */
    void triggerConnect(const QString &profileId);
    /** @brief 触发删除（等价右键→删除）。 */
    void triggerDelete(const QString &profileId);
    /** @brief 顶层分组数（可见树）。 */
    [[nodiscard]] int visibleGroupCount() const;
    /** @brief 会话叶子总数（可见树）。 */
    [[nodiscard]] int visibleSessionCount() const;

signals:
    /** @brief 双击会话请求连接（规格 §七连接流程起点）。 */
    void connectRequested(const ZzSessionProfile &profile);

private slots:
    void rebuildTree();
    void onTreeDoubleClicked(const QModelIndex &index);
    void showContextMenu(const QPoint &pos);
    void newSession(const QString &groupPathPrefix);
    void editSession(const QString &profileId);
    void duplicateSession(const QString &profileId);

private:
    ZzSessionModel *m_model;
    ZzCredentialStore *m_store;
    QTreeView *m_tree;
    QStandardItemModel *m_treeModel;
};
```

- [ ] **步骤 5：实现 `src/panel/ZzSessionPanel.cpp`**

```cpp
#include "ZzSessionPanel.h"

#include <functional>

#include <QtCore/QUuid>
#include <QtGui/QStandardItemModel>
#include <QtWidgets/QMenu>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>

#include "ZzSessionEditDialog.h"
#include "session/ZzSessionModel.h"

namespace {

/** @brief 树节点角色键：会话 profile id（分组项无此数据）。 */
constexpr int kProfileIdRole = Qt::UserRole + 1;

} // namespace

ZzSessionPanel::ZzSessionPanel(ZzSessionModel *model,
                               ZzCredentialStore *store,
                               QWidget *parent)
    : QDockWidget(QStringLiteral("会话"), parent)
    , m_model(model)
    , m_store(store)
{
    setObjectName(panelId()); // QDockWidget 布局持久化键
    setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea); // 可停靠左右
    setFeatures(QDockWidget::DockWidgetClosable
                | QDockWidget::DockWidgetMovable); // 可折叠/拖动

    m_treeModel = new QStandardItemModel(this);
    m_tree = new QTreeView(this);
    m_tree->setModel(m_treeModel);
    m_tree->setHeaderHidden(true);
    m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
    setWidget(m_tree);

    connect(m_tree, &QTreeView::doubleClicked,
            this, &ZzSessionPanel::onTreeDoubleClicked);
    connect(m_tree, &QTreeView::customContextMenuRequested,
            this, &ZzSessionPanel::showContextMenu);
    connect(m_model, &ZzSessionModel::changed,
            this, &ZzSessionPanel::rebuildTree);
    rebuildTree();
}

QString ZzSessionPanel::panelId() const
{
    return QStringLiteral("sessions");
}

QString ZzSessionPanel::panelTitle() const
{
    return QStringLiteral("会话");
}

QWidget *ZzSessionPanel::panelWidget()
{
    return this;
}

void ZzSessionPanel::triggerConnect(const QString &profileId)
{
    if (profileId.isEmpty()) {
        return; // 分组项不触发
    }
    const ZzSessionProfile profile = m_model->profileById(profileId);
    if (!profile.id.isEmpty()) {
        emit connectRequested(profile);
    }
}

void ZzSessionPanel::triggerDelete(const QString &profileId)
{
    m_model->removeProfile(profileId);
}

int ZzSessionPanel::visibleGroupCount() const
{
    // 只统计不带 profile id 的顶层项（未分组会话与会话同层，不算分组）
    int count = 0;
    for (int row = 0; row < m_treeModel->rowCount(); ++row) {
        if (!m_treeModel->item(row)->data(kProfileIdRole).isValid()) {
            ++count;
        }
    }
    return count;
}

int ZzSessionPanel::visibleSessionCount() const
{
    int count = 0;
    // 递归统计带 profile id 的叶子项
    std::function<void(QStandardItem *)> walk = [&](QStandardItem *item) {
        for (int row = 0; row < item->rowCount(); ++row) {
            QStandardItem *child = item->child(row);
            if (child->data(kProfileIdRole).isValid()) {
                ++count;
            } else {
                walk(child);
            }
        }
    };
    walk(m_treeModel->invisibleRootItem());
    return count;
}

void ZzSessionPanel::rebuildTree()
{
    m_treeModel->clear();
    // 分组即路径字符串（规格 §6.1）：按 "/" 拆层建组，重命名分组=改前缀
    QHash<QString, QStandardItem *> groupItems;
    for (const ZzSessionProfile &profile : m_model->profiles()) {
        QStandardItem *parentItem = m_treeModel->invisibleRootItem();
        QString accumulated;
        const QStringList segments = profile.groupPath.split(
            QLatin1Char('/'), Qt::SkipEmptyParts);
        for (const QString &segment : segments) {
            accumulated = accumulated.isEmpty()
                ? segment : accumulated + QLatin1Char('/') + segment;
            QStandardItem *&group = groupItems[accumulated];
            if (!group) {
                group = new QStandardItem(segment);
                group->setEditable(false);
                parentItem->appendRow(group);
            }
            parentItem = group;
        }
        auto *sessionItem = new QStandardItem(profile.name);
        sessionItem->setEditable(false);
        sessionItem->setData(profile.id, kProfileIdRole);
        sessionItem->setToolTip(profile.protocol == QStringLiteral("local")
            ? QStringLiteral("本地 Shell")
            : QStringLiteral("%1@%2:%3").arg(profile.user, profile.host)
                  .arg(profile.port));
        parentItem->appendRow(sessionItem);
    }
    m_tree->expandAll();
}

void ZzSessionPanel::onTreeDoubleClicked(const QModelIndex &index)
{
    triggerConnect(index.data(kProfileIdRole).toString());
}

void ZzSessionPanel::showContextMenu(const QPoint &pos)
{
    const QModelIndex index = m_tree->indexAt(pos);
    QMenu menu(this);
    if (!index.isValid()) {
        // 空白区：新建会话
        QAction *newAction = menu.addAction(QStringLiteral("新建会话"));
        if (menu.exec(m_tree->viewport()->mapToGlobal(pos)) == newAction) {
            newSession(QString());
        }
        return;
    }
    const QString profileId = index.data(kProfileIdRole).toString();
    if (profileId.isEmpty()) {
        // 分组项：在此分组下新建
        QAction *newAction =
            menu.addAction(QStringLiteral("在此分组新建会话"));
        if (menu.exec(m_tree->viewport()->mapToGlobal(pos)) == newAction) {
            // 分组的完整路径 = 逐层标题拼接
            QStringList segments;
            for (QModelIndex it = index; it.isValid(); it = it.parent()) {
                segments.prepend(it.data().toString());
            }
            newSession(segments.join(QLatin1Char('/')));
        }
        return;
    }
    // 会话项：新建/编辑/删除/复制（规格 §七）
    QAction *newAction = menu.addAction(QStringLiteral("新建会话"));
    QAction *editAction = menu.addAction(QStringLiteral("编辑"));
    QAction *deleteAction = menu.addAction(QStringLiteral("删除"));
    QAction *duplicateAction = menu.addAction(QStringLiteral("复制"));
    QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(pos));
    if (chosen == newAction) {
        newSession(QString());
    } else if (chosen == editAction) {
        editSession(profileId);
    } else if (chosen == deleteAction) {
        triggerDelete(profileId);
    } else if (chosen == duplicateAction) {
        duplicateSession(profileId);
    }
}

void ZzSessionPanel::newSession(const QString &groupPathPrefix)
{
    ZzSessionEditDialog dialog(m_store, {}, groupPathPrefix, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    ZzSessionProfile profile = dialog.profile();
    profile.id = QUuid::createUuid().toString(QUuid::WithoutBraces);
    m_model->addProfile(profile);
}

void ZzSessionPanel::editSession(const QString &profileId)
{
    const ZzSessionProfile existing = m_model->profileById(profileId);
    if (existing.id.isEmpty()) {
        return;
    }
    ZzSessionEditDialog dialog(m_store, existing, QString(), this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    m_model->updateProfile(dialog.profile());
}

void ZzSessionPanel::duplicateSession(const QString &profileId)
{
    ZzSessionProfile copy = m_model->profileById(profileId);
    if (copy.id.isEmpty()) {
        return;
    }
    copy.id = QUuid::createUuid().toString(QUuid::WithoutBraces);
    copy.name = copy.name + QStringLiteral("（副本）");
    // 密码引用随副本共享同一条凭据记录，语义正确（同一台机器同一个密码）
    m_model->addProfile(copy);
}
```

- [ ] **步骤 6：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    panel/ZzSessionPanel.h
    panel/ZzSessionPanel.cpp
    panel/ZzSessionEditDialog.h
    panel/ZzSessionEditDialog.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzSessionPanel unit/tst_ZzSessionPanel.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzSessionPanel --output-on-failure
```

预期：PASS（4 个用例）。

- [ ] **步骤 7：Commit**

```bash
git add src/panel tests/unit/tst_ZzSessionPanel.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 会话面板 ZzSessionPanel（树形分组/双击连接/右键增删改复制）与编辑对话框"
```

---

## 任务 11：设置页（ZzSettingsPage）

规格 §七：设置页 v0.1 仅全局：默认终端类型、编码、字号、配色。即改即存（写 ZzAppSettings），已打开标签经 settingsChanged 实时应用（装配在任务 14）。

**文件：**
- 创建：`src/settings/ZzSettingsPage.h`、`src/settings/ZzSettingsPage.cpp`
- 创建：`tests/unit/tst_ZzSettingsPage.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzSettingsPage.cpp`**

```cpp
#include <QtTest/QtTest>

#include <QtWidgets/QComboBox>
#include <QtWidgets/QSpinBox>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzSettingsPage.h"

/**
 * @brief 验证设置页：表单回显当前值、修改即写存储、配色选项来自 QTermWidget。
 */
class tst_ZzSettingsPage : public QObject
{
    Q_OBJECT
private:
    QString m_path;

private slots:
    void init()
    {
        m_path = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-settingspage-test.ini"));
        QFile::remove(m_path);
    }

    void reflectsCurrentSettings()
    {
        ZzAppSettings settings(m_path);
        settings.setFontSize(20);
        settings.setEncoding(QStringLiteral("GBK"));

        ZzSettingsPage page(&settings);
        QCOMPARE(page.fontSizeSpin()->value(), 20);
        QCOMPARE(page.encodingCombo()->currentText(), QStringLiteral("GBK"));
    }

    void editingWritesThrough()
    {
        ZzAppSettings settings(m_path);
        ZzSettingsPage page(&settings);
        QSignalSpy spy(&settings, &ZzAppSettings::settingsChanged);

        page.fontSizeSpin()->setValue(14);
        page.encodingCombo()->setCurrentText(QStringLiteral("Big5"));
        page.terminalTypeCombo()->setCurrentText(QStringLiteral("vt100"));

        QCOMPARE(settings.fontSize(), 14);
        QCOMPARE(settings.encoding(), QStringLiteral("Big5"));
        QCOMPARE(settings.terminalType(), QStringLiteral("vt100"));
        QVERIFY(spy.count() >= 3);
    }

    void colorSchemesComeFromTerminal()
    {
        ZzAppSettings settings(m_path);
        ZzSettingsPage page(&settings);
        QCOMPARE(page.colorSchemeCombo()->count(),
                 QTermWidget::availableColorSchemes().count());
        QVERIFY(page.colorSchemeCombo()->count() > 0);
    }
};

QTEST_MAIN(tst_ZzSettingsPage)
#include "tst_ZzSettingsPage.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzSettingsPage
```

预期：编译失败，报错 `settings/ZzSettingsPage.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/settings/ZzSettingsPage.h`**

```cpp
#pragma once

#include <QtWidgets/QWidget>

class QComboBox;
class QSpinBox;
class ZzAppSettings;

/**
 * @brief 全局设置页（规格 §七）：终端类型、编码、字号、配色、内存历史行数。
 *
 * 即改即存：控件变更直接写 ZzAppSettings 并触发 settingsChanged，
 * 不需要"保存"按钮。作为框架页面挂在导航 Footer（任务 14 装配）。
 */
class ZzSettingsPage : public QWidget
{
    Q_OBJECT
public:
    explicit ZzSettingsPage(ZzAppSettings *settings, QWidget *parent = nullptr);

    // ---- 测试观察口 ----
    [[nodiscard]] QComboBox *terminalTypeCombo() const;
    [[nodiscard]] QComboBox *encodingCombo() const;
    [[nodiscard]] QSpinBox *fontSizeSpin() const;
    [[nodiscard]] QComboBox *colorSchemeCombo() const;
    [[nodiscard]] QSpinBox *historyLinesSpin() const;

private:
    ZzAppSettings *m_settings;
    QComboBox *m_terminalTypeCombo;
    QComboBox *m_encodingCombo;
    QSpinBox *m_fontSizeSpin;
    QComboBox *m_colorSchemeCombo;
    QSpinBox *m_historyLinesSpin;
};
```

- [ ] **步骤 4：实现 `src/settings/ZzSettingsPage.cpp`**

```cpp
#include "ZzSettingsPage.h"

#include <QtWidgets/QComboBox>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QSpinBox>

#include "qtermwidget.h"
#include "settings/ZzAppSettings.h"

ZzSettingsPage::ZzSettingsPage(ZzAppSettings *settings, QWidget *parent)
    : QWidget(parent)
    , m_settings(settings)
{
    auto *layout = new QFormLayout(this);

    m_terminalTypeCombo = new QComboBox(this);
    m_terminalTypeCombo->setEditable(true);
    m_terminalTypeCombo->addItems({
        QStringLiteral("xterm-256color"), QStringLiteral("xterm"),
        QStringLiteral("vt100"), QStringLiteral("linux"),
    });
    m_terminalTypeCombo->setCurrentText(m_settings->terminalType());
    layout->addRow(QStringLiteral("终端类型："), m_terminalTypeCombo);

    m_encodingCombo = new QComboBox(this);
    m_encodingCombo->addItems({
        QStringLiteral("UTF-8"), QStringLiteral("GBK"),
        QStringLiteral("GB18030"), QStringLiteral("Big5"),
        QStringLiteral("Shift-JIS"), QStringLiteral("EUC-KR"),
    });
    m_encodingCombo->setCurrentText(m_settings->encoding());
    layout->addRow(QStringLiteral("默认编码："), m_encodingCombo);

    m_fontSizeSpin = new QSpinBox(this);
    m_fontSizeSpin->setRange(6, 32);
    m_fontSizeSpin->setSuffix(QStringLiteral(" pt"));
    m_fontSizeSpin->setValue(m_settings->fontSize());
    layout->addRow(QStringLiteral("字号："), m_fontSizeSpin);

    m_colorSchemeCombo = new QComboBox(this);
    m_colorSchemeCombo->addItems(QTermWidget::availableColorSchemes());
    m_colorSchemeCombo->setCurrentText(m_settings->colorScheme());
    layout->addRow(QStringLiteral("配色方案："), m_colorSchemeCombo);

    m_historyLinesSpin = new QSpinBox(this);
    m_historyLinesSpin->setRange(1000, 100000);
    m_historyLinesSpin->setSingleStep(1000);
    m_historyLinesSpin->setValue(m_settings->historyLines());
    layout->addRow(QStringLiteral("内存历史行数："), m_historyLinesSpin);

    auto *note = new QLabel(
        QStringLiteral("改动立即生效：新标签使用新值，已打开标签实时应用字号/配色/编码。"),
        this);
    note->setWordWrap(true);
    layout->addRow(note);

    // 即改即存
    connect(m_terminalTypeCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setTerminalType);
    connect(m_encodingCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setEncoding);
    connect(m_fontSizeSpin, &QSpinBox::valueChanged,
            m_settings, &ZzAppSettings::setFontSize);
    connect(m_colorSchemeCombo, &QComboBox::currentTextChanged,
            m_settings, &ZzAppSettings::setColorScheme);
    connect(m_historyLinesSpin, &QSpinBox::valueChanged,
            m_settings, &ZzAppSettings::setHistoryLines);
}

QComboBox *ZzSettingsPage::terminalTypeCombo() const { return m_terminalTypeCombo; }
QComboBox *ZzSettingsPage::encodingCombo() const { return m_encodingCombo; }
QSpinBox *ZzSettingsPage::fontSizeSpin() const { return m_fontSizeSpin; }
QComboBox *ZzSettingsPage::colorSchemeCombo() const { return m_colorSchemeCombo; }
QSpinBox *ZzSettingsPage::historyLinesSpin() const { return m_historyLinesSpin; }
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    settings/ZzSettingsPage.h
    settings/ZzSettingsPage.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzSettingsPage unit/tst_ZzSettingsPage.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzSettingsPage --output-on-failure
```

预期：PASS（3 个用例）。

- [ ] **步骤 6：Commit**

```bash
git add src/settings/ZzSettingsPage.h src/settings/ZzSettingsPage.cpp tests/unit/tst_ZzSettingsPage.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 全局设置页 ZzSettingsPage（即改即存，配色取自 QTermWidget）"
```

---

## 任务 12：滚动历史桥（ZzScrollbackBridge：ZzTermWidget ↔ ZzLogEngine）

规格 §5.4：ZzTermWidget 滚动出的行追加到 ZzLogEngine 热层；向上滚动超出内存历史时从 ZzLogEngine 读回。规格 §八：日志引擎 I/O 失败降级为纯内存模式并提示用户。依赖计划 02 的 ZzLogEngine。

> **前置依赖（计划 05）：** 追加路径用 QTermWidget 现成信号 `dupDisplayOutput`（逐行 UTF-8 输出）即可落地；读回路径把行重新注入 QTermWidget 显示层依赖**计划 05 提供的 ZzTermWidget 新 API（预计形态 `setHistoryProvider`/等价机制）**。本任务步骤 5 的读回接线必须在计划 05 完成后进行；步骤 1~4（追加、读回透传、降级）不依赖计划 05，可先行验证。

**文件：**
- 创建：`src/terminal/ZzScrollbackBridge.h`、`src/terminal/ZzScrollbackBridge.cpp`
- 创建：`tests/unit/tst_ZzScrollbackBridge.cpp`、`tests/perf/tst_PerfScrollback.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`、`tests/perf/CMakeLists.txt`、`src/terminal/ZzTerminalView.h/.cpp`（新增 `enableScrollback`）

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzScrollbackBridge.cpp`**

```cpp
#include <QtTest/QtTest>

#include "qtermwidget.h"
#include "log/ZzLogEngine.h"
#include "terminal/ZzScrollbackBridge.h"

/**
 * @brief 验证滚动历史桥：输出逐行进引擎、读回透传、降级信号透传（规格 §5.4/§八）。
 */
class tst_ZzScrollbackBridge : public QObject
{
    Q_OBJECT
private slots:
    void appendsLinesToEngine()
    {
        QTermWidget term;
        ZzLogEngine engine(QStringLiteral("bridge-test-append"));
        ZzScrollbackBridge bridge(&term, &engine);

        // dupDisplayOutput 按行吐 UTF-8 文本
        emit term.dupDisplayOutput("total 0\n", 8);
        emit term.dupDisplayOutput("drwxr-xr-x  2 root root 4096\n", 28);
        QCoreApplication::processEvents();

        QCOMPARE(engine.totalLines(), 2);
        QCOMPARE(engine.readBack(2, 10),
                 QStringList({QStringLiteral("total 0"),
                              QStringLiteral("drwxr-xr-x  2 root root 4096")}));
    }

    void readBackDelegates()
    {
        QTermWidget term;
        ZzLogEngine engine(QStringLiteral("bridge-test-read"));
        engine.appendLines({QStringLiteral("line-1"), QStringLiteral("line-2"),
                            QStringLiteral("line-3")});
        ZzScrollbackBridge bridge(&term, &engine);
        QCOMPARE(bridge.readOlderLines(3, 2),
                 QStringList({QStringLiteral("line-1"), QStringLiteral("line-2")}));
    }

    void degradationForwards()
    {
        QTermWidget term;
        ZzLogEngine engine(QStringLiteral("bridge-test-degrade"));
        ZzScrollbackBridge bridge(&term, &engine);
        QSignalSpy spy(&bridge, &ZzScrollbackBridge::degraded);
        bridge.simulateDegradationForTest(QStringLiteral("磁盘空间不足"));
        QCOMPARE(spy.count(), 1);
        QVERIFY(spy.first().at(0).toString().contains(QStringLiteral("磁盘")));
    }
};

QTEST_MAIN(tst_ZzScrollbackBridge)
#include "tst_ZzScrollbackBridge.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzScrollbackBridge
```

预期：编译失败，报错 `terminal/ZzScrollbackBridge.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/terminal/ZzScrollbackBridge.h`**

```cpp
#pragma once

#include <QtCore/QObject>
#include <QtCore/QStringList>

class QTermWidget;
class ZzLogEngine;

/**
 * @brief 滚动历史桥（规格 §5.4）：QTermWidget 滚出的行进 ZzLogEngine，
 *        向上滚动超出内存历史时从引擎读回。
 *
 * 追加路径：监听 QTermWidget::dupDisplayOutput（逐行 UTF-8）→ appendLines。
 * 读回路径：readOlderLines 委托引擎 readBack；构造函数在计划 05 交付的
 * QTermWidget::setHistoryProvider（或等价机制）可用时完成注入接线（步骤 5）。
 * 降级路径：引擎 degradedToMemoryOnly → degraded 信号 → 状态栏提示（规格 §八）。
 */
class ZzScrollbackBridge : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 绑定终端与日志引擎（均不拥有，调用方保证存活期覆盖本桥）。
     */
    ZzScrollbackBridge(QTermWidget *term, ZzLogEngine *engine,
                       QObject *parent = nullptr);

    /**
     * @brief 读回 beforeLine 之前至多 maxLines 行（供滚动加载与测试）。
     */
    [[nodiscard]] QStringList readOlderLines(qint64 beforeLine, int maxLines) const;

    /** @brief 引擎当前总行数。 */
    [[nodiscard]] qint64 totalLines() const;

    /** @brief 测试辅助：直接转发一次降级事件（等价引擎 I/O 失败）。 */
    void simulateDegradationForTest(const QString &reason);

signals:
    /** @brief 引擎降级为纯内存模式（规格 §八：提示用户，不影响终端交互）。 */
    void degraded(const QString &reason);

private:
    QTermWidget *m_term;
    ZzLogEngine *m_engine;
};
```

- [ ] **步骤 4：实现 `src/terminal/ZzScrollbackBridge.cpp`**

```cpp
#include "ZzScrollbackBridge.h"

#include "qtermwidget.h"
#include "log/ZzLogEngine.h"

ZzScrollbackBridge::ZzScrollbackBridge(QTermWidget *term, ZzLogEngine *engine,
                                       QObject *parent)
    : QObject(parent)
    , m_term(term)
    , m_engine(engine)
{
    // 追加路径：dupDisplayOutput 逐行吐 UTF-8，去掉行尾换行后入热层
    connect(m_term, &QTermWidget::dupDisplayOutput, this,
            [this](const char *data, int len) {
                QString line = QString::fromUtf8(data, len);
                while (line.endsWith(QLatin1Char('\n'))
                       || line.endsWith(QLatin1Char('\r'))) {
                    line.chop(1);
                }
                m_engine->appendLines({line});
            });
    // 降级路径（规格 §八）：引擎 I/O 失败 → 状态栏提示，不打断终端
    connect(m_engine, &ZzLogEngine::degradedToMemoryOnly, this,
            [this](const QString &reason) { emit degraded(reason); });
}

QStringList ZzScrollbackBridge::readOlderLines(qint64 beforeLine,
                                               int maxLines) const
{
    return m_engine->readBack(beforeLine, maxLines);
}

qint64 ZzScrollbackBridge::totalLines() const
{
    return m_engine->totalLines();
}

void ZzScrollbackBridge::simulateDegradationForTest(const QString &reason)
{
    emit degraded(reason);
}
```

- [ ] **步骤 5：读回注入接线（前置：计划 05 已交付 ZzTermWidget 历史提供器 API）**

规格 §5.4 的读回路径依赖计划 05 提供的 ZzTermWidget 新 API，预计形态为
`QTermWidget::setHistoryProvider(std::function<QStringList(qint64 beforeLine, int maxLines)>)`
（或等价机制，以计划 05 实际交付签名为准）。在 `ZzScrollbackBridge` 构造函数末尾
（降级 connect 之后）追加：

```cpp
    // 读回路径（规格 §5.4）：滚动超出内存历史时，由计划 05 的注入机制回调索取更老的行
    m_term->setHistoryProvider([this](qint64 beforeLine, int maxLines) {
        return m_engine->readBack(beforeLine, maxLines);
    });
```

若计划 05 实际 API 形态不同（如越顶信号 + 注入槽、或虚接口），按"滚动越顶 → 引擎 readBack → 行注入显示层"的语义等价改写本接线处——这是装配层对该 API 的唯一引用点。

验证：

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R "tst_ZzScrollbackBridge|tst_ZzTerminalView" --output-on-failure
```

预期：编译通过（`setHistoryProvider` 由计划 05 引入 `qtermwidget.h`，缺该 API 时此处编译失败即说明计划 05 未完成，停止本任务先执行计划 05），既有测试全部 PASS。

- [ ] **步骤 6：给 ZzTerminalView 增加 `enableScrollback`（修改 `src/terminal/ZzTerminalView.h/.cpp`）**

`ZzTerminalView.h`：头部前向声明区（`class ZzAppSettings;` 旁）加 `class ZzScrollbackBridge;`；类声明中 `applySettings` 之后加公有方法，私有成员区加桥成员：

```cpp
    /** @brief 启用滚动历史桥：为该会话创建 ZzLogEngine 并接线（ZzTabManager 开会话时调用）。 */
    void enableScrollback(const QString &sessionId);
```

```cpp
    ZzScrollbackBridge *m_scrollbackBridge = nullptr; ///< 滚动历史桥（可空，以本视图为父）
```

`ZzTerminalView.cpp`：`#include` 区加 `#include "log/ZzLogEngine.h"` 与 `#include "terminal/ZzScrollbackBridge.h"`，文件末尾加：

```cpp
void ZzTerminalView::enableScrollback(const QString &sessionId)
{
    if (m_scrollbackBridge) {
        return; // 每会话只建一次
    }
    auto *engine = new ZzLogEngine(sessionId, this);
    m_scrollbackBridge = new ZzScrollbackBridge(m_term, engine, this);
    // 降级 → 状态栏提示（经 errorOccurred 同一路径到 ZzTabManager::statusMessage）
    connect(m_scrollbackBridge, &ZzScrollbackBridge::degraded, this,
            [this](const QString &reason) {
                emit errorOccurred(QStringLiteral("滚动历史已降级为内存模式：%1")
                                       .arg(reason));
            });
}
```

同时在 `ZzTabManager::openSession` 与 `reconnectTab` 的 `view->openEndpoint(...)` 之前各加一行（`src/tab/ZzTabManager.cpp`）：

```cpp
    view->enableScrollback(profile.id);
```

- [ ] **步骤 7：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    terminal/ZzScrollbackBridge.h
    terminal/ZzScrollbackBridge.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzScrollbackBridge unit/tst_ZzScrollbackBridge.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R "tst_ZzScrollbackBridge|tst_ZzTabManager" --output-on-failure
```

预期：PASS（桥 3 个用例，TabManager 回归 6 个用例）。本任务起依赖计划 02（`log/ZzLogEngine.h`）。

- [ ] **步骤 8：附带性能测试 `tests/perf/tst_PerfScrollback.cpp`（规格 §9.1）**

```cpp
#include <QtTest/QtTest>

#include "ZzPerfRecorder.h"
#include "log/ZzLogEngine.h"
#include "qtermwidget.h"
#include "terminal/ZzScrollbackBridge.h"

/**
 * @brief 性能门控：经桥向引擎追加 10 万行（热层+温层归档路径）。阈值 5000ms（Release）。
 * @note 滚动帧时间 ≤16ms 的渲染侧指标由 ZzTermWidget 自身性能测试覆盖；
 *       此处门控的是"输出行 → 引擎"这条装配链路不成为吞吐瓶颈。
 */
class tst_PerfScrollback : public QObject
{
    Q_OBJECT
private slots:
    void appendHundredThousandLines()
    {
        if (!ZzPerfRecorder::gatingEnabled()) {
            QSKIP("性能门控仅在 Release 构建下有效（规格 §9.1）");
        }
        QTermWidget term;
        ZzLogEngine engine(QStringLiteral("perf-scrollback"));
        ZzScrollbackBridge bridge(&term, &engine);

        QElapsedTimer timer;
        timer.start();
        for (int i = 0; i < 100000; ++i) {
            const QByteArray line =
                QStringLiteral("perf-line-%1 abcdefghijklmnopqrstuvwxyz\n")
                    .arg(i).toUtf8();
            emit term.dupDisplayOutput(line.constData(), line.size());
        }
        QCoreApplication::processEvents();
        const double elapsed = static_cast<double>(timer.elapsed());
        QCOMPARE(engine.totalLines(), 100000);

        const bool ok = ZzPerfRecorder::recordAndCheck(
            QStringLiteral("scrollback-append"),
            QStringLiteral("滚动历史追加 10 万行"), 5000.0, elapsed);
        QVERIFY2(ok, qPrintable(QStringLiteral("实测 %1ms 超过阈值 5000ms").arg(elapsed)));
    }
};

QTEST_MAIN(tst_PerfScrollback)
#include "tst_PerfScrollback.moc"
```

`tests/perf/CMakeLists.txt` 末尾追加：

```cmake
zz_add_perf_test(tst_PerfScrollback tst_PerfScrollback.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug && cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_PerfScrollback --output-on-failure
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release -R tst_PerfScrollback --output-on-failure
```

预期：Debug SKIP；Release PASS 并生成 `tests/perf/records/<今日>-scrollback-append.json`。

- [ ] **步骤 9：Commit**

```bash
git add src/terminal/ZzScrollbackBridge.h src/terminal/ZzScrollbackBridge.cpp src/terminal/ZzTerminalView.h src/terminal/ZzTerminalView.cpp src/tab/ZzTabManager.cpp tests/unit/tst_ZzScrollbackBridge.cpp tests/perf/tst_PerfScrollback.cpp tests/perf/records src/CMakeLists.txt tests/CMakeLists.txt tests/perf/CMakeLists.txt
git commit -m "feat: 滚动历史桥 ZzScrollbackBridge（输出入引擎/读回注入接线/降级提示，含性能门控）"
```

---

## 任务 13：错误处理 UI（标签内错误横幅 + 状态栏提示）

规格 §八：GUI 层错误走状态栏 + 标签内提示，不弹窗轰炸；连接失败保留在标签内显示错误与"重试"按钮。状态栏瞬时消息链路（ZzTabManager::statusMessage → ZzAppShell）已在任务 7/9 建好；本任务补齐标签内错误横幅。

**文件：**
- 修改：`src/terminal/ZzTerminalView.h`、`src/terminal/ZzTerminalView.cpp`
- 创建：`tests/unit/tst_ZzErrorUi.cpp`
- 修改：`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzErrorUi.cpp`**

```cpp
#include <QtTest/QtTest>

#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>

#include "ZzMockTransport.h"
#include "terminal/ZzTerminalView.h"

/**
 * @brief 验证标签内错误横幅：出错显示、重试重连、连通自动隐藏（规格 §八）。
 */
class tst_ZzErrorUi : public QObject
{
    Q_OBJECT
private slots:
    void init()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
    }

    void errorShowsBannerWithRetry()
    {
        ZzTerminalView view;
        view.resize(640, 480);
        view.show(); // 横幅可见性断言需要父子均 show
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        view.openEndpoint(ZzTransportEndpoint{});
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
        QVERIFY(!view.errorBanner()->isVisible());

        transport->simulateError(1001, QStringLiteral("认证失败"));
        QVERIFY(view.errorBanner()->isVisible());
        QVERIFY(view.errorLabel()->text().contains(QStringLiteral("认证失败")));

        // 点重试 → 用记忆的参数重新 open
        QTest::mouseClick(view.retryButton(), Qt::LeftButton);
        QCOMPARE(transport->openCallCount, 2);
        QVERIFY(!view.errorBanner()->isVisible()); // 点击即收起
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
    }

    void bannerHidesOnReconnect()
    {
        ZzTerminalView view;
        view.resize(640, 480);
        view.show();
        auto *transport = new ZzMockTransport(&view);
        view.setTransport(transport);
        transport->simulateError(1002, QStringLiteral("网络不可达"));
        QVERIFY(view.errorBanner()->isVisible());

        view.openEndpoint(ZzTransportEndpoint{});
        QTRY_COMPARE(view.transportState(), ZzTransportInterface::State::Connected);
        QVERIFY(!view.errorBanner()->isVisible());
    }
};

QTEST_MAIN(tst_ZzErrorUi)
#include "tst_ZzErrorUi.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzErrorUi
```

预期：编译失败，报错 `class ZzTerminalView has no member named 'errorBanner'`。

- [ ] **步骤 3：给 ZzTerminalView 增加错误横幅（修改 `src/terminal/ZzTerminalView.h`）**

在私有区新增成员与观察口（公有区）：

```cpp
public:
    // ---- 错误横幅观察口（测试用，任务 13） ----
    /** @brief 标签内错误提示条。 */
    [[nodiscard]] QWidget *errorBanner() const;
    /** @brief 错误文本标签。 */
    [[nodiscard]] QLabel *errorLabel() const;
    /** @brief 重试按钮。 */
    [[nodiscard]] QPushButton *retryButton() const;

private:
    void showErrorBanner(const QString &message);
    void hideErrorBanner();
```

私有成员区新增：

```cpp
    QWidget *m_errorBanner = nullptr;  ///< 标签内错误提示条（默认隐藏）
    QLabel *m_errorLabel = nullptr;
    QPushButton *m_retryButton = nullptr;
```

头部 `#include` 区加前向声明 `class QLabel; class QPushButton;`。

- [ ] **步骤 4：实现横幅（修改 `src/terminal/ZzTerminalView.cpp`）**

`#include` 区追加：

```cpp
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QPushButton>
```

构造函数中、`layout->addWidget(m_term, 1)` 之前插入横幅构建：

```cpp
    // 标签内错误横幅：默认隐藏（规格 §八：错误走标签内提示，不弹窗轰炸）
    m_errorBanner = new QWidget(this);
    m_errorBanner->setObjectName(QStringLiteral("zzErrorBanner"));
    m_errorBanner->setStyleSheet(
        QStringLiteral("#zzErrorBanner { background: #4a2b2b; color: #ffd7d7; }"));
    auto *bannerLayout = new QHBoxLayout(m_errorBanner);
    bannerLayout->setContentsMargins(8, 4, 8, 4);
    m_errorLabel = new QLabel(m_errorBanner);
    m_errorLabel->setWordWrap(true);
    m_retryButton = new QPushButton(QStringLiteral("重试"), m_errorBanner);
    m_retryButton->setObjectName(QStringLiteral("zzRetryButton"));
    bannerLayout->addWidget(m_errorLabel, 1);
    bannerLayout->addWidget(m_retryButton);
    m_errorBanner->hide();
    connect(m_retryButton, &QPushButton::clicked, this, [this]() {
        hideErrorBanner();
        if (m_transport) {
            m_transport->open(m_lastEndpoint); // 用记忆的参数重试
        }
    });
    layout->addWidget(m_errorBanner);
```

`setTransport` 中两个 lambda 改为（错误显示横幅、连通隐藏）：

```cpp
    connect(m_transport, &ZzTransportInterface::stateChanged, this,
            [this](ZzTransportInterface::State state) {
                if (state == ZzTransportInterface::State::Connected) {
                    hideErrorBanner();
                }
                emit stateChanged(state);
            });
    connect(m_transport, &ZzTransportInterface::errorOccurred, this,
            [this](int, const QString &message) {
                showErrorBanner(message);
                emit errorOccurred(message);
            });
```

文件末尾追加：

```cpp
QWidget *ZzTerminalView::errorBanner() const { return m_errorBanner; }
QLabel *ZzTerminalView::errorLabel() const { return m_errorLabel; }
QPushButton *ZzTerminalView::retryButton() const { return m_retryButton; }

void ZzTerminalView::showErrorBanner(const QString &message)
{
    m_errorLabel->setText(message);
    m_errorBanner->show();
}

void ZzTerminalView::hideErrorBanner()
{
    m_errorBanner->hide();
}
```

- [ ] **步骤 5：更新 CMake 并运行测试验证通过**

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzErrorUi unit/tst_ZzErrorUi.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug --output-on-failure
```

预期：全量 PASS（含 `tst_ZzErrorUi` 2 个用例；任务 6 的 `tst_ZzTerminalView` 回归通过）。

- [ ] **步骤 6：Commit**

```bash
git add src/terminal/ZzTerminalView.h src/terminal/ZzTerminalView.cpp tests/unit/tst_ZzErrorUi.cpp tests/CMakeLists.txt
git commit -m "feat: 标签内错误横幅与重试按钮（错误走状态栏+标签内提示，不弹窗）"
```

---

## 任务 14：壳层装配（ZzClawTermModule + ZzAppShell + main.cpp）

规格 §七壳层：无边框标题栏（ZzWindowKit）+ 会话面板 Dock + 标签区 + 状态栏（连接状态 | 编码 | 行列）。基于 ZzPureToolsPro 的 `ZzApplicationBuilder` 模式：终端区/设置页注册为框架页面，装配回调里挂 Dock 与状态栏。

**文件：**
- 创建：`src/ZzClawTermModule.h`、`src/ZzClawTermModule.cpp`、`src/ZzAppShell.h`、`src/ZzAppShell.cpp`
- 修改：`src/main.cpp`（替换骨架版）
- 创建：`tests/unit/tst_ZzAppShell.cpp`
- 修改：`src/CMakeLists.txt`、`tests/CMakeLists.txt`

- [ ] **步骤 1：编写失败的测试 `tests/unit/tst_ZzAppShell.cpp`**

```cpp
#include <QtTest/QtTest>

#include <QtWidgets/QDockWidget>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>

#include "ZzAppShell.h"
#include "ZzMockTransport.h"
#include "session/ZzSessionProfile.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 壳层装配冒烟：普通 QMainWindow 上验证 dock、状态栏、双击到标签的完整链路。
 *
 * ZzAppShell::assemble 只依赖 QMainWindow&，因此无需拉起完整框架即可离屏测试。
 */
class tst_ZzAppShell : public QObject
{
    Q_OBJECT
private:
    QString m_dir;

private slots:
    void initTestCase()
    {
        qRegisterMetaType<ZzTransportInterface::State>();
        ZzTransportRegistry::instance().registerTransport(
            QStringLiteral("mock"),
            [](QObject *parent) { return new ZzMockTransport(parent); });
    }

    void cleanupTestCase()
    {
        ZzTransportRegistry::instance().clear();
    }

    void init()
    {
        m_dir = QDir(QDir::tempPath())
            .filePath(QStringLiteral("zzclawterm-shell-test"));
        QDir(m_dir).removeRecursively();
        QDir().mkpath(m_dir);
    }

    void assembleInstallsDockAndStatusBar()
    {
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QVERIFY(shell.sessionPanel() != nullptr);
        // 会话面板已停靠
        QCOMPARE(window.findChild<QDockWidget *>(
                     QStringLiteral("sessions")), shell.sessionPanel());
        // 状态栏三要素
        QVERIFY(shell.statusStateLabel() != nullptr);
        QVERIFY(shell.statusEncodingLabel() != nullptr);
        QVERIFY(shell.statusSizeLabel() != nullptr);
    }

    void doubleClickOpensTabAndUpdatesStatusBar()
    {
        ZzAppShell shell(m_dir);
        QMainWindow window;
        QVERIFY(shell.assemble(window));
        QWidget container;
        QVERIFY(shell.createTerminalPage(&container));
        QVERIFY(shell.tabManager() != nullptr);

        // 放一条会话进模型，等价于用户双击
        ZzSessionProfile profile;
        profile.id = QStringLiteral("shell-1");
        profile.name = QStringLiteral("装配机");
        profile.protocol = QStringLiteral("mock");
        shell.sessionModel()->addProfile(profile);
        shell.sessionPanel()->triggerConnect(QStringLiteral("shell-1"));

        QCOMPARE(shell.tabManager()->count(), 1);
        QTRY_COMPARE(shell.statusStateLabel()->text(), QStringLiteral("已连接"));
        QVERIFY(!shell.statusEncodingLabel()->text().isEmpty());
    }
};

QTEST_MAIN(tst_ZzAppShell)
#include "tst_ZzAppShell.moc"
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug -R tst_ZzAppShell
```

预期：编译失败，报错 `ZzAppShell.h: No such file or directory`。

- [ ] **步骤 3：实现 `src/ZzClawTermModule.h/.cpp`（框架应用模块）**

`ZzClawTermModule.h`：

```cpp
#pragma once

#include <ZzPureTools/ZzApplicationModule.h>

/**
 * @brief ZzClawTerm 应用模块：v0.1 无跨模块依赖，仅满足框架生命周期协议。
 */
class ZzClawTermModule final : public ZzPureTools::ZzApplicationModule
{
public:
    /** @brief 模块身份：稳定 id、版本、空依赖集。 */
    [[nodiscard]] ZzPureTools::ZzModuleDescriptor descriptor() const override;

    /** @brief 启动：装配在窗口回调与页面工厂中完成，此处直接成功。 */
    [[nodiscard]] ZzCore::ZzResult<void> start() override;

    /** @brief 协作停止请求（幂等）。 */
    void requestStop() noexcept override;

    /** @brief 最终资源清理（幂等，不可抛）。 */
    void stop() noexcept override;

private:
    bool m_started = false;
};
```

`ZzClawTermModule.cpp`：

```cpp
#include "ZzClawTermModule.h"

ZzPureTools::ZzModuleDescriptor ZzClawTermModule::descriptor() const
{
    return ZzPureTools::ZzModuleDescriptor{
        ZzPureTools::ZzModuleId(QStringLiteral("com.zzclawterm.app")),
        QStringLiteral("0.1.0"),
        {}};
}

ZzCore::ZzResult<void> ZzClawTermModule::start()
{
    m_started = true;
    return ZzCore::ZzResult<void>::success();
}

void ZzClawTermModule::requestStop() noexcept
{
    m_started = false;
}

void ZzClawTermModule::stop() noexcept
{
    m_started = false;
}
```

- [ ] **步骤 4：实现 `src/ZzAppShell.h`**

```cpp
#pragma once

#include <memory>

#include <QtCore/QObject>
#include <QtCore/QPointer>

#include <ZzCore/ZzResult.h>
#include <ZzPureTools/ZzPageInstance.h>

class QLabel;
class QMainWindow;
class QStatusBar;
class QWidget;
class ZzCredentialStore;
class ZzSessionModel;
class ZzSessionPanel;
class ZzTabManager;

/**
 * @brief 组合根：持有后端服务（会话模型/凭据库），装配窗口 Dock、
 *        状态栏，并提供框架页面工厂（规格 §三/§七）。
 *
 * assemble() 只依赖 QMainWindow&——ZzApplicationWindow 是其子类，
 * 测试可用普通 QMainWindow 离屏验证全部装配链路。
 */
class ZzAppShell : public QObject
{
    Q_OBJECT
public:
    /**
     * @brief 构造组合根。
     * @param configDir 配置目录（sessions.json / credentials.dat 所在）；
     *        空串=按 QStandardPaths::AppConfigLocation 解析（生产路径）。
     */
    explicit ZzAppShell(const QString &configDir = QString(),
                        QObject *parent = nullptr);

    /** @brief 析构时清空面板登记册（面板随窗口销毁，避免悬挂指针）。 */
    ~ZzAppShell() override;

    /**
     * @brief 装配窗口：停靠会话面板、安装状态栏三要素、接好双击到开标签的链路。
     *        作为 ZzApplicationBuilder 的窗口装配回调调用。
     */
    [[nodiscard]] ZzCore::ZzResult<void> assemble(QMainWindow &window);

    /** @brief 终端区页面工厂（ZzTabManager 所在页，Persistent）。 */
    [[nodiscard]] ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
        createTerminalPage(QWidget *pageParent);

    /** @brief 设置页页面工厂（导航 Footer）。 */
    [[nodiscard]] ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
        createSettingsPage(QWidget *pageParent);

    // ---- 测试观察口 ----
    [[nodiscard]] ZzSessionPanel *sessionPanel() const;
    [[nodiscard]] ZzTabManager *tabManager() const;
    [[nodiscard]] ZzSessionModel *sessionModel() const;
    [[nodiscard]] ZzCredentialStore *credentialStore() const;
    [[nodiscard]] QLabel *statusStateLabel() const;
    [[nodiscard]] QLabel *statusEncodingLabel() const;
    [[nodiscard]] QLabel *statusSizeLabel() const;

public slots:
    /** @brief 状态栏瞬时提示（5 秒自动消退，规格 §八错误不弹窗）。 */
    void showStatusMessage(const QString &message);

private:
    /** @brief 装配 ZzTabManager 的认证/主机密钥/状态栏接线（创建终端页时调用）。 */
    void wireTabManager(ZzTabManager *tabs);

    QString m_configDir;
    ZzSessionModel *m_sessionModel = nullptr;      ///< this 为父
    ZzCredentialStore *m_credentialStore = nullptr; ///< this 为父
    QPointer<ZzSessionPanel> m_sessionPanel;       ///< 窗口拥有
    QPointer<ZzTabManager> m_tabManager;           ///< pageParent 拥有
    QPointer<QLabel> m_stateLabel;
    QPointer<QLabel> m_encodingLabel;
    QPointer<QLabel> m_sizeLabel;
    QPointer<QStatusBar> m_statusBar;
};
```

- [ ] **步骤 5：实现 `src/ZzAppShell.cpp`**

```cpp
#include "ZzAppShell.h"

#include <utility>

#include <QtCore/QStandardPaths>
#include <QtWidgets/QDockWidget>
#include <QtWidgets/QLabel>
#include <QtWidgets/QMainWindow>
#include <QtWidgets/QStatusBar>

#include "dialog/ZzHostKeyDialog.h"
#include "dialog/ZzMasterPasswordDialog.h"
#include "panel/ZzPanelRegistry.h"
#include "panel/ZzSessionPanel.h"
#include "session/ZzCredentialStore.h"
#include "session/ZzSessionModel.h"
#include "settings/ZzAppSettings.h"
#include "settings/ZzSettingsPage.h"
#include "tab/ZzTabManager.h"

namespace {

/** @brief 传输状态 → 状态栏文案。 */
QString zzStateText(ZzTransportInterface::State state)
{
    switch (state) {
    case ZzTransportInterface::State::Connected:
        return QStringLiteral("已连接");
    case ZzTransportInterface::State::Connecting:
        return QStringLiteral("连接中…");
    case ZzTransportInterface::State::Disconnected:
        return QStringLiteral("未连接");
    }
    return QStringLiteral("未连接");
}

} // namespace

ZzAppShell::ZzAppShell(const QString &configDir, QObject *parent)
    : QObject(parent)
    , m_configDir(configDir.isEmpty()
        ? QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
        : configDir)
{
    m_sessionModel = new ZzSessionModel(
        m_configDir + QStringLiteral("/sessions.json"), this);
    m_sessionModel->load();
    m_credentialStore = new ZzCredentialStore(
        m_configDir + QStringLiteral("/credentials.dat"), this);
}

ZzAppShell::~ZzAppShell()
{
    // 面板随窗口销毁，登记册中的裸指针随之失效，统一清空
    ZzPanelRegistry::instance().clear();
}

ZzCore::ZzResult<void> ZzAppShell::assemble(QMainWindow &window)
{
    // 会话面板 Dock（规格 §七：可折叠、可停靠左右）
    auto *panel = new ZzSessionPanel(m_sessionModel, m_credentialStore, &window);
    ZzPanelRegistry::instance().registerPanel(panel);
    window.addDockWidget(Qt::LeftDockWidgetArea, panel);
    m_sessionPanel = panel;

    // 状态栏三要素：连接状态 | 编码 | 行列
    m_statusBar = window.statusBar();
    m_stateLabel = new QLabel(QStringLiteral("未连接"), m_statusBar);
    m_encodingLabel = new QLabel(m_statusBar);
    m_sizeLabel = new QLabel(m_statusBar);
    m_statusBar->addPermanentWidget(m_stateLabel);
    m_statusBar->addPermanentWidget(m_encodingLabel);
    m_statusBar->addPermanentWidget(m_sizeLabel);

    // 双击会话 → 开标签（终端页可能尚未创建，经 QPointer 惰性转发）
    connect(panel, &ZzSessionPanel::connectRequested, this,
            [this](const ZzSessionProfile &profile) {
                if (m_tabManager) {
                    m_tabManager->openSession(profile);
                }
            });
    return ZzCore::ZzResult<void>::success();
}

ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
ZzAppShell::createTerminalPage(QWidget *pageParent)
{
    auto view = std::make_unique<ZzTabManager>(pageParent);
    wireTabManager(view.get());
    m_tabManager = view.get();

    auto viewModel = std::make_unique<QObject>();
    auto presenter = std::make_unique<QObject>();
    QWidget *viewObserver = view.release();
    return ZzPureTools::ZzPageInstance::create(
        pageParent, viewObserver, std::move(viewModel), std::move(presenter));
}

ZzCore::ZzResult<std::unique_ptr<ZzPureTools::ZzPageInstance>>
ZzAppShell::createSettingsPage(QWidget *pageParent)
{
    auto view = std::make_unique<ZzSettingsPage>(
        &ZzAppSettings::instance(), pageParent);
    auto viewModel = std::make_unique<QObject>();
    auto presenter = std::make_unique<QObject>();
    QWidget *viewObserver = view.release();
    return ZzPureTools::ZzPageInstance::create(
        pageParent, viewObserver, std::move(viewModel), std::move(presenter));
}

void ZzAppShell::wireTabManager(ZzTabManager *tabs)
{
    // SSH 认证：密码经主密码解锁后从凭据库取（规格 §七连接流程）
    ZzCredentialStore *store = m_credentialStore;
    tabs->setPasswordProvider(
        [store, tabs](const ZzSessionProfile &profile) -> QString {
            if (profile.credentialId.isEmpty()) {
                return {};
            }
            if (!ZzMasterPasswordDialog::ensureUnlocked(store, tabs)) {
                return {}; // 用户取消解锁 → 取消密码认证
            }
            return store->credential(profile.credentialId);
        });
    // 主机密钥确认（规格 §八安全底线）
    tabs->setHostKeyConfirmer(
        [tabs](const QString &host, const QString &fingerprint, bool changed) {
            return ZzHostKeyDialog::confirm(host, fingerprint, changed, tabs);
        });

    // 状态栏：状态 / 编码 / 行列 / 瞬时消息
    tabs->connect(tabs, &ZzTabManager::currentStateChanged, this,
                  [this](ZzTransportInterface::State state) {
                      if (m_stateLabel) {
                          m_stateLabel->setText(zzStateText(state));
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::currentEncodingChanged, this,
                  [this](const QString &encoding) {
                      if (m_encodingLabel) {
                          m_encodingLabel->setText(encoding);
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::currentSizeChanged, this,
                  [this](int cols, int rows) {
                      if (m_sizeLabel) {
                          m_sizeLabel->setText(
                              QStringLiteral("%1×%2").arg(cols).arg(rows));
                      }
                  });
    tabs->connect(tabs, &ZzTabManager::statusMessage, this,
                  &ZzAppShell::showStatusMessage);

    // 设置变更实时应用到全部已打开标签（规格 §七）
    tabs->connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged,
                  tabs, [tabs]() {
                      const ZzAppSettings &settings = ZzAppSettings::instance();
                      for (int i = 0; i < tabs->count(); ++i) {
                          if (auto *view = tabs->viewAt(i)) {
                              view->applySettings(settings);
                          }
                      }
                  });
}

void ZzAppShell::showStatusMessage(const QString &message)
{
    if (m_statusBar) {
        m_statusBar->showMessage(message, 5000);
    }
}

ZzSessionPanel *ZzAppShell::sessionPanel() const { return m_sessionPanel; }
ZzTabManager *ZzAppShell::tabManager() const { return m_tabManager; }
ZzSessionModel *ZzAppShell::sessionModel() const { return m_sessionModel; }
ZzCredentialStore *ZzAppShell::credentialStore() const { return m_credentialStore; }
QLabel *ZzAppShell::statusStateLabel() const { return m_stateLabel; }
QLabel *ZzAppShell::statusEncodingLabel() const { return m_encodingLabel; }
QLabel *ZzAppShell::statusSizeLabel() const { return m_sizeLabel; }
```

- [ ] **步骤 6：替换 `src/main.cpp` 为完整装配版**

```cpp
#include <cstdlib>
#include <memory>
#include <utility>

#include <QtCore/QCoreApplication>

#include <ZzFluentUI/ZzNavigationPlacement.h>
#include <ZzPureTools/ZzApplicationBuilder.h>
#include <ZzPureTools/ZzNavigationNode.h>
#include <ZzPureTools/ZzPageLifetimePolicy.h>
#include <ZzPureTools/ZzPageRegistration.h>
#include <ZzPureTools/ZzPureApplication.h>
#include <ZzPureTools/ZzRouteId.h>
#include <ZzWindowKit/ZzWindowKitBootstrap.h>

#include "ZzAppShell.h"
#include "ZzClawTermModule.h"
#include "transport/ZzLocalPtyTransport.h"
#include "transport/ZzSshTransport.h"
#include "transport/ZzTransportRegistry.h"

/**
 * @brief 应用入口：框架 bootstrap → 注册传输协议 → 装配页面/导航/窗口回调。
 */
int main(int argc, char *argv[])
{
    const auto bootstrap = ZzWindowKit::ZzWindowKitBootstrap::prepare();
    if (!bootstrap) {
        return EXIT_FAILURE;
    }

    ZzPureTools::ZzPureApplication application(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("ZzClawTerm"));
    QCoreApplication::setOrganizationName(QStringLiteral("ZzClaw"));

    // 内置传输协议注册（规格 §2.3：与未来第三方插件同一条注册路径）
    auto &transports = ZzTransportRegistry::instance();
    transports.registerTransport(QStringLiteral("ssh"),
        [](QObject *parent) -> ZzTransportInterface * {
            return new ZzSshTransport(parent);
        });
    transports.registerTransport(QStringLiteral("local"),
        [](QObject *parent) -> ZzTransportInterface * {
            return new ZzLocalPtyTransport(parent);
        });

    ZzAppShell shell;

    ZzPureTools::ZzApplicationBuilder builder;
    if (!builder.addModule(std::make_unique<ZzClawTermModule>())) {
        return EXIT_FAILURE;
    }

    const ZzPureTools::ZzRouteId terminalRoute(QStringLiteral("terminal"));
    const ZzPureTools::ZzRouteId settingsRoute(QStringLiteral("settings"));

    ZzPureTools::ZzPageRegistration terminalPage;
    terminalPage.routeId = terminalRoute;
    terminalPage.lifetime = ZzPureTools::ZzPageLifetimePolicy::Persistent;
    terminalPage.factory = [&shell](QWidget *pageParent) {
        return shell.createTerminalPage(pageParent);
    };
    if (!builder.addPage(std::move(terminalPage))) {
        return EXIT_FAILURE;
    }

    ZzPureTools::ZzPageRegistration settingsPage;
    settingsPage.routeId = settingsRoute;
    settingsPage.lifetime = ZzPureTools::ZzPageLifetimePolicy::Persistent;
    settingsPage.factory = [&shell](QWidget *pageParent) {
        return shell.createSettingsPage(pageParent);
    };
    if (!builder.addPage(std::move(settingsPage))) {
        return EXIT_FAILURE;
    }

    ZzPureTools::ZzNavigationNode terminalNode{
        terminalRoute, QStringLiteral("ZzClawTerm"),
        QStringLiteral("Terminal"), {}};
    if (!builder.addNavigationNode(std::move(terminalNode))) {
        return EXIT_FAILURE;
    }
    ZzPureTools::ZzNavigationNode settingsNode{
        settingsRoute, QStringLiteral("ZzClawTerm"),
        QStringLiteral("Settings"), {}};
    settingsNode.placement = ZzFluentUI::ZzNavigationPlacement::Footer;
    if (!builder.addNavigationNode(std::move(settingsNode))) {
        return EXIT_FAILURE;
    }

    if (!builder.setInitialRoute(terminalRoute)
        || !builder.setWindowSetupCallback(
            [&shell](ZzPureTools::ZzApplicationWindow &window) {
                return shell.assemble(window);
            })
        || !builder.build(application)) {
        return EXIT_FAILURE;
    }

    return application.exec();
}
```

- [ ] **步骤 7：更新 CMake 并运行测试验证通过**

`src/CMakeLists.txt` 的 `ZZCLAWTERM_APP_SOURCES` 追加：

```cmake
    ZzClawTermModule.h
    ZzClawTermModule.cpp
    ZzAppShell.h
    ZzAppShell.cpp
```

`tests/CMakeLists.txt` 末尾追加：

```cmake
zz_add_qtest(tst_ZzAppShell unit/tst_ZzAppShell.cpp)
```

运行：

```bash
cmake --preset linux-gcc-debug
cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug --output-on-failure
```

预期：全量 PASS（含 `tst_ZzAppShell` 2 个用例）。再验证可执行能起框架窗口（离屏+冒烟退出已移除，改为手动/人工验收覆盖）：

```bash
QT_QPA_PLATFORM=offscreen timeout 5 ./build/linux-gcc-debug/src/ZzClawTerm; echo "exit=$?"
```

预期：进程被 timeout 终止（exit=124）即窗口事件循环正常运行；若立即非零退出则检查框架 bootstrap 与构建链路。

- [ ] **步骤 8：Commit**

```bash
git add src/ZzClawTermModule.h src/ZzClawTermModule.cpp src/ZzAppShell.h src/ZzAppShell.cpp src/main.cpp tests/unit/tst_ZzAppShell.cpp src/CMakeLists.txt tests/CMakeLists.txt
git commit -m "feat: 壳层装配 ZzAppShell 与 main.cpp（Dock/状态栏/页面工厂/认证与主机密钥接线）"
```

---

## 任务 15：三平台打包与人工验收清单

规格 §二/§十二：三平台可执行包；三平台人工验收清单（参照 ZzPureTools 模式）。

**文件：**
- 创建：`scripts/package-windows.ps1`、`scripts/package-macos.sh`、`scripts/package-linux.sh`
- 创建：`docs/acceptance/v0.1-manual-acceptance.md`

- [ ] **步骤 1：创建 `scripts/package-windows.ps1`**

```powershell
# ZzClawTerm Windows 打包：Release 构建 → windeployqt 收集依赖 → zip 绿色包
# 用法：pwsh scripts/package-windows.ps1 [-QtRoot "D:\Qt\6.8.2\msvc2022_64"]
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
```

- [ ] **步骤 2：创建 `scripts/package-macos.sh`**

```bash
#!/usr/bin/env bash
# ZzClawTerm macOS 打包：Release 构建 → macdeployqt 生成自包含 .app → DMG
# 用法：QT_ROOT=~/Qt/6.8.2/macos bash scripts/package-macos.sh
set -euo pipefail
: "${QT_ROOT:?请通过 QT_ROOT 环境变量提供 Qt 前缀}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cmake --preset macos-clang-release
cmake --build --preset macos-clang-release

DIST="$ROOT/dist/macos"
rm -rf "$DIST"
mkdir -p "$DIST"
cp -R "$ROOT/build/macos-clang-release/src/ZzClawTerm.app" "$DIST/"

"$QT_ROOT/bin/macdeployqt" "$DIST/ZzClawTerm.app" -dmg \
    -always-overwrite -verbose=1
mv "$DIST/ZzClawTerm.dmg" "$ROOT/dist/ZzClawTerm-v0.1-macos.dmg"
echo "产出：dist/ZzClawTerm-v0.1-macos.dmg"
```

- [ ] **步骤 3：创建 `scripts/package-linux.sh`**

```bash
#!/usr/bin/env bash
# ZzClawTerm Linux 打包：Release 构建 → linuxdeploy + qt 插件 → AppImage
# 依赖：linuxdeploy-x86_64.AppImage 与 linuxdeploy-plugin-qt-x86_64.AppImage
#       已放入 PATH（自行从 GitHub 发布页下载并 chmod +x）
# 用法：QT_ROOT=/home/zz/Qt/6.11.1/gcc_64 bash scripts/package-linux.sh
set -euo pipefail
: "${QT_ROOT:?请通过 QT_ROOT 环境变量提供 Qt 前缀}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cmake --preset linux-gcc-release
cmake --build --preset linux-gcc-release

APPDIR="$ROOT/dist/linux/ZzClawTerm.AppDir"
rm -rf "$ROOT/dist/linux"
mkdir -p "$APPDIR/usr/bin"
cp "$ROOT/build/linux-gcc-release/src/ZzClawTerm" "$APPDIR/usr/bin/"

# 桌面入口与图标（图标沿用占位 PNG，正式图标随 UI 资源任务补充）
mkdir -p "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
cat > "$APPDIR/usr/share/applications/zzclawterm.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=ZzClawTerm
Comment=Cross-platform SSH terminal
Exec=ZzClawTerm
Icon=zzclawterm
Categories=System;TerminalEmulator;
EOF

export QMAKE="$QT_ROOT/bin/qmake6"
export LD_LIBRARY_PATH="$QT_ROOT/lib:${LD_LIBRARY_PATH:-}"
linuxdeploy-x86_64.AppImage \
    --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/ZzClawTerm" \
    --desktop-file "$APPDIR/usr/share/applications/zzclawterm.desktop" \
    --plugin qt \
    --output appimage
mv ZzClawTerm-*.AppImage "$ROOT/dist/ZzClawTerm-v0.1-linux-x86_64.AppImage"
echo "产出：dist/ZzClawTerm-v0.1-linux-x86_64.AppImage"
```

随后赋予执行权限：

```bash
chmod +x scripts/package-macos.sh scripts/package-linux.sh
```

- [ ] **步骤 4：创建 `docs/acceptance/v0.1-manual-acceptance.md`（人工验收清单）**

```markdown
# ZzClawTerm v0.1 三平台人工验收清单

> 对应设计规格 §十二 v0.1 验收标准。每个平台逐项打勾；任何一项不通过即打回。
> 验收包：`dist/ZzClawTerm-v0.1-windows-x64.zip` / `-macos.dmg` / `-linux-x86_64.AppImage`。

## 1. 会话管理（规格 §十二）

- [ ] 新建 SSH 会话（密码认证），保存后重启应用会话仍在
- [ ] 会话按 `生产环境/Web 服务器` 路径在面板中呈现两级分组
- [ ] 编辑会话（改端口）立即生效；删除会话后树刷新
- [ ] 复制会话生成"（副本）"条目
- [ ] 双击会话建立到真实 SSH 服务器的连接

## 2. 终端交互（规格 §十二）

- [ ] vim 正常编辑/保存/退出
- [ ] top/htop 全屏刷新无残影
- [ ] 中文输入与显示正常（UTF-8；GBK 会话切换编码后正常）
- [ ] 256 色正常（`msgcat --color=test` 或等价脚本观察）
- [ ] 鼠标滚轮滚动、选区复制粘贴正常

## 3. 滚动历史（规格 §5/§十二）

- [ ] `seq 1 1000000` 输出 100 万行后滚动不卡顿、内容无丢失
- [ ] 滚动过程中内存占用有界（任务管理器/top 观察，不随行数线性增长）
- [ ] 日志引擎写盘失败（磁盘满）时状态栏提示降级为内存模式，终端交互不受影响

## 4. 断线与重连（规格 §七/§八）

- [ ] 拔网/`kill` sshd 后标签变灰保留，不自动关闭
- [ ] 右键断线标签 →"重新连接"可恢复会话
- [ ] 连接失败时标签内显示错误与"重试"按钮，无弹窗轰炸
- [ ] 主机密钥首次连接弹出指纹确认；改动 known_hosts.json 中指纹后弹出变更警告

## 5. 凭据与设置（规格 §六/§七）

- [ ] 首次启动设置主密码；重启后解锁一次即可连接所有密码会话
- [ ] 错误主密码无法解锁
- [ ] 设置页改字号/配色/编码，新标签生效且已打开标签实时应用
- [ ] 状态栏正确显示 连接状态 | 编码 | 行列

## 6. 本地 shell（规格 §七）

- [ ] 新建"本地 Shell"会话可直接打开本机终端（powershell / bash / zsh）
- [ ] 本地 shell 与 SSH 标签可并存、分别关闭

## 7. 平台特项

### Windows
- [ ] zip 解压即用，无缺失 DLL 报错
- [ ] 无边框窗口拖动/最大化/最小化正常，DPI 缩放（150%）下界面不糊

### macOS
- [ ] .app 从 DMG 拖入 Applications 可启动（Gatekeeper 提示属预期，自用场景右键打开）
- [ ] 无边框窗口与系统深色模式协调

### Linux
- [ ] AppImage `chmod +x` 后直接运行
- [ ] Wayland 与 X11 会话下窗口均正常

## 8. 性能门控复核（规格 §9.1）

- [ ] `ctest --preset <平台>-release` 全绿（含 perf 标签测试）
- [ ] `tests/perf/records/` 中本周期各功能记录齐全且 passed=true
```

- [ ] **步骤 5：本机执行 Linux 打包脚本验证**

```bash
cd /home/zz/Jackfahdin/github/ZzClawTerm
QT_ROOT=/home/zz/Qt/6.11.1/gcc_64 bash scripts/package-linux.sh
```

预期：产出 `dist/ZzClawTerm-v0.1-linux-x86_64.AppImage`（若本机无 linuxdeploy，至少验证到 AppDir 组装步骤无脚本语法错误：`bash -n scripts/package-linux.sh`）。Windows/macOS 脚本在对应平台人工验收时执行（清单第 7 节覆盖）。

- [ ] **步骤 6：全量回归 + Release 性能门控 + Commit**

```bash
cmake --preset linux-gcc-debug && cmake --build --preset linux-gcc-debug
ctest --preset linux-gcc-debug --output-on-failure
cmake --preset linux-gcc-release && cmake --build --preset linux-gcc-release
ctest --preset linux-gcc-release --output-on-failure
```

预期：Debug/Release 全绿；`tests/perf/records/` 含 app-settings、tab-lifecycle、connect-flow、scrollback-append、perf-infra-selfcheck 五份记录且 `passed=true`。

注意：`dist/` 为构建产物，不入库——先在 `.gitignore` 追加一行 `dist/`，再提交：

```bash
printf '\n# 打包产物\ndist/\n' >> .gitignore
git add .gitignore scripts docs/acceptance
git commit -m "build: 三平台打包脚本（windeployqt/macdeployqt/linuxdeploy）与 v0.1 人工验收清单"
```

---

## 附：跨计划协调问题（执行本计划前需主会话确认）

1. **滚动历史读回注入（归属已明确：计划 05）**：规格 §5.4 要求"向上滚动超出 ZzTermWidget 内存历史时从 ZzLogEngine 读回"。ZzTermWidget 侧所需的历史注入 API 由**计划 05** 提供，预计形态 `setHistoryProvider(std::function<QStringList(qint64 beforeLine, int maxLines)>)`/等价机制（见契约区"计划 05"小节）。任务 12 步骤 5 的读回接线必须在计划 05 完成后进行；完成前，"100 万行滚动无丢失"验收项只能覆盖到引擎侧持久化，不能覆盖 UI 读回。
2. **计划 01 契约**：`ZzSshConnection` 的密码索取（`passwordRequested`/`providePassword`）、主机密钥确认（`hostKeyUnknown`/`hostKeyChanged`/`acceptHostKey(bool)`/`rejectHostKey`）、`openShellChannel()` 所有权语义是本计划按规格 §4.2 推定的，需与计划 01 实际头文件对齐（不一致时改 `ZzSshTransport` 一处即可）。
3. **计划 03 契约**：`ZzSessionProfile` 需带 `id`、`protocol`（"ssh"/"local"）、`credentialId` 字段并提供 `Q_DECLARE_METATYPE`；local 会话的 shell 路径约定存于 `host` 字段。`ZzCredentialStore` 需 `hasMasterPassword`/`setMasterPassword`/`addCredential`（返回 id）。目标名 `ZzSessionCore`，目录 `src/session/`，并交付 `src/session/CMakeLists.txt`（本计划 `src/CMakeLists.txt` 的条件引入逻辑保持不变）。
4. **计划 02 契约**：`ZzLogEngine(const QString &sessionId, QObject *)`、`appendLines(QStringList)`、`readBack(qint64,int)`、`totalLines()`、`degradedToMemoryOnly(QString)` 信号。目标名 `ZzLogEngine`，目录 `src/log/`，并交付 `src/log/CMakeLists.txt`（本计划 `src/CMakeLists.txt` 的条件引入逻辑保持不变）。
5. **子模块 URL（已确认）**：`ZzSshCore` 远端为 `gitcode.com/JackfahdinQt/ZzSshCore`；libssh2 移植版为 `gitcode.com/JackfahdinImport/libssh2`；OpenSSL 为 `gitcode.com/ZzThirdParty/openssl`（缺 macOS 构建）。ZzPureToolsPro 远端仓库内是否同样嵌套一层 `ZzPureToolsPro/` 工程目录（本计划 CMake 已做两种布局的兼容探测，仅需确认实际生效路径）。
6. **执行顺序**：按头部链条——计划 01 → 本计划任务 1（骨架）→ 计划 02 → 计划 03 → 本计划任务 2 起依次执行；计划 05 须在任务 12 步骤 5（读回接线）前完成。
