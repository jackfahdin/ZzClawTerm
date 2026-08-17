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
# bundle 缺失（子模块未初始化）时回退系统 OpenSSL。
if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl/OpenSSLBundleConfig.cmake")
    list(PREPEND CMAKE_PREFIX_PATH "${CMAKE_CURRENT_SOURCE_DIR}/third_party/openssl")
    find_package(OpenSSLBundle QUIET)
    if(OpenSSLBundle_FOUND)
        message(STATUS "OpenSSL：使用 vendored bundle ${OPENSSL_VERSION}（${OPENSSL_BUNDLE_LINKAGE}）")
    endif()
endif()

# libssh2（CMake 移植版）与 ZzSshCore（计划 01）：ZzSshCore 链接 libssh2 目标
zz_require_submodule(third_party/libssh2)
# ZzPureToolsPro 将 BUILD_SHARED_LIBS 默认置 ON；libssh2 固定静态构建，与计划 01 验证配置一致
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
# libssh2 使用 OpenSSL 作为加密后端（规格 §十一），不构建其示例与测试
set(CRYPTO_BACKEND "OpenSSL" CACHE STRING "" FORCE)
set(BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/libssh2)
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

add_subdirectory(src)

if(ZZCLAWTERM_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
