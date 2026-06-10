# ─────────────────────────────────────────────────────────────
# ZzClawTerm 可执行目标
# ─────────────────────────────────────────────────────────────

set(ZZ_SOURCES
    main.cpp

    # 平台适配层
    platform/ZzPtyInterface.cpp
    platform/ZzPtyFactory.cpp

    # 核心: 终端引擎
    core/terminal/ZzVTParser.cpp
    core/terminal/ZzVTParserPrivate.cpp
    core/terminal/ZzTextBuffer.cpp
    core/terminal/ZzTextBufferPrivate.cpp
    core/terminal/ZzTerminalCore.cpp
    core/terminal/ZzTerminalCorePrivate.cpp

    # 核心: 日志引擎
    core/log/ZzRingBuffer.cpp
    core/log/ZzRingBufferPrivate.cpp
    core/log/ZzLineIndex.cpp
    core/log/ZzLineIndexPrivate.cpp
    core/log/ZzColdStorage.cpp
    core/log/ZzColdStoragePrivate.cpp
    core/log/ZzLogEngine.cpp
    core/log/ZzLogEnginePrivate.cpp

    # 核心: 搜索引擎
    core/search/ZzSearchEngine.cpp
    core/search/ZzSearchEnginePrivate.cpp

    # 会话
    session/ZzLocalSession.cpp
    session/ZzLocalSessionPrivate.cpp

    # 业务: 配置与主题
    business/ZzConfigManager.cpp
    business/ZzConfigManagerPrivate.cpp
    business/ZzThemeManager.cpp
    business/ZzThemeManagerPrivate.cpp

    # UI
    ui/ZzTerminalWidget.cpp
    ui/ZzTerminalWidgetPrivate.cpp
    ui/ZzSearchBar.cpp
    ui/ZzSearchBarPrivate.cpp
    ui/ZzMainWindow.cpp
    ui/ZzMainWindowPrivate.cpp
)

# 平台特定 PTY 实现
if(WIN32)
    list(APPEND ZZ_SOURCES
        platform/ZzPtyWindows.cpp
        platform/ZzPtyWindowsPrivate.cpp
    )
else()
    list(APPEND ZZ_SOURCES
        platform/ZzPtyUnix.cpp
        platform/ZzPtyUnixPrivate.cpp
    )
endif()

qt_add_executable(ZzClawTerm WIN32 MACOSX_BUNDLE ${ZZ_SOURCES})

target_include_directories(ZzClawTerm PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
)

target_link_libraries(ZzClawTerm PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Widgets
    Qt6::Sql
    zz_warnings
)

# 可选依赖
if(ZZ_ENABLE_LZ4)
    target_link_libraries(ZzClawTerm PRIVATE lz4::lz4)
    target_compile_definitions(ZzClawTerm PRIVATE ZZ_HAVE_LZ4=1)
endif()
if(ZZ_ENABLE_ZSTD)
    target_link_libraries(ZzClawTerm PRIVATE zstd::libzstd)
    target_compile_definitions(ZzClawTerm PRIVATE ZZ_HAVE_ZSTD=1)
endif()
if(ZZ_ENABLE_FREETYPE)
    target_link_libraries(ZzClawTerm PRIVATE Freetype::Freetype)
    target_compile_definitions(ZzClawTerm PRIVATE ZZ_HAVE_FREETYPE=1)
endif()

# Unix PTY 需要的系统库 (Linux: util)
if(UNIX AND NOT APPLE)
    target_link_libraries(ZzClawTerm PRIVATE util)
endif()
