set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Test)

# 注册一个 QTest 可执行目标：源码为同名 .cpp，链接 ZzSessionCore，并登记进 ctest。
function(zz_session_add_test target)
    add_executable(${target} ${target}.cpp)
    target_link_libraries(${target} PRIVATE ZzSessionCore Qt6::Core Qt6::Test)
    add_test(NAME ${target} COMMAND ${target})
endfunction()

zz_session_add_test(ZzSessionProfileTest)
zz_session_add_test(ZzSessionModelTest)
zz_session_add_test(ZzCredentialStoreTest)
zz_session_add_test(ZzForwardRuleTest)

# 规格 9.1：性能记录统一经 ZzPerfRecorder（ZzPerfInfra）；Release 数字才有效
add_executable(ZzCredentialPerfTest ZzCredentialPerfTest.cpp)
target_link_libraries(ZzCredentialPerfTest PRIVATE ZzSessionCore ZzPerfInfra Qt6::Core Qt6::Test)
add_test(NAME ZzCredentialPerfTest COMMAND ZzCredentialPerfTest)
# 与 ZzLogEnginePerfTest 一致：性能门控归入 perf 标签，便于 ctest -L perf 统一筛选
set_tests_properties(ZzCredentialPerfTest PROPERTIES LABELS "perf")
