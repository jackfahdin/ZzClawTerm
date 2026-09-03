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
        # 显式钉死控制台子系统，防御继承 GUI 子系统导致 ctest 捕获不到输出。
        #（ZzPureToolsPro 已改静态链接，测试进程不再依赖 ZzPureTools.dll /
        # ZzCore.dll 的 PATH 搜索。）
        set_target_properties(${name} PROPERTIES WIN32_EXECUTABLE OFF)
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

zz_add_qtest(tst_ZzShellContextMenu unit/tst_ZzShellContextMenu.cpp)
# 该用例自定义 main() 走 ZzWindowKit bootstrap（同生产 main.cpp），需显式链接
target_link_libraries(tst_ZzShellContextMenu PRIVATE Zz::WindowKit)
if(APPLE)
    # 该用例刻意走真实 ZzApplicationWindow 分支；ZzWindowKit Cocoa 后端硬性
    # 要求 cocoa 平台（offscreen 下 attach 返回 Unsupported 导致 build 失败）。
    # GitHub macOS runner 具备 WindowServer 会话，可真实创建窗口，故单独用
    # cocoa 覆盖 zz_add_qtest 的 offscreen 默认。
    set_tests_properties(tst_ZzShellContextMenu PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=cocoa")
endif()
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
zz_add_qtest(tst_ZzSshConfigPage unit/tst_ZzSshConfigPage.cpp)
zz_add_qtest(tst_ZzSessionConfigWindow unit/tst_ZzSessionConfigWindow.cpp)
zz_add_qtest(tst_ZzTunnelIndicator unit/tst_ZzTunnelIndicator.cpp)
zz_add_qtest(tst_ZzXAuthority unit/tst_ZzXAuthority.cpp)
zz_add_qtest(tst_ZzXServerDownloader unit/tst_ZzXServerDownloader.cpp)
zz_add_qtest(tst_ZzXServerManager unit/tst_ZzXServerManager.cpp)
zz_add_qtest(tst_ZzX11Service unit/tst_ZzX11Service.cpp)
zz_add_qtest(tst_ZzX11Viewport unit/tst_ZzX11Viewport.cpp)
zz_add_qtest(tst_ZzLanguageManager unit/tst_ZzLanguageManager.cpp)
zz_add_qtest(tst_ZzAboutDialog unit/tst_ZzAboutDialog.cpp)
zz_add_qtest(tst_ZzMenuBarService unit/tst_ZzMenuBarService.cpp)

# i18n 冒烟用例需真实 en.qm：lrelease 编译 ts 后嵌入两个测试二进制的资源
#（前缀 /i18n，与 ZzLanguageManager 装载路径 :/i18n/zzclawterm_en.qm 一致）。
# 刻意不用 qt_add_translations：它会注册 lupdate 并扫描测试源码，污染 en.ts。
if(TARGET Qt6::lrelease)
    qt_add_lrelease(
        TS_FILES "${CMAKE_SOURCE_DIR}/resources/i18n/zzclawterm_en.ts"
        LRELEASE_TARGET zz_test_translations_lrelease
        OPTIONS -compress -nounfinished
        QM_FILES_OUTPUT_VARIABLE zz_test_qm_files)
    foreach(i18n_test tst_ZzLanguageManager tst_ZzMenuBarService)
        qt_add_resources(${i18n_test} "${i18n_test}_translations"
            PREFIX "/i18n"
            BASE "${CMAKE_CURRENT_BINARY_DIR}"
            FILES ${zz_test_qm_files})
    endforeach()
endif()
