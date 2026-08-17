# 统一注册一个 QWidget 级 QTest：离屏运行，链接应用静态库。
function(zz_add_qtest name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE ZzClawTermApp Qt6::Test Qt6::Widgets)
    add_test(NAME ${name} COMMAND ${name})
    # 三平台统一离屏；Windows 上保持控制台子系统以便 ctest 捕获输出
    set_tests_properties(${name} PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endfunction()

add_subdirectory(mocks)

zz_add_qtest(tst_Skeleton unit/tst_Skeleton.cpp)

add_subdirectory(perf)
