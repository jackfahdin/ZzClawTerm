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

# ─────────────────────────────────────────────────────────────
# 构建后部署: 将可执行文件与全部 Qt 运行时依赖打包到自包含的
# dist 目录, 直接可运行, 避免每次清理后丢失依赖。
# 可用 -DZZ_DEPLOY=OFF 关闭 (例如快速迭代时)。
# ─────────────────────────────────────────────────────────────
option(ZZ_DEPLOY "构建后自动部署到 dist 目录 (含 Qt 依赖)" ON)
set(ZZ_DIST_DIR "${CMAKE_BINARY_DIR}/dist" CACHE PATH "部署输出目录")

if(ZZ_DEPLOY)
    # 定位 Qt 工具目录 (qmake 同级)。
    get_target_property(_zz_qmake Qt6::qmake IMPORTED_LOCATION)
    get_filename_component(_zz_qt_bin "${_zz_qmake}" DIRECTORY)

    if(WIN32)
        find_program(ZZ_WINDEPLOYQT windeployqt HINTS "${_zz_qt_bin}")
        if(ZZ_WINDEPLOYQT)
            add_custom_command(TARGET ZzClawTerm POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E make_directory "${ZZ_DIST_DIR}"
                COMMAND ${CMAKE_COMMAND} -E copy_if_different
                        "$<TARGET_FILE:ZzClawTerm>" "${ZZ_DIST_DIR}/"
                COMMAND "${ZZ_WINDEPLOYQT}" --release --no-translations
                        --compiler-runtime
                        "${ZZ_DIST_DIR}/$<TARGET_FILE_NAME:ZzClawTerm>"
                COMMENT "部署 ZzClawTerm 与 Qt 依赖到 ${ZZ_DIST_DIR}"
                VERBATIM)
        else()
            message(WARNING "未找到 windeployqt, 跳过自动部署")
        endif()
    elseif(APPLE)
        find_program(ZZ_MACDEPLOYQT macdeployqt HINTS "${_zz_qt_bin}")
        if(ZZ_MACDEPLOYQT)
            add_custom_command(TARGET ZzClawTerm POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E make_directory "${ZZ_DIST_DIR}"
                COMMAND ${CMAKE_COMMAND} -E copy_directory
                        "$<TARGET_BUNDLE_DIR:ZzClawTerm>" "${ZZ_DIST_DIR}/ZzClawTerm.app"
                COMMAND "${ZZ_MACDEPLOYQT}" "${ZZ_DIST_DIR}/ZzClawTerm.app"
                COMMENT "部署 ZzClawTerm.app 与 Qt 依赖到 ${ZZ_DIST_DIR}"
                VERBATIM)
        else()
            message(WARNING "未找到 macdeployqt, 跳过自动部署")
        endif()
    else()
        # Linux: 复制可执行文件 (完整 AppImage 由 scripts/ 处理)。
        add_custom_command(TARGET ZzClawTerm POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E make_directory "${ZZ_DIST_DIR}"
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                    "$<TARGET_FILE:ZzClawTerm>" "${ZZ_DIST_DIR}/"
            COMMENT "复制 ZzClawTerm 到 ${ZZ_DIST_DIR}"
            VERBATIM)
    endif()
endif()
