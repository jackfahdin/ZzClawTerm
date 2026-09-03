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

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Widgets Network Test Xml Multimedia)

# 翻译工具链（可选）：缺失时跳过翻译嵌入，不影响主构建
find_package(Qt6 QUIET COMPONENTS LinguistTools)

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

# ZzPureToolsPro（应用框架）：仓库内嵌套一层同名工程目录，做兼容探测。
# 其 CMake 默认 BUILD_SHARED_LIBS=ON；此处在其 add_subdirectory 之前强制 OFF
# 改静态链接——动态库打包依赖各平台 deploy 工具额外拷贝 ZzPureTools/ZzCore/
# ZzFluentUI/ZzWindowKit 产物（windeployqt 不认非 Qt 依赖，已实际漏发），
# 静态后产物只有一个可执行文件，不可能再漏。generate_export_header 对静态
# 目标自动导出空宏；ZzFluentUI 图标资源由 ZzIconAssets.cpp 自 Q_INIT_RESOURCE，
# 静态归档不会被链接器剥离（该对象文件被控件代码直接引用）。
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
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

# OpenSSL：优先接入 vendored 预编译 bundle（third_party/openssl 提供
# OpenSSLBundleConfig.cmake，创建 OpenSSL::Crypto/OpenSSL::SSL 导入目标）。
# 注意 FindOpenSSL 模块模式不认 bundle 设置的普通变量，会自行 find_library
# 搜索（结果入缓存）：命中后可能指向系统库（Linux）或 NOTFOUND（Windows
# 无系统库时 REQUIRED 直接失败）。因此命中 bundle 后把其选中根写入缓存
# OPENSSL_ROOT_DIR（见下），让后续各 find_package(OpenSSL) 一致解析到
# bundle 真实产物；FindOpenSSL 的 NOT TARGET 守卫保证不重复建导入目标。
# bundle 无对应平台产物（如 macOS，third_party/openssl 仅有
# android/linux/windows 目录）或产物未构建完整（Windows 侧 static/shared
# 包缺 include/ 与库文件）时跳过，回退系统 OpenSSL（CI 上 macOS 经
# OPENSSL_ROOT_DIR 指向 brew openssl@3，Windows 指向 runner 自带 OpenSSL 3.x）。
if(NOT APPLE AND EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl/OpenSSLBundleConfig.cmake")
    set(ZZ_OPENSSL_BUNDLE_USABLE TRUE)
    if(WIN32)
        # Windows 产物完整性的粗检：任一 toolchain/linkage 下存在 ssl.h 才算可用
        #（bundle 布局为 windows/<toolchain>/x64/<cfg>/<linkage>/include/openssl/ssl.h）
        file(GLOB ZZ_OPENSSL_BUNDLE_SSL_HEADERS
            "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl/windows/*/x64/*/*/include/openssl/ssl.h")
        if(NOT ZZ_OPENSSL_BUNDLE_SSL_HEADERS)
            set(ZZ_OPENSSL_BUNDLE_USABLE FALSE)
            message(STATUS "OpenSSL：vendored bundle Windows 产物不完整，回退系统 OpenSSL")
        endif()
    endif()
    if(ZZ_OPENSSL_BUNDLE_USABLE)
        list(PREPEND CMAKE_PREFIX_PATH "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl")
        find_package(OpenSSLBundle QUIET)
        if(OpenSSLBundle_FOUND)
            message(STATUS "OpenSSL：使用 vendored bundle ${OPENSSL_VERSION}（${OPENSSL_BUNDLE_LINKAGE}）")
            # 把 bundle 选中根固化为缓存 OPENSSL_ROOT_DIR：此后任何作用域的
            # find_package(OpenSSL)（ZzSshCore、其嵌套 libssh2、src/session）
            # 的 find_library/find_path 都按 <根>/lib、<根>/include 命中 bundle 真实
            # 产物。否则模块模式会无视 bundle 的普通变量自行搜索：Linux 上静默
            # 命中系统库导致变量与导入目标指向两套产物，Windows 上无系统库可中
            # 直接 NOTFOUND 而 REQUIRED 失败。
            set(OPENSSL_ROOT_DIR "${OPENSSL_BUNDLE_SELECTED_ROOT}" CACHE PATH
                "vendored OpenSSL bundle 选中根" FORCE)
        endif()
    endif()
endif()

# ZzSshCore（计划 01）：libssh2 由 ZzSshCore 自带的嵌套子模块
#（third_party/ZzSshCore/third_party/libssh2）编译提供——其 CMake 检测到上层
# 没有 libssh2 目标时会自动 add_subdirectory 嵌套副本（唯一事实源，
# 消除主仓库/嵌套两份指针的漂移风险）。需要 git submodule update --init --recursive。
# BUILD_SHARED_LIBS 已在 ZzPureToolsPro 之前统一强制 OFF，libssh2 同为静态构建，
# 与计划 01 验证配置一致
zz_require_submodule(third_party/ZzSshCore)
# ZzSshCore 自带测试已在计划 01 全量验证，应用构建不重复编译
set(ZZSSHCORE_BUILD_TESTS OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/ZzSshCore)

# LZ4（温层压缩，供计划 02 的 ZzLogEngine 链接 lz4_static；官方 CMake 位于 build/cmake）
zz_require_submodule(third_party/lz4/build/cmake)
set(LZ4_BUILD_CLI OFF CACHE BOOL "" FORCE)
set(LZ4_BUILD_LEGACY_LZ4C OFF CACHE BOOL "" FORCE)
set(LZ4_BUILD_SHARED_LZ4_LIBRARY OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/lz4/build/cmake)

# ZSTD（冷层压缩，供 ZzLogEngine 链接 libzstd_static；官方 CMake 位于 build/cmake）
zz_require_submodule(third_party/zstd/build/cmake)
set(ZSTD_BUILD_PROGRAMS OFF CACHE BOOL "不构建 zstd 命令行工具" FORCE)
set(ZSTD_BUILD_SHARED OFF CACHE BOOL "不构建 zstd 动态库" FORCE)
set(ZSTD_BUILD_TESTS OFF CACHE BOOL "不构建 zstd 测试" FORCE)
add_subdirectory(third_party/zstd/build/cmake)

# SQLite amalgamation（vendored，Public Domain）：冷层持久存储 + FTS5
if(NOT EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/sqlite/CMakeLists.txt")
    message(FATAL_ERROR
        "SQLite amalgamation 缺失：third_party/sqlite\n请先执行冷层实现计划任务 1 的下载步骤")
endif()
# 根工程只声明 CXX；sqlite3.c 为 C 源，需在根作用域启用 C
#（zstd/lz4 自带 project() 声明 C，无需此处理）
enable_language(C)
add_subdirectory(third_party/sqlite)

add_subdirectory(src)

if(ZZCLAWTERM_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
