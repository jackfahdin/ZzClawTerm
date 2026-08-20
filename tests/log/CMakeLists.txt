# 日志引擎测试：每个测试类一个可执行文件并注册进 ctest。
function(zz_add_log_test name)
    add_executable(${name} ${name}.cpp)
    target_link_libraries(${name} PRIVATE ZzLogEngine Qt6::Core Qt6::Test)
    set_target_properties(${name} PROPERTIES AUTOMOC ON)
    add_test(NAME ${name} COMMAND ${name})
endfunction()

zz_add_log_test(ZzLineIndexTest)
zz_add_log_test(ZzRingBufferTest)
zz_add_log_test(ZzMmapBufferTest)
zz_add_log_test(ZzLogEngineTest)
zz_add_log_test(ZzLogArchiveWorkerTest)

# 性能门控测试（规格 §9.1）：结果写入 tests/perf/records/，Release 构建数字才有效。
add_executable(ZzLogEnginePerfTest ZzLogEnginePerfTest.cpp)
target_link_libraries(ZzLogEnginePerfTest PRIVATE ZzLogEngine Qt6::Core Qt6::Test)
set_target_properties(ZzLogEnginePerfTest PROPERTIES AUTOMOC ON)
target_compile_definitions(ZzLogEnginePerfTest PRIVATE
    PERF_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records")
add_test(NAME ZzLogEnginePerfTest COMMAND ZzLogEnginePerfTest)
set_tests_properties(ZzLogEnginePerfTest PROPERTIES LABELS "perf")

# 冷层测试：直接链接 sqlite3_static 以便用 sqlite3 C API 独立验证 schema/WAL
add_executable(ZzColdStorageTest ZzColdStorageTest.cpp)
target_link_libraries(ZzColdStorageTest PRIVATE ZzLogEngine sqlite3_static Qt6::Core Qt6::Test)
set_target_properties(ZzColdStorageTest PROPERTIES AUTOMOC ON)
add_test(NAME ZzColdStorageTest COMMAND ZzColdStorageTest)
