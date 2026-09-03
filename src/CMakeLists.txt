# ZzClawTermApp：除 main.cpp 外的全部应用源码，供可执行与测试共同链接。
set(ZZCLAWTERM_APP_SOURCES
    transport/ZzTransportEndpoint.h
    transport/ZzTransportInterface.h
    transport/ZzTransportRegistry.h
    transport/ZzTransportRegistry.cpp
    transport/ZzLocalPtyTransport.h
    transport/ZzLocalPtyTransport.cpp
    transport/ZzSshTransport.h
    transport/ZzSshTransport.cpp
    transport/ZzTunnelHandle.h
    transport/ZzTunnelManager.h
    transport/ZzTunnelManager.cpp
    transport/ZzSshTunnelHandle.h
    transport/ZzSshTunnelHandle.cpp
    panel/ZzPanelInterface.h
    panel/ZzPanelRegistry.h
    panel/ZzPanelRegistry.cpp
    panel/ZzSessionPanel.h
    panel/ZzSessionPanel.cpp
    panel/ZzSftpOps.h
    panel/ZzSftpSessionOps.h
    panel/ZzSftpSessionOps.cpp
    panel/ZzSftpPanel.h
    panel/ZzSftpPanel.cpp
    settings/ZzAppSettings.h
    settings/ZzAppSettings.cpp
    settings/ZzLanguageManager.h
    settings/ZzLanguageManager.cpp
    settings/ZzSettingsPage.h
    settings/ZzSettingsPage.cpp
    terminal/ZzTerminalView.h
    terminal/ZzTerminalView.cpp
    terminal/ZzScrollbackBridge.h
    terminal/ZzScrollbackBridge.cpp
    tab/ZzTabManager.h
    tab/ZzTabManager.cpp
    tab/ZzSplitContainer.h
    tab/ZzSplitContainer.cpp
    dialog/ZzAboutDialog.h
    dialog/ZzAboutDialog.cpp
    dialog/ZzSshConfigPage.h
    dialog/ZzSshConfigPage.cpp
    dialog/ZzLocalShellConfigPage.h
    dialog/ZzLocalShellConfigPage.cpp
    dialog/ZzSessionConfigWindow.h
    dialog/ZzSessionConfigWindow.cpp
    menu/ZzMenuBarService.h
    menu/ZzMenuBarService.cpp
    dialog/ZzHostKeyDialog.h
    dialog/ZzHostKeyDialog.cpp
    dialog/ZzMasterPasswordDialog.h
    dialog/ZzMasterPasswordDialog.cpp
    x11/ZzXServerDownloader.h
    x11/ZzXServerDownloader.cpp
    x11/ZzXAuthority.h
    x11/ZzXAuthority.cpp
    x11/ZzXServerManager.h
    x11/ZzXServerManager.cpp
    x11/ZzX11Service.h
    x11/ZzX11Service.cpp
    x11/ZzX11Viewport.h
    x11/ZzX11Viewport.cpp
    x11/ZzX11ViewportPrivate.h
    x11/ZzX11ViewportPrivate.cpp
    ZzClawTermModule.h
    ZzClawTermModule.cpp
    ZzAppShell.h
    ZzAppShell.cpp
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
        Zz::FluentUI
        ZzSshCore
        ZzLog::ZzLog
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

# 界面翻译：lrelease 把 en.ts 编成 qm 嵌入 exe 资源（:/i18n/zzclawterm_en.qm）；
# lupdate 手动目标 update_translations 供翻译维护时刷新 ts。
# 简体中文为源文本，无需 zh_CN.qm（卸载 translator 即回退）。
if(TARGET Qt6::lrelease)
    # 子库存在时才纳入 tr() 扫描范围（session/log 经条件 add_subdirectory）
    set(ZZ_I18N_SOURCE_TARGETS ZzClawTermApp ZzClawTerm)
    foreach(optional_target ZzSessionCore ZzLogEngine)
        if(TARGET ${optional_target})
            list(APPEND ZZ_I18N_SOURCE_TARGETS ${optional_target})
        endif()
    endforeach()
    qt_add_translations(ZzClawTerm
        TS_FILES "${CMAKE_SOURCE_DIR}/resources/i18n/zzclawterm_en.ts"
        SOURCE_TARGETS ${ZZ_I18N_SOURCE_TARGETS}
        LRELEASE_OPTIONS -compress -nounfinished)
else()
    message(STATUS "LinguistTools 不可用：跳过界面翻译嵌入")
endif()

set_target_properties(ZzClawTerm PROPERTIES
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
)
if(WIN32)
    # Windows 应用图标：.rc 把多尺寸 ico 编译进 exe 资源
    target_sources(ZzClawTerm PRIVATE windows/zzclawterm.rc)
endif()
if(APPLE)
    # macOS 应用图标：icns 打进 Contents/Resources，Info.plist 声明
    set(ZZ_APP_ICNS "${CMAKE_SOURCE_DIR}/resources/icons/generated/zzclawterm.icns")
    set_source_files_properties("${ZZ_APP_ICNS}" PROPERTIES
        MACOSX_PACKAGE_LOCATION Resources)
    target_sources(ZzClawTerm PRIVATE "${ZZ_APP_ICNS}")
    set_target_properties(ZzClawTerm PROPERTIES
        MACOSX_BUNDLE_ICON_FILE zzclawterm.icns
        MACOSX_BUNDLE_BUNDLE_NAME "ZzClawTerm"
        MACOSX_BUNDLE_BUNDLE_VERSION "0.1"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "0.1"
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/macos/Info.plist"
    )
endif()
# 运行时窗口图标（Wayland 任务栏图标唯一来源）：AUTORCC 编译 appicon.qrc
#（Qt 6.11 的 qt_add_resources 仅认 <target> <name> FILES 新签名，弃用 .qrc 列表旧式）
set(ZZ_APP_ICON_QRC "${CMAKE_SOURCE_DIR}/resources/icons/appicon.qrc")
set_target_properties(ZzClawTerm PROPERTIES AUTORCC ON)
target_sources(ZzClawTerm PRIVATE "${ZZ_APP_ICON_QRC}")
# main.cpp 直接使用 ZzWindowKit bootstrap 头文件；ZzPureTools 仅私有链接它，需显式链接
target_link_libraries(ZzClawTerm PRIVATE ZzClawTermApp Zz::WindowKit)

install(TARGETS ZzClawTerm
    RUNTIME DESTINATION bin
    BUNDLE DESTINATION .
)
