cmake_minimum_required(VERSION 3.25)

project(ZzClawTerm
    VERSION 0.1.0
    DESCRIPTION "高性能跨平台终端工具 (替代并超越 MobaXterm/WindTerm)"
    LANGUAGES CXX
)

# ─────────────────────────────────────────────────────────────
# C++20 标准
# ─────────────────────────────────────────────────────────────
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "构建类型" FORCE)
endif()

# ─────────────────────────────────────────────────────────────
# 构建选项 (重型原生库默认关闭, 保证开箱即可构建)
# ─────────────────────────────────────────────────────────────
option(ZZ_ENABLE_LZ4        "启用 LZ4 温层压缩"        OFF)
option(ZZ_ENABLE_ZSTD       "启用 ZSTD 冷层压缩"       OFF)
option(ZZ_ENABLE_FREETYPE   "启用 FreeType 字形光栅化" OFF)
option(ZZ_ENABLE_TESTS      "构建单元/基准测试"        OFF)
option(ZZ_GPU_RENDER        "使用 OpenGL(GPU) 加速渲染 (关闭则回退 CPU 软件渲染)" ON)

# ─────────────────────────────────────────────────────────────
# 平台配置与编译警告
# ─────────────────────────────────────────────────────────────
include(cmake/PlatformSetup.cmake)
include(cmake/CompilerWarnings.cmake)

# ─────────────────────────────────────────────────────────────
# 依赖
# ─────────────────────────────────────────────────────────────
set(ZZ_QT_COMPONENTS Core Gui Widgets Sql)
if(ZZ_GPU_RENDER)
    list(APPEND ZZ_QT_COMPONENTS OpenGLWidgets)
endif()
find_package(Qt6 6.5 REQUIRED COMPONENTS ${ZZ_QT_COMPONENTS})
qt_standard_project_setup()

if(ZZ_ENABLE_LZ4)
    find_package(lz4 CONFIG REQUIRED)
endif()
if(ZZ_ENABLE_ZSTD)
    find_package(zstd CONFIG REQUIRED)
endif()
if(ZZ_ENABLE_FREETYPE)
    find_package(Freetype REQUIRED)
endif()

add_subdirectory(src)

if(ZZ_ENABLE_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()

# ─────────────────────────────────────────────────────────────
# 安装
# ─────────────────────────────────────────────────────────────
include(GNUInstallDirs)
install(TARGETS ZzClawTerm
    BUNDLE  DESTINATION .
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)
