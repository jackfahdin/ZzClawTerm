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

# OpenSSL：优先接入 vendored 预编译 bundle（third_party/openssl 提供
# OpenSSLBundleConfig.cmake，创建 OpenSSL::Crypto/OpenSSL::SSL 导入目标并
# 设置 OPENSSL_INCLUDE_DIR/OPENSSL_*_LIBRARY 变量；其后 libssh2 与
# src/session 的 find_package(OpenSSL) 模块会沿用已设变量与既有导入目标，
# FindOpenSSL 的 NOT TARGET 守卫保证不重复建目标）。
# bundle 无对应平台产物（如 macOS，third_party/openssl 仅有
# android/linux/windows 目录）或产物未构建完整（Windows 侧 static/shared
# 包缺 include/ 与库文件）时跳过，回退系统 OpenSSL（CI 上 macOS 经
# OPENSSL_ROOT_DIR 指向 brew openssl@3，Windows 指向 runner 自带 OpenSSL 3.x）。
if(NOT APPLE AND EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl/OpenSSLBundleConfig.cmake")
    set(ZZ_OPENSSL_BUNDLE_USABLE TRUE)
    if(WIN32)
        # Windows 产物完整性的粗检：任一 toolchain/linkage 下存在 ssl.h 才算可用
        file(GLOB ZZ_OPENSSL_BUNDLE_SSL_HEADERS
            "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl/windows/*/x64/*/include/openssl/ssl.h")
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
        endif()
    endif()
endif()

# ZzSshCore（计划 01）：libssh2 由 ZzSshCore 自带的嵌套子模块
#（third_party/ZzSshCore/third_party/libssh2）编译提供——其 CMake 检测到上层
# 没有 libssh2 目标时会自动 add_subdirectory 嵌套副本（唯一事实源，
# 消除主仓库/嵌套两份指针的漂移风险）。需要 git submodule update --init --recursive。
# ZzPureToolsPro 将 BUILD_SHARED_LIBS 默认置 ON；libssh2 固定静态构建，与计划 01 验证配置一致
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
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
