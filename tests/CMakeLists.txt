# 统一注册一个 QWidget 级 QTest：离屏运行，链接应用静态库。
function(zz_add_qtest name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE ZzClawTermApp ZzTestMocks Qt6::Test Qt6::Widgets)
    add_test(NAME ${name} COMMAND ${name})
    # 三平台统一离屏；Windows 上保持控制台子系统以便 ctest 捕获输出
    set_tests_properties(${name} PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
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
zz_add_qtest(tst_ZzSshTransport unit/tst_ZzSshTransport.cpp)
