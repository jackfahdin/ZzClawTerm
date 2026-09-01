# 统一注册一个 QWidget 级 QTest：离屏运行，链接应用静态库。
function(zz_add_qtest name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE ZzClawTermApp ZzTestMocks Qt6::Test Qt6::Widgets)
    add_test(NAME ${name} COMMAND ${name} -o "$<TARGET_FILE:${name}>.qtest.log,txt")
    # 三平台统一离屏；Windows 上保持控制台子系统以便 ctest 捕获输出。
    # -o 额外落一份 QTest 文本结果到可执行文件旁：GitHub Actions Windows
    # runner 会丢弃 ctest 孙进程的终端输出（actions/runner#1206），文件
    # 输出不受控制台代理影响，失败时 ci.yml 兜底 cat 这些日志。
    set_tests_properties(${name} PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
    if(WIN32)
        # ZzPureToolsPro 以 DLL 构建（BUILD_SHARED_LIBS 在主仓强制 OFF 之前
        # 已按其默认值 ON 生效）；Windows 无 rpath，测试进程需经 PATH 找到
        # ZzPureTools.dll / ZzCore.dll，否则 0xc0000135（STATUS_DLL_NOT_FOUND）。
        # 显式钉死控制台子系统，防御继承 GUI 子系统导致 ctest 捕获不到输出。
        set_target_properties(${name} PROPERTIES WIN32_EXECUTABLE OFF)
        set_property(TEST ${name} APPEND PROPERTY ENVIRONMENT_MODIFICATION
            "PATH=path_list_prepend:$<TARGET_FILE_DIR:ZzPureTools>"
            "PATH=path_list_prepend:$<TARGET_FILE_DIR:ZzCore>")
    endif()
endfunction()

add_subdirectory(mocks)

# 性能基建（ZzPerfInfra）先于各后端目录注册，供其性能门控测试链接
add_subdirectory(perf)

# 后端库测试按目录条件接入（计划 02/03 交付后自动生效），与 src/ 侧模式一致
foreach(sub log session)
    if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${sub}/CMakeLists.txt")
        add_subdirectory(${sub})
    else()
        message(STATUS "缺少 tests/${sub}（由计划 02/03 交付），相关测试暂不注册")
    endif()
endforeach()

zz_add_qtest(tst_Skeleton unit/tst_Skeleton.cpp)
zz_add_qtest(tst_ZzAppSettings unit/tst_ZzAppSettings.cpp)
zz_add_qtest(tst_ZzTransportRegistry unit/tst_ZzTransportRegistry.cpp)
zz_add_qtest(tst_ZzLocalPtyTransport unit/tst_ZzLocalPtyTransport.cpp)
zz_add_qtest(tst_ZzTerminalView unit/tst_ZzTerminalView.cpp)
zz_add_qtest(tst_ZzTabManager unit/tst_ZzTabManager.cpp)
zz_add_qtest(tst_ZzSplitContainer unit/tst_ZzSplitContainer.cpp)
zz_add_qtest(tst_ZzSshTransport unit/tst_ZzSshTransport.cpp)
zz_add_qtest(tst_ZzConnectFlow unit/tst_ZzConnectFlow.cpp)
zz_add_qtest(tst_ZzSessionPanel unit/tst_ZzSessionPanel.cpp)
zz_add_qtest(tst_ZzSftpPanel unit/tst_ZzSftpPanel.cpp)
zz_add_qtest(tst_ZzSettingsPage unit/tst_ZzSettingsPage.cpp)
zz_add_qtest(tst_ZzScrollbackBridge unit/tst_ZzScrollbackBridge.cpp)
zz_add_qtest(tst_ZzErrorUi unit/tst_ZzErrorUi.cpp)
zz_add_qtest(tst_ZzAppShell unit/tst_ZzAppShell.cpp)
zz_add_qtest(tst_ZzClawTermModule unit/tst_ZzClawTermModule.cpp)
zz_add_qtest(tst_ZzTunnelManager unit/tst_ZzTunnelManager.cpp)
zz_add_qtest(tst_ZzSshTunnelHandle unit/tst_ZzSshTunnelHandle.cpp)
zz_add_qtest(tst_ZzSessionEditDialog unit/tst_ZzSessionEditDialog.cpp)
zz_add_qtest(tst_ZzTunnelIndicator unit/tst_ZzTunnelIndicator.cpp)
zz_add_qtest(tst_ZzXAuthority unit/tst_ZzXAuthority.cpp)
zz_add_qtest(tst_ZzXServerDownloader unit/tst_ZzXServerDownloader.cpp)
zz_add_qtest(tst_ZzXServerManager unit/tst_ZzXServerManager.cpp)
zz_add_qtest(tst_ZzX11Service unit/tst_ZzX11Service.cpp)
zz_add_qtest(tst_ZzX11Viewport unit/tst_ZzX11Viewport.cpp)
