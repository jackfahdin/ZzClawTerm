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
    panel/ZzSessionEditDialog.h
    panel/ZzSessionEditDialog.cpp
    settings/ZzAppSettings.h
    settings/ZzAppSettings.cpp
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
# main.cpp 直接使用 ZzWindowKit bootstrap 头文件；ZzPureTools 仅私有链接它，需显式链接
target_link_libraries(ZzClawTerm PRIVATE ZzClawTermApp Zz::WindowKit)

install(TARGETS ZzClawTerm
    RUNTIME DESTINATION bin
    BUNDLE DESTINATION .
)
