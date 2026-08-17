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
